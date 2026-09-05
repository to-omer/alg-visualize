//! Bounded electrical-flow interior-point minimum-cost flow.
//!
//! Daitch and Spielman's standard-MCF recovery isolates an integral optimum,
//! follows a dual logarithmic central path with approximate Newton solves, and
//! rounds a sufficiently accurate flow.  This small-input realization keeps
//! those source-level operations.  It uses deterministic dense elimination for
//! the electrical Schur-complement Laplacian and bounded enumeration only to
//! certify isolation, the perturbed optimum objective, and fixed coordinates of
//! the feasible polytope.  The optimum vector is never injected into recovery:
//! every final coordinate is obtained by rounding the central estimate `mu/s`.
//! Consequently it does not claim the source's nearly-linear solver runtime.

#![allow(clippy::cast_precision_loss)]

use num_traits::ToPrimitive;
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Maximum original nodes admitted by the bounded realization.
pub const ELECTRICAL_IPM_MCF_MAX_NODES: usize = 6;
/// Maximum original edges admitted by the bounded realization.
pub const ELECTRICAL_IPM_MCF_MAX_EDGES: usize = 5;
/// Maximum capacity admitted on an original edge.
pub const ELECTRICAL_IPM_MCF_MAX_CAPACITY: u64 = 8;
/// Maximum absolute original unit cost.
pub const ELECTRICAL_IPM_MCF_MAX_COST: u64 = 32;
/// Maximum integer assignments inspected by the exact isolation scaffold.
pub const ELECTRICAL_IPM_MCF_MAX_ENUMERATED_ASSIGNMENTS: u64 = 100_000;
/// Maximum independent isolation draws.
pub const ELECTRICAL_IPM_MCF_MAX_ISOLATION_ATTEMPTS: u64 = 64;
/// Maximum short-step barrier reductions.
pub const ELECTRICAL_IPM_MCF_MAX_BARRIER_REDUCTIONS: u64 = 4_096;
/// Maximum Newton centering steps at one barrier value.
pub const ELECTRICAL_IPM_MCF_MAX_CENTERING_STEPS: u64 = 96;
/// Maximum public trace events.
pub const ELECTRICAL_IPM_MCF_MAX_TRACE_EVENTS: usize = 16_384;
/// Reproducible isolation seed.
pub const ELECTRICAL_IPM_MCF_DEFAULT_SEED: u64 = 0xbb67_ae85_84ca_a73b;

const POSITIVE_FLOOR: f64 = 1.0e-14;
const CENTER_TOLERANCE: f64 = 2.0e-7;
const ARMIJO: f64 = 1.0e-4;

/// Replay-safe finite IEEE-754 value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ElectricalIpmMcfScalar(u64);

impl ElectricalIpmMcfScalar {
    fn try_new(value: f64) -> Result<Self, ElectricalIpmMcfError> {
        if !value.is_finite() {
            return Err(ElectricalIpmMcfError::NumericalFailure);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    /// Recovers the finite value.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Stable decimal projection for scene serialization.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Public source or recovery boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElectricalIpmMcfStage {
    /// Validated input before transformations.
    Ready,
    /// Lower bounds and the affine integer feasible set were enumerated.
    NormalizeLowerBounds,
    /// One source-prescribed isolation perturbation was drawn and inspected.
    IsolationAttempt,
    /// A unique perturbed optimum objective was certified.
    SelectIsolatedCosts,
    /// Coordinates fixed on the feasible face were contracted.
    ContractFixedFace,
    /// A strictly dual-interior starting point was installed.
    InitializeDualInterior,
    /// The Newton Schur-complement electrical Laplacian was assembled.
    AssembleElectricalLaplacian,
    /// The anchored electrical Newton system was solved.
    SolveNewtonDirection,
    /// A positivity-preserving damped Newton update was accepted.
    DampedCenteringStep,
    /// The iterate met the central-neighborhood residual bound.
    Centered,
    /// The short-step barrier parameter was decreased.
    DecreaseBarrier,
    /// The central primal estimate met the source recovery accuracy.
    ApproximateFlow,
    /// Every coordinate was rounded to its nearest integer.
    RoundNearestInteger,
    /// Isolation, rounding, feasibility, and optimality were checked.
    CheckCertificate,
    /// Certified original MCF optimum.
    Optimal,
}

/// Original-node projection of the dual electrical system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfNodeState {
    /// Canonical node identity.
    pub node: NodeIndex,
    /// Current dual potential.
    pub potential: ElectricalIpmMcfScalar,
    /// Latest Newton potential direction.
    pub potential_direction: ElectricalIpmMcfScalar,
    /// Current approximate primal balance residual.
    pub balance_residual: ElectricalIpmMcfScalar,
    /// Whether this node is the gauge anchor of its working component.
    pub anchored: bool,
}

/// Original-edge projection of isolation, barrier, and electrical quantities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Isolation perturbation in `[1,2mU]`.
    pub perturbation: u64,
    /// Integer isolated cost `Q*c+r`.
    pub isolated_cost: i128,
    /// Whether every integer feasible flow fixes this coordinate.
    pub fixed_on_face: bool,
    /// Lower coordinate of the contracted feasible face.
    pub face_lower: u64,
    /// Upper coordinate of the contracted feasible face.
    pub face_upper: u64,
    /// Central primal estimate in original coordinates.
    pub fractional_flow: ElectricalIpmMcfScalar,
    /// Upper-complement estimate `mu/w`.
    pub upper_complement: ElectricalIpmMcfScalar,
    /// Lower dual slack `s`.
    pub lower_slack: ElectricalIpmMcfScalar,
    /// Upper-bound dual multiplier `w`.
    pub upper_multiplier: ElectricalIpmMcfScalar,
    /// Electrical Schur-complement resistance.
    pub resistance: ElectricalIpmMcfScalar,
    /// Reciprocal electrical resistance.
    pub conductance: ElectricalIpmMcfScalar,
    /// Latest electrical current induced by the Newton potentials.
    pub electrical_current: ElectricalIpmMcfScalar,
    /// Latest lower-slack direction.
    pub lower_slack_direction: ElectricalIpmMcfScalar,
    /// Latest upper-multiplier direction.
    pub upper_multiplier_direction: ElectricalIpmMcfScalar,
    /// Final rounded flow, once recovery begins.
    pub final_flow: Option<u64>,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElectricalIpmMcfMetrics {
    /// Integer assignments inspected while enumerating the feasible set.
    pub enumerated_assignments: u64,
    /// Feasible integer flows retained by the isolation scaffold.
    pub feasible_flows: u64,
    /// Isolation perturbation vectors drawn.
    pub isolation_attempts: u64,
    /// Random u64 words consumed.
    pub random_draws: u64,
    /// Coordinates contracted because they are fixed on the feasible face.
    pub fixed_coordinates: u64,
    /// Electrical Laplacians assembled.
    pub laplacian_assemblies: u64,
    /// Anchored dense Newton systems solved.
    pub newton_solves: u64,
    /// Gaussian pivots.
    pub elimination_pivots: u64,
    /// Accepted damped centering steps.
    pub centering_steps: u64,
    /// Barrier parameter reductions.
    pub barrier_reductions: u64,
    /// Backtracking reductions.
    pub line_search_reductions: u64,
    /// Nearest-integer recovery operations.
    pub rounding_operations: u64,
    /// Independent terminal certificate checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete public state at one atomic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfSnapshot {
    /// Current source phase.
    pub stage: ElectricalIpmMcfStage,
    /// Original-node projections.
    pub nodes: Vec<ElectricalIpmMcfNodeState>,
    /// Original-edge projections.
    pub edges: Vec<ElectricalIpmMcfEdgeState>,
    /// Current logarithmic-barrier parameter.
    pub mu: ElectricalIpmMcfScalar,
    /// Source short-step decrement `epsilon_3`.
    pub epsilon_3: ElectricalIpmMcfScalar,
    /// Source exact-recovery accuracy target.
    pub recovery_epsilon: ElectricalIpmMcfScalar,
    /// Current `2m*mu` barrier duality-gap bound.
    pub duality_gap_bound: ElectricalIpmMcfScalar,
    /// Maximum normalized primal/bound stationarity residual.
    pub centrality_residual: ElectricalIpmMcfScalar,
    /// Maximum approximate balance residual.
    pub balance_residual: ElectricalIpmMcfScalar,
    /// Latest accepted Newton step length.
    pub step_size: ElectricalIpmMcfScalar,
    /// Latest electrical energy.
    pub electrical_energy: ElectricalIpmMcfScalar,
    /// Latest anchored linear-system residual.
    pub linear_residual: ElectricalIpmMcfScalar,
    /// Current dual barrier objective.
    pub barrier_objective: ElectricalIpmMcfScalar,
    /// Isolation scaling constant `Q=4m^2U^2`.
    pub isolation_scale: i128,
    /// Isolation perturbation upper bound `2mU`.
    pub perturbation_bound: u64,
    /// One-based accepted or current isolation attempt.
    pub isolation_attempt: u64,
    /// Exact perturbed objective of the isolated optimum.
    pub isolated_optimum_cost: i128,
    /// Exact gap from the isolated optimum to the next perturbed flow.
    pub isolated_gap: i128,
    /// Seed used by the reproducible isolation draws.
    pub seed: u64,
    /// Exact work counters.
    pub metrics: ElectricalIpmMcfMetrics,
}

/// One reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfTraceEvent {
    /// Stable event identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: ElectricalIpmMcfSnapshot,
    /// State after the transition.
    pub after: ElectricalIpmMcfSnapshot,
}

/// Certified bounded result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfResult {
    /// Original-edge integral optimum.
    pub flows: Vec<u64>,
    /// Independent minimum-cost certificate for the unperturbed costs.
    pub certificate: MinCostFlowCertificate,
    /// Reproducible isolation seed.
    pub seed: u64,
    /// Terminal public state.
    pub final_snapshot: ElectricalIpmMcfSnapshot,
    /// Exact work counters.
    pub metrics: ElectricalIpmMcfMetrics,
}

/// Result plus the replay-audited published source boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalIpmMcfTraceResult {
    /// Certified result.
    pub result: ElectricalIpmMcfResult,
    /// Ready boundary.
    pub base_snapshot: ElectricalIpmMcfSnapshot,
    /// Ordered reversible transitions. Repetitive short-step iterations are
    /// sampled at declared aggregate boundaries while metrics count every step.
    pub events: Vec<ElectricalIpmMcfTraceEvent>,
    /// Terminal boundary.
    pub final_snapshot: ElectricalIpmMcfSnapshot,
}

/// Admission, source invariant, numerical, recovery, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ElectricalIpmMcfError {
    /// Input exceeds the declared bounded research band.
    #[error("electrical-flow IPM MCF input exceeds admission limits")]
    AdmissionLimit,
    /// The requested balance vector is malformed.
    #[error("electrical-flow IPM MCF requires a balanced divergence vector")]
    InvalidDivergence,
    /// The source method requires a feasible instance.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// The bounded isolation guard did not find a unique optimum in time.
    #[error("electrical-flow IPM MCF isolation guard was exhausted")]
    IsolationGuardExhausted,
    /// Isolation changed the unperturbed optimum or found no feasible optimum.
    #[error("electrical-flow IPM MCF isolation invariant failed")]
    IsolationInvariant,
    /// A finite positive central-path quantity or solve invariant failed.
    #[error("electrical-flow IPM MCF numerical invariant failed")]
    NumericalFailure,
    /// The short-step or centering guard was exhausted.
    #[error("electrical-flow IPM MCF did not converge within its bounded guard")]
    NonConvergence,
    /// Nearest-integer recovery did not reproduce the isolated optimum.
    #[error("electrical-flow IPM MCF exact recovery invariant failed")]
    RecoveryFailure,
    /// Checked integer arithmetic overflowed.
    #[error("electrical-flow IPM MCF arithmetic overflow")]
    ArithmeticOverflow,
    /// Independent minimum-cost certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// The public trace differs from a complete seeded re-execution.
    #[error("electrical-flow IPM MCF trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Debug)]
struct ExactRng {
    state: u64,
}

impl ExactRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    fn next(
        &mut self,
        metrics: &mut ElectricalIpmMcfMetrics,
    ) -> Result<u64, ElectricalIpmMcfError> {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        metrics.random_draws = metrics
            .random_draws
            .checked_add(1)
            .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
        Ok(value)
    }
}

#[derive(Clone, Debug)]
struct WorkingArc {
    original: usize,
    from: usize,
    to: usize,
    base: u64,
    width: u64,
    cost: f64,
    s: f64,
    w: f64,
    ds: f64,
    dw: f64,
    resistance: f64,
    conductance: f64,
    current: f64,
}

#[derive(Clone, Debug)]
struct KernelState {
    node_count: usize,
    lower_divergence: Vec<i128>,
    feasible: Vec<Vec<u64>>,
    isolated_optimum_cost: i128,
    perturbations: Vec<u64>,
    isolated_costs: Vec<i128>,
    face_lower: Vec<u64>,
    face_upper: Vec<u64>,
    fixed: Vec<bool>,
    working: Vec<WorkingArc>,
    original_to_working: Vec<Option<usize>>,
    target: Vec<f64>,
    y: Vec<f64>,
    dy: Vec<f64>,
    node_residual: Vec<f64>,
    anchors: Vec<bool>,
    final_flows: Vec<Option<u64>>,
    mu: f64,
    epsilon_3: f64,
    recovery_epsilon: f64,
    centrality_residual: f64,
    balance_residual: f64,
    step_size: f64,
    electrical_energy: f64,
    linear_residual: f64,
    barrier_objective: f64,
    isolation_scale: i128,
    perturbation_bound: u64,
    isolation_attempt: u64,
    isolated_gap: i128,
    seed: u64,
    metrics: ElectricalIpmMcfMetrics,
    stage: ElectricalIpmMcfStage,
    rng: ExactRng,
}

struct Recorder<'a> {
    graph: &'a FlowNetwork,
    state: KernelState,
    current: ElectricalIpmMcfSnapshot,
    events: Vec<ElectricalIpmMcfTraceEvent>,
    enabled: bool,
}

impl Recorder<'_> {
    fn emit(
        &mut self,
        catalog_id: &'static str,
        stage: ElectricalIpmMcfStage,
    ) -> Result<(), ElectricalIpmMcfError> {
        self.state.stage = stage;
        self.state.metrics.state_transitions = self
            .state
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
        let after = project_snapshot(self.graph, &self.state)?;
        if self.enabled {
            if self.events.len() >= ELECTRICAL_IPM_MCF_MAX_TRACE_EVENTS {
                return Err(ElectricalIpmMcfError::AdmissionLimit);
            }
            self.events.push(ElectricalIpmMcfTraceEvent {
                catalog_id,
                before: self.current.clone(),
                after: after.clone(),
            });
        }
        self.current = after;
        Ok(())
    }
}

struct InternalRun {
    result: ElectricalIpmMcfResult,
    base_snapshot: ElectricalIpmMcfSnapshot,
    events: Vec<ElectricalIpmMcfTraceEvent>,
}

/// Solves with the reproducible default isolation seed.
///
/// # Errors
///
/// Returns an error when the input is outside the bounded domain, infeasible,
/// numerically invalid, or fails exact recovery and certification.
pub fn solve_electrical_flow_interior_point_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<ElectricalIpmMcfResult, ElectricalIpmMcfError> {
    solve_electrical_flow_interior_point_mcf_with_seed(
        graph,
        required_divergence,
        ELECTRICAL_IPM_MCF_DEFAULT_SEED,
    )
}

/// Solves the default-seed execution while reporting its feasibility precheck
/// to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_electrical_flow_interior_point_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<ElectricalIpmMcfResult, ElectricalIpmMcfError> {
    run_internal_with_feasibility(
        graph,
        required_divergence,
        ELECTRICAL_IPM_MCF_DEFAULT_SEED,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves with an explicit reproducible isolation seed.
///
/// # Errors
///
/// Returns an error when the input is outside the bounded domain, infeasible,
/// numerically invalid, or fails seeded isolation, recovery, and certification.
pub fn solve_electrical_flow_interior_point_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<ElectricalIpmMcfResult, ElectricalIpmMcfError> {
    run_internal(graph, required_divergence, seed, false).map(|run| run.result)
}

/// Records the published default-seed source boundaries.
///
/// # Errors
///
/// Returns any solve failure or a failure to verify the published trace against
/// a complete seeded re-execution.
pub fn trace_electrical_flow_interior_point_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<ElectricalIpmMcfTraceResult, ElectricalIpmMcfError> {
    trace_electrical_flow_interior_point_mcf_with_seed(
        graph,
        required_divergence,
        ELECTRICAL_IPM_MCF_DEFAULT_SEED,
    )
}

/// Records the published seeded source boundaries.
///
/// # Errors
///
/// Returns any solve failure or a failure to verify the published trace against
/// a complete re-execution with `seed`.
pub fn trace_electrical_flow_interior_point_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<ElectricalIpmMcfTraceResult, ElectricalIpmMcfError> {
    let run = run_internal(graph, required_divergence, seed, true)?;
    let trace = ElectricalIpmMcfTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_electrical_flow_interior_point_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Records the default-seed execution while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_electrical_flow_interior_point_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<ElectricalIpmMcfTraceResult, ElectricalIpmMcfError> {
    let run = run_internal_with_feasibility(
        graph,
        required_divergence,
        ELECTRICAL_IPM_MCF_DEFAULT_SEED,
        true,
        feasibility,
    )?;
    let trace = ElectricalIpmMcfTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_electrical_flow_interior_point_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Independently checks every published boundary and terminal certificate.
///
/// # Errors
///
/// Returns [`ElectricalIpmMcfError::TraceVerification`] when any boundary,
/// terminal result, metric, seed-dependent value, or transition link differs.
pub fn check_electrical_flow_interior_point_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &ElectricalIpmMcfTraceResult,
) -> Result<(), ElectricalIpmMcfError> {
    validate_admission(graph, required_divergence)?;
    if trace.base_snapshot
        != expected_electrical_ipm_base(graph, required_divergence, trace.result.seed)?
        || trace.final_snapshot.stage != ElectricalIpmMcfStage::Optimal
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.base_snapshot.seed != trace.result.seed
        || trace.final_snapshot.seed != trace.result.seed
        || trace.events.is_empty()
    {
        return Err(ElectricalIpmMcfError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor
            || !valid_electrical_ipm_event(event)
            || event.after.metrics.state_transitions <= event.before.metrics.state_transitions
            || event.after.nodes.len() != graph.nodes().len()
            || event.after.edges.len() != graph.edges().len()
            || event.after.seed != trace.result.seed
            || event.after.nodes.iter().enumerate().any(|(index, state)| {
                state.node.as_usize() != index
                    || !state.potential.get().is_finite()
                    || !state.potential_direction.get().is_finite()
                    || !state.balance_residual.get().is_finite()
            })
            || event
                .after
                .edges
                .iter()
                .zip(graph.edges())
                .any(|(state, edge)| {
                    &state.edge != edge.id()
                        || !state.fractional_flow.get().is_finite()
                        || !state.upper_complement.get().is_finite()
                        || !state.lower_slack.get().is_finite()
                        || !state.upper_multiplier.get().is_finite()
                        || !state.resistance.get().is_finite()
                        || !state.conductance.get().is_finite()
                        || !state.electrical_current.get().is_finite()
                        || state
                            .final_flow
                            .is_some_and(|flow| flow < edge.lower() || flow > edge.capacity())
                })
        {
            return Err(ElectricalIpmMcfError::TraceVerification);
        }
        cursor = &event.after;
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if cursor != &trace.final_snapshot
        || certificate != trace.result.certificate
        || trace
            .final_snapshot
            .edges
            .iter()
            .zip(&trace.result.flows)
            .any(|(state, flow)| state.final_flow != Some(*flow))
    {
        return Err(ElectricalIpmMcfError::TraceVerification);
    }
    Ok(())
}

fn expected_electrical_ipm_base(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
) -> Result<ElectricalIpmMcfSnapshot, ElectricalIpmMcfError> {
    let mut metrics = ElectricalIpmMcfMetrics::default();
    let _feasible = enumerate_feasible(graph, required, &mut metrics)?;
    let zero = ElectricalIpmMcfScalar::try_new(0.0)?;
    Ok(ElectricalIpmMcfSnapshot {
        stage: ElectricalIpmMcfStage::Ready,
        nodes: graph
            .node_indices()
            .map(|node| ElectricalIpmMcfNodeState {
                node,
                potential: zero,
                potential_direction: zero,
                balance_residual: zero,
                anchored: false,
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| {
                Ok(ElectricalIpmMcfEdgeState {
                    edge: edge.id().clone(),
                    perturbation: 0,
                    isolated_cost: i128::from(edge.cost()),
                    fixed_on_face: false,
                    face_lower: edge.lower(),
                    face_upper: edge.capacity(),
                    fractional_flow: ElectricalIpmMcfScalar::try_new(edge.lower() as f64)?,
                    upper_complement: zero,
                    lower_slack: zero,
                    upper_multiplier: zero,
                    resistance: zero,
                    conductance: zero,
                    electrical_current: zero,
                    lower_slack_direction: zero,
                    upper_multiplier_direction: zero,
                    final_flow: None,
                })
            })
            .collect::<Result<Vec<_>, ElectricalIpmMcfError>>()?,
        mu: zero,
        epsilon_3: zero,
        recovery_epsilon: zero,
        duality_gap_bound: zero,
        centrality_residual: zero,
        balance_residual: zero,
        step_size: zero,
        electrical_energy: zero,
        linear_residual: zero,
        barrier_objective: zero,
        isolation_scale: 0,
        perturbation_bound: 0,
        isolation_attempt: 0,
        isolated_optimum_cost: 0,
        isolated_gap: 0,
        seed,
        metrics,
    })
}

#[allow(clippy::unnested_or_patterns)]
fn valid_electrical_ipm_event(event: &ElectricalIpmMcfTraceEvent) -> bool {
    let catalog_matches_stage = matches!(
        (event.catalog_id, event.after.stage),
        (
            "electrical-flow-interior-point-mcf.normalize-lower-bounds",
            ElectricalIpmMcfStage::NormalizeLowerBounds
        ) | (
            "electrical-flow-interior-point-mcf.isolation-attempt",
            ElectricalIpmMcfStage::IsolationAttempt
        ) | (
            "electrical-flow-interior-point-mcf.select-isolated-costs",
            ElectricalIpmMcfStage::SelectIsolatedCosts
        ) | (
            "electrical-flow-interior-point-mcf.contract-fixed-face",
            ElectricalIpmMcfStage::ContractFixedFace
        ) | (
            "electrical-flow-interior-point-mcf.initialize-dual",
            ElectricalIpmMcfStage::InitializeDualInterior
        ) | (
            "electrical-flow-interior-point-mcf.newton-centering-iteration",
            ElectricalIpmMcfStage::DampedCenteringStep
        ) | (
            "electrical-flow-interior-point-mcf.centered",
            ElectricalIpmMcfStage::Centered
        ) | (
            "electrical-flow-interior-point-mcf.decrease-barrier",
            ElectricalIpmMcfStage::DecreaseBarrier
        ) | (
            "electrical-flow-interior-point-mcf.approximate-flow",
            ElectricalIpmMcfStage::ApproximateFlow
        ) | (
            "electrical-flow-interior-point-mcf.round-nearest-integer",
            ElectricalIpmMcfStage::RoundNearestInteger
        ) | (
            "electrical-flow-interior-point-mcf.check-certificate",
            ElectricalIpmMcfStage::CheckCertificate
        ) | (
            "electrical-flow-interior-point-mcf.optimal",
            ElectricalIpmMcfStage::Optimal
        )
    );
    let stage_transition = matches!(
        (event.before.stage, event.after.stage),
        (
            ElectricalIpmMcfStage::Ready,
            ElectricalIpmMcfStage::NormalizeLowerBounds
        ) | (
            ElectricalIpmMcfStage::NormalizeLowerBounds | ElectricalIpmMcfStage::IsolationAttempt,
            ElectricalIpmMcfStage::IsolationAttempt
        ) | (
            ElectricalIpmMcfStage::IsolationAttempt,
            ElectricalIpmMcfStage::SelectIsolatedCosts
        ) | (
            ElectricalIpmMcfStage::SelectIsolatedCosts,
            ElectricalIpmMcfStage::ContractFixedFace
        ) | (
            ElectricalIpmMcfStage::ContractFixedFace,
            ElectricalIpmMcfStage::InitializeDualInterior
        ) | (
            ElectricalIpmMcfStage::InitializeDualInterior
                | ElectricalIpmMcfStage::DecreaseBarrier
                | ElectricalIpmMcfStage::DampedCenteringStep,
            ElectricalIpmMcfStage::DampedCenteringStep
        ) | (
            ElectricalIpmMcfStage::InitializeDualInterior
                | ElectricalIpmMcfStage::DecreaseBarrier
                | ElectricalIpmMcfStage::DampedCenteringStep,
            ElectricalIpmMcfStage::Centered
        ) | (
            ElectricalIpmMcfStage::Centered,
            ElectricalIpmMcfStage::DecreaseBarrier | ElectricalIpmMcfStage::ApproximateFlow
        ) | (
            ElectricalIpmMcfStage::ApproximateFlow,
            ElectricalIpmMcfStage::RoundNearestInteger
        ) | (
            ElectricalIpmMcfStage::RoundNearestInteger,
            ElectricalIpmMcfStage::CheckCertificate
        ) | (
            ElectricalIpmMcfStage::CheckCertificate,
            ElectricalIpmMcfStage::Optimal
        )
    );
    catalog_matches_stage && stage_transition
}

fn validate_admission(graph: &FlowNetwork, required: &[i128]) -> Result<(), ElectricalIpmMcfError> {
    if graph.nodes().len() > ELECTRICAL_IPM_MCF_MAX_NODES
        || graph.edges().len() > ELECTRICAL_IPM_MCF_MAX_EDGES
        || required.len() != graph.nodes().len()
        || graph.edges().iter().any(|edge| {
            edge.capacity() > ELECTRICAL_IPM_MCF_MAX_CAPACITY
                || edge.cost().unsigned_abs() > ELECTRICAL_IPM_MCF_MAX_COST
        })
    {
        return Err(ElectricalIpmMcfError::AdmissionLimit);
    }
    if required
        .iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(*value))
        != Some(0)
    {
        return Err(ElectricalIpmMcfError::InvalidDivergence);
    }
    let assignments = graph
        .edges()
        .iter()
        .try_fold(1_u64, |count, edge| {
            count.checked_mul(edge.capacity() - edge.lower() + 1)
        })
        .ok_or(ElectricalIpmMcfError::AdmissionLimit)?;
    if assignments > ELECTRICAL_IPM_MCF_MAX_ENUMERATED_ASSIGNMENTS {
        return Err(ElectricalIpmMcfError::AdmissionLimit);
    }
    Ok(())
}

fn enumerate_feasible(
    graph: &FlowNetwork,
    required: &[i128],
    metrics: &mut ElectricalIpmMcfMetrics,
) -> Result<Vec<Vec<u64>>, ElectricalIpmMcfError> {
    let mut feasible = Vec::new();
    let mut current = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    enumerate_coordinate(graph, required, 0, &mut current, &mut feasible, metrics)?;
    if feasible.is_empty() {
        return Err(ElectricalIpmMcfError::NumericalFailure);
    }
    metrics.feasible_flows =
        u64::try_from(feasible.len()).map_err(|_| ElectricalIpmMcfError::ArithmeticOverflow)?;
    Ok(feasible)
}

fn enumerate_coordinate(
    graph: &FlowNetwork,
    required: &[i128],
    index: usize,
    current: &mut [u64],
    feasible: &mut Vec<Vec<u64>>,
    metrics: &mut ElectricalIpmMcfMetrics,
) -> Result<(), ElectricalIpmMcfError> {
    if index == graph.edges().len() {
        metrics.enumerated_assignments = metrics
            .enumerated_assignments
            .checked_add(1)
            .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
        if divergences(graph, current)? == required {
            feasible.push(current.to_vec());
        }
        return Ok(());
    }
    let edge = &graph.edges()[index];
    for value in edge.lower()..=edge.capacity() {
        current[index] = value;
        enumerate_coordinate(graph, required, index + 1, current, feasible, metrics)?;
    }
    Ok(())
}

fn exact_cost(graph: &FlowNetwork, flow: &[u64]) -> Result<i128, ElectricalIpmMcfError> {
    graph
        .edges()
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (edge, &value)| {
            i128::from(value)
                .checked_mul(i128::from(edge.cost()))
                .and_then(|term| sum.checked_add(term))
                .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)
        })
}

fn perturbed_cost(costs: &[i128], flow: &[u64]) -> Result<i128, ElectricalIpmMcfError> {
    costs
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (&cost, &value)| {
            cost.checked_mul(i128::from(value))
                .and_then(|term| sum.checked_add(term))
                .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)
        })
}

fn isolate(recorder: &mut Recorder<'_>) -> Result<(), ElectricalIpmMcfError> {
    let m = u64::try_from(recorder.graph.edges().len().max(1))
        .map_err(|_| ElectricalIpmMcfError::ArithmeticOverflow)?;
    let u = recorder
        .graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(1);
    let perturbation_bound = m
        .checked_mul(u)
        .and_then(|value| value.checked_mul(2))
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    let q = i128::from(m)
        .checked_mul(i128::from(m))
        .and_then(|value| value.checked_mul(i128::from(u)))
        .and_then(|value| value.checked_mul(i128::from(u)))
        .and_then(|value| value.checked_mul(4))
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    recorder.state.perturbation_bound = perturbation_bound;
    recorder.state.isolation_scale = q;
    let original_best = recorder
        .state
        .feasible
        .iter()
        .map(|flow| exact_cost(recorder.graph, flow))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(ElectricalIpmMcfError::IsolationInvariant)?;
    for attempt in 1..=ELECTRICAL_IPM_MCF_MAX_ISOLATION_ATTEMPTS {
        recorder.state.metrics.isolation_attempts = attempt;
        recorder.state.isolation_attempt = attempt;
        let mut perturbations = Vec::with_capacity(recorder.graph.edges().len());
        let mut isolated = Vec::with_capacity(recorder.graph.edges().len());
        for edge in recorder.graph.edges() {
            let draw = recorder.state.rng.next(&mut recorder.state.metrics)?;
            let value = 1 + draw % perturbation_bound;
            perturbations.push(value);
            isolated.push(
                q.checked_mul(i128::from(edge.cost()))
                    .and_then(|scaled| scaled.checked_add(i128::from(value)))
                    .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?,
            );
        }
        recorder.state.perturbations = perturbations;
        recorder.state.isolated_costs = isolated;
        let mut ranked = recorder
            .state
            .feasible
            .iter()
            .map(|flow| {
                Ok((
                    perturbed_cost(&recorder.state.isolated_costs, flow)?,
                    exact_cost(recorder.graph, flow)?,
                ))
            })
            .collect::<Result<Vec<_>, ElectricalIpmMcfError>>()?;
        ranked.sort_unstable();
        let unique = ranked.len() == 1 || ranked[0].0 < ranked[1].0;
        recorder.state.isolated_optimum_cost = ranked[0].0;
        recorder.state.isolated_gap = if ranked.len() == 1 {
            1
        } else {
            ranked[1].0 - ranked[0].0
        };
        recorder.emit(
            "electrical-flow-interior-point-mcf.isolation-attempt",
            ElectricalIpmMcfStage::IsolationAttempt,
        )?;
        if unique {
            if ranked[0].1 != original_best {
                return Err(ElectricalIpmMcfError::IsolationInvariant);
            }
            recorder.emit(
                "electrical-flow-interior-point-mcf.select-isolated-costs",
                ElectricalIpmMcfStage::SelectIsolatedCosts,
            )?;
            return Ok(());
        }
    }
    Err(ElectricalIpmMcfError::IsolationGuardExhausted)
}

fn contract_feasible_face(
    recorder: &mut Recorder<'_>,
    required: &[i128],
) -> Result<(), ElectricalIpmMcfError> {
    let edge_count = recorder.graph.edges().len();
    recorder.state.face_lower = vec![u64::MAX; edge_count];
    recorder.state.face_upper = vec![0; edge_count];
    for flow in &recorder.state.feasible {
        for (index, &value) in flow.iter().enumerate() {
            recorder.state.face_lower[index] = recorder.state.face_lower[index].min(value);
            recorder.state.face_upper[index] = recorder.state.face_upper[index].max(value);
        }
    }
    recorder.state.fixed = recorder
        .state
        .face_lower
        .iter()
        .zip(&recorder.state.face_upper)
        .map(|(lower, upper)| lower == upper)
        .collect();
    recorder.state.metrics.fixed_coordinates =
        u64::try_from(recorder.state.fixed.iter().filter(|&&value| value).count())
            .map_err(|_| ElectricalIpmMcfError::ArithmeticOverflow)?;
    let base_divergence = divergences(recorder.graph, &recorder.state.face_lower)?;
    recorder.state.lower_divergence.clone_from(&base_divergence);
    recorder.state.target = required
        .iter()
        .zip(base_divergence)
        .map(|(&expected, actual)| (expected - actual) as f64)
        .collect();
    recorder.state.original_to_working = vec![None; edge_count];
    recorder.state.working.clear();
    for (original, edge) in recorder.graph.edges().iter().enumerate() {
        let width = recorder.state.face_upper[original] - recorder.state.face_lower[original];
        if width == 0 {
            continue;
        }
        let index = recorder.state.working.len();
        recorder.state.original_to_working[original] = Some(index);
        recorder.state.working.push(WorkingArc {
            original,
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            base: recorder.state.face_lower[original],
            width,
            cost: recorder.state.isolated_costs[original] as f64
                / recorder.state.isolation_scale as f64,
            s: 1.0,
            w: 1.0,
            ds: 0.0,
            dw: 0.0,
            resistance: 0.0,
            conductance: 0.0,
            current: 0.0,
        });
    }
    // The bounded oracle has now supplied only face bounds and scalar
    // isolation facts.  Dropping every enumerated vector makes it impossible
    // for the path-following or recovery phases to project the optimum vector.
    recorder.state.feasible.clear();
    recorder.emit(
        "electrical-flow-interior-point-mcf.contract-fixed-face",
        ElectricalIpmMcfStage::ContractFixedFace,
    )
}

fn initialize_dual(recorder: &mut Recorder<'_>) -> Result<(), ElectricalIpmMcfError> {
    recorder.state.y.fill(0.0);
    for arc in &mut recorder.state.working {
        arc.w = (1.0 - arc.cost).max(1.0);
        arc.s = arc.cost + arc.w;
        if arc.s <= 0.0 || arc.w <= 0.0 || !arc.s.is_finite() || !arc.w.is_finite() {
            return Err(ElectricalIpmMcfError::NumericalFailure);
        }
    }
    let m = recorder.graph.edges().len().max(1) as f64;
    let max_cost = recorder
        .state
        .working
        .iter()
        .map(|arc| arc.cost.abs())
        .fold(1.0_f64, f64::max);
    let max_width = recorder
        .state
        .working
        .iter()
        .map(|arc| arc.width)
        .max()
        .unwrap_or(1) as f64;
    recorder.state.mu = 4.0 * (max_cost + 1.0) * (max_width + 1.0) * (m + 1.0);
    recorder.state.epsilon_3 = 1.0 / (20.0 * (m.sqrt() + 1.0));
    let u = recorder
        .graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    recorder.state.recovery_epsilon = 1.0 / (12.0 * m * m * u * u * u);
    update_derived(&mut recorder.state)?;
    recorder.emit(
        "electrical-flow-interior-point-mcf.initialize-dual",
        ElectricalIpmMcfStage::InitializeDualInterior,
    )
}

fn update_derived(state: &mut KernelState) -> Result<(), ElectricalIpmMcfError> {
    state.node_residual.fill(0.0);
    let mut bound_residual = 0.0_f64;
    for arc in &mut state.working {
        let bt_y = state.y[arc.from] - state.y[arc.to];
        arc.s = arc.cost - bt_y + arc.w;
        if arc.s <= POSITIVE_FLOOR
            || arc.w <= POSITIVE_FLOOR
            || !arc.s.is_finite()
            || !arc.w.is_finite()
        {
            return Err(ElectricalIpmMcfError::NumericalFailure);
        }
        let x = state.mu / arc.s;
        let z = state.mu / arc.w;
        state.node_residual[arc.from] += x;
        state.node_residual[arc.to] -= x;
        bound_residual = bound_residual.max((arc.width as f64 - x - z).abs());
    }
    for (residual, target) in state.node_residual.iter_mut().zip(&state.target) {
        *residual -= target;
    }
    state.balance_residual = state
        .node_residual
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    let scale = 1.0
        + state
            .target
            .iter()
            .map(|value| value.abs())
            .fold(0.0, f64::max)
            .max(
                state
                    .working
                    .iter()
                    .map(|arc| arc.width as f64)
                    .fold(0.0, f64::max),
            );
    state.centrality_residual = state.balance_residual.max(bound_residual) / scale;
    state.barrier_objective = barrier_objective(state)?;
    Ok(())
}

fn barrier_objective(state: &KernelState) -> Result<f64, ElectricalIpmMcfError> {
    let linear_y = state
        .target
        .iter()
        .zip(&state.y)
        .map(|(b, y)| -b * y)
        .sum::<f64>();
    let linear_w = state
        .working
        .iter()
        .map(|arc| arc.width as f64 * arc.w)
        .sum::<f64>();
    let barrier = state
        .working
        .iter()
        .map(|arc| arc.s.ln() + arc.w.ln())
        .sum::<f64>();
    let result = linear_y + linear_w - state.mu * barrier;
    if result.is_finite() {
        Ok(result)
    } else {
        Err(ElectricalIpmMcfError::NumericalFailure)
    }
}

fn union_find_root(parent: &mut [usize], node: usize) -> usize {
    if parent[node] != node {
        parent[node] = union_find_root(parent, parent[node]);
    }
    parent[node]
}

fn component_anchors(state: &mut KernelState) {
    let mut parent = (0..state.node_count).collect::<Vec<_>>();
    for arc in &state.working {
        if arc.from == arc.to {
            continue;
        }
        let left = union_find_root(&mut parent, arc.from);
        let right = union_find_root(&mut parent, arc.to);
        if left != right {
            parent[right] = left;
        }
    }
    state.anchors.fill(false);
    let mut minimum = vec![usize::MAX; state.node_count];
    for node in 0..state.node_count {
        let root = union_find_root(&mut parent, node);
        minimum[root] = minimum[root].min(node);
    }
    for node in minimum.into_iter().filter(|&node| node != usize::MAX) {
        state.anchors[node] = true;
    }
}

fn assemble_laplacian(recorder: &mut Recorder<'_>) -> Result<(), ElectricalIpmMcfError> {
    component_anchors(&mut recorder.state);
    for arc in &mut recorder.state.working {
        arc.resistance = (arc.s * arc.s + arc.w * arc.w) / recorder.state.mu;
        arc.conductance = 1.0 / arc.resistance;
        arc.current = 0.0;
        arc.ds = 0.0;
        arc.dw = 0.0;
        if !arc.resistance.is_finite()
            || !arc.conductance.is_finite()
            || arc.resistance <= 0.0
            || arc.conductance <= 0.0
        {
            return Err(ElectricalIpmMcfError::NumericalFailure);
        }
    }
    recorder.state.metrics.laplacian_assemblies = recorder
        .state
        .metrics
        .laplacian_assemblies
        .checked_add(1)
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    Ok(())
}

struct NewtonSystem {
    node_to_row: Vec<Option<usize>>,
    matrix: Vec<Vec<f64>>,
    rhs: Vec<f64>,
    potential_gradient: Vec<f64>,
    multiplier_gradient: Vec<f64>,
}

fn build_newton_system(state: &KernelState) -> NewtonSystem {
    let mut node_to_row = vec![None; state.node_count];
    let mut dimension = 0;
    for (node, anchored) in state.anchors.iter().enumerate() {
        if !anchored {
            node_to_row[node] = Some(dimension);
            dimension += 1;
        }
    }
    let mut matrix = vec![vec![0.0; dimension]; dimension];
    let mut rhs = vec![0.0; dimension];
    let potential_gradient = state.node_residual.clone();
    let multiplier_gradient = state
        .working
        .iter()
        .map(|arc| arc.width as f64 - state.mu / arc.s - state.mu / arc.w)
        .collect::<Vec<_>>();
    for (node, row) in node_to_row.iter().enumerate() {
        if let Some(row) = row {
            rhs[*row] = -potential_gradient[node];
        }
    }
    for (arc_index, arc) in state.working.iter().enumerate() {
        let coefficient = arc.w * arc.w / (arc.s * arc.s + arc.w * arc.w);
        let correction = coefficient * multiplier_gradient[arc_index];
        if let Some(from) = node_to_row[arc.from] {
            rhs[from] -= correction;
        }
        if let Some(to) = node_to_row[arc.to] {
            rhs[to] += correction;
        }
        if let Some(from) = node_to_row[arc.from] {
            matrix[from][from] += arc.conductance;
        }
        if let Some(to) = node_to_row[arc.to] {
            matrix[to][to] += arc.conductance;
        }
        if let (Some(from), Some(to)) = (node_to_row[arc.from], node_to_row[arc.to]) {
            matrix[from][to] -= arc.conductance;
            matrix[to][from] -= arc.conductance;
        }
    }

    NewtonSystem {
        node_to_row,
        matrix,
        rhs,
        potential_gradient,
        multiplier_gradient,
    }
}

fn solve_newton(recorder: &mut Recorder<'_>) -> Result<f64, ElectricalIpmMcfError> {
    let NewtonSystem {
        node_to_row,
        mut matrix,
        mut rhs,
        potential_gradient,
        multiplier_gradient,
    } = build_newton_system(&recorder.state);
    let dimension = rhs.len();
    let original_matrix = matrix.clone();
    let original_rhs = rhs.clone();
    let pivots = if dimension == 0 {
        0
    } else {
        gaussian_solve(&mut matrix, &mut rhs)?
    };
    recorder.state.metrics.elimination_pivots = recorder
        .state
        .metrics
        .elimination_pivots
        .checked_add(pivots)
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    recorder.state.metrics.newton_solves = recorder
        .state
        .metrics
        .newton_solves
        .checked_add(1)
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    recorder.state.dy.fill(0.0);
    for (node, row) in node_to_row.iter().enumerate() {
        if let Some(row) = row {
            recorder.state.dy[node] = rhs[*row];
        }
    }
    recorder.state.linear_residual = original_matrix
        .iter()
        .zip(&original_rhs)
        .map(|(values, expected)| {
            let actual = values
                .iter()
                .zip(&rhs)
                .map(|(value, solution)| value * solution)
                .sum::<f64>();
            (actual - expected).abs()
        })
        .fold(0.0, f64::max);
    recorder.state.electrical_energy = 0.0;
    let mut directional = 0.0;
    for (index, arc) in recorder.state.working.iter_mut().enumerate() {
        let bt = recorder.state.dy[arc.from] - recorder.state.dy[arc.to];
        let inverse_slack_squared = 1.0 / (arc.s * arc.s);
        let inverse_multiplier_squared = 1.0 / (arc.w * arc.w);
        arc.dw = (-multiplier_gradient[index] + recorder.state.mu * inverse_slack_squared * bt)
            / (recorder.state.mu * (inverse_slack_squared + inverse_multiplier_squared));
        arc.ds = -bt + arc.dw;
        arc.current = arc.conductance * bt;
        recorder.state.electrical_energy += arc.resistance * arc.current * arc.current;
        directional += multiplier_gradient[index] * arc.dw;
    }
    directional += potential_gradient
        .iter()
        .zip(&recorder.state.dy)
        .map(|(gradient, direction)| gradient * direction)
        .sum::<f64>();
    if !directional.is_finite() || directional >= POSITIVE_FLOOR {
        return Err(ElectricalIpmMcfError::NumericalFailure);
    }
    Ok(directional)
}

fn gaussian_solve(matrix: &mut [Vec<f64>], rhs: &mut [f64]) -> Result<u64, ElectricalIpmMcfError> {
    let n = rhs.len();
    if matrix.len() != n || matrix.iter().any(|row| row.len() != n) {
        return Err(ElectricalIpmMcfError::NumericalFailure);
    }
    let mut pivots = 0;
    for column in 0..n {
        let pivot = (column..n)
            .max_by(|&left, &right| {
                matrix[left][column]
                    .abs()
                    .total_cmp(&matrix[right][column].abs())
            })
            .ok_or(ElectricalIpmMcfError::NumericalFailure)?;
        if matrix[pivot][column].abs() <= POSITIVE_FLOOR {
            return Err(ElectricalIpmMcfError::NumericalFailure);
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let divisor = matrix[column][column];
        for value in &mut matrix[column][column..] {
            *value /= divisor;
        }
        rhs[column] /= divisor;
        let pivot_row = matrix[column].clone();
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = matrix[row][column];
            if factor == 0.0 {
                continue;
            }
            for (value, pivot_value) in matrix[row][column..].iter_mut().zip(&pivot_row[column..]) {
                *value -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[column];
        }
        pivots += 1;
    }
    if rhs.iter().any(|value| !value.is_finite()) {
        return Err(ElectricalIpmMcfError::NumericalFailure);
    }
    Ok(pivots)
}

fn apply_damped_step(
    recorder: &mut Recorder<'_>,
    directional: f64,
) -> Result<(), ElectricalIpmMcfError> {
    let old_y = recorder.state.y.clone();
    let old_w = recorder
        .state
        .working
        .iter()
        .map(|arc| arc.w)
        .collect::<Vec<_>>();
    let old_objective = recorder.state.barrier_objective;
    let mut alpha = 1.0_f64;
    for arc in &recorder.state.working {
        if arc.ds < 0.0 {
            alpha = alpha.min(-0.99 * arc.s / arc.ds);
        }
        if arc.dw < 0.0 {
            alpha = alpha.min(-0.99 * arc.w / arc.dw);
        }
    }
    alpha = alpha.min(1.0);
    for reduction in 0..=64_u64 {
        for (value, (&base, &direction)) in recorder
            .state
            .y
            .iter_mut()
            .zip(old_y.iter().zip(&recorder.state.dy))
        {
            *value = base + alpha * direction;
        }
        for (arc, &base) in recorder.state.working.iter_mut().zip(&old_w) {
            arc.w = base + alpha * arc.dw;
        }
        match update_derived(&mut recorder.state) {
            Ok(())
                if recorder.state.barrier_objective
                    <= old_objective + ARMIJO * alpha * directional =>
            {
                recorder.state.step_size = alpha;
                recorder.state.metrics.centering_steps = recorder
                    .state
                    .metrics
                    .centering_steps
                    .checked_add(1)
                    .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
                recorder.state.metrics.line_search_reductions = recorder
                    .state
                    .metrics
                    .line_search_reductions
                    .checked_add(reduction)
                    .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
                recorder.emit(
                    "electrical-flow-interior-point-mcf.newton-centering-iteration",
                    ElectricalIpmMcfStage::DampedCenteringStep,
                )?;
                return Ok(());
            }
            _ => {
                alpha *= 0.5;
                recorder.state.y.clone_from(&old_y);
                for (arc, &base) in recorder.state.working.iter_mut().zip(&old_w) {
                    arc.w = base;
                }
                update_derived(&mut recorder.state)?;
            }
        }
    }
    Err(ElectricalIpmMcfError::NumericalFailure)
}

fn center(recorder: &mut Recorder<'_>) -> Result<(), ElectricalIpmMcfError> {
    update_derived(&mut recorder.state)?;
    for _ in 0..ELECTRICAL_IPM_MCF_MAX_CENTERING_STEPS {
        if recorder.state.centrality_residual <= CENTER_TOLERANCE {
            recorder.emit(
                "electrical-flow-interior-point-mcf.centered",
                ElectricalIpmMcfStage::Centered,
            )?;
            return Ok(());
        }
        assemble_laplacian(recorder)?;
        let directional = solve_newton(recorder)?;
        apply_damped_step(recorder, directional)?;
    }
    Err(ElectricalIpmMcfError::NonConvergence)
}

fn rounded_flow(state: &KernelState) -> Result<Vec<u64>, ElectricalIpmMcfError> {
    let mut result = state.face_lower.clone();
    for arc in &state.working {
        let estimate = state.mu / arc.s;
        if !estimate.is_finite() || estimate < -0.5 || estimate > arc.width as f64 + 0.5 {
            return Err(ElectricalIpmMcfError::RecoveryFailure);
        }
        let nearest = estimate.round();
        let rounded = nearest
            .to_u64()
            .ok_or(ElectricalIpmMcfError::RecoveryFailure)?;
        if rounded > arc.width || (estimate - rounded as f64).abs() > 1.0 / 3.0 + 1.0e-8 {
            return Err(ElectricalIpmMcfError::RecoveryFailure);
        }
        result[arc.original] = arc
            .base
            .checked_add(rounded)
            .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    }
    Ok(result)
}

fn candidate_matches_isolated_optimum(
    graph: &FlowNetwork,
    required: &[i128],
    state: &KernelState,
    candidate: &[u64],
) -> Result<bool, ElectricalIpmMcfError> {
    Ok(divergences(graph, candidate)? == required
        && perturbed_cost(&state.isolated_costs, candidate)? == state.isolated_optimum_cost)
}

fn run_path_following(
    recorder: &mut Recorder<'_>,
    required: &[i128],
) -> Result<Vec<u64>, ElectricalIpmMcfError> {
    if recorder.state.working.is_empty() {
        let candidate = recorder.state.face_lower.clone();
        if !candidate_matches_isolated_optimum(
            recorder.graph,
            required,
            &recorder.state,
            &candidate,
        )? {
            return Err(ElectricalIpmMcfError::RecoveryFailure);
        }
        recorder.state.mu = 0.0;
        recorder.state.centrality_residual = 0.0;
        recorder.state.balance_residual = 0.0;
        recorder.emit(
            "electrical-flow-interior-point-mcf.centered",
            ElectricalIpmMcfStage::Centered,
        )?;
        recorder.emit(
            "electrical-flow-interior-point-mcf.approximate-flow",
            ElectricalIpmMcfStage::ApproximateFlow,
        )?;
        return Ok(candidate);
    }
    center(recorder)?;
    for _ in 0..ELECTRICAL_IPM_MCF_MAX_BARRIER_REDUCTIONS {
        let gap = 2.0 * recorder.state.working.len() as f64 * recorder.state.mu;
        if gap <= recorder.state.recovery_epsilon {
            let candidate = rounded_flow(&recorder.state)?;
            if candidate_matches_isolated_optimum(
                recorder.graph,
                required,
                &recorder.state,
                &candidate,
            )? {
                recorder.emit(
                    "electrical-flow-interior-point-mcf.approximate-flow",
                    ElectricalIpmMcfStage::ApproximateFlow,
                )?;
                return Ok(candidate);
            }
        }
        recorder.state.mu *= 1.0 - recorder.state.epsilon_3;
        recorder.state.metrics.barrier_reductions = recorder
            .state
            .metrics
            .barrier_reductions
            .checked_add(1)
            .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
        update_derived(&mut recorder.state)?;
        recorder.emit(
            "electrical-flow-interior-point-mcf.decrease-barrier",
            ElectricalIpmMcfStage::DecreaseBarrier,
        )?;
        center(recorder)?;
    }
    Err(ElectricalIpmMcfError::NonConvergence)
}

fn run_internal(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
    trace: bool,
) -> Result<InternalRun, ElectricalIpmMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(graph, required, seed, trace, &mut feasibility)
}

fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
    trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, ElectricalIpmMcfError> {
    validate_admission(graph, required)?;
    feasibility.find_feasible_flow(graph, required, FeasibilityUse::PrecheckOnly)?;
    let mut metrics = ElectricalIpmMcfMetrics::default();
    let feasible = enumerate_feasible(graph, required, &mut metrics)?;
    let state = KernelState {
        node_count: graph.nodes().len(),
        lower_divergence: vec![0; graph.nodes().len()],
        feasible,
        isolated_optimum_cost: 0,
        perturbations: vec![0; graph.edges().len()],
        isolated_costs: graph
            .edges()
            .iter()
            .map(|edge| i128::from(edge.cost()))
            .collect(),
        face_lower: graph.edges().iter().map(FlowEdge::lower).collect(),
        face_upper: graph.edges().iter().map(FlowEdge::capacity).collect(),
        fixed: vec![false; graph.edges().len()],
        working: Vec::new(),
        original_to_working: vec![None; graph.edges().len()],
        target: required.iter().map(|&value| value as f64).collect(),
        y: vec![0.0; graph.nodes().len()],
        dy: vec![0.0; graph.nodes().len()],
        node_residual: vec![0.0; graph.nodes().len()],
        anchors: vec![false; graph.nodes().len()],
        final_flows: vec![None; graph.edges().len()],
        mu: 0.0,
        epsilon_3: 0.0,
        recovery_epsilon: 0.0,
        centrality_residual: 0.0,
        balance_residual: 0.0,
        step_size: 0.0,
        electrical_energy: 0.0,
        linear_residual: 0.0,
        barrier_objective: 0.0,
        isolation_scale: 0,
        perturbation_bound: 0,
        isolation_attempt: 0,
        isolated_gap: 0,
        seed,
        metrics,
        stage: ElectricalIpmMcfStage::Ready,
        rng: ExactRng::new(seed),
    };
    let base_snapshot = project_snapshot(graph, &state)?;
    let mut recorder = Recorder {
        graph,
        current: base_snapshot.clone(),
        state,
        events: Vec::new(),
        enabled: trace,
    };
    recorder.emit(
        "electrical-flow-interior-point-mcf.normalize-lower-bounds",
        ElectricalIpmMcfStage::NormalizeLowerBounds,
    )?;
    isolate(&mut recorder)?;
    contract_feasible_face(&mut recorder, required)?;
    initialize_dual(&mut recorder)?;
    let flows = run_path_following(&mut recorder, required)?;
    recorder.state.metrics.rounding_operations =
        u64::try_from(flows.len()).map_err(|_| ElectricalIpmMcfError::ArithmeticOverflow)?;
    recorder.state.final_flows = flows.iter().copied().map(Some).collect();
    recorder.emit(
        "electrical-flow-interior-point-mcf.round-nearest-integer",
        ElectricalIpmMcfStage::RoundNearestInteger,
    )?;
    if !candidate_matches_isolated_optimum(graph, required, &recorder.state, &flows)? {
        return Err(ElectricalIpmMcfError::RecoveryFailure);
    }
    let certificate = check_min_cost_flow(graph, required, &flows)?;
    recorder.state.metrics.certificate_checks = recorder
        .state
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(ElectricalIpmMcfError::ArithmeticOverflow)?;
    recorder.emit(
        "electrical-flow-interior-point-mcf.check-certificate",
        ElectricalIpmMcfStage::CheckCertificate,
    )?;
    recorder.emit(
        "electrical-flow-interior-point-mcf.optimal",
        ElectricalIpmMcfStage::Optimal,
    )?;
    let final_snapshot = recorder.current.clone();
    let result = ElectricalIpmMcfResult {
        flows,
        certificate,
        seed,
        final_snapshot,
        metrics: recorder.state.metrics,
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn project_edge_states(
    graph: &FlowNetwork,
    state: &KernelState,
) -> Result<Vec<ElectricalIpmMcfEdgeState>, ElectricalIpmMcfError> {
    let scalar = ElectricalIpmMcfScalar::try_new;
    let mut edges = Vec::with_capacity(graph.edges().len());
    for (original, edge) in graph.edges().iter().enumerate() {
        let working = state
            .original_to_working
            .get(original)
            .and_then(|value| *value)
            .and_then(|index| state.working.get(index));
        let (fractional, complement, lower_slack, upper, resistance, conductance, current, ds, dw) =
            working.map_or(
                (
                    state
                        .face_lower
                        .get(original)
                        .copied()
                        .unwrap_or(edge.lower()) as f64,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ),
                |arc| {
                    (
                        arc.base as f64 + state.mu / arc.s,
                        state.mu / arc.w,
                        arc.s,
                        arc.w,
                        arc.resistance,
                        arc.conductance,
                        arc.current,
                        arc.ds,
                        arc.dw,
                    )
                },
            );
        edges.push(ElectricalIpmMcfEdgeState {
            edge: edge.id().clone(),
            perturbation: state.perturbations.get(original).copied().unwrap_or(0),
            isolated_cost: state
                .isolated_costs
                .get(original)
                .copied()
                .unwrap_or(i128::from(edge.cost())),
            fixed_on_face: state.fixed.get(original).copied().unwrap_or(false),
            face_lower: state
                .face_lower
                .get(original)
                .copied()
                .unwrap_or(edge.lower()),
            face_upper: state
                .face_upper
                .get(original)
                .copied()
                .unwrap_or(edge.capacity()),
            fractional_flow: scalar(fractional)?,
            upper_complement: scalar(complement)?,
            lower_slack: scalar(lower_slack)?,
            upper_multiplier: scalar(upper)?,
            resistance: scalar(resistance)?,
            conductance: scalar(conductance)?,
            electrical_current: scalar(current)?,
            lower_slack_direction: scalar(ds)?,
            upper_multiplier_direction: scalar(dw)?,
            final_flow: state.final_flows.get(original).copied().flatten(),
        });
    }
    Ok(edges)
}

fn project_snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
) -> Result<ElectricalIpmMcfSnapshot, ElectricalIpmMcfError> {
    let scalar = ElectricalIpmMcfScalar::try_new;
    let nodes = graph
        .node_indices()
        .map(|node| {
            let index = node.as_usize();
            Ok(ElectricalIpmMcfNodeState {
                node,
                potential: scalar(state.y[index])?,
                potential_direction: scalar(state.dy[index])?,
                balance_residual: scalar(state.node_residual[index])?,
                anchored: state.anchors[index],
            })
        })
        .collect::<Result<Vec<_>, ElectricalIpmMcfError>>()?;
    let edges = project_edge_states(graph, state)?;
    Ok(ElectricalIpmMcfSnapshot {
        stage: state.stage,
        nodes,
        edges,
        mu: scalar(state.mu)?,
        epsilon_3: scalar(state.epsilon_3)?,
        recovery_epsilon: scalar(state.recovery_epsilon)?,
        duality_gap_bound: scalar(2.0 * state.working.len() as f64 * state.mu)?,
        centrality_residual: scalar(state.centrality_residual)?,
        balance_residual: scalar(state.balance_residual)?,
        step_size: scalar(state.step_size)?,
        electrical_energy: scalar(state.electrical_energy)?,
        linear_residual: scalar(state.linear_residual)?,
        barrier_objective: scalar(state.barrier_objective)?,
        isolation_scale: state.isolation_scale,
        perturbation_bound: state.perturbation_bound,
        isolation_attempt: state.isolation_attempt,
        isolated_optimum_cost: state.isolated_optimum_cost,
        isolated_gap: state.isolated_gap,
        seed: state.seed,
        metrics: state.metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn network(node_ids: &[&str], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edges = edges
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
            .collect();
        FlowNetwork::new(nodes, edges).expect("network")
    }

    #[test]
    fn solves_parallel_choice_through_electrical_newton_systems() {
        let graph = network(
            &["a", "s", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 3, 1),
                ("st", "s", "t", 0, 3, 5),
            ],
        );
        let result =
            solve_electrical_flow_interior_point_mcf(&graph, &[0, 3, -3]).expect("electrical IPM");
        assert_eq!(result.flows, vec![3, 3, 0]);
        assert_eq!(result.certificate.total_cost, 6);
        assert!(result.metrics.newton_solves > 0);
        assert!(result.metrics.barrier_reductions > 0);
    }

    #[test]
    fn contracts_a_forced_one_edge_face_and_still_records_recovery() {
        let graph = network(&["s", "t"], &[("st", "s", "t", 0, 2, 1)]);
        let trace =
            trace_electrical_flow_interior_point_mcf_with_seed(&graph, &[2, -2], 7).expect("trace");
        assert_eq!(trace.result.flows, vec![2]);
        assert_eq!(trace.result.metrics.fixed_coordinates, 1);
        for stage in [
            ElectricalIpmMcfStage::IsolationAttempt,
            ElectricalIpmMcfStage::ContractFixedFace,
            ElectricalIpmMcfStage::ApproximateFlow,
            ElectricalIpmMcfStage::Optimal,
        ] {
            assert!(
                trace.events.iter().any(|event| event.after.stage == stage),
                "missing {stage:?}"
            );
        }
    }

    #[test]
    fn handles_lower_bounds_negative_costs_and_a_circulation() {
        let graph = network(
            &["a", "b"],
            &[("ab", "a", "b", 1, 3, -2), ("ba", "b", "a", 0, 3, 0)],
        );
        let result =
            solve_electrical_flow_interior_point_mcf(&graph, &[0, 0]).expect("circulation");
        assert_eq!(result.flows, vec![3, 3]);
        assert_eq!(result.certificate.total_cost, -6);
    }

    #[test]
    fn seeded_trace_reexecutes_by_bit_identity() {
        let graph = network(
            &["s", "t"],
            &[("cheap", "s", "t", 0, 3, 1), ("dear", "s", "t", 0, 3, 4)],
        );
        let left =
            trace_electrical_flow_interior_point_mcf_with_seed(&graph, &[2, -2], 19).expect("left");
        let right = trace_electrical_flow_interior_point_mcf_with_seed(&graph, &[2, -2], 19)
            .expect("right");
        assert_eq!(left, right);
        check_electrical_flow_interior_point_mcf_trace(&graph, &[2, -2], &left).expect("checker");
    }

    #[test]
    fn publishes_exactly_one_detail_per_newton_centering_iteration() {
        let graph = network(
            &["s", "a", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 3, 1),
                ("st", "s", "t", 0, 3, 4),
            ],
        );
        let trace = trace_electrical_flow_interior_point_mcf(&graph, &[3, 0, -3]).expect("trace");
        let centering_events = trace
            .events
            .iter()
            .filter(|event| {
                event.catalog_id == "electrical-flow-interior-point-mcf.newton-centering-iteration"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            u64::try_from(centering_events.len()).expect("event count"),
            trace.result.metrics.newton_solves
        );
        assert!(centering_events.iter().all(|event| {
            event.after.metrics.newton_solves == event.before.metrics.newton_solves + 1
                && event.after.metrics.laplacian_assemblies
                    == event.before.metrics.laplacian_assemblies + 1
                && event.after.metrics.centering_steps == event.before.metrics.centering_steps + 1
        }));
    }

    #[test]
    fn trace_checker_rejects_a_mutated_scalar() {
        let graph = network(
            &["s", "t"],
            &[("a", "s", "t", 0, 2, 1), ("b", "s", "t", 0, 2, 2)],
        );
        let mut trace = trace_electrical_flow_interior_point_mcf(&graph, &[1, -1]).expect("trace");
        trace.events[0].after.isolation_scale += 1;
        assert_eq!(
            check_electrical_flow_interior_point_mcf_trace(&graph, &[1, -1], &trace),
            Err(ElectricalIpmMcfError::TraceVerification)
        );
        let mut catalog_trace =
            trace_electrical_flow_interior_point_mcf(&graph, &[1, -1]).expect("trace");
        catalog_trace.events[0].catalog_id = "electrical-flow-interior-point-mcf.optimal";
        assert_eq!(
            check_electrical_flow_interior_point_mcf_trace(&graph, &[1, -1], &catalog_trace,),
            Err(ElectricalIpmMcfError::TraceVerification)
        );
        let mut base_trace =
            trace_electrical_flow_interior_point_mcf(&graph, &[1, -1]).expect("trace");
        base_trace.base_snapshot.isolation_scale += 1;
        base_trace.events[0].before.isolation_scale += 1;
        assert_eq!(
            check_electrical_flow_interior_point_mcf_trace(&graph, &[1, -1], &base_trace),
            Err(ElectricalIpmMcfError::TraceVerification)
        );
    }

    #[test]
    fn rejects_infeasible_and_oversized_inputs_without_mutation() {
        let graph = network(&["s", "t"], &[("e", "s", "t", 0, 1, 1)]);
        let before = graph.clone();
        assert!(matches!(
            solve_electrical_flow_interior_point_mcf(&graph, &[2, -2]),
            Err(ElectricalIpmMcfError::Feasibility(_))
        ));
        assert_eq!(graph, before);
        let oversized = network(
            &["a", "b", "c", "d", "e", "f", "g"],
            &[("ab", "a", "b", 0, 1, 0)],
        );
        assert_eq!(
            solve_electrical_flow_interior_point_mcf(&oversized, &[0; 7]),
            Err(ElectricalIpmMcfError::AdmissionLimit)
        );
    }
}
