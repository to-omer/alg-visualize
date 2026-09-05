//! Bertsekas--Eckstein pure serial epsilon-relaxation for exact min-cost flow.

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArc, ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot,
};

/// Integer epsilon after multiplying every original cost by `n + 1`.
pub const EPSILON_RELAXATION_EPSILON: i128 = 1;
/// Conservative interactive node limit for pure epsilon-relaxation.
pub const EPSILON_RELAXATION_MAX_NODES: usize = 256;
/// Conservative interactive edge limit for pure epsilon-relaxation.
pub const EPSILON_RELAXATION_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling on complete up iterations.
pub const EPSILON_RELAXATION_MAX_UP_ITERATIONS: u64 = 100_000;
/// Deterministic ceiling on pushes and price rises.
pub const EPSILON_RELAXATION_MAX_STATE_TRANSITIONS: u64 = 500_000;
/// Deterministic ceiling on incident residual-arc scans.
pub const EPSILON_RELAXATION_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Deterministic ceiling on conservative kernel and invariant-check work units.
pub const EPSILON_RELAXATION_MAX_WORK_UNITS: u128 = 10_000_000;
/// Maximum recorded events for one eager interactive trace.
pub const EPSILON_RELAXATION_MAX_TRACE_EVENTS: usize = 10_000;
/// Maximum aggregate scene-entity units admitted by the eager WASM projection.
pub const EPSILON_RELAXATION_MAX_TRACE_PROJECTION_UNITS: usize = 250_000;

/// Exact deterministic counters from pure serial epsilon-relaxation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EpsilonRelaxationMetrics {
    /// Complete up iterations begun at positive-surplus nodes.
    pub up_iterations: u64,
    /// Single-node price rises from Step 4.
    pub price_rises: u64,
    /// Positive residual arcs inspected while finding pushes or breakpoints.
    pub residual_arc_scans: u128,
    /// Admissible-arc pushes.
    pub pushes: u64,
    /// Pushes that exhaust their residual arc.
    pub saturating_pushes: u64,
    /// Pushes that drive the selected node surplus to zero first.
    pub nonsaturating_pushes: u64,
    /// Total integral flow units moved by pushes.
    pub pushed_flow_units: u128,
}

impl EpsilonRelaxationMetrics {
    /// Projects source-specific counters into the stable scene metric catalog.
    #[must_use]
    pub const fn projected_trace_metrics(self) -> FlowTraceMetrics {
        FlowTraceMetrics {
            bfs_runs: 0,
            relaxation_passes: self.price_rises as u128,
            residual_arc_scans: self.residual_arc_scans,
            augmentations: self.pushes as u128,
            path_searches: self.up_iterations as u128,
            scaling_phases: 0,
            blocking_flow_phases: 0,
            relabels: self.price_rises as u128,
            retreats: 0,
            reverse_bfs_runs: self.pushed_flow_units,
            gap_terminations: 0,
            pushes: self.pushes as u128,
            saturating_pushes: self.saturating_pushes as u128,
            nonsaturating_pushes: self.nonsaturating_pushes as u128,
            discharges: self.up_iterations as u128,
            active_vertex_selections: self.up_iterations as u128,
        }
    }

    fn state_transitions(self) -> Result<u64, EpsilonRelaxationError> {
        self.price_rises
            .checked_add(self.pushes)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)
    }
}

/// Certified result of Bertsekas--Eckstein pure epsilon-relaxation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpsilonRelaxationResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Final source-convention prices in the `(n + 1)`-scaled cost domain.
    pub prices: Vec<i128>,
    /// Cost multiplier used to make epsilon one strictly smaller than `d / n`.
    pub cost_scale: i128,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: EpsilonRelaxationMetrics,
}

/// Certified epsilon-relaxation result with reversible pedagogical events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpsilonRelaxationTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: EpsilonRelaxationResult,
    /// Replay boundary at the source-defined epsilon-CS state.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent minimum-cost certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Epsilon-relaxation construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EpsilonRelaxationError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds epsilon-relaxation admission limits")]
    AdmissionLimit,
    /// A deterministic transition or scan ceiling was reached.
    #[error("epsilon-relaxation work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and original bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Divergence reconstruction or the final independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("epsilon-relaxation arithmetic overflow")]
    ArithmeticOverflow,
    /// A state failed epsilon-complementary slackness.
    #[error("epsilon-relaxation complementary-slackness invariant failed")]
    EpsilonComplementarySlackness,
    /// Surplus accounting or a complete up iteration was inconsistent.
    #[error("epsilon-relaxation up-iteration invariant failed")]
    UpIterationInvariant,
    /// The source-required admissible graph became cyclic.
    #[error("epsilon-relaxation admissible graph became cyclic")]
    AdmissibleCycle,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced min-cost flow by pure serial epsilon-relaxation.
///
/// Costs are multiplied by `n + 1`, epsilon is fixed to one, and prices start
/// at zero. Negative-cost arcs start at their upper bounds and all other arcs
/// at their lower bounds. This makes the initial admissible graph empty. A
/// feasibility construction is used only as a precheck and is discarded.
///
/// # Errors
///
/// Rejects admission, infeasibility, checked arithmetic, residual mutation,
/// deterministic work limits, invariant, or final certificate failures.
pub fn solve_epsilon_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<EpsilonRelaxationResult, EpsilonRelaxationError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_epsilon_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<EpsilonRelaxationResult, EpsilonRelaxationError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every selection, Step 4 price rise, admissible push, and completion.
///
/// # Errors
///
/// Returns the same failures as [`solve_epsilon_relaxation`] plus trace
/// projection failures.
pub fn trace_epsilon_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<EpsilonRelaxationTraceResult, EpsilonRelaxationError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(EpsilonRelaxationError::UpIterationInvariant)?;
    Ok(EpsilonRelaxationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces epsilon-relaxation while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_epsilon_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<EpsilonRelaxationTraceResult, EpsilonRelaxationError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(EpsilonRelaxationError::UpIterationInvariant)?;
    Ok(EpsilonRelaxationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: EpsilonRelaxationResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Default)]
struct WorkBudget {
    units: u128,
}

impl WorkBudget {
    fn charge(&mut self, units: u128) -> Result<(), EpsilonRelaxationError> {
        self.units = self
            .units
            .checked_add(units)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        if self.units > EPSILON_RELAXATION_MAX_WORK_UNITS {
            return Err(EpsilonRelaxationError::WorkLimit);
        }
        Ok(())
    }
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
) -> Result<InternalRun, EpsilonRelaxationError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_events, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, EpsilonRelaxationError> {
    validate_admission(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let cost_scale = i128::try_from(graph.nodes().len())
        .map_err(|_| EpsilonRelaxationError::ArithmeticOverflow)?
        .checked_add(1)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    let initial_flows = graph
        .edges()
        .iter()
        .map(|edge| {
            if edge.cost() < 0 {
                edge.capacity()
            } else {
                edge.lower()
            }
        })
        .collect::<Vec<_>>();
    let mut state = ResidualState::from_flows(graph, &initial_flows)?;
    let mut prices = vec![0_i128; graph.nodes().len()];
    let mut work = WorkBudget::default();
    charge_surplus_reconstruction_work(graph, &mut work)?;
    let mut surpluses = surpluses(graph, required_divergence, state.flows())?;
    let mut metrics = EpsilonRelaxationMetrics::default();
    validate_state_with_budget(
        graph,
        required_divergence,
        &state,
        &prices,
        &surpluses,
        cost_scale,
        &mut work,
    )?;
    let mut recorder = start_trace_recorder(graph, &state, &surpluses, record_events)?;

    record_trace(
        recorder.as_mut(),
        graph,
        &state,
        &prices,
        &surpluses,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "epsilon-relaxation.initialize-epsilon-cs-state",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "epsilon-relaxation:scale-costs-and-initialize-bound-flows",
        },
        TraceView::empty(),
        Some(("epsilon", EPSILON_RELAXATION_EPSILON)),
    )?;

    run_up_iterations(
        graph,
        required_divergence,
        &mut state,
        &mut prices,
        &mut surpluses,
        cost_scale,
        &mut metrics,
        &mut work,
        recorder.as_mut(),
    )?;

    validate_state_with_budget(
        graph,
        required_divergence,
        &state,
        &prices,
        &surpluses,
        cost_scale,
        &mut work,
    )?;
    record_trace(
        recorder.as_mut(),
        graph,
        &state,
        &prices,
        &surpluses,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "epsilon-relaxation.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "epsilon-relaxation:return-feasible-epsilon-cs-flow",
        },
        TraceView::empty(),
        Some(("positive-surplus", 0)),
    )?;

    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    let result = EpsilonRelaxationResult {
        flows,
        prices,
        cost_scale,
        certificate,
        metrics,
    };
    Ok(InternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_up_iterations(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &mut [i128],
    surpluses: &mut Vec<i128>,
    cost_scale: i128,
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), EpsilonRelaxationError> {
    while let Some(root) = first_positive_surplus(graph, surpluses) {
        metrics.up_iterations = metrics
            .up_iterations
            .checked_add(1)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        if metrics.up_iterations > EPSILON_RELAXATION_MAX_UP_ITERATIONS {
            return Err(EpsilonRelaxationError::WorkLimit);
        }
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            surpluses,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "epsilon-relaxation.select-positive-surplus",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "epsilon-relaxation:select-positive-surplus-node",
            },
            TraceView::selected(root),
            Some(("surplus", surpluses[root.as_usize()])),
        )?;
        complete_up_iteration(
            graph,
            required_divergence,
            state,
            prices,
            surpluses,
            root,
            cost_scale,
            metrics,
            work,
            recorder.as_deref_mut(),
        )?;
        if surpluses[root.as_usize()] != 0 {
            return Err(EpsilonRelaxationError::UpIterationInvariant);
        }
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            surpluses,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "epsilon-relaxation.complete-up-iteration",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "epsilon-relaxation:finish-complete-up-iteration",
            },
            TraceView::selected(root),
            Some(("surplus", 0)),
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn complete_up_iteration(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &mut [i128],
    surpluses: &mut Vec<i128>,
    root: NodeIndex,
    cost_scale: i128,
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), EpsilonRelaxationError> {
    while surpluses[root.as_usize()] > 0 {
        if let Some(arc) = first_admissible_arc(state, prices, root, cost_scale, metrics, work)? {
            push_admissible_arc(
                graph,
                required_divergence,
                state,
                prices,
                surpluses,
                root,
                &arc,
                cost_scale,
                metrics,
                work,
                recorder.as_deref_mut(),
            )?;
            continue;
        }
        raise_price(
            graph,
            required_divergence,
            state,
            prices,
            surpluses,
            root,
            cost_scale,
            metrics,
            work,
            recorder.as_deref_mut(),
        )?;
    }
    Ok(())
}

fn first_admissible_arc(
    state: &ResidualState<'_>,
    prices: &[i128],
    root: NodeIndex,
    cost_scale: i128,
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
) -> Result<Option<ResidualArc>, EpsilonRelaxationError> {
    for arc in state.outgoing_arcs(root) {
        record_arc_scan(metrics, work)?;
        if arc.from != arc.to && is_admissible(prices, &arc, cost_scale)? {
            return Ok(Some(arc));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn push_admissible_arc(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &[i128],
    surpluses: &mut Vec<i128>,
    root: NodeIndex,
    arc: &ResidualArc,
    cost_scale: i128,
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), EpsilonRelaxationError> {
    if arc.from != root || !is_admissible(prices, arc, cost_scale)? {
        return Err(EpsilonRelaxationError::UpIterationInvariant);
    }
    let delta = surpluses[root.as_usize()].min(i128::from(arc.capacity));
    if delta <= 0 {
        return Err(EpsilonRelaxationError::UpIterationInvariant);
    }
    let delta = u64::try_from(delta).map_err(|_| EpsilonRelaxationError::UpIterationInvariant)?;
    state.augment(std::slice::from_ref(&arc.id), delta)?;
    charge_surplus_reconstruction_work(graph, work)?;
    *surpluses = surpluses_from_state(graph, required_divergence, state)?;
    metrics.pushes = metrics
        .pushes
        .checked_add(1)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    if delta == arc.capacity {
        metrics.saturating_pushes = metrics
            .saturating_pushes
            .checked_add(1)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    } else {
        metrics.nonsaturating_pushes = metrics
            .nonsaturating_pushes
            .checked_add(1)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    }
    metrics.pushed_flow_units = metrics
        .pushed_flow_units
        .checked_add(u128::from(delta))
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    validate_transition_limit(*metrics)?;
    charge_epsilon_cs_work(graph, work)?;
    validate_epsilon_cs(graph, state, prices, cost_scale)?;
    charge_surplus_validation_work(graph, work)?;
    validate_surpluses(graph, required_divergence, state, surpluses)?;
    record_trace(
        recorder,
        graph,
        state,
        prices,
        surpluses,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "epsilon-relaxation.push-admissible-arc",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "epsilon-relaxation:push-on-epsilon-balanced-residual",
        },
        TraceView {
            search_order: vec![root, arc.to],
            active_path: vec![arc.id.clone()],
        },
        Some(("delta", i128::from(delta))),
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn raise_price(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
    prices: &mut [i128],
    surpluses: &[i128],
    root: NodeIndex,
    cost_scale: i128,
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), EpsilonRelaxationError> {
    let old_price = prices[root.as_usize()];
    let mut next_price = None;
    for arc in state.outgoing_arcs(root) {
        record_arc_scan(metrics, work)?;
        if arc.from == arc.to {
            continue;
        }
        let candidate = prices[arc.to.as_usize()]
            .checked_add(scaled_residual_cost(&arc, cost_scale)?)
            .and_then(|value| value.checked_add(EPSILON_RELAXATION_EPSILON))
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        next_price = Some(next_price.map_or(candidate, |current: i128| current.min(candidate)));
    }
    let next_price = next_price
        .filter(|candidate| *candidate > old_price)
        .ok_or(EpsilonRelaxationError::UpIterationInvariant)?;
    record_trace(
        recorder.as_deref_mut(),
        graph,
        state,
        prices,
        surpluses,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "epsilon-relaxation.scan-price-breakpoint",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "epsilon-relaxation:scan-step-4-incident-minimum",
        },
        TraceView::selected(root),
        Some(("candidate-price", next_price)),
    )?;
    prices[root.as_usize()] = next_price;
    metrics.price_rises = metrics
        .price_rises
        .checked_add(1)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    validate_transition_limit(*metrics)?;
    charge_epsilon_cs_work(graph, work)?;
    validate_epsilon_cs(graph, state, prices, cost_scale)?;
    charge_surplus_validation_work(graph, work)?;
    validate_surpluses(graph, required_divergence, state, surpluses)?;
    charge_admissible_acyclic_work(graph, work)?;
    validate_admissible_acyclic(graph, state, prices, cost_scale)?;
    work.charge(
        u128::try_from(graph.edges().len())
            .map_err(|_| EpsilonRelaxationError::ArithmeticOverflow)?,
    )?;
    let active_path = state
        .outgoing_arcs(root)
        .into_iter()
        .filter(|arc| arc.from != arc.to)
        .map(|arc| {
            if is_admissible(prices, &arc, cost_scale)? {
                Ok(Some(arc.id))
            } else {
                Ok(None)
            }
        })
        .collect::<Result<Vec<_>, EpsilonRelaxationError>>()?
        .into_iter()
        .flatten()
        .collect();
    record_trace(
        recorder,
        graph,
        state,
        prices,
        surpluses,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "epsilon-relaxation.raise-price",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "epsilon-relaxation:set-price-to-step-4-minimum",
        },
        TraceView {
            search_order: vec![root],
            active_path,
        },
        Some((
            "delta",
            next_price
                .checked_sub(old_price)
                .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?,
        )),
    )?;
    Ok(())
}

fn is_admissible(
    prices: &[i128],
    arc: &ResidualArc,
    cost_scale: i128,
) -> Result<bool, EpsilonRelaxationError> {
    let tension = prices
        .get(arc.from.as_usize())
        .copied()
        .and_then(|from| {
            prices
                .get(arc.to.as_usize())
                .and_then(|to| from.checked_sub(*to))
        })
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    let boundary = scaled_residual_cost(arc, cost_scale)?
        .checked_add(EPSILON_RELAXATION_EPSILON)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    Ok(tension == boundary)
}

fn scaled_residual_cost(
    arc: &ResidualArc,
    cost_scale: i128,
) -> Result<i128, EpsilonRelaxationError> {
    arc.cost
        .checked_mul(cost_scale)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)
}

fn surpluses_from_state(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
) -> Result<Vec<i128>, EpsilonRelaxationError> {
    surpluses(graph, required_divergence, state.flows())
}

fn surpluses(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
) -> Result<Vec<i128>, EpsilonRelaxationError> {
    if required_divergence.len() != graph.nodes().len() {
        return Err(EpsilonRelaxationError::Feasibility(
            FeasibilityError::InvalidDivergence,
        ));
    }
    divergences(graph, flows)?
        .into_iter()
        .zip(required_divergence)
        .map(|(actual, &required)| {
            required
                .checked_sub(actual)
                .ok_or(EpsilonRelaxationError::ArithmeticOverflow)
        })
        .collect()
}

#[cfg(test)]
fn validate_state(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
    prices: &[i128],
    recorded_surpluses: &[i128],
    cost_scale: i128,
) -> Result<(), EpsilonRelaxationError> {
    validate_epsilon_cs(graph, state, prices, cost_scale)?;
    validate_surpluses(graph, required_divergence, state, recorded_surpluses)?;
    validate_admissible_acyclic(graph, state, prices, cost_scale)
}

fn validate_state_with_budget(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
    prices: &[i128],
    recorded_surpluses: &[i128],
    cost_scale: i128,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    charge_epsilon_cs_work(graph, work)?;
    validate_epsilon_cs(graph, state, prices, cost_scale)?;
    charge_surplus_validation_work(graph, work)?;
    validate_surpluses(graph, required_divergence, state, recorded_surpluses)?;
    charge_admissible_acyclic_work(graph, work)?;
    validate_admissible_acyclic(graph, state, prices, cost_scale)
}

fn validate_epsilon_cs(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    cost_scale: i128,
) -> Result<(), EpsilonRelaxationError> {
    if prices.len() != graph.nodes().len() {
        return Err(EpsilonRelaxationError::EpsilonComplementarySlackness);
    }
    for edge_index in graph.edge_indices() {
        let edge = graph
            .edge(edge_index)
            .ok_or(EpsilonRelaxationError::EpsilonComplementarySlackness)?;
        let flow = state.flows()[edge_index.as_usize()];
        let tension = prices[edge.from().as_usize()]
            .checked_sub(prices[edge.to().as_usize()])
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        let scaled_cost = i128::from(edge.cost())
            .checked_mul(cost_scale)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        let upper_boundary = scaled_cost
            .checked_add(EPSILON_RELAXATION_EPSILON)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        let lower_boundary = scaled_cost
            .checked_sub(EPSILON_RELAXATION_EPSILON)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        if (flow < edge.capacity() && tension > upper_boundary)
            || (edge.lower() < flow && tension < lower_boundary)
        {
            return Err(EpsilonRelaxationError::EpsilonComplementarySlackness);
        }
    }
    Ok(())
}

fn validate_surpluses(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
    recorded_surpluses: &[i128],
) -> Result<(), EpsilonRelaxationError> {
    if recorded_surpluses != surpluses_from_state(graph, required_divergence, state)? {
        return Err(EpsilonRelaxationError::UpIterationInvariant);
    }
    let sum = recorded_surpluses.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)
    })?;
    if sum != 0 {
        return Err(EpsilonRelaxationError::UpIterationInvariant);
    }
    Ok(())
}

fn validate_admissible_acyclic(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    cost_scale: i128,
) -> Result<(), EpsilonRelaxationError> {
    let mut indegree = vec![0_usize; graph.nodes().len()];
    let mut outgoing = vec![Vec::new(); graph.nodes().len()];
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            if !is_admissible(prices, &arc, cost_scale)? {
                continue;
            }
            if arc.from == arc.to {
                return Err(EpsilonRelaxationError::AdmissibleCycle);
            }
            indegree[arc.to.as_usize()] = indegree[arc.to.as_usize()]
                .checked_add(1)
                .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
            outgoing[arc.from.as_usize()].push(arc.to);
        }
    }
    let mut queue = graph
        .node_indices()
        .filter(|node| indegree[node.as_usize()] == 0)
        .collect::<std::collections::VecDeque<_>>();
    let mut visited = 0_usize;
    while let Some(node) = queue.pop_front() {
        visited = visited
            .checked_add(1)
            .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
        for &head in &outgoing[node.as_usize()] {
            indegree[head.as_usize()] = indegree[head.as_usize()]
                .checked_sub(1)
                .ok_or(EpsilonRelaxationError::AdmissibleCycle)?;
            if indegree[head.as_usize()] == 0 {
                queue.push_back(head);
            }
        }
    }
    if visited != graph.nodes().len() {
        return Err(EpsilonRelaxationError::AdmissibleCycle);
    }
    Ok(())
}

fn first_positive_surplus(graph: &FlowNetwork, surpluses: &[i128]) -> Option<NodeIndex> {
    graph
        .node_indices()
        .find(|node| surpluses[node.as_usize()] > 0)
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), EpsilonRelaxationError> {
    if graph.nodes().len() > EPSILON_RELAXATION_MAX_NODES
        || graph.edges().len() > EPSILON_RELAXATION_MAX_EDGES
    {
        return Err(EpsilonRelaxationError::AdmissionLimit);
    }
    Ok(())
}

fn record_arc_scan(
    metrics: &mut EpsilonRelaxationMetrics,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    work.charge(1)?;
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > EPSILON_RELAXATION_MAX_RESIDUAL_ARC_SCANS {
        return Err(EpsilonRelaxationError::WorkLimit);
    }
    Ok(())
}

fn charge_epsilon_cs_work(
    graph: &FlowNetwork,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    work.charge(graph_work_units(graph, 1, 0)?)
}

fn charge_surplus_reconstruction_work(
    graph: &FlowNetwork,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    work.charge(graph_work_units(graph, 2, 1)?)
}

fn charge_surplus_validation_work(
    graph: &FlowNetwork,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    work.charge(graph_work_units(graph, 2, 2)?)
}

fn charge_admissible_acyclic_work(
    graph: &FlowNetwork,
    work: &mut WorkBudget,
) -> Result<(), EpsilonRelaxationError> {
    // At most two residual arcs per original edge are inspected, then every
    // admissible arc is visited once more by Kahn's topological check.
    work.charge(graph_work_units(graph, 4, 2)?)
}

fn graph_work_units(
    graph: &FlowNetwork,
    edge_multiplier: u128,
    node_multiplier: u128,
) -> Result<u128, EpsilonRelaxationError> {
    let edges = u128::try_from(graph.edges().len())
        .map_err(|_| EpsilonRelaxationError::ArithmeticOverflow)?;
    let nodes = u128::try_from(graph.nodes().len())
        .map_err(|_| EpsilonRelaxationError::ArithmeticOverflow)?;
    edges
        .checked_mul(edge_multiplier)
        .and_then(|edge_units| {
            nodes
                .checked_mul(node_multiplier)
                .and_then(|node_units| edge_units.checked_add(node_units))
        })
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)
}

fn validate_transition_limit(
    metrics: EpsilonRelaxationMetrics,
) -> Result<(), EpsilonRelaxationError> {
    if metrics.state_transitions()? > EPSILON_RELAXATION_MAX_STATE_TRANSITIONS {
        return Err(EpsilonRelaxationError::WorkLimit);
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    surpluses: &[i128],
    record_events: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_events {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        surpluses.to_vec(),
        FlowTraceMetrics::default(),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
}

impl TraceView {
    fn empty() -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
        }
    }

    fn selected(node: NodeIndex) -> Self {
        Self {
            search_order: vec![node],
            active_path: Vec::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    surpluses: &[i128],
    metrics: EpsilonRelaxationMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), EpsilonRelaxationError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let next_event_count = recorder
        .event_count()
        .checked_add(1)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    let entities_per_scene = graph
        .nodes()
        .len()
        .checked_mul(2)
        .and_then(|nodes| {
            graph
                .edges()
                .len()
                .checked_mul(4)
                .and_then(|edges| nodes.checked_add(edges))
        })
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    let projection_units = next_event_count
        .checked_mul(entities_per_scene)
        .ok_or(EpsilonRelaxationError::ArithmeticOverflow)?;
    if next_event_count > EPSILON_RELAXATION_MAX_TRACE_EVENTS
        || projection_units > EPSILON_RELAXATION_MAX_TRACE_PROJECTION_UNITS
    {
        return Err(EpsilonRelaxationError::Trace(FlowTraceError::EventLimit));
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        prices.iter().copied().map(Some).collect(),
        view.search_order,
        view.active_path,
        surpluses.to_vec(),
        metrics.projected_trace_metrics(),
    );
    recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::certificate::supply_divergences;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

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
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("valid graph")
    }

    #[test]
    fn raises_one_scaled_price_then_pushes_a_positive_cost_arc() {
        let graph = network(&[("s", 2), ("t", -2)], &[("st", "s", "t", 0, 3, 5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_epsilon_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [2]);
        assert_eq!(result.certificate.total_cost, 10);
        assert_eq!(result.cost_scale, 3);
        assert_eq!(result.prices, [16, 0]);
        assert_eq!(result.metrics.up_iterations, 1);
        assert_eq!(result.metrics.price_rises, 1);
        assert_eq!(result.metrics.pushes, 1);
        assert_eq!(result.metrics.nonsaturating_pushes, 1);
    }

    #[test]
    fn uses_an_epsilon_minus_balanced_reverse_residual() {
        let graph = network(&[("a", 0), ("b", 0)], &[("ab", "a", "b", 0, 2, -5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_epsilon_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [0]);
        assert_eq!(result.prices, [0, 16]);
        assert_eq!(result.metrics.pushes, 1);
        assert_eq!(result.metrics.saturating_pushes, 1);
    }

    #[test]
    fn compares_full_width_surplus_before_each_parallel_push() {
        let graph = network(
            &[("a", 0), ("b", 0)],
            &[
                ("ab-0", "a", "b", 0, u64::MAX, -1),
                ("ab-1", "a", "b", 0, u64::MAX, -1),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_epsilon_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [0, 0]);
        assert_eq!(result.metrics.pushes, 2);
        assert_eq!(result.metrics.pushed_flow_units, u128::from(u64::MAX) * 2);
    }

    #[test]
    fn creates_a_negative_cost_cycle_through_a_forward_push() {
        let graph = network(
            &[("a", 0), ("b", 0)],
            &[("ab", "a", "b", 0, 1, -3), ("ba", "b", "a", 0, 1, 1)],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_epsilon_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [1, 1]);
        assert_eq!(result.certificate.total_cost, -2);
        assert_eq!(result.prices, [0, 4]);
    }

    #[test]
    fn keeps_self_loops_at_the_cost_sign_bound() {
        let graph = network(
            &[("v", 0)],
            &[
                ("negative", "v", "v", 1, 4, -3),
                ("positive", "v", "v", 2, 5, 7),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_epsilon_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [4, 2]);
        assert_eq!(result.certificate.total_cost, 2);
        assert_eq!(result.metrics.up_iterations, 0);
    }

    #[test]
    fn trace_replays_price_and_push_events_in_both_directions() {
        let graph = network(&[("s", 2), ("t", -2)], &[("st", "s", "t", 0, 3, 5)]);
        let target = supply_divergences(&graph).expect("target");
        let fast = solve_epsilon_relaxation(&graph, &target).expect("fast result");
        let traced = trace_epsilon_relaxation(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        assert_eq!(traced.events.len(), 7);
        assert_eq!(
            traced.events[3].catalog_id,
            "epsilon-relaxation.raise-price"
        );
        assert_eq!(
            traced.events[4].catalog_id,
            "epsilon-relaxation.push-admissible-arc"
        );
        let mut replay = traced.base_snapshot.clone();
        let mut previous_prices = replay.node_labels.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            for (before, after) in previous_prices.iter().zip(&replay.node_labels) {
                if let (Some(before), Some(after)) = (before, after) {
                    assert!(after >= before, "source prices must never decrease");
                }
            }
            let replay_state = ResidualState::from_flows(&graph, &replay.flows)
                .expect("replay flow remains bounded");
            if replay.node_labels.iter().all(Option::is_some) {
                validate_state(
                    &graph,
                    &target,
                    &replay_state,
                    &replay
                        .node_labels
                        .iter()
                        .map(|price| price.expect("all prices present"))
                        .collect::<Vec<_>>(),
                    &replay.remaining_divergence,
                    traced.result.cost_scale,
                )
                .expect("every visible boundary preserves epsilon-CS and acyclicity");
            }
            previous_prices = replay.node_labels.clone();
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn agrees_with_cycle_canceling_on_planted_lower_bound_transshipments() {
        for case in 0_u64..32 {
            let lower_a = case % 3;
            let lower_b = (case / 3) % 2;
            let planted_a = lower_a + 1 + (case % 2);
            let planted_b = lower_b + 1;
            let planted_c = 1 + ((case / 2) % 3);
            let graph = network(
                &[("a", 0), ("b", 0), ("c", 0)],
                &[
                    (
                        "ab-0",
                        "a",
                        "b",
                        lower_a,
                        lower_a + 4,
                        i64::try_from(case % 7).expect("bounded cost") - 3,
                    ),
                    (
                        "ab-1",
                        "a",
                        "b",
                        lower_b,
                        lower_b + 3,
                        i64::try_from(case % 5).expect("bounded cost") - 2,
                    ),
                    (
                        "bc",
                        "b",
                        "c",
                        0,
                        5,
                        i64::try_from((case / 3) % 7).expect("bounded cost") - 3,
                    ),
                    (
                        "ca",
                        "c",
                        "a",
                        0,
                        5,
                        i64::try_from((case / 5) % 7).expect("bounded cost") - 3,
                    ),
                ],
            );
            let planted = vec![planted_a, planted_b, planted_c, planted_c];
            let target = divergences(&graph, &planted).expect("planted divergences");
            let actual = solve_epsilon_relaxation(&graph, &target)
                .unwrap_or_else(|error| panic!("epsilon-relaxation case {case}: {error}"));
            let oracle = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("cycle-canceling case {case}: {error}"));
            assert_eq!(actual.certificate.total_cost, oracle.certificate.total_cost);
        }
    }

    #[test]
    fn eager_trace_budget_does_not_limit_the_fast_price_ladder_solve() {
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("node id"), 160),
                FlowNode::new(NodeId::parse("t").expect("node id"), -160),
            ],
            (1_i64..=160)
                .map(|cost| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("edge-{cost:03}")).expect("edge id"),
                    from: NodeId::parse("s").expect("tail"),
                    to: NodeId::parse("t").expect("head"),
                    lower: 0,
                    capacity: 1,
                    cost,
                })
                .collect(),
        )
        .expect("valid graph");
        let target = supply_divergences(&graph).expect("target");

        let fast = solve_epsilon_relaxation(&graph, &target).expect("fast minimum cost");
        assert_eq!(fast.flows, vec![1; 160]);
        assert_eq!(fast.certificate.total_cost, 12_880);
        assert!(matches!(
            trace_epsilon_relaxation(&graph, &target),
            Err(EpsilonRelaxationError::Trace(FlowTraceError::EventLimit))
        ));
    }

    #[test]
    fn invariant_checks_are_bounded_by_the_fast_work_budget() {
        let edge_count = 1_200_i64;
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("node id"), edge_count),
                FlowNode::new(NodeId::parse("t").expect("node id"), -edge_count),
            ],
            (1_i64..=edge_count)
                .map(|cost| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("edge-{cost:04}")).expect("edge id"),
                    from: NodeId::parse("s").expect("tail"),
                    to: NodeId::parse("t").expect("head"),
                    lower: 0,
                    capacity: 1,
                    cost,
                })
                .collect(),
        )
        .expect("valid graph");
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_epsilon_relaxation(&graph, &target),
            Err(EpsilonRelaxationError::WorkLimit)
        ));
    }

    #[test]
    fn rejects_infeasible_balances_before_price_work() {
        let graph = network(&[("s", 2), ("t", -2)], &[]);
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_epsilon_relaxation(&graph, &target),
            Err(EpsilonRelaxationError::Feasibility(
                FeasibilityError::Infeasible(_)
            ))
        ));
    }
}
