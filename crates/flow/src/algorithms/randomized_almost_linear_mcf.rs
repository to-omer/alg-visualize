//! Project-owned randomized minimum-cost-flow oracle demonstrator.
//!
//! Chen et al. combine isolation, an alpha-power interior-point potential,
//! sampled tree chains, lazy coordinate detection, and final-point rounding. Their
//! dynamic data structures and outer stopping loop are intentionally not
//! reproduced here. For the small graphs admitted by the visualizer, a project
//! optimum-vector oracle constructs the initial/final-point data around one
//! bounded source tree-chain prefix, while exact minimum-ratio-cycle search
//! replaces the approximate query. This endpoint is neither a source component
//! nor the paper's solver, and never claims its asymptotic running time.

#![allow(clippy::cast_precision_loss)]

use num_bigint::BigInt;
use num_rational::BigRational;
#[cfg(test)]
use num_traits::Signed;
use num_traits::{One, ToPrimitive, Zero};
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

use super::{
    MinimumRatioCycleMcfError, MinimumRatioCycleMcfSnapshot, trace_minimum_ratio_cycle_mcf,
};

/// Stable default seed used when the caller does not supply one.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED: u64 = 0x434b_4c50_4d43_4632;
/// Maximum nodes admitted by the bounded demonstrator.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_NODES: usize = 6;
/// Maximum edges admitted by the bounded demonstrator.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_EDGES: usize = 8;
/// Maximum capacity admitted by the bounded demonstrator.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_CAPACITY: u64 = 8;
/// Maximum absolute cost admitted by the bounded demonstrator.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_COST: u64 = 32;
/// Maximum assignments inspected by exact feasible-face construction.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ASSIGNMENTS: u64 = 100_000;
/// Maximum independent isolation attempts.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ISOLATION_ATTEMPTS: usize = 16;
/// Maximum visible trace boundaries.
pub const RANDOMIZED_ALMOST_LINEAR_MCF_MAX_TRACE_EVENTS: usize = 8_192;

const CATALOG_ID: &str = "randomized-almost-linear-mcf-oracle-demonstrator";
const DETECT_DENOMINATOR: f64 = 1_000.0;

/// Replay-safe finite IEEE-754 value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RandomizedAlmostLinearMcfScalar(u64);

impl RandomizedAlmostLinearMcfScalar {
    fn try_new(value: f64) -> Result<Self, RandomizedAlmostLinearMcfError> {
        if !value.is_finite() {
            return Err(RandomizedAlmostLinearMcfError::NumericalFailure);
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

    /// Stable decimal scene projection.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Explicit upper bound on isolation failure probability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfProbability {
    /// Numerator.
    pub numerator: u64,
    /// Denominator.
    pub denominator: u64,
}

/// One source or bounded-replacement boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomizedAlmostLinearMcfStage {
    /// Valid input, before exact face construction.
    Ready,
    /// A geometrically spaced complete bounded-flow assignment is being checked.
    InspectFeasibleAssignment,
    /// The bounded feasible integer face was enumerated.
    EnumerateFeasibleSet,
    /// Independent isolation weights were sampled.
    SampleIsolationCosts,
    /// A unique isolated optimum was selected.
    SelectIsolatedOptimum,
    /// The feasible-face barycenter installed a relative-interior point.
    InitializeRelativeInterior,
    /// A geometrically spaced signed-vector state from the exact ratio oracle.
    InspectOracleVector,
    /// A bounded spanning-forest population was exposed.
    BuildForestPool,
    /// One seeded tree-chain representative was selected.
    SampleTreeChain,
    /// Source gradient and length coordinates were refreshed.
    RefreshGradientLength,
    /// The exact bounded minimum-ratio-cycle oracle answered the query.
    QueryMinimumRatioCycle,
    /// One source-scaled potential-reduction step was applied.
    PotentialReductionStep,
    /// Lazily stale coordinates exceeding the source threshold were detected.
    DetectChangedCoordinates,
    /// The bounded tree-chain view was rebuilt.
    RebuildTreeChain,
    /// A source-valid near-optimal rational point was materialized exactly.
    ConstructFinalPoint,
    /// Every final-point coordinate was rounded to its nearest integer.
    RoundNearestInteger,
    /// Original-cost optimality was independently certified.
    CheckCertificate,
    /// The bounded end-to-end execution is complete.
    Optimal,
}

/// Node projection for the active tree chain and selected cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfNodeState {
    /// Canonical node index.
    pub node: NodeIndex,
    /// Required outflow-minus-inflow divergence at this node.
    pub required_divergence: i128,
    /// Rooted forest component.
    pub component: usize,
    /// Rooted forest parent.
    pub parent: Option<NodeIndex>,
    /// Rooted forest depth.
    pub depth: usize,
    /// Whether the selected cycle touches this node.
    pub on_selected_cycle: bool,
}

/// Edge projection for isolation, tree sampling, and lazy refresh.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Whether the coordinate is fixed on the feasible face.
    pub fixed_on_face: bool,
    /// Relative-interior flow.
    pub initial_flow: RandomizedAlmostLinearMcfScalar,
    /// Current fractional flow after the visible source step.
    pub current_flow: RandomizedAlmostLinearMcfScalar,
    /// Last lazily refreshed flow.
    pub stale_flow: RandomizedAlmostLinearMcfScalar,
    /// Exact source-valid point used by the nearest-integer recovery lemma.
    pub final_point_flow: Option<BigRational>,
    /// Final integral flow after nearest-integer rounding.
    pub final_flow: Option<u64>,
    /// Independent isolation draw.
    pub isolation_draw: u64,
    /// Lexicographically dominating isolated unit cost `D c_e + z_e`.
    pub isolated_cost: i128,
    /// Coordinate of the unique isolated integral optimum once selected.
    pub isolated_optimum_flow: Option<u64>,
    /// Membership in the sampled spanning forest.
    pub tree_edge: bool,
    /// Current signed vector inspected by the exact ratio-cycle oracle.
    pub candidate_sign: i8,
    /// Selected cycle sign in `{-1,0,1}`.
    pub selected_sign: i8,
    /// Source gradient coordinate.
    pub gradient: RandomizedAlmostLinearMcfScalar,
    /// Source length coordinate.
    pub length: RandomizedAlmostLinearMcfScalar,
    /// Whether lazy detection refreshed this coordinate.
    pub detected: bool,
}

/// Bounded work and audit counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfMetrics {
    /// Integer assignments inspected.
    pub enumerated_assignments: u64,
    /// Feasible integer flows retained.
    pub feasible_flows: u64,
    /// Isolation attempts performed.
    pub isolation_attempts: u64,
    /// Sampled random words.
    pub random_draws: u64,
    /// Bounded forest representatives.
    pub forest_pool_size: u64,
    /// Ternary cycle vectors inspected by the exact minimum-ratio subroutine.
    pub oracle_vector_evaluations: u64,
    /// Exact minimum-ratio queries.
    pub ratio_queries: u64,
    /// Source potential steps.
    pub source_steps: u64,
    /// Coordinates inspected by Detect.
    pub detect_scans: u64,
    /// Coordinates refreshed by Detect.
    pub detected_coordinates: u64,
    /// Tree-chain rebuilds.
    pub rebuilds: u64,
    /// Independent certificate checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete replay state at one atomic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfSnapshot {
    /// Current source or bounded-replacement phase.
    pub stage: RandomizedAlmostLinearMcfStage,
    /// Stable seeded run identity.
    pub seed: u64,
    /// Node projections.
    pub nodes: Vec<RandomizedAlmostLinearMcfNodeState>,
    /// Edge projections.
    pub edges: Vec<RandomizedAlmostLinearMcfEdgeState>,
    /// Last coordinate assigned by the exact feasible-face enumeration.
    pub assignment_cursor: Option<EdgeId>,
    /// Source exponent `1/(1000 log(mU))`.
    pub alpha: RandomizedAlmostLinearMcfScalar,
    /// Lazy refresh threshold used by the bounded tree-chain replacement.
    pub epsilon: RandomizedAlmostLinearMcfScalar,
    /// Approximation-quality parameter inherited from the exact oracle.
    pub kappa: RandomizedAlmostLinearMcfScalar,
    /// Source step multiplier.
    pub eta: RandomizedAlmostLinearMcfScalar,
    /// Original objective at the relative-interior point.
    pub initial_cost: RandomizedAlmostLinearMcfScalar,
    /// Original objective at the current fractional point.
    pub current_cost: RandomizedAlmostLinearMcfScalar,
    /// Exact optimal original objective.
    pub optimum_cost: i128,
    /// Objective of the unique isolated optimum.
    pub isolated_optimum_cost: i128,
    /// Current alpha-power potential.
    pub potential: RandomizedAlmostLinearMcfScalar,
    /// Selected minimum-ratio value, when a non-stationary query exists.
    pub minimum_ratio: Option<RandomizedAlmostLinearMcfScalar>,
    /// Current isolation attempt, one-based after sampling.
    pub isolation_attempt: usize,
    /// Isolation scale `D = 4m^2U^2`.
    pub isolation_scale: u128,
    /// Source union-bound failure estimate after the current attempts.
    pub failure_probability_bound: RandomizedAlmostLinearMcfProbability,
    /// Number of forest representatives in the bounded pool.
    pub forest_pool_size: usize,
    /// Seeded sampled forest ordinal.
    pub sampled_forest_index: Option<usize>,
    /// Exact perturbed-objective gap of the published final point.
    pub final_point_gap: Option<BigRational>,
    /// Source threshold `1/(12m^3U^3)` for final-point recovery.
    pub final_point_threshold: BigRational,
    /// Weight assigned to the feasible-face barycenter.
    pub final_point_mix: Option<BigRational>,
    /// Whether nearest-integer recovery has completed.
    pub exact_recovery: bool,
    /// Exact counters.
    pub metrics: RandomizedAlmostLinearMcfMetrics,
}

/// One reversible boundary transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfTraceEvent {
    /// Stable catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: RandomizedAlmostLinearMcfSnapshot,
    /// State after the transition.
    pub after: RandomizedAlmostLinearMcfSnapshot,
}

/// Independently certified bounded result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfResult {
    /// Integral optimum flow.
    pub flows: Vec<u64>,
    /// Original total cost.
    pub total_cost: i128,
    /// Independent residual optimality certificate.
    pub certificate: MinCostFlowCertificate,
    /// Final replay state.
    pub final_snapshot: RandomizedAlmostLinearMcfSnapshot,
}

/// Full reversible trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMcfTraceResult {
    /// Initial valid state.
    pub base_snapshot: RandomizedAlmostLinearMcfSnapshot,
    /// Atomic transitions.
    pub events: Vec<RandomizedAlmostLinearMcfTraceEvent>,
    /// Final state.
    pub final_snapshot: RandomizedAlmostLinearMcfSnapshot,
    /// Certified result.
    pub result: RandomizedAlmostLinearMcfResult,
}

/// Admission, numerical, or independent-audit failure.
#[derive(Debug, Error)]
pub enum RandomizedAlmostLinearMcfError {
    /// Graph lies outside the deliberately small executable band.
    #[error("graph exceeds randomized almost-linear MCF visualization limits")]
    AdmissionLimit,
    /// Required divergence has invalid length or nonzero sum.
    #[error("required divergence is invalid")]
    InvalidDivergence,
    /// Exact bounded arithmetic overflowed.
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
    /// A source floating-point quantity became invalid.
    #[error("non-finite or invalid source quantity")]
    NumericalFailure,
    /// The exact source final-point or nearest-integer precondition failed.
    #[error("final-point recovery precondition failed")]
    FinalPointRounding,
    /// Isolation failed to obtain a unique optimum within the explicit cap.
    #[error("isolation attempts exhausted")]
    IsolationExhausted,
    /// Trace replay did not match seeded re-execution.
    #[error("trace verification failed")]
    TraceVerification,
    /// Feasible-flow construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Exact minimum-ratio primitive failed.
    #[error(transparent)]
    MinimumRatio(#[from] MinimumRatioCycleMcfError),
    /// Independent min-cost certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
}

#[derive(Clone)]
struct FeasibleFace {
    flows: Vec<Vec<u64>>,
    flow_sums: Vec<u128>,
    fixed: Vec<bool>,
    barycenter: Vec<f64>,
    optimum_cost: i128,
}

struct IsolationOutcome {
    draws: Vec<u64>,
    costs: Vec<i128>,
    isolated_flow: Vec<u64>,
    total_cost: i128,
    attempts: usize,
}

struct FinalPointOutcome {
    flows: Vec<BigRational>,
    gap: BigRational,
    mix: BigRational,
    rounded_flows: Vec<u64>,
}

struct SplitMix64(u64);

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

struct InternalRun {
    result: RandomizedAlmostLinearMcfResult,
    base_snapshot: RandomizedAlmostLinearMcfSnapshot,
    events: Vec<RandomizedAlmostLinearMcfTraceEvent>,
}

/// Solves within the bounded executable band using the stable default seed.
///
/// # Errors
///
/// Rejects out-of-band or infeasible inputs, exhausted isolation, invalid
/// numerical state, or a failed independent certificate.
pub fn solve_randomized_almost_linear_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RandomizedAlmostLinearMcfResult, RandomizedAlmostLinearMcfError> {
    solve_randomized_almost_linear_mcf_with_seed(
        graph,
        required_divergence,
        RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED,
    )
}

/// Solves the default-seed execution while reporting its feasibility precheck
/// to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_randomized_almost_linear_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RandomizedAlmostLinearMcfResult, RandomizedAlmostLinearMcfError> {
    run_internal_with_feasibility(
        graph,
        required_divergence,
        RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves within the bounded executable band using an explicit seed.
///
/// # Errors
///
/// Rejects out-of-band or infeasible inputs, exhausted isolation, invalid
/// numerical state, or a failed independent certificate.
pub fn solve_randomized_almost_linear_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<RandomizedAlmostLinearMcfResult, RandomizedAlmostLinearMcfError> {
    run_internal(graph, required_divergence, seed, false).map(|run| run.result)
}

/// Records the complete bounded run using the stable default seed.
///
/// # Errors
///
/// Returns any solve failure or a seeded replay mismatch.
pub fn trace_randomized_almost_linear_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RandomizedAlmostLinearMcfTraceResult, RandomizedAlmostLinearMcfError> {
    trace_randomized_almost_linear_mcf_with_seed(
        graph,
        required_divergence,
        RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED,
    )
}

/// Records the complete bounded run using an explicit seed.
///
/// # Errors
///
/// Returns any solve failure or a seeded replay mismatch.
pub fn trace_randomized_almost_linear_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<RandomizedAlmostLinearMcfTraceResult, RandomizedAlmostLinearMcfError> {
    let run = run_internal(graph, required_divergence, seed, true)?;
    let trace = RandomizedAlmostLinearMcfTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
    };
    check_randomized_almost_linear_mcf_trace(graph, required_divergence, seed, &trace)?;
    Ok(trace)
}

/// Records the default-seed execution while explicitly publishing any
/// feasibility precheck performed by the source run.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_randomized_almost_linear_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RandomizedAlmostLinearMcfTraceResult, RandomizedAlmostLinearMcfError> {
    let seed = RANDOMIZED_ALMOST_LINEAR_MCF_DEFAULT_SEED;
    let run = run_internal_with_feasibility(graph, required_divergence, seed, true, feasibility)?;
    let trace = RandomizedAlmostLinearMcfTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
    };
    check_randomized_almost_linear_mcf_trace(graph, required_divergence, seed, &trace)?;
    Ok(trace)
}

/// Checks a seeded trace using structural invariants and an independent certificate.
///
/// # Errors
///
/// Rejects any input, transition-link, result, or certificate mismatch.
pub fn check_randomized_almost_linear_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
    trace: &RandomizedAlmostLinearMcfTraceResult,
) -> Result<(), RandomizedAlmostLinearMcfError> {
    validate_admission(graph, required_divergence)?;
    if trace.base_snapshot != expected_randomized_mcf_base(graph, required_divergence, seed)?
        || trace.final_snapshot.stage != RandomizedAlmostLinearMcfStage::Optimal
        || trace.final_snapshot.seed != seed
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.flows.len() != graph.edges().len()
        || trace.events.is_empty()
        || trace.events.len() > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_TRACE_EVENTS
    {
        return Err(RandomizedAlmostLinearMcfError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != CATALOG_ID
            || &event.before != cursor
            || !valid_randomized_mcf_stage_transition(event.before.stage, event.after.stage)
            || event.after.seed != seed
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
        {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
        verify_snapshot_shape(graph, required_divergence, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot
        || trace.final_snapshot.metrics.certificate_checks == 0
        || !trace.final_snapshot.exact_recovery
    {
        return Err(RandomizedAlmostLinearMcfError::TraceVerification);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    let total_cost = graph
        .edges()
        .iter()
        .zip(&trace.result.flows)
        .try_fold(0_i128, |sum, (edge, &flow)| {
            sum.checked_add(i128::from(edge.cost()).checked_mul(i128::from(flow))?)
        })
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    if certificate != trace.result.certificate
        || total_cost != trace.result.total_cost
        || total_cost != trace.final_snapshot.optimum_cost
    {
        return Err(RandomizedAlmostLinearMcfError::TraceVerification);
    }
    for (edge_state, &flow) in trace.final_snapshot.edges.iter().zip(&trace.result.flows) {
        if edge_state.final_flow != Some(flow) {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
    }
    Ok(())
}

fn expected_randomized_mcf_base(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
) -> Result<RandomizedAlmostLinearMcfSnapshot, RandomizedAlmostLinearMcfError> {
    ready_snapshot(graph, required, seed, isolation_scale(graph)?)
}

#[allow(clippy::unnested_or_patterns)]
const fn valid_randomized_mcf_stage_transition(
    before: RandomizedAlmostLinearMcfStage,
    after: RandomizedAlmostLinearMcfStage,
) -> bool {
    matches!(
        (before, after),
        (
            RandomizedAlmostLinearMcfStage::Ready,
            RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment
        ) | (
            RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment,
            RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment
        ) | (
            RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment,
            RandomizedAlmostLinearMcfStage::EnumerateFeasibleSet
        ) | (
            RandomizedAlmostLinearMcfStage::EnumerateFeasibleSet,
            RandomizedAlmostLinearMcfStage::SampleIsolationCosts
        ) | (
            RandomizedAlmostLinearMcfStage::SampleIsolationCosts,
            RandomizedAlmostLinearMcfStage::SelectIsolatedOptimum
        ) | (
            RandomizedAlmostLinearMcfStage::SelectIsolatedOptimum,
            RandomizedAlmostLinearMcfStage::InitializeRelativeInterior
        ) | (
            RandomizedAlmostLinearMcfStage::InitializeRelativeInterior,
            RandomizedAlmostLinearMcfStage::InspectOracleVector
        ) | (
            RandomizedAlmostLinearMcfStage::InspectOracleVector,
            RandomizedAlmostLinearMcfStage::InspectOracleVector
        ) | (
            RandomizedAlmostLinearMcfStage::InspectOracleVector,
            RandomizedAlmostLinearMcfStage::BuildForestPool
        ) | (
            RandomizedAlmostLinearMcfStage::InitializeRelativeInterior,
            RandomizedAlmostLinearMcfStage::BuildForestPool
        ) | (
            RandomizedAlmostLinearMcfStage::BuildForestPool,
            RandomizedAlmostLinearMcfStage::SampleTreeChain
        ) | (
            RandomizedAlmostLinearMcfStage::SampleTreeChain,
            RandomizedAlmostLinearMcfStage::RefreshGradientLength
        ) | (
            RandomizedAlmostLinearMcfStage::RefreshGradientLength,
            RandomizedAlmostLinearMcfStage::QueryMinimumRatioCycle
        ) | (
            RandomizedAlmostLinearMcfStage::QueryMinimumRatioCycle,
            RandomizedAlmostLinearMcfStage::PotentialReductionStep
        ) | (
            RandomizedAlmostLinearMcfStage::PotentialReductionStep,
            RandomizedAlmostLinearMcfStage::DetectChangedCoordinates
        ) | (
            RandomizedAlmostLinearMcfStage::DetectChangedCoordinates,
            RandomizedAlmostLinearMcfStage::RebuildTreeChain
        ) | (
            RandomizedAlmostLinearMcfStage::RebuildTreeChain,
            RandomizedAlmostLinearMcfStage::ConstructFinalPoint
        ) | (
            RandomizedAlmostLinearMcfStage::ConstructFinalPoint,
            RandomizedAlmostLinearMcfStage::RoundNearestInteger
        ) | (
            RandomizedAlmostLinearMcfStage::RoundNearestInteger,
            RandomizedAlmostLinearMcfStage::CheckCertificate
        ) | (
            RandomizedAlmostLinearMcfStage::CheckCertificate,
            RandomizedAlmostLinearMcfStage::Optimal
        )
    )
}

fn verify_snapshot_shape(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &RandomizedAlmostLinearMcfSnapshot,
) -> Result<(), RandomizedAlmostLinearMcfError> {
    if snapshot.nodes.len() != graph.nodes().len() || snapshot.edges.len() != graph.edges().len() {
        return Err(RandomizedAlmostLinearMcfError::TraceVerification);
    }
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if node.node.as_usize() != index || node.required_divergence != required_divergence[index] {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
    }
    for (state, edge) in snapshot.edges.iter().zip(graph.edges()) {
        let isolated_optimum_visible = !matches!(
            snapshot.stage,
            RandomizedAlmostLinearMcfStage::Ready
                | RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment
                | RandomizedAlmostLinearMcfStage::EnumerateFeasibleSet
                | RandomizedAlmostLinearMcfStage::SampleIsolationCosts
        );
        if state.edge != *edge.id()
            || state.current_flow.get() < 0.0
            || state.current_flow.get() > edge.capacity() as f64
            || state.initial_flow.get() < 0.0
            || state.initial_flow.get() > edge.capacity() as f64
            || state.stale_flow.get() < 0.0
            || state.stale_flow.get() > edge.capacity() as f64
            || ![-1, 0, 1].contains(&state.candidate_sign)
            || ![-1, 0, 1].contains(&state.selected_sign)
            || (snapshot.stage != RandomizedAlmostLinearMcfStage::InspectOracleVector
                && state.candidate_sign != 0)
            || !state.gradient.get().is_finite()
            || !state.length.get().is_finite()
            || state.length.get() < 0.0
            || isolated_optimum_visible != state.isolated_optimum_flow.is_some()
            || state
                .isolated_optimum_flow
                .is_some_and(|flow| flow > edge.capacity())
        {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
        if let Some(flow) = state.final_flow
            && flow > edge.capacity()
        {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
    }
    if snapshot.stage == RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment {
        if snapshot
            .assignment_cursor
            .as_ref()
            .is_none_or(|cursor| graph.edges().iter().all(|edge| edge.id() != cursor))
        {
            return Err(RandomizedAlmostLinearMcfError::TraceVerification);
        }
    } else if snapshot.assignment_cursor.is_some() {
        return Err(RandomizedAlmostLinearMcfError::TraceVerification);
    }
    Ok(())
}

// This is the single orchestration ledger: keeping the source phases adjacent
// makes their published order auditable, while numerical work remains in helpers.
#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
    record_events: bool,
) -> Result<InternalRun, RandomizedAlmostLinearMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(graph, required, seed, record_events, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, RandomizedAlmostLinearMcfError> {
    validate_admission(graph, required)?;
    let scale = isolation_scale(graph)?;
    let mut snapshot = ready_snapshot(graph, required, seed, scale)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    let mut metrics = RandomizedAlmostLinearMcfMetrics::default();
    let assignment_cursor = graph
        .edges()
        .last()
        .map(|edge| edge.id().clone())
        .ok_or(RandomizedAlmostLinearMcfError::AdmissionLimit)?;
    let mut face = enumerate_feasible_face(
        graph,
        required,
        &mut metrics,
        feasibility,
        &mut |assignment, observed_metrics| {
            if !observed_metrics.enumerated_assignments.is_power_of_two() {
                return Ok(());
            }
            let visible_flows = assignment
                .iter()
                .map(|&flow| RandomizedAlmostLinearMcfScalar::try_new(flow as f64))
                .collect::<Result<Vec<_>, _>>()?;
            transition(&mut snapshot, &mut events, record_events, |state| {
                state.stage = RandomizedAlmostLinearMcfStage::InspectFeasibleAssignment;
                state.assignment_cursor = Some(assignment_cursor.clone());
                state.metrics = observed_metrics;
                for (edge, flow) in state.edges.iter_mut().zip(visible_flows) {
                    edge.current_flow = flow;
                }
            })
        },
    )?;
    let active_edges = face.fixed.iter().filter(|&&fixed| !fixed).count().max(1);
    let u = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(2);
    let alpha = 1.0 / (1_000.0 * (active_edges as f64 * u as f64).max(2.0).ln());
    let epsilon = alpha / DETECT_DENOMINATOR;
    let initial_cost = fractional_cost(graph, &face.barycenter)?;
    let alpha_scalar = RandomizedAlmostLinearMcfScalar::try_new(alpha)?;
    let epsilon_scalar = RandomizedAlmostLinearMcfScalar::try_new(epsilon)?;
    let initial_cost_scalar = RandomizedAlmostLinearMcfScalar::try_new(initial_cost)?;
    let barycenter = face
        .barycenter
        .iter()
        .map(|&flow| RandomizedAlmostLinearMcfScalar::try_new(flow))
        .collect::<Result<Vec<_>, _>>()?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::EnumerateFeasibleSet;
        state.assignment_cursor = None;
        state.alpha = alpha_scalar;
        state.epsilon = epsilon_scalar;
        state.initial_cost = initial_cost_scalar;
        state.current_cost = state.initial_cost;
        state.optimum_cost = face.optimum_cost;
        state.metrics = metrics;
        for (index, (edge, flow)) in state.edges.iter_mut().zip(barycenter).enumerate() {
            edge.fixed_on_face = face.fixed[index];
            edge.initial_flow = flow;
            edge.current_flow = flow;
            edge.stale_flow = flow;
        }
    })?;

    let mut rng = SplitMix64::new(seed);
    let mut isolation = isolate_optimum(graph, &face, scale, &mut rng, &mut metrics)?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::SampleIsolationCosts;
        state.isolation_attempt = isolation.attempts;
        state.failure_probability_bound = failure_bound(isolation.attempts);
        state.metrics = metrics;
        for (edge, (&draw, &cost)) in state
            .edges
            .iter_mut()
            .zip(isolation.draws.iter().zip(&isolation.costs))
        {
            edge.isolation_draw = draw;
            edge.isolated_cost = cost;
        }
    })?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::SelectIsolatedOptimum;
        state.isolated_optimum_cost = isolation.total_cost;
        for (edge, &flow) in state.edges.iter_mut().zip(&isolation.isolated_flow) {
            edge.isolated_optimum_flow = Some(flow);
        }
    })?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::InitializeRelativeInterior;
    })?;

    let primitive = trace_minimum_ratio_cycle_mcf(graph, required)?;
    for primitive_event in &primitive.events {
        let enumerated = primitive_event.after.metrics.enumerated_vectors;
        if enumerated <= metrics.oracle_vector_evaluations {
            continue;
        }
        metrics.oracle_vector_evaluations = enumerated;
        let mut projection = snapshot.clone();
        install_primitive_projection(&mut projection, &primitive_event.after)?;
        projection.stage = RandomizedAlmostLinearMcfStage::InspectOracleVector;
        projection.metrics = metrics;
        transition(&mut snapshot, &mut events, record_events, move |state| {
            *state = projection;
        })?;
    }
    metrics.oracle_vector_evaluations = primitive.result.metrics.enumerated_vectors;
    let mut primitive_projection = snapshot.clone();
    install_primitive_projection(&mut primitive_projection, &primitive.final_snapshot)?;
    let pool_size = bounded_forest_pool_size(&primitive.final_snapshot);
    metrics.forest_pool_size =
        u64::try_from(pool_size).map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::BuildForestPool;
        copy_tree_projection(state, &primitive_projection);
        for edge in &mut state.edges {
            edge.candidate_sign = 0;
        }
        state.forest_pool_size = pool_size;
        state.metrics = metrics;
    })?;
    let sampled = usize::try_from(rng.next() % pool_size.max(1) as u64)
        .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    metrics.random_draws = metrics.random_draws.saturating_add(1);
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::SampleTreeChain;
        state.sampled_forest_index = Some(sampled);
        state.metrics = metrics;
    })?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::RefreshGradientLength;
        copy_gradient_projection(state, &primitive_projection);
    })?;
    metrics.ratio_queries = 1;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::QueryMinimumRatioCycle;
        copy_cycle_projection(state, &primitive_projection);
        state.metrics = metrics;
    })?;
    if primitive.final_snapshot.selected_edge_count > 0 {
        metrics.source_steps = 1;
    }
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::PotentialReductionStep;
        copy_step_projection(state, &primitive_projection);
        state.metrics = metrics;
    })?;

    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::DetectChangedCoordinates;
        detect_coordinates(state, epsilon, &mut metrics);
        state.metrics = metrics;
    })?;
    metrics.rebuilds = 1;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::RebuildTreeChain;
        state.metrics = metrics;
    })?;

    let final_point = construct_final_point(graph, required, &face, &isolation, scale)?;
    // The exhaustive vectors are a bounded final-point oracle only. Once the
    // exact source precondition has been materialized, no integral winner is
    // retained for terminal publication.
    face.flows.clear();
    isolation.isolated_flow.clear();
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::ConstructFinalPoint;
        state.final_point_gap = Some(final_point.gap.clone());
        state.final_point_mix = Some(final_point.mix.clone());
        for (edge, flow) in state.edges.iter_mut().zip(&final_point.flows) {
            edge.final_point_flow = Some(flow.clone());
        }
    })?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::RoundNearestInteger;
        state.exact_recovery = true;
        for (edge, &flow) in state.edges.iter_mut().zip(&final_point.rounded_flows) {
            edge.final_flow = Some(flow);
        }
    })?;
    let certificate = check_min_cost_flow(graph, required, &final_point.rounded_flows)?;
    if certificate.total_cost != face.optimum_cost {
        return Err(RandomizedAlmostLinearMcfError::NumericalFailure);
    }
    metrics.certificate_checks = 1;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::CheckCertificate;
        state.metrics = metrics;
    })?;
    transition(&mut snapshot, &mut events, record_events, |state| {
        state.stage = RandomizedAlmostLinearMcfStage::Optimal;
    })?;
    let result = RandomizedAlmostLinearMcfResult {
        flows: final_point.rounded_flows,
        total_cost: certificate.total_cost,
        certificate,
        final_snapshot: snapshot,
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
    })
}

fn transition(
    snapshot: &mut RandomizedAlmostLinearMcfSnapshot,
    events: &mut Vec<RandomizedAlmostLinearMcfTraceEvent>,
    record: bool,
    update: impl FnOnce(&mut RandomizedAlmostLinearMcfSnapshot),
) -> Result<(), RandomizedAlmostLinearMcfError> {
    let before = snapshot.clone();
    let transition_count = snapshot
        .metrics
        .state_transitions
        .checked_add(1)
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    update(snapshot);
    snapshot.metrics.state_transitions = transition_count;
    if record {
        if events.len() >= RANDOMIZED_ALMOST_LINEAR_MCF_MAX_TRACE_EVENTS {
            return Err(RandomizedAlmostLinearMcfError::AdmissionLimit);
        }
        events.push(RandomizedAlmostLinearMcfTraceEvent {
            catalog_id: CATALOG_ID,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(())
}

fn validate_admission(
    graph: &FlowNetwork,
    required: &[i128],
) -> Result<(), RandomizedAlmostLinearMcfError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_EDGES
        || graph.edges().iter().any(|edge| {
            edge.capacity() > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_CAPACITY
                || edge.cost().unsigned_abs() > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_COST
        })
    {
        return Err(RandomizedAlmostLinearMcfError::AdmissionLimit);
    }
    if required.len() != graph.nodes().len()
        || required
            .iter()
            .try_fold(0_i128, |sum, &value| sum.checked_add(value))
            != Some(0)
    {
        return Err(RandomizedAlmostLinearMcfError::InvalidDivergence);
    }
    let assignments = graph.edges().iter().try_fold(1_u64, |count, edge| {
        count.checked_mul(edge.capacity() - edge.lower() + 1)
    });
    if assignments.is_none_or(|count| count > RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ASSIGNMENTS) {
        return Err(RandomizedAlmostLinearMcfError::AdmissionLimit);
    }
    Ok(())
}

fn enumerate_feasible_face(
    graph: &FlowNetwork,
    required: &[i128],
    metrics: &mut RandomizedAlmostLinearMcfMetrics,
    feasibility: &mut FeasibilityExecution,
    observe: &mut impl FnMut(
        &[u64],
        RandomizedAlmostLinearMcfMetrics,
    ) -> Result<(), RandomizedAlmostLinearMcfError>,
) -> Result<FeasibleFace, RandomizedAlmostLinearMcfError> {
    let mut flows = Vec::new();
    let mut current = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    enumerate_coordinate(
        graph,
        required,
        0,
        &mut current,
        &mut flows,
        metrics,
        observe,
    )?;
    if flows.is_empty() {
        feasibility.find_feasible_flow(graph, required, FeasibilityUse::PrecheckOnly)?;
        return Err(RandomizedAlmostLinearMcfError::NumericalFailure);
    }
    metrics.feasible_flows = u64::try_from(flows.len())
        .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    let first = &flows[0];
    let fixed = (0..graph.edges().len())
        .map(|index| flows.iter().all(|flow| flow[index] == first[index]))
        .collect::<Vec<_>>();
    let count = flows.len() as f64;
    let flow_sums = (0..graph.edges().len())
        .map(|index| {
            flows.iter().try_fold(0_u128, |sum, flow| {
                sum.checked_add(u128::from(flow[index]))
                    .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let barycenter = (0..graph.edges().len())
        .map(|index| flow_sums[index] as f64 / count)
        .collect::<Vec<_>>();
    let optimum_cost = flows
        .iter()
        .map(|flow| exact_cost(graph, flow))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(RandomizedAlmostLinearMcfError::NumericalFailure)?;
    Ok(FeasibleFace {
        flows,
        flow_sums,
        fixed,
        barycenter,
        optimum_cost,
    })
}

fn enumerate_coordinate(
    graph: &FlowNetwork,
    required: &[i128],
    index: usize,
    current: &mut [u64],
    feasible: &mut Vec<Vec<u64>>,
    metrics: &mut RandomizedAlmostLinearMcfMetrics,
    observe: &mut impl FnMut(
        &[u64],
        RandomizedAlmostLinearMcfMetrics,
    ) -> Result<(), RandomizedAlmostLinearMcfError>,
) -> Result<(), RandomizedAlmostLinearMcfError> {
    if index == graph.edges().len() {
        metrics.enumerated_assignments = metrics
            .enumerated_assignments
            .checked_add(1)
            .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
        observe(current, *metrics)?;
        if divergences(graph, current)? == required {
            feasible.push(current.to_vec());
        }
        return Ok(());
    }
    let edge = &graph.edges()[index];
    for value in edge.lower()..=edge.capacity() {
        current[index] = value;
        enumerate_coordinate(
            graph,
            required,
            index + 1,
            current,
            feasible,
            metrics,
            observe,
        )?;
    }
    Ok(())
}

fn isolation_scale(graph: &FlowNetwork) -> Result<u128, RandomizedAlmostLinearMcfError> {
    let m = graph.edges().len() as u128;
    let u = u128::from(
        graph
            .edges()
            .iter()
            .map(FlowEdge::capacity)
            .max()
            .unwrap_or(1)
            .max(1),
    );
    4_u128
        .checked_mul(m)
        .and_then(|value| value.checked_mul(m))
        .and_then(|value| value.checked_mul(u))
        .and_then(|value| value.checked_mul(u))
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
}

fn isolate_optimum(
    graph: &FlowNetwork,
    face: &FeasibleFace,
    scale: u128,
    rng: &mut SplitMix64,
    metrics: &mut RandomizedAlmostLinearMcfMetrics,
) -> Result<IsolationOutcome, RandomizedAlmostLinearMcfError> {
    let m = graph.edges().len() as u64;
    let u = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(1);
    let draw_limit = 2_u64
        .checked_mul(m)
        .and_then(|value| value.checked_mul(u))
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    for attempt in 1..=RANDOMIZED_ALMOST_LINEAR_MCF_MAX_ISOLATION_ATTEMPTS {
        let draws = (0..graph.edges().len())
            .map(|_| 1 + rng.next() % draw_limit)
            .collect::<Vec<_>>();
        metrics.random_draws = metrics.random_draws.saturating_add(m);
        metrics.isolation_attempts = u64::try_from(attempt)
            .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
        let costs = graph
            .edges()
            .iter()
            .zip(&draws)
            .map(|(edge, &draw)| {
                i128::try_from(scale)
                    .ok()
                    .and_then(|factor| factor.checked_mul(i128::from(edge.cost())))
                    .and_then(|value| value.checked_add(i128::from(draw)))
                    .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let totals = face
            .flows
            .iter()
            .map(|flow| dot_cost(&costs, flow))
            .collect::<Result<Vec<_>, _>>()?;
        let minimum = totals
            .iter()
            .copied()
            .min()
            .ok_or(RandomizedAlmostLinearMcfError::NumericalFailure)?;
        let winners = totals
            .iter()
            .enumerate()
            .filter(|(_, total)| **total == minimum)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if winners.len() == 1 {
            let winner = face.flows[winners[0]].clone();
            if exact_cost(graph, &winner)? != face.optimum_cost {
                return Err(RandomizedAlmostLinearMcfError::NumericalFailure);
            }
            return Ok(IsolationOutcome {
                draws,
                costs,
                isolated_flow: winner,
                total_cost: minimum,
                attempts: attempt,
            });
        }
    }
    Err(RandomizedAlmostLinearMcfError::IsolationExhausted)
}

fn source_final_point_threshold_denominator(
    graph: &FlowNetwork,
) -> Result<u128, RandomizedAlmostLinearMcfError> {
    let m = u128::try_from(graph.edges().len())
        .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    let u = u128::from(
        graph
            .edges()
            .iter()
            .map(FlowEdge::capacity)
            .max()
            .unwrap_or(1)
            .max(1),
    );
    12_u128
        .checked_mul(m)
        .and_then(|value| value.checked_mul(m))
        .and_then(|value| value.checked_mul(m))
        .and_then(|value| value.checked_mul(u))
        .and_then(|value| value.checked_mul(u))
        .and_then(|value| value.checked_mul(u))
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
}

fn source_final_point_threshold(
    graph: &FlowNetwork,
) -> Result<BigRational, RandomizedAlmostLinearMcfError> {
    Ok(BigRational::new(
        BigInt::one(),
        BigInt::from(source_final_point_threshold_denominator(graph)?),
    ))
}

// Chen et al.'s final-point lemma applies to a feasible point whose perturbed
// objective is within 1/(12m^3U^3) of the isolated optimum. The bounded face
// oracle materializes such a point as (1-1/K)w + (1/K)b, where w is the
// isolated optimum and b is the exact feasible-face barycenter. The terminal
// flow is then produced only by nearest-integer rounding of this public point.
#[allow(clippy::too_many_lines)]
fn construct_final_point(
    graph: &FlowNetwork,
    required: &[i128],
    face: &FeasibleFace,
    isolation: &IsolationOutcome,
    scale: u128,
) -> Result<FinalPointOutcome, RandomizedAlmostLinearMcfError> {
    let face_size = u128::try_from(face.flows.len())
        .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    if face_size == 0 || face.flow_sums.len() != graph.edges().len() {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    let objective_delta_sum = face.flows.iter().try_fold(
        0_u128,
        |sum, flow| -> Result<u128, RandomizedAlmostLinearMcfError> {
            let objective = dot_cost(&isolation.costs, flow)?;
            let delta = objective
                .checked_sub(isolation.total_cost)
                .filter(|value| *value >= 0)
                .ok_or(RandomizedAlmostLinearMcfError::FinalPointRounding)?;
            sum.checked_add(
                u128::try_from(delta)
                    .map_err(|_| RandomizedAlmostLinearMcfError::ArithmeticOverflow)?,
            )
            .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
        },
    )?;
    let coordinate_deltas = face
        .flow_sums
        .iter()
        .zip(&isolation.isolated_flow)
        .map(|(&sum, &flow)| {
            let isolated_sum = u128::from(flow)
                .checked_mul(face_size)
                .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
            Ok::<u128, RandomizedAlmostLinearMcfError>(sum.abs_diff(isolated_sum))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let threshold_denominator = source_final_point_threshold_denominator(graph)?;
    let mut mix_denominator = 4_u128;
    loop {
        let accuracy_left = BigInt::from(objective_delta_sum) * BigInt::from(threshold_denominator);
        let accuracy_right =
            BigInt::from(face_size) * BigInt::from(mix_denominator) * BigInt::from(scale);
        let rounding_safe = coordinate_deltas.iter().all(|&delta| {
            BigInt::from(4_u8) * BigInt::from(delta)
                < BigInt::from(face_size) * BigInt::from(mix_denominator)
        });
        if accuracy_left <= accuracy_right && rounding_safe {
            break;
        }
        mix_denominator = mix_denominator
            .checked_mul(2)
            .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)?;
    }
    let point_denominator = BigInt::from(face_size) * BigInt::from(mix_denominator);
    let point_numerators = face
        .flow_sums
        .iter()
        .zip(&isolation.isolated_flow)
        .map(|(&sum, &flow)| {
            BigInt::from(flow) * &point_denominator + BigInt::from(sum)
                - BigInt::from(flow) * BigInt::from(face_size)
        })
        .collect::<Vec<_>>();
    verify_final_point_feasibility(graph, required, &point_numerators, &point_denominator)?;
    let flows = point_numerators
        .iter()
        .map(|numerator| BigRational::new(numerator.clone(), point_denominator.clone()))
        .collect::<Vec<_>>();
    let gap = BigRational::new(
        BigInt::from(objective_delta_sum),
        BigInt::from(face_size) * BigInt::from(mix_denominator) * BigInt::from(scale),
    );
    let threshold = source_final_point_threshold(graph)?;
    let mix = BigRational::new(BigInt::one(), BigInt::from(mix_denominator));
    if gap < BigRational::zero() || gap > threshold {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    let scaled_point_cost = isolation
        .costs
        .iter()
        .zip(&flows)
        .fold(BigRational::zero(), |sum, (&cost, flow)| {
            sum + BigRational::from_integer(BigInt::from(cost)) * flow
        });
    let measured_gap = (scaled_point_cost
        - BigRational::from_integer(BigInt::from(isolation.total_cost)))
        / BigRational::from_integer(BigInt::from(scale));
    if measured_gap != gap {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    let rounded_flows = flows
        .iter()
        .map(round_nonnegative_rational)
        .collect::<Result<Vec<_>, _>>()?;
    if rounded_flows != isolation.isolated_flow {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    Ok(FinalPointOutcome {
        flows,
        gap,
        mix,
        rounded_flows,
    })
}

fn verify_final_point_feasibility(
    graph: &FlowNetwork,
    required: &[i128],
    numerators: &[BigInt],
    denominator: &BigInt,
) -> Result<(), RandomizedAlmostLinearMcfError> {
    if numerators.len() != graph.edges().len() || denominator <= &BigInt::zero() {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    let mut divergence = vec![BigInt::zero(); graph.nodes().len()];
    for (edge, numerator) in graph.edges().iter().zip(numerators) {
        if numerator < &(BigInt::from(edge.lower()) * denominator)
            || numerator > &(BigInt::from(edge.capacity()) * denominator)
        {
            return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
        }
        divergence[edge.from().as_usize()] += numerator;
        divergence[edge.to().as_usize()] -= numerator;
    }
    if divergence
        .iter()
        .zip(required)
        .any(|(actual, &expected)| actual != &(BigInt::from(expected) * denominator))
    {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    Ok(())
}

fn round_nonnegative_rational(value: &BigRational) -> Result<u64, RandomizedAlmostLinearMcfError> {
    if value < &BigRational::zero() {
        return Err(RandomizedAlmostLinearMcfError::FinalPointRounding);
    }
    let quotient = value.numer() / value.denom();
    let remainder = value.numer() % value.denom();
    let rounded = if remainder * BigInt::from(2_u8) >= *value.denom() {
        quotient + BigInt::one()
    } else {
        quotient
    };
    rounded
        .to_u64()
        .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
}

fn ready_snapshot(
    graph: &FlowNetwork,
    required: &[i128],
    seed: u64,
    scale: u128,
) -> Result<RandomizedAlmostLinearMcfSnapshot, RandomizedAlmostLinearMcfError> {
    let capacity_scale = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(2);
    let alpha = 1.0
        / (1_000.0
            * (graph.edges().len().max(1) as f64 * capacity_scale as f64)
                .max(2.0)
                .ln());
    let lower_point = graph
        .edges()
        .iter()
        .map(|edge| edge.lower() as f64)
        .collect::<Vec<_>>();
    let initial_cost = fractional_cost(graph, &lower_point)?;
    let nodes = graph
        .node_indices()
        .map(|node| RandomizedAlmostLinearMcfNodeState {
            node,
            required_divergence: required[node.as_usize()],
            component: node.as_usize(),
            parent: None,
            depth: 0,
            on_selected_cycle: false,
        })
        .collect();
    let edges = graph
        .edges()
        .iter()
        .map(|edge| {
            let flow = RandomizedAlmostLinearMcfScalar::try_new(edge.lower() as f64)?;
            Ok(RandomizedAlmostLinearMcfEdgeState {
                edge: edge.id().clone(),
                fixed_on_face: false,
                initial_flow: flow,
                current_flow: flow,
                stale_flow: flow,
                final_point_flow: None,
                final_flow: None,
                isolation_draw: 0,
                isolated_cost: 0,
                isolated_optimum_flow: None,
                tree_edge: false,
                candidate_sign: 0,
                selected_sign: 0,
                gradient: RandomizedAlmostLinearMcfScalar::try_new(0.0)?,
                length: RandomizedAlmostLinearMcfScalar::try_new(0.0)?,
                detected: false,
            })
        })
        .collect::<Result<Vec<_>, RandomizedAlmostLinearMcfError>>()?;
    Ok(RandomizedAlmostLinearMcfSnapshot {
        stage: RandomizedAlmostLinearMcfStage::Ready,
        seed,
        nodes,
        edges,
        assignment_cursor: None,
        alpha: RandomizedAlmostLinearMcfScalar::try_new(alpha)?,
        epsilon: RandomizedAlmostLinearMcfScalar::try_new(alpha / DETECT_DENOMINATOR)?,
        kappa: RandomizedAlmostLinearMcfScalar::try_new(0.0)?,
        eta: RandomizedAlmostLinearMcfScalar::try_new(0.0)?,
        initial_cost: RandomizedAlmostLinearMcfScalar::try_new(initial_cost)?,
        current_cost: RandomizedAlmostLinearMcfScalar::try_new(initial_cost)?,
        optimum_cost: 0,
        isolated_optimum_cost: 0,
        potential: RandomizedAlmostLinearMcfScalar::try_new(0.0)?,
        minimum_ratio: None,
        isolation_attempt: 0,
        isolation_scale: scale,
        failure_probability_bound: RandomizedAlmostLinearMcfProbability {
            numerator: 1,
            denominator: 1,
        },
        forest_pool_size: 0,
        sampled_forest_index: None,
        final_point_gap: None,
        final_point_threshold: source_final_point_threshold(graph)?,
        final_point_mix: None,
        exact_recovery: false,
        metrics: RandomizedAlmostLinearMcfMetrics::default(),
    })
}

fn install_primitive_projection(
    state: &mut RandomizedAlmostLinearMcfSnapshot,
    primitive: &MinimumRatioCycleMcfSnapshot,
) -> Result<(), RandomizedAlmostLinearMcfError> {
    for (node, source) in state.nodes.iter_mut().zip(&primitive.nodes) {
        node.component = source.component;
        node.parent = source.parent;
        node.depth = source.depth;
        node.on_selected_cycle = source.on_selected;
    }
    for (edge, source) in state.edges.iter_mut().zip(&primitive.edges) {
        edge.tree_edge = source.tree_edge;
        edge.candidate_sign = source.candidate_sign;
        edge.selected_sign = source.selected_sign;
        edge.gradient = RandomizedAlmostLinearMcfScalar::try_new(source.gradient.get())?;
        edge.length = RandomizedAlmostLinearMcfScalar::try_new(source.length.get())?;
        edge.current_flow = RandomizedAlmostLinearMcfScalar::try_new(source.updated_flow.get())?;
    }
    state.current_cost = RandomizedAlmostLinearMcfScalar::try_new(primitive.current_cost.get())?;
    state.potential = RandomizedAlmostLinearMcfScalar::try_new(primitive.current_potential.get())?;
    state.minimum_ratio = primitive
        .best_ratio
        .map(|value| RandomizedAlmostLinearMcfScalar::try_new(value.get()))
        .transpose()?;
    state.kappa = RandomizedAlmostLinearMcfScalar::try_new(primitive.kappa.get())?;
    state.eta = RandomizedAlmostLinearMcfScalar::try_new(primitive.eta.get())?;
    Ok(())
}

fn copy_gradient_projection(
    state: &mut RandomizedAlmostLinearMcfSnapshot,
    projection: &RandomizedAlmostLinearMcfSnapshot,
) {
    copy_tree_projection(state, projection);
    for (edge, projected) in state.edges.iter_mut().zip(&projection.edges) {
        edge.gradient = projected.gradient;
        edge.length = projected.length;
    }
}

fn copy_cycle_projection(
    state: &mut RandomizedAlmostLinearMcfSnapshot,
    projection: &RandomizedAlmostLinearMcfSnapshot,
) {
    for (node, projected) in state.nodes.iter_mut().zip(&projection.nodes) {
        node.on_selected_cycle = projected.on_selected_cycle;
    }
    for (edge, projected) in state.edges.iter_mut().zip(&projection.edges) {
        edge.selected_sign = projected.selected_sign;
    }
    state.minimum_ratio = projection.minimum_ratio;
    state.kappa = projection.kappa;
}

fn copy_step_projection(
    state: &mut RandomizedAlmostLinearMcfSnapshot,
    projection: &RandomizedAlmostLinearMcfSnapshot,
) {
    for (edge, projected) in state.edges.iter_mut().zip(&projection.edges) {
        edge.current_flow = projected.current_flow;
    }
    state.current_cost = projection.current_cost;
    state.potential = projection.potential;
    state.eta = projection.eta;
}

fn copy_tree_projection(
    state: &mut RandomizedAlmostLinearMcfSnapshot,
    projection: &RandomizedAlmostLinearMcfSnapshot,
) {
    for (node, projected) in state.nodes.iter_mut().zip(&projection.nodes) {
        node.component = projected.component;
        node.parent = projected.parent;
        node.depth = projected.depth;
    }
    for (edge, projected) in state.edges.iter_mut().zip(&projection.edges) {
        edge.tree_edge = projected.tree_edge;
    }
}

fn detect_coordinates(
    snapshot: &mut RandomizedAlmostLinearMcfSnapshot,
    epsilon: f64,
    metrics: &mut RandomizedAlmostLinearMcfMetrics,
) {
    for edge in &mut snapshot.edges {
        metrics.detect_scans = metrics.detect_scans.saturating_add(1);
        let movement = (edge.current_flow.get() - edge.stale_flow.get()).abs();
        edge.detected = edge.length.get() * movement >= epsilon;
        if edge.detected {
            edge.stale_flow = edge.current_flow;
            metrics.detected_coordinates = metrics.detected_coordinates.saturating_add(1);
        }
    }
}

fn bounded_forest_pool_size(primitive: &MinimumRatioCycleMcfSnapshot) -> usize {
    primitive
        .metrics
        .fundamental_cycles
        .saturating_add(1)
        .min(64) as usize
}

fn failure_bound(attempts: usize) -> RandomizedAlmostLinearMcfProbability {
    RandomizedAlmostLinearMcfProbability {
        numerator: 1,
        denominator: 1_u64
            .checked_shl(u32::try_from(attempts).unwrap_or(63))
            .unwrap_or(u64::MAX),
    }
}

fn exact_cost(graph: &FlowNetwork, flow: &[u64]) -> Result<i128, RandomizedAlmostLinearMcfError> {
    graph
        .edges()
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (edge, &value)| {
            i128::from(value)
                .checked_mul(i128::from(edge.cost()))
                .and_then(|term| sum.checked_add(term))
                .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
        })
}

fn dot_cost(costs: &[i128], flow: &[u64]) -> Result<i128, RandomizedAlmostLinearMcfError> {
    costs
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (&cost, &value)| {
            cost.checked_mul(i128::from(value))
                .and_then(|term| sum.checked_add(term))
                .ok_or(RandomizedAlmostLinearMcfError::ArithmeticOverflow)
        })
}

fn fractional_cost(
    graph: &FlowNetwork,
    flow: &[f64],
) -> Result<f64, RandomizedAlmostLinearMcfError> {
    RandomizedAlmostLinearMcfScalar::try_new(
        graph
            .edges()
            .iter()
            .zip(flow)
            .map(|(edge, &value)| edge.cost() as f64 * value)
            .sum::<f64>(),
    )
    .map(RandomizedAlmostLinearMcfScalar::get)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph() -> FlowNetwork {
        let nodes = ["s", "a", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edges = [
            ("sa", "s", "a", 0, 2, 0),
            ("at", "a", "t", 0, 2, 1),
            ("st", "s", "t", 0, 2, 3),
            ("as", "a", "s", 0, 1, 2),
        ]
        .into_iter()
        .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower,
            capacity,
            cost,
        })
        .collect();
        FlowNetwork::new(nodes, edges).expect("graph")
    }

    #[test]
    fn seeded_trace_is_replayable_and_certified() {
        let graph = graph();
        let required = [2, 0, -2];
        let trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 7).expect("trace");
        assert_eq!(trace.result.total_cost, 2);
        assert_eq!(
            trace.final_snapshot.stage,
            RandomizedAlmostLinearMcfStage::Optimal
        );
        assert!(trace.final_snapshot.exact_recovery);
        assert!(
            trace
                .final_snapshot
                .final_point_gap
                .as_ref()
                .is_some_and(|gap| gap >= &BigRational::zero()
                    && gap <= &trace.final_snapshot.final_point_threshold)
        );
        check_randomized_almost_linear_mcf_trace(&graph, &required, 7, &trace).expect("replay");
    }

    #[test]
    fn published_final_point_precedes_and_determines_nearest_integer_rounding() {
        let graph = graph();
        let required = [2, 0, -2];
        let trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 7).expect("trace");
        let point = trace
            .events
            .iter()
            .find(|event| event.after.stage == RandomizedAlmostLinearMcfStage::ConstructFinalPoint)
            .map(|event| &event.after)
            .expect("final point boundary");
        assert!(!point.exact_recovery);
        assert!(point.edges.iter().all(|edge| {
            edge.final_point_flow.is_some()
                && edge.final_flow.is_none()
                && edge.final_point_flow.as_ref().is_some_and(|flow| {
                    let rounded = round_nonnegative_rational(flow).expect("round");
                    let distance = (flow - BigRational::from_integer(BigInt::from(rounded))).abs();
                    distance < BigRational::new(BigInt::one(), BigInt::from(4_u8))
                })
        }));
        let rounded = trace
            .events
            .iter()
            .find(|event| event.after.stage == RandomizedAlmostLinearMcfStage::RoundNearestInteger)
            .map(|event| &event.after)
            .expect("rounding boundary");
        assert!(rounded.exact_recovery);
        assert_eq!(
            rounded
                .edges
                .iter()
                .map(|edge| edge.final_flow.expect("rounded flow"))
                .collect::<Vec<_>>(),
            trace.result.flows
        );
    }

    #[test]
    fn ratio_oracle_checkpoints_publish_real_vectors_at_geometric_density() {
        let graph = graph();
        let required = [2, 0, -2];
        let trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 7).expect("trace");
        let checkpoints = trace
            .events
            .iter()
            .filter(|event| {
                event.after.stage == RandomizedAlmostLinearMcfStage::InspectOracleVector
            })
            .collect::<Vec<_>>();
        let total = trace.final_snapshot.metrics.oracle_vector_evaluations;
        let minimum = u64::from(u64::BITS - total.leading_zeros());
        assert!(
            u64::try_from(checkpoints.len()).is_ok_and(|count| count >= minimum),
            "{total} oracle evaluations need at least {minimum} visible checkpoints"
        );
        assert!(checkpoints.windows(2).all(|pair| {
            pair[0].after.metrics.oracle_vector_evaluations
                < pair[1].after.metrics.oracle_vector_evaluations
        }));
        assert!(checkpoints.iter().all(|event| {
            event
                .after
                .edges
                .iter()
                .any(|edge| edge.candidate_sign != 0 || edge.selected_sign != 0)
                && event.before != event.after
        }));
        assert!(trace.events.iter().all(|event| {
            event.after.stage == RandomizedAlmostLinearMcfStage::InspectOracleVector
                || event
                    .after
                    .edges
                    .iter()
                    .all(|edge| edge.candidate_sign == 0)
        }));
    }

    #[test]
    fn seed_changes_isolation_draws_without_changing_optimum() {
        let graph = graph();
        let required = [2, 0, -2];
        let first =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 1).expect("first");
        let second =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 2).expect("second");
        assert_ne!(
            first
                .final_snapshot
                .edges
                .iter()
                .map(|edge| edge.isolation_draw)
                .collect::<Vec<_>>(),
            second
                .final_snapshot
                .edges
                .iter()
                .map(|edge| edge.isolation_draw)
                .collect::<Vec<_>>()
        );
        assert_eq!(first.result.total_cost, second.result.total_cost);
    }

    #[test]
    fn tampered_trace_is_rejected() {
        let graph = graph();
        let required = [2, 0, -2];
        let mut trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 9).expect("trace");
        trace.final_snapshot.isolation_attempt += 1;
        assert!(matches!(
            check_randomized_almost_linear_mcf_trace(&graph, &required, 9, &trace),
            Err(RandomizedAlmostLinearMcfError::TraceVerification)
        ));
        let mut stage_trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 9).expect("trace");
        stage_trace.events[0].after.stage = RandomizedAlmostLinearMcfStage::SampleIsolationCosts;
        stage_trace.events[1].before.stage = RandomizedAlmostLinearMcfStage::SampleIsolationCosts;
        assert!(matches!(
            check_randomized_almost_linear_mcf_trace(&graph, &required, 9, &stage_trace),
            Err(RandomizedAlmostLinearMcfError::TraceVerification)
        ));
        let mut base_trace =
            trace_randomized_almost_linear_mcf_with_seed(&graph, &required, 9).expect("trace");
        base_trace.base_snapshot.nodes[0].required_divergence += 1;
        base_trace.events[0].before.nodes[0].required_divergence += 1;
        assert!(matches!(
            check_randomized_almost_linear_mcf_trace(&graph, &required, 9, &base_trace),
            Err(RandomizedAlmostLinearMcfError::TraceVerification)
        ));
    }
}
