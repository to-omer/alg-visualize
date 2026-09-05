//! Chen--Yao--Yin prediction-assisted, cost-scaled epsilon-relaxation.
//!
//! The implementation follows Algorithm 2 and the robust exponent-guessing
//! schedule from Remark 1.  Every guess is an explicit attempt.  A failed
//! first scale is abandoned after `n^3` push/price transitions, while the last
//! source-bounded guess is allowed to run to the global deterministic ceiling.
//! No ordinary epsilon-relaxation fallback is hidden behind this descriptor.

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};

/// Conservative node limit for the explicit research kernel.
pub const PREDICTION_EPSILON_MAX_NODES: usize = 64;
/// Conservative edge limit for the explicit research kernel.
pub const PREDICTION_EPSILON_MAX_EDGES: usize = 512;
/// Maximum robust exponent guesses, including the final uncapped guess.
pub const PREDICTION_EPSILON_MAX_ATTEMPTS: u32 = 128;
/// Global ceiling on pushes and price rises across abandoned attempts.
pub const PREDICTION_EPSILON_MAX_STATE_TRANSITIONS: u64 = 1_000_000;
/// Global ceiling on positive residual-arc scans.
pub const PREDICTION_EPSILON_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;
/// Maximum native events retained by the eager trace profile.
pub const PREDICTION_EPSILON_MAX_TRACE_EVENTS: usize = 20_000;
/// Maximum aggregate scene entities admitted by the eager WASM projection.
pub const PREDICTION_EPSILON_MAX_TRACE_PROJECTION_UNITS: usize = 250_000;
/// Preserve all small-instance scans and logarithmic witnesses thereafter.
const PREDICTION_EPSILON_TRACE_SCAN_PREFIX: u128 = 512;

/// Exact counters for the prediction-sensitive state machine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PredictionAssistedEpsilonMetrics {
    /// Robust exponent guesses begun.
    pub attempts: u32,
    /// Guesses abandoned at their first scale.
    pub aborted_attempts: u32,
    /// Completed epsilon-relaxation cost scales.
    pub scaling_phases: u32,
    /// Complete positive-surplus node iterations begun.
    pub up_iterations: u64,
    /// Step-4 price rises.
    pub price_rises: u64,
    /// Epsilon-balanced residual pushes.
    pub pushes: u64,
    /// Pushes exhausting their residual arc.
    pub saturating_pushes: u64,
    /// Pushes exhausting the selected node surplus first.
    pub nonsaturating_pushes: u64,
    /// Positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Integral flow units moved.
    pub pushed_flow_units: u128,
    /// Predictions changed by the source-defined upper clip.
    pub clipped_predictions: u32,
    /// Largest exponent attempted.
    pub maximum_exponent_attempted: u32,
}

impl PredictionAssistedEpsilonMetrics {
    fn transitions(self) -> Result<u64, PredictionAssistedEpsilonError> {
        self.pushes
            .checked_add(self.price_rises)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
    }
}

/// Stable publication stage for the learning-augmented solver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PredictionAssistedEpsilonStage {
    /// Prediction was shifted and clipped into `[0,(n-1)C]`.
    PreprocessPrediction,
    /// One robust exponent guess began.
    BeginAttempt,
    /// One scaled epsilon-CS state was initialized with an empty admissible graph.
    InitializeScale,
    /// A canonical positive-surplus node was selected.
    SelectSurplus,
    /// One outgoing residual arc was tested for epsilon admissibility.
    InspectAdmissibleArc,
    /// One outgoing residual arc was tested as the next price breakpoint.
    InspectPriceBreakpointArc,
    /// An epsilon-balanced residual push was committed.
    Push,
    /// The selected node price rose to the next exact breakpoint.
    RaisePrice,
    /// One complete positive-surplus iteration ended.
    CompleteUpIteration,
    /// One cost scale ended with a feasible epsilon-CS flow.
    CompleteScale,
    /// A too-small exponent guess was abandoned under Remark 1.
    AbortAttempt,
    /// The exact original-cost optimum was independently certified.
    Optimal,
}

/// Complete deterministic publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionAssistedEpsilonSnapshot {
    /// Current source-level stage.
    pub stage: PredictionAssistedEpsilonStage,
    /// Exact unmodified prediction supplied in canonical node order.
    pub raw_predicted_prices: Vec<i128>,
    /// Preprocessed prediction in the original cost domain.
    pub predicted_prices: Vec<i128>,
    /// Whether Algorithm 1 clipped each shifted prediction at `(n-1)C`.
    pub prediction_clipped: Vec<bool>,
    /// Current prices in the active scaled-cost domain.
    pub prices: Vec<i128>,
    /// Current edge costs `floor((n+1)a/c^t)`.
    pub scaled_costs: Vec<i128>,
    /// Current bounded pseudoflow.
    pub flows: Vec<u64>,
    /// Current paper-convention node surplus.
    pub surpluses: Vec<i128>,
    /// One-based robust attempt ordinal, or zero before the first attempt.
    pub attempt: u32,
    /// Largest source-bounded attempt ordinal.
    pub maximum_attempt: u32,
    /// Current guessed exponent `T`.
    pub exponent: u32,
    /// Current descending scale exponent `t`.
    pub scale_exponent: Option<u32>,
    /// Canonical active node.
    pub active_node: Option<usize>,
    /// Canonical active residual arc.
    pub active_arc: Option<ResidualArcId>,
    /// Exact aggregate counters.
    pub metrics: PredictionAssistedEpsilonMetrics,
}

/// One source-level state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionAssistedEpsilonTraceEvent {
    /// Stage closed by the transition.
    pub stage: PredictionAssistedEpsilonStage,
    /// Exact stage-specific scalar.
    pub detail: Option<(&'static str, i128)>,
    /// State before the transition.
    pub before: PredictionAssistedEpsilonSnapshot,
    /// State after the transition.
    pub after: PredictionAssistedEpsilonSnapshot,
}

/// Exact certified result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionAssistedEpsilonResult {
    /// Canonical original-edge flows.
    pub flows: Vec<u64>,
    /// Final prices in the `(n+1)`-scaled original-cost domain.
    pub prices: Vec<i128>,
    /// Exact unmodified prediction supplied in canonical node order.
    pub raw_predicted_prices: Vec<i128>,
    /// Shifted and clipped input prediction.
    pub predicted_prices: Vec<i128>,
    /// Whether Algorithm 1 clipped each shifted prediction at `(n-1)C`.
    pub prediction_clipped: Vec<bool>,
    /// Cost-scaling multiplier selected from `[2,4]`.
    pub scaling_parameter: u32,
    /// Successful robust exponent guess.
    pub selected_exponent: u32,
    /// Infinity error against the independent certificate's normalized dual.
    pub certificate_aligned_prediction_error: i128,
    /// Solver-independent objective and residual dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact source-level counters.
    pub metrics: PredictionAssistedEpsilonMetrics,
    /// Certified terminal state used by the fast profile.
    pub final_snapshot: PredictionAssistedEpsilonSnapshot,
}

/// Certified result plus complete deterministic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PredictionAssistedEpsilonTraceResult {
    /// Same result as the fast profile.
    pub result: PredictionAssistedEpsilonResult,
    /// Prediction-preprocessing boundary.
    pub base_snapshot: PredictionAssistedEpsilonSnapshot,
    /// Complete source-level transition sequence.
    pub events: Vec<PredictionAssistedEpsilonTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: PredictionAssistedEpsilonSnapshot,
}

/// Input, work, arithmetic, invariant, replay, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PredictionAssistedEpsilonError {
    /// Graph or prediction shape exceeds the conservative research band.
    #[error("prediction-assisted epsilon-relaxation input exceeds admission limits")]
    AdmissionLimit,
    /// Scaling parameter is outside the source-recommended closed interval.
    #[error("prediction-assisted epsilon-relaxation scaling parameter must be in [2,4]")]
    ScalingParameter,
    /// Deterministic transition, scan, attempt, or trace ceiling was reached.
    #[error("prediction-assisted epsilon-relaxation work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual-state construction or mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent divergence or optimality certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded the declared integer domain.
    #[error("prediction-assisted epsilon-relaxation arithmetic overflow")]
    ArithmeticOverflow,
    /// A source-defined local update or shape invariant was invalid.
    #[error("prediction-assisted epsilon-relaxation local invariant failed")]
    Invariant,
    /// Epsilon complementary slackness was violated.
    #[error("prediction-assisted epsilon-relaxation epsilon-CS invariant failed")]
    EpsilonComplementarySlackness,
    /// Incremental surplus accounting disagreed with the flow vector.
    #[error("prediction-assisted epsilon-relaxation surplus invariant failed")]
    SurplusInvariant,
    /// The source-required initially empty admissible graph became cyclic.
    #[error("prediction-assisted epsilon-relaxation admissible graph became cyclic")]
    AdmissibleCycle,
    /// Supplied trace differs from deterministic replay or local transitions.
    #[error("prediction-assisted epsilon-relaxation trace invariant failed")]
    TraceInvariant,
}

/// Runs Algorithm 2 with the robust exponent schedule from Remark 1.
///
/// Predictions are exact integers in canonical node order.  Learning the
/// prediction is outside this solver boundary.
///
/// # Errors
///
/// Rejects invalid input, infeasibility, arithmetic overflow, deterministic
/// work ceilings, invariant failures, or independent certificate failures.
pub fn solve_prediction_assisted_epsilon_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
) -> Result<PredictionAssistedEpsilonResult, PredictionAssistedEpsilonError> {
    run_internal(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        false,
    )
    .map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_prediction_assisted_epsilon_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    feasibility: &mut FeasibilityExecution,
) -> Result<PredictionAssistedEpsilonResult, PredictionAssistedEpsilonError> {
    run_internal_with_feasibility(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Records preprocessing, every exponent attempt, scale, push, and price rise.
///
/// # Errors
///
/// Returns the same failures as
/// [`solve_prediction_assisted_epsilon_relaxation`] plus the eager trace limit.
pub fn trace_prediction_assisted_epsilon_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
) -> Result<PredictionAssistedEpsilonTraceResult, PredictionAssistedEpsilonError> {
    let run = run_internal(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        true,
    )?;
    let trace = PredictionAssistedEpsilonTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
    };
    check_prediction_assisted_epsilon_trace(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        &trace,
    )?;
    Ok(trace)
}

/// Traces prediction-assisted epsilon-relaxation while explicitly publishing
/// its feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_prediction_assisted_epsilon_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    feasibility: &mut FeasibilityExecution,
) -> Result<PredictionAssistedEpsilonTraceResult, PredictionAssistedEpsilonError> {
    let run = run_internal_with_feasibility(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        true,
        feasibility,
    )?;
    let trace = PredictionAssistedEpsilonTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
    };
    check_prediction_assisted_epsilon_trace(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        &trace,
    )?;
    Ok(trace)
}

/// Independently checks snapshot invariants, local chaining, and replay.
///
/// # Errors
///
/// Rejects malformed boundaries, invalid local transitions, an uncertified
/// result, or disagreement with a fresh deterministic run.
pub fn check_prediction_assisted_epsilon_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    trace: &PredictionAssistedEpsilonTraceResult,
) -> Result<(), PredictionAssistedEpsilonError> {
    validate_snapshot(graph, required_divergence, &trace.base_snapshot)?;
    let mut previous = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != previous || event.after.stage != event.stage {
            return Err(PredictionAssistedEpsilonError::TraceInvariant);
        }
        validate_snapshot(graph, required_divergence, &event.after)?;
        validate_local_transition(graph, event)?;
        previous = &event.after;
    }
    if previous != &trace.final_snapshot
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.flows != trace.result.flows
    {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if certificate != trace.result.certificate {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    let replay = run_internal(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        true,
    )?;
    if replay.base_snapshot != trace.base_snapshot
        || replay.events != trace.events
        || replay.result != trace.result
    {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    Ok(())
}

struct InternalRun {
    result: PredictionAssistedEpsilonResult,
    base_snapshot: PredictionAssistedEpsilonSnapshot,
    events: Vec<PredictionAssistedEpsilonTraceEvent>,
}

struct Work<'graph> {
    graph: &'graph FlowNetwork,
    required_divergence: &'graph [i128],
    state: ResidualState<'graph>,
    raw_predicted_prices: Vec<i128>,
    predicted_prices: Vec<i128>,
    prediction_clipped: Vec<bool>,
    prices: Vec<i128>,
    scaled_costs: Vec<i128>,
    surpluses: Vec<i128>,
    stage: PredictionAssistedEpsilonStage,
    attempt: u32,
    maximum_attempt: u32,
    exponent: u32,
    scale_exponent: Option<u32>,
    active_node: Option<usize>,
    active_arc: Option<ResidualArcId>,
    metrics: PredictionAssistedEpsilonMetrics,
    pending_residual_arc_scans: u128,
    record_events: bool,
    events: Vec<PredictionAssistedEpsilonTraceEvent>,
}

impl Work<'_> {
    fn snapshot(&self) -> PredictionAssistedEpsilonSnapshot {
        PredictionAssistedEpsilonSnapshot {
            stage: self.stage,
            raw_predicted_prices: self.raw_predicted_prices.clone(),
            predicted_prices: self.predicted_prices.clone(),
            prediction_clipped: self.prediction_clipped.clone(),
            prices: self.prices.clone(),
            scaled_costs: self.scaled_costs.clone(),
            flows: self.state.flows().to_vec(),
            surpluses: self.surpluses.clone(),
            attempt: self.attempt,
            maximum_attempt: self.maximum_attempt,
            exponent: self.exponent,
            scale_exponent: self.scale_exponent,
            active_node: self.active_node,
            active_arc: self.active_arc.clone(),
            metrics: self.metrics,
        }
    }

    fn emit<F>(
        &mut self,
        stage: PredictionAssistedEpsilonStage,
        detail: Option<(&'static str, i128)>,
        mutate: F,
    ) -> Result<(), PredictionAssistedEpsilonError>
    where
        F: FnOnce(&mut Self) -> Result<(), PredictionAssistedEpsilonError>,
    {
        let before = self.record_events.then(|| self.snapshot());
        mutate(self)?;
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(self.pending_residual_arc_scans)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        self.pending_residual_arc_scans = 0;
        self.stage = stage;
        if self.record_events {
            if self.events.len() >= PREDICTION_EPSILON_MAX_TRACE_EVENTS {
                return Err(PredictionAssistedEpsilonError::WorkLimit);
            }
            self.events.push(PredictionAssistedEpsilonTraceEvent {
                stage,
                detail,
                before: before.ok_or(PredictionAssistedEpsilonError::TraceInvariant)?,
                after: self.snapshot(),
            });
        }
        Ok(())
    }

    fn scan(
        &mut self,
        stage: PredictionAssistedEpsilonStage,
        root: NodeIndex,
        arc: &ResidualArc,
    ) -> Result<(), PredictionAssistedEpsilonError> {
        self.pending_residual_arc_scans = self
            .pending_residual_arc_scans
            .checked_add(1)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        let completed = self
            .metrics
            .residual_arc_scans
            .checked_add(self.pending_residual_arc_scans)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        if completed > PREDICTION_EPSILON_MAX_RESIDUAL_ARC_SCANS {
            return Err(PredictionAssistedEpsilonError::WorkLimit);
        }
        if self.record_events
            && (completed <= PREDICTION_EPSILON_TRACE_SCAN_PREFIX || completed.is_power_of_two())
        {
            let arc_id = arc.id.clone();
            let capacity = i128::from(arc.capacity);
            self.emit(stage, Some(("residual-capacity", capacity)), |work| {
                work.active_node = Some(root.as_usize());
                work.active_arc = Some(arc_id);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn check_transition_limit(&self) -> Result<(), PredictionAssistedEpsilonError> {
        if self.metrics.transitions()? > PREDICTION_EPSILON_MAX_STATE_TRANSITIONS {
            return Err(PredictionAssistedEpsilonError::WorkLimit);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhaseOutcome {
    Complete,
    AttemptBudgetExceeded,
}

fn run_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    record_events: bool,
) -> Result<InternalRun, PredictionAssistedEpsilonError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        record_events,
        &mut feasibility,
    )
}

fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, PredictionAssistedEpsilonError> {
    let mut prepared = prepare_run(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
        record_trace,
        feasibility,
    )?;
    let selected_exponent = run_attempts(&mut prepared)?;
    let flows = prepared.work.state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    let certificate_error = certificate_aligned_error(&prepared.preprocessed, &certificate)?;
    prepared.work.emit(
        PredictionAssistedEpsilonStage::Optimal,
        Some(("prediction-error", certificate_error)),
        |work| {
            work.active_node = None;
            work.active_arc = None;
            Ok(())
        },
    )?;
    let final_snapshot = prepared.work.snapshot();
    let result = PredictionAssistedEpsilonResult {
        flows,
        prices: prepared.work.prices.clone(),
        raw_predicted_prices: predicted_prices.to_vec(),
        predicted_prices: prepared.preprocessed,
        prediction_clipped: prepared.prediction_clipped,
        scaling_parameter,
        selected_exponent,
        certificate_aligned_prediction_error: certificate_error,
        certificate,
        metrics: prepared.work.metrics,
        final_snapshot,
    };
    Ok(InternalRun {
        result,
        base_snapshot: prepared.base_snapshot,
        events: prepared.work.events,
    })
}

struct PreparedRun<'a> {
    work: Work<'a>,
    base_snapshot: PredictionAssistedEpsilonSnapshot,
    preprocessed: Vec<i128>,
    prediction_clipped: Vec<bool>,
    cost_scale: i128,
    scaling_parameter: i128,
    attempt_work_cap: u64,
}

fn prepare_run<'a>(
    graph: &'a FlowNetwork,
    required_divergence: &'a [i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<PreparedRun<'a>, PredictionAssistedEpsilonError> {
    validate_input(
        graph,
        required_divergence,
        predicted_prices,
        scaling_parameter,
    )?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let cost_scale = i128::try_from(graph.nodes().len())
        .map_err(|_| PredictionAssistedEpsilonError::ArithmeticOverflow)?
        .checked_add(1)
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
    let maximum_cost = graph
        .edges()
        .iter()
        .map(|edge| i128::from(edge.cost()).abs())
        .max()
        .unwrap_or(0);
    let prediction_upper = i128::try_from(graph.nodes().len().saturating_sub(1))
        .map_err(|_| PredictionAssistedEpsilonError::ArithmeticOverflow)?
        .checked_mul(maximum_cost)
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
    let (preprocessed, prediction_clipped, clipped_predictions) =
        preprocess_prediction(predicted_prices, prediction_upper)?;
    let robust_bound = i128::try_from(graph.nodes().len())
        .map_err(|_| PredictionAssistedEpsilonError::ArithmeticOverflow)?
        .checked_mul(maximum_cost)
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
    let maximum_attempt = maximum_exponent(robust_bound, scaling_parameter)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_state = ResidualState::from_flows(graph, &lower_flows)?;
    let lower_surpluses = surpluses(graph, required_divergence, &lower_flows)?;
    let work = Work {
        graph,
        required_divergence,
        state: lower_state,
        raw_predicted_prices: predicted_prices.to_vec(),
        predicted_prices: preprocessed.clone(),
        prediction_clipped: prediction_clipped.clone(),
        prices: preprocessed.clone(),
        scaled_costs: vec![0; graph.edges().len()],
        surpluses: lower_surpluses,
        stage: PredictionAssistedEpsilonStage::PreprocessPrediction,
        attempt: 0,
        maximum_attempt,
        exponent: 0,
        scale_exponent: None,
        active_node: None,
        active_arc: None,
        metrics: PredictionAssistedEpsilonMetrics {
            clipped_predictions,
            ..PredictionAssistedEpsilonMetrics::default()
        },
        pending_residual_arc_scans: 0,
        record_events,
        events: Vec::new(),
    };
    let base_snapshot = work.snapshot();
    let c = i128::from(scaling_parameter);
    let n_cubed = u64::try_from(graph.nodes().len())
        .map_err(|_| PredictionAssistedEpsilonError::ArithmeticOverflow)?
        .checked_pow(3)
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?
        .max(1);
    Ok(PreparedRun {
        work,
        base_snapshot,
        preprocessed,
        prediction_clipped,
        cost_scale,
        scaling_parameter: c,
        attempt_work_cap: n_cubed,
    })
}

fn run_attempts(prepared: &mut PreparedRun<'_>) -> Result<u32, PredictionAssistedEpsilonError> {
    for exponent in 1..=prepared.work.maximum_attempt {
        if run_exponent_attempt(prepared, exponent)? {
            return Ok(exponent);
        }
    }
    Err(PredictionAssistedEpsilonError::WorkLimit)
}

fn run_exponent_attempt(
    prepared: &mut PreparedRun<'_>,
    exponent: u32,
) -> Result<bool, PredictionAssistedEpsilonError> {
    let work = &mut prepared.work;
    work.emit(
        PredictionAssistedEpsilonStage::BeginAttempt,
        Some(("exponent", i128::from(exponent))),
        |work| {
            work.attempt = work
                .attempt
                .checked_add(1)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.exponent = exponent;
            work.scale_exponent = None;
            work.active_node = None;
            work.active_arc = None;
            work.metrics.attempts = work
                .metrics
                .attempts
                .checked_add(1)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.metrics.maximum_exponent_attempted = exponent;
            Ok(())
        },
    )?;
    let divisor = checked_pow(prepared.scaling_parameter, exponent)?;
    let initial_prices = prepared
        .preprocessed
        .iter()
        .map(|price| {
            price
                .checked_mul(prepared.cost_scale)
                .map(|value| floor_div(value, divisor))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut preceding_prices = initial_prices;
    for scale_exponent in (0..exponent).rev() {
        let phase_prices = preceding_prices
            .iter()
            .map(|price| {
                price
                    .checked_mul(prepared.scaling_parameter)
                    .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scaled_costs = scaled_costs(
            work.graph,
            prepared.cost_scale,
            prepared.scaling_parameter,
            scale_exponent,
        )?;
        initialize_scale(work, phase_prices, scaled_costs, scale_exponent)?;
        let first_scale = scale_exponent + 1 == exponent;
        let attempt_cap =
            (first_scale && exponent < work.maximum_attempt).then_some(prepared.attempt_work_cap);
        match run_scale(work, attempt_cap)? {
            PhaseOutcome::Complete => {
                preceding_prices.clone_from(&work.prices);
                work.emit(
                    PredictionAssistedEpsilonStage::CompleteScale,
                    Some(("scale-exponent", i128::from(scale_exponent))),
                    |work| {
                        work.active_node = None;
                        work.active_arc = None;
                        work.metrics.scaling_phases = work
                            .metrics
                            .scaling_phases
                            .checked_add(1)
                            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
            }
            PhaseOutcome::AttemptBudgetExceeded => {
                work.emit(
                    PredictionAssistedEpsilonStage::AbortAttempt,
                    Some(("work-unit-budget", i128::from(prepared.attempt_work_cap))),
                    |work| {
                        work.active_node = None;
                        work.active_arc = None;
                        work.metrics.aborted_attempts = work
                            .metrics
                            .aborted_attempts
                            .checked_add(1)
                            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
                        Ok(())
                    },
                )?;
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn initialize_scale(
    work: &mut Work<'_>,
    prices: Vec<i128>,
    scaled_costs: Vec<i128>,
    scale_exponent: u32,
) -> Result<(), PredictionAssistedEpsilonError> {
    let initial_flows = work
        .graph
        .edges()
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let reduced = prices[edge.from().as_usize()]
                .checked_sub(prices[edge.to().as_usize()])
                .and_then(|value| value.checked_sub(scaled_costs[edge_index]))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            Ok(if reduced >= 1 {
                edge.capacity()
            } else {
                edge.lower()
            })
        })
        .collect::<Result<Vec<_>, PredictionAssistedEpsilonError>>()?;
    let state = ResidualState::from_flows(work.graph, &initial_flows)?;
    let surplus = surpluses(work.graph, work.required_divergence, &initial_flows)?;
    work.emit(
        PredictionAssistedEpsilonStage::InitializeScale,
        Some(("scale-exponent", i128::from(scale_exponent))),
        |work| {
            work.state = state;
            work.prices = prices;
            work.scaled_costs = scaled_costs;
            work.surpluses = surplus;
            work.scale_exponent = Some(scale_exponent);
            work.active_node = None;
            work.active_arc = None;
            Ok(())
        },
    )?;
    validate_work_state(work)?;
    validate_admissible_acyclic(work)?;
    Ok(())
}

fn run_scale(
    work: &mut Work<'_>,
    attempt_work_cap: Option<u64>,
) -> Result<PhaseOutcome, PredictionAssistedEpsilonError> {
    let initial_work = current_attempt_work(work)?;
    while let Some(root) = first_positive_surplus(work.graph, &work.surpluses) {
        if attempt_work_cap.is_some_and(|cap| {
            current_attempt_work(work)
                .ok()
                .and_then(|now| now.checked_sub(initial_work))
                .is_some_and(|used| used >= u128::from(cap))
        }) {
            return Ok(PhaseOutcome::AttemptBudgetExceeded);
        }
        work.emit(
            PredictionAssistedEpsilonStage::SelectSurplus,
            Some(("surplus", work.surpluses[root.as_usize()])),
            |work| {
                work.active_node = Some(root.as_usize());
                work.active_arc = None;
                work.metrics.up_iterations = work
                    .metrics
                    .up_iterations
                    .checked_add(1)
                    .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
                Ok(())
            },
        )?;
        while work.surpluses[root.as_usize()] > 0 {
            if attempt_work_cap.is_some_and(|cap| {
                current_attempt_work(work)
                    .ok()
                    .and_then(|now| now.checked_sub(initial_work))
                    .is_some_and(|used| used >= u128::from(cap))
            }) {
                return Ok(PhaseOutcome::AttemptBudgetExceeded);
            }
            if let Some(arc) = first_admissible_arc(work, root)? {
                push(work, root, &arc)?;
            } else {
                raise_price(work, root)?;
            }
        }
        work.emit(
            PredictionAssistedEpsilonStage::CompleteUpIteration,
            Some(("surplus", 0)),
            |work| {
                work.active_node = Some(root.as_usize());
                work.active_arc = None;
                Ok(())
            },
        )?;
    }
    validate_work_state(work)?;
    if work.surpluses.iter().any(|surplus| *surplus != 0) {
        return Err(PredictionAssistedEpsilonError::SurplusInvariant);
    }
    Ok(PhaseOutcome::Complete)
}

fn current_attempt_work(work: &Work<'_>) -> Result<u128, PredictionAssistedEpsilonError> {
    u128::from(work.metrics.transitions()?)
        .checked_add(work.metrics.residual_arc_scans)
        .and_then(|value| value.checked_add(work.pending_residual_arc_scans))
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
}

fn first_admissible_arc(
    work: &mut Work<'_>,
    root: NodeIndex,
) -> Result<Option<ResidualArc>, PredictionAssistedEpsilonError> {
    for arc in work.state.outgoing_arcs(root) {
        work.scan(
            PredictionAssistedEpsilonStage::InspectAdmissibleArc,
            root,
            &arc,
        )?;
        if arc.from != arc.to && is_admissible(work, &arc)? {
            return Ok(Some(arc));
        }
    }
    Ok(None)
}

fn push(
    work: &mut Work<'_>,
    root: NodeIndex,
    arc: &ResidualArc,
) -> Result<(), PredictionAssistedEpsilonError> {
    let amount = work.surpluses[root.as_usize()].min(i128::from(arc.capacity));
    let amount = u64::try_from(amount).map_err(|_| PredictionAssistedEpsilonError::Invariant)?;
    if amount == 0 || !is_admissible(work, arc)? {
        return Err(PredictionAssistedEpsilonError::SurplusInvariant);
    }
    let arc_id = arc.id.clone();
    work.emit(
        PredictionAssistedEpsilonStage::Push,
        Some(("delta", i128::from(amount))),
        |work| {
            work.state.augment(std::slice::from_ref(&arc_id), amount)?;
            work.surpluses[root.as_usize()] = work.surpluses[root.as_usize()]
                .checked_sub(i128::from(amount))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.surpluses[arc.to.as_usize()] = work.surpluses[arc.to.as_usize()]
                .checked_add(i128::from(amount))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.active_node = Some(root.as_usize());
            work.active_arc = Some(arc_id.clone());
            work.metrics.pushes = work
                .metrics
                .pushes
                .checked_add(1)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            if amount == arc.capacity {
                work.metrics.saturating_pushes = work
                    .metrics
                    .saturating_pushes
                    .checked_add(1)
                    .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            } else {
                work.metrics.nonsaturating_pushes = work
                    .metrics
                    .nonsaturating_pushes
                    .checked_add(1)
                    .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            }
            work.metrics.pushed_flow_units = work
                .metrics
                .pushed_flow_units
                .checked_add(u128::from(amount))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.check_transition_limit()?;
            Ok(())
        },
    )?;
    validate_work_state(work)?;
    validate_admissible_acyclic(work)?;
    Ok(())
}

fn raise_price(work: &mut Work<'_>, root: NodeIndex) -> Result<(), PredictionAssistedEpsilonError> {
    let old_price = work.prices[root.as_usize()];
    let mut next_price = None;
    for arc in work.state.outgoing_arcs(root) {
        work.scan(
            PredictionAssistedEpsilonStage::InspectPriceBreakpointArc,
            root,
            &arc,
        )?;
        if arc.from == arc.to {
            continue;
        }
        let candidate = work.prices[arc.to.as_usize()]
            .checked_add(residual_scaled_cost(work, &arc.id)?)
            .and_then(|value| value.checked_add(1))
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        next_price = Some(next_price.map_or(candidate, |current: i128| current.min(candidate)));
    }
    let next_price = next_price
        .filter(|candidate| *candidate > old_price)
        .ok_or(PredictionAssistedEpsilonError::Invariant)?;
    work.emit(
        PredictionAssistedEpsilonStage::RaisePrice,
        Some((
            "delta",
            next_price
                .checked_sub(old_price)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?,
        )),
        |work| {
            work.prices[root.as_usize()] = next_price;
            work.active_node = Some(root.as_usize());
            work.active_arc = None;
            work.metrics.price_rises = work
                .metrics
                .price_rises
                .checked_add(1)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            work.check_transition_limit()?;
            Ok(())
        },
    )?;
    validate_work_state(work)?;
    validate_admissible_acyclic(work)?;
    Ok(())
}

fn is_admissible(
    work: &Work<'_>,
    arc: &ResidualArc,
) -> Result<bool, PredictionAssistedEpsilonError> {
    let reduced = work.prices[arc.from.as_usize()]
        .checked_sub(work.prices[arc.to.as_usize()])
        .and_then(|value| value.checked_sub(residual_scaled_cost(work, &arc.id).ok()?))
        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
    Ok(reduced == 1)
}

fn residual_scaled_cost(
    work: &Work<'_>,
    arc: &ResidualArcId,
) -> Result<i128, PredictionAssistedEpsilonError> {
    let edge = work
        .graph
        .edge_index(arc.original_edge())
        .ok_or(PredictionAssistedEpsilonError::Invariant)?;
    let cost = *work
        .scaled_costs
        .get(edge.as_usize())
        .ok_or(PredictionAssistedEpsilonError::Invariant)?;
    match arc.direction() {
        ResidualDirection::Forward => Ok(cost),
        ResidualDirection::Reverse => cost
            .checked_neg()
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow),
    }
}

fn validate_work_state(work: &Work<'_>) -> Result<(), PredictionAssistedEpsilonError> {
    validate_epsilon_cs(
        work.graph,
        work.state.flows(),
        &work.prices,
        &work.scaled_costs,
    )?;
    let expected = surpluses(work.graph, work.required_divergence, work.state.flows())?;
    if expected != work.surpluses {
        return Err(PredictionAssistedEpsilonError::Invariant);
    }
    Ok(())
}

fn validate_epsilon_cs(
    graph: &FlowNetwork,
    flows: &[u64],
    prices: &[i128],
    scaled_costs: &[i128],
) -> Result<(), PredictionAssistedEpsilonError> {
    if flows.len() != graph.edges().len()
        || prices.len() != graph.nodes().len()
        || scaled_costs.len() != graph.edges().len()
    {
        return Err(PredictionAssistedEpsilonError::Invariant);
    }
    for ((edge, &flow), &cost) in graph.edges().iter().zip(flows).zip(scaled_costs) {
        let reduced = prices[edge.from().as_usize()]
            .checked_sub(prices[edge.to().as_usize()])
            .and_then(|value| value.checked_sub(cost))
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        if (flow < edge.capacity() && reduced > 1) || (flow > edge.lower() && reduced < -1) {
            return Err(PredictionAssistedEpsilonError::EpsilonComplementarySlackness);
        }
    }
    Ok(())
}

fn validate_admissible_acyclic(work: &Work<'_>) -> Result<(), PredictionAssistedEpsilonError> {
    let mut colors = vec![0_u8; work.graph.nodes().len()];
    for node in work.graph.node_indices() {
        if colors[node.as_usize()] == 0 {
            visit_admissible(work, node, &mut colors)?;
        }
    }
    Ok(())
}

fn visit_admissible(
    work: &Work<'_>,
    node: NodeIndex,
    colors: &mut [u8],
) -> Result<(), PredictionAssistedEpsilonError> {
    colors[node.as_usize()] = 1;
    for arc in work.state.outgoing_arcs(node) {
        if arc.from == arc.to || !is_admissible(work, &arc)? {
            continue;
        }
        match colors[arc.to.as_usize()] {
            1 => return Err(PredictionAssistedEpsilonError::AdmissibleCycle),
            0 => visit_admissible(work, arc.to, colors)?,
            _ => {}
        }
    }
    colors[node.as_usize()] = 2;
    Ok(())
}

fn validate_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &PredictionAssistedEpsilonSnapshot,
) -> Result<(), PredictionAssistedEpsilonError> {
    if snapshot.raw_predicted_prices.len() != graph.nodes().len()
        || snapshot.predicted_prices.len() != graph.nodes().len()
        || snapshot.prediction_clipped.len() != graph.nodes().len()
        || snapshot.prices.len() != graph.nodes().len()
        || snapshot.scaled_costs.len() != graph.edges().len()
        || snapshot.flows.len() != graph.edges().len()
        || snapshot.surpluses.len() != graph.nodes().len()
        || snapshot
            .active_node
            .is_some_and(|node| node >= graph.nodes().len())
    {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    let maximum_cost = graph
        .edges()
        .iter()
        .map(|edge| i128::from(edge.cost()).abs())
        .max()
        .unwrap_or(0);
    let prediction_upper = i128::try_from(graph.nodes().len().saturating_sub(1))
        .map_err(|_| PredictionAssistedEpsilonError::TraceInvariant)?
        .checked_mul(maximum_cost)
        .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
    let (expected_prediction, expected_clipped, clipped_count) =
        preprocess_prediction(&snapshot.raw_predicted_prices, prediction_upper)?;
    if snapshot.predicted_prices != expected_prediction
        || snapshot.prediction_clipped != expected_clipped
        || snapshot.metrics.clipped_predictions != clipped_count
    {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    ResidualState::from_flows(graph, &snapshot.flows)?;
    if surpluses(graph, required_divergence, &snapshot.flows)? != snapshot.surpluses {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    if !matches!(
        snapshot.stage,
        PredictionAssistedEpsilonStage::PreprocessPrediction
            | PredictionAssistedEpsilonStage::BeginAttempt
    ) {
        validate_epsilon_cs(
            graph,
            &snapshot.flows,
            &snapshot.prices,
            &snapshot.scaled_costs,
        )?;
    }
    if let Some(arc) = &snapshot.active_arc {
        graph
            .edge_index(arc.original_edge())
            .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
    }
    if matches!(
        snapshot.stage,
        PredictionAssistedEpsilonStage::InspectAdmissibleArc
            | PredictionAssistedEpsilonStage::InspectPriceBreakpointArc
    ) {
        let node = NodeIndex::try_from_usize(
            snapshot
                .active_node
                .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?,
        )
        .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
        let arc = snapshot
            .active_arc
            .as_ref()
            .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
        let state = ResidualState::from_flows(graph, &snapshot.flows)?;
        if !state
            .outgoing_arcs(node)
            .iter()
            .any(|candidate| &candidate.id == arc)
        {
            return Err(PredictionAssistedEpsilonError::TraceInvariant);
        }
    }
    Ok(())
}

fn validate_local_transition(
    graph: &FlowNetwork,
    event: &PredictionAssistedEpsilonTraceEvent,
) -> Result<(), PredictionAssistedEpsilonError> {
    if event.after.metrics.attempts < event.before.metrics.attempts
        || event.after.metrics.pushes < event.before.metrics.pushes
        || event.after.metrics.price_rises < event.before.metrics.price_rises
        || event.after.metrics.residual_arc_scans < event.before.metrics.residual_arc_scans
    {
        return Err(PredictionAssistedEpsilonError::TraceInvariant);
    }
    match event.stage {
        PredictionAssistedEpsilonStage::Push => {
            if event.after.metrics.pushes != event.before.metrics.pushes + 1
                || event.after.active_arc.is_none()
                || changed_count(&event.before.flows, &event.after.flows) != 1
                || event.before.prices != event.after.prices
            {
                return Err(PredictionAssistedEpsilonError::TraceInvariant);
            }
        }
        PredictionAssistedEpsilonStage::RaisePrice => {
            let active = event
                .after
                .active_node
                .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
            if event.after.metrics.price_rises != event.before.metrics.price_rises + 1
                || event.before.flows != event.after.flows
                || changed_count(&event.before.prices, &event.after.prices) != 1
                || event.after.prices[active] <= event.before.prices[active]
            {
                return Err(PredictionAssistedEpsilonError::TraceInvariant);
            }
        }
        PredictionAssistedEpsilonStage::InspectAdmissibleArc
        | PredictionAssistedEpsilonStage::InspectPriceBreakpointArc => {
            if event.after.metrics.residual_arc_scans <= event.before.metrics.residual_arc_scans
                || event.before.flows != event.after.flows
                || event.before.prices != event.after.prices
                || event.before.surpluses != event.after.surpluses
                || event.after.active_node.is_none()
                || event.after.active_arc.is_none()
            {
                return Err(PredictionAssistedEpsilonError::TraceInvariant);
            }
        }
        PredictionAssistedEpsilonStage::InitializeScale => {
            let state = ResidualState::from_flows(graph, &event.after.flows)?;
            for node in graph.node_indices() {
                for arc in state.outgoing_arcs(node) {
                    if arc.from == arc.to {
                        continue;
                    }
                    let edge = graph
                        .edge_index(arc.id.original_edge())
                        .ok_or(PredictionAssistedEpsilonError::TraceInvariant)?;
                    let cost = event.after.scaled_costs[edge.as_usize()];
                    let residual_cost = match arc.id.direction() {
                        ResidualDirection::Forward => cost,
                        ResidualDirection::Reverse => cost
                            .checked_neg()
                            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?,
                    };
                    let reduced = event.after.prices[arc.from.as_usize()]
                        .checked_sub(event.after.prices[arc.to.as_usize()])
                        .and_then(|value| value.checked_sub(residual_cost))
                        .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
                    if reduced == 1 {
                        return Err(PredictionAssistedEpsilonError::TraceInvariant);
                    }
                }
            }
        }
        PredictionAssistedEpsilonStage::BeginAttempt
        | PredictionAssistedEpsilonStage::SelectSurplus
        | PredictionAssistedEpsilonStage::CompleteUpIteration
        | PredictionAssistedEpsilonStage::CompleteScale
        | PredictionAssistedEpsilonStage::AbortAttempt
        | PredictionAssistedEpsilonStage::Optimal
        | PredictionAssistedEpsilonStage::PreprocessPrediction => {}
    }
    Ok(())
}

fn changed_count<T: Eq>(before: &[T], after: &[T]) -> usize {
    before
        .iter()
        .zip(after)
        .filter(|(before, after)| before != after)
        .count()
}

fn validate_input(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    predicted_prices: &[i128],
    scaling_parameter: u32,
) -> Result<(), PredictionAssistedEpsilonError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > PREDICTION_EPSILON_MAX_NODES
        || graph.edges().len() > PREDICTION_EPSILON_MAX_EDGES
        || required_divergence.len() != graph.nodes().len()
        || predicted_prices.len() != graph.nodes().len()
    {
        return Err(PredictionAssistedEpsilonError::AdmissionLimit);
    }
    if !(2..=4).contains(&scaling_parameter) {
        return Err(PredictionAssistedEpsilonError::ScalingParameter);
    }
    Ok(())
}

fn preprocess_prediction(
    prediction: &[i128],
    upper: i128,
) -> Result<(Vec<i128>, Vec<bool>, u32), PredictionAssistedEpsilonError> {
    let minimum = prediction
        .iter()
        .copied()
        .min()
        .ok_or(PredictionAssistedEpsilonError::AdmissionLimit)?;
    let mut clipped = 0_u32;
    let mut clipped_nodes = Vec::with_capacity(prediction.len());
    let values = prediction
        .iter()
        .map(|&price| {
            let shifted = price.checked_sub(minimum).unwrap_or(i128::MAX);
            if shifted > upper {
                clipped = clipped
                    .checked_add(1)
                    .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
                clipped_nodes.push(true);
                Ok(upper)
            } else {
                clipped_nodes.push(false);
                Ok(shifted)
            }
        })
        .collect::<Result<Vec<_>, PredictionAssistedEpsilonError>>()?;
    Ok((values, clipped_nodes, clipped))
}

fn maximum_exponent(
    robust_bound: i128,
    scaling_parameter: u32,
) -> Result<u32, PredictionAssistedEpsilonError> {
    let c = i128::from(scaling_parameter);
    let bound = robust_bound.max(c);
    let mut exponent = 1_u32;
    let mut power = c;
    while power <= bound / c {
        power = power
            .checked_mul(c)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        exponent = exponent
            .checked_add(1)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
        if exponent > PREDICTION_EPSILON_MAX_ATTEMPTS {
            return Err(PredictionAssistedEpsilonError::WorkLimit);
        }
    }
    Ok(exponent)
}

fn checked_pow(base: i128, exponent: u32) -> Result<i128, PredictionAssistedEpsilonError> {
    let mut value = 1_i128;
    for _ in 0..exponent {
        value = value
            .checked_mul(base)
            .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
    }
    Ok(value)
}

fn scaled_costs(
    graph: &FlowNetwork,
    cost_scale: i128,
    c: i128,
    exponent: u32,
) -> Result<Vec<i128>, PredictionAssistedEpsilonError> {
    let divisor = checked_pow(c, exponent)?;
    graph
        .edges()
        .iter()
        .map(|edge| {
            i128::from(edge.cost())
                .checked_mul(cost_scale)
                .map(|cost| floor_div(cost, divisor))
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
        })
        .collect()
}

fn floor_div(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder != 0 && numerator < 0 {
        quotient - 1
    } else {
        quotient
    }
}

fn surpluses(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
) -> Result<Vec<i128>, PredictionAssistedEpsilonError> {
    if required_divergence.len() != graph.nodes().len() {
        return Err(PredictionAssistedEpsilonError::AdmissionLimit);
    }
    divergences(graph, flows)?
        .into_iter()
        .zip(required_divergence)
        .map(|(actual, &required)| {
            required
                .checked_sub(actual)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
        })
        .collect()
}

fn first_positive_surplus(graph: &FlowNetwork, surplus: &[i128]) -> Option<NodeIndex> {
    graph
        .node_indices()
        .find(|node| surplus[node.as_usize()] > 0)
}

fn certificate_aligned_error(
    prediction: &[i128],
    certificate: &MinCostFlowCertificate,
) -> Result<i128, PredictionAssistedEpsilonError> {
    let paper_prices = certificate
        .potentials
        .iter()
        .map(|potential| {
            potential
                .checked_neg()
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let minimum = paper_prices
        .iter()
        .copied()
        .min()
        .ok_or(PredictionAssistedEpsilonError::Invariant)?;
    prediction
        .iter()
        .zip(paper_prices)
        .try_fold(0_i128, |error, (&predicted, optimal)| {
            let normalized = optimal
                .checked_sub(minimum)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            let difference = predicted
                .checked_sub(normalized)
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?
                .checked_abs()
                .ok_or(PredictionAssistedEpsilonError::ArithmeticOverflow)?;
            Ok(error.max(difference))
        })
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node id"), supply))
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge id"),
                        from: NodeId::parse(from).expect("from"),
                        to: NodeId::parse(to).expect("to"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("network")
    }

    #[test]
    fn exact_prediction_runs_one_attempt_and_certifies() {
        let graph = network(
            &[("s", 2), ("a", 0), ("t", -2)],
            &[
                ("at", "a", "t", 0, 2, 1),
                ("direct", "s", "t", 0, 2, 5),
                ("sa", "s", "a", 0, 2, 1),
            ],
        );
        let required = vec![0, 2, -2];
        let result = solve_prediction_assisted_epsilon_relaxation(&graph, &required, &[1, 2, 0], 2)
            .expect("prediction-assisted result");
        assert_eq!(result.certificate.total_cost, 4);
        assert_eq!(result.flows, vec![2, 0, 2]);
        assert_eq!(result.metrics.attempts, 1);
        assert_eq!(result.metrics.aborted_attempts, 0);
        assert_eq!(
            result.final_snapshot.stage,
            PredictionAssistedEpsilonStage::Optimal
        );
    }

    #[test]
    fn trace_exposes_preprocess_scales_and_replays() {
        let graph = network(
            &[("s", 3), ("m", 0), ("t", -3)],
            &[
                ("direct", "s", "t", 0, 3, 8),
                ("mt", "m", "t", 0, 3, -1),
                ("sm", "s", "m", 0, 3, 2),
            ],
        );
        let required = vec![0, 3, -3];
        let trace = trace_prediction_assisted_epsilon_relaxation(
            &graph,
            &required,
            &[-100, 200, i128::MAX],
            2,
        )
        .expect("trace");
        assert!(trace.result.metrics.clipped_predictions >= 1);
        assert!(trace.result.metrics.aborted_attempts >= 1);
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.stage == PredictionAssistedEpsilonStage::InitializeScale })
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.stage == PredictionAssistedEpsilonStage::Push })
        );
        let scan_events = trace
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.stage,
                    PredictionAssistedEpsilonStage::InspectAdmissibleArc
                        | PredictionAssistedEpsilonStage::InspectPriceBreakpointArc
                )
            })
            .collect::<Vec<_>>();
        assert!(
            scan_events.len() > 8,
            "small runs must expose each residual-arc inspection"
        );
        assert!(scan_events.iter().all(|event| {
            event.after.active_node.is_some()
                && event.after.active_arc.is_some()
                && event.after.metrics.residual_arc_scans > event.before.metrics.residual_arc_scans
        }));
        check_prediction_assisted_epsilon_trace(
            &graph,
            &required,
            &[-100, 200, i128::MAX],
            2,
            &trace,
        )
        .expect("checked trace");
    }

    #[test]
    fn negative_cost_scaling_uses_mathematical_floor() {
        assert_eq!(floor_div(-1, 2), -1);
        assert_eq!(floor_div(-4, 2), -2);
        assert_eq!(floor_div(3, 2), 1);
        let graph = network(&[("s", 1), ("t", -1)], &[("st", "s", "t", 0, 1, -3)]);
        let result = solve_prediction_assisted_epsilon_relaxation(&graph, &[1, -1], &[0, 0], 3)
            .expect("negative cost result");
        assert_eq!(result.certificate.total_cost, -3);
        assert_eq!(result.flows, vec![1]);
    }

    #[test]
    fn bounded_prediction_grid_and_all_source_scalings_remain_exact() {
        let graph = network(
            &[("s", 2), ("a", 0), ("t", -2)],
            &[
                ("aa", "a", "a", 0, 2, -3),
                ("at", "a", "t", 0, 3, 1),
                ("direct-cheap", "s", "t", 0, 1, 4),
                ("direct-expensive", "s", "t", 0, 2, 5),
                ("sa", "s", "a", 1, 3, 2),
                ("ts", "t", "s", 0, 1, 10),
            ],
        );
        let required = vec![0, 2, -2];
        let mut reference = None;
        for scaling in 2..=4 {
            for a in -2..=2 {
                for s in -2..=2 {
                    for t in -2..=2 {
                        let result = solve_prediction_assisted_epsilon_relaxation(
                            &graph,
                            &required,
                            &[a, s, t],
                            scaling,
                        )
                        .expect("bounded prediction");
                        assert_eq!(
                            check_min_cost_flow(&graph, &required, &result.flows)
                                .expect("independent certificate"),
                            result.certificate,
                        );
                        if let Some(objective) = reference {
                            assert_eq!(result.certificate.total_cost, objective);
                        } else {
                            reference = Some(result.certificate.total_cost);
                        }
                    }
                }
            }
        }
        let mut fast = solve_prediction_assisted_epsilon_relaxation(
            &graph,
            &required,
            &[1_001, 1_002, 1_000],
            2,
        )
        .expect("shifted fast prediction");
        let traced = trace_prediction_assisted_epsilon_relaxation(&graph, &required, &[1, 2, 0], 2)
            .expect("unshifted trace prediction");
        assert_eq!(fast.predicted_prices, traced.result.predicted_prices);
        fast.raw_predicted_prices = traced.result.raw_predicted_prices.clone();
        fast.final_snapshot.raw_predicted_prices =
            traced.result.final_snapshot.raw_predicted_prices.clone();
        assert_eq!(fast, traced.result);
    }

    #[test]
    fn trace_corruption_is_rejected() {
        let graph = network(&[("s", 1), ("t", -1)], &[("st", "s", "t", 0, 1, 2)]);
        let mut trace = trace_prediction_assisted_epsilon_relaxation(&graph, &[1, -1], &[0, 0], 2)
            .expect("trace");
        trace.events[0].after.attempt = 99;
        assert!(matches!(
            check_prediction_assisted_epsilon_trace(&graph, &[1, -1], &[0, 0], 2, &trace,),
            Err(PredictionAssistedEpsilonError::TraceInvariant)
        ));
    }

    #[test]
    fn raw_trace_boundaries_pass_local_checks() {
        let graph = network(&[("s", 1), ("t", -1)], &[("st", "s", "t", 0, 1, 2)]);
        let run = run_internal(&graph, &[1, -1], &[0, 0], 2, true).expect("raw trace");
        validate_snapshot(&graph, &[1, -1], &run.base_snapshot).expect("base snapshot");
        let mut previous = &run.base_snapshot;
        for (index, event) in run.events.iter().enumerate() {
            assert_eq!(
                &event.before, previous,
                "chain at {index} {:?}",
                event.stage
            );
            validate_snapshot(&graph, &[1, -1], &event.after)
                .unwrap_or_else(|error| panic!("snapshot at {index} {:?}: {error}", event.stage));
            validate_local_transition(&graph, event)
                .unwrap_or_else(|error| panic!("local at {index} {:?}: {error}", event.stage));
            previous = &event.after;
        }
        assert_eq!(previous, &run.result.final_snapshot);
    }

    #[test]
    fn rejects_bad_shape_scaling_and_infeasibility() {
        let graph = network(&[("s", 1), ("t", -1)], &[("st", "s", "t", 0, 0, 1)]);
        assert!(matches!(
            solve_prediction_assisted_epsilon_relaxation(&graph, &[1, -1], &[0], 2,),
            Err(PredictionAssistedEpsilonError::AdmissionLimit)
        ));
        assert!(matches!(
            solve_prediction_assisted_epsilon_relaxation(&graph, &[1, -1], &[0, 0], 5,),
            Err(PredictionAssistedEpsilonError::ScalingParameter)
        ));
        assert!(matches!(
            solve_prediction_assisted_epsilon_relaxation(&graph, &[1, -1], &[0, 0], 2,),
            Err(PredictionAssistedEpsilonError::Feasibility(_))
        ));
    }
}
