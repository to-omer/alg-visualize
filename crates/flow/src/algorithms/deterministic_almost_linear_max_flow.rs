//! Project-owned deterministic max-flow oracle demonstrator.
//!
//! The paper replaces randomized tree-chain sampling with a deterministic
//! shift-and-rebuild game over low-stretch forests, contracted cores, sparse
//! spanners, and explicit embeddings. This endpoint displays a bounded source
//! prefix with a finite exact forest population and stable branch ordering; it
//! is neither a source component nor the paper's solver.
//!
//! Before the prefix, a project cut oracle supplies the exact scalar target
//! used to initialize the optimum objective, log gap, and source potential.
//! It deliberately does not implement the paper's expander decomposition,
//! dynamic low-recourse spanner, or dynamic min-ratio data structures, and does
//! not claim an almost-linear project runtime. After bounded literal potential
//! steps, a separate project optimum-vector oracle materializes the paper's additive-half
//! final-point precondition. Kang--Payor fractional-cycle cancellation then
//! rounds the circulation without increasing its cost, and an independent
//! max-flow/min-cut certificate checks the integral result.

#![allow(clippy::cast_precision_loss)]

use std::cmp::Ordering;
use std::collections::VecDeque;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowModelError, FlowNetwork, NodeIndex, UnresolvedFlowEdge};

use super::{
    CostedFlowRoundingError, CostedFlowRoundingEventKind, CostedFlowRoundingSnapshot,
    trace_costed_flow_rounding,
};

/// Original-node admission ceiling.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES: usize = 7;
/// Original-edge admission ceiling.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES: usize = 8;
/// Exact finite spanning-forest population ceiling.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS: usize = 131_072;
/// Public reversible-boundary ceiling.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS: usize = 4_096;
/// Literal potential-reduction steps before exact final-point construction.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS: u64 = 6;
/// Number of levels in the explicit shifted tree chain.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS: usize = 2;
/// Branches per level in the bounded shift-and-rebuild game.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES: usize = 3;
/// Full branch wraps allowed at one level before shifting its parent.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_PASSES: u64 = 2;
/// Maximum original-edge assignments inspected by the bounded final-point oracle.
pub const DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS: u64 = 100_000;

const POSITIVE_FLOOR: f64 = 1.0e-12;
const NUMERICAL_TOLERANCE: f64 = 1.0e-8;
const CHANGE_THRESHOLD: f64 = 0.01;

/// Finite replay-safe scalar stored by exact IEEE-754 bit identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeterministicAlmostLinearScalar(u64);

impl DeterministicAlmostLinearScalar {
    fn try_new(value: f64) -> Result<Self, DeterministicAlmostLinearMaxFlowError> {
        if !value.is_finite() {
            return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    #[must_use]
    /// Recovers the finite scalar.
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    #[must_use]
    /// Returns a stable scene-ready decimal.
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Source-level and deterministic final-point publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeterministicAlmostLinearMaxFlowStage {
    /// Valid graph before the reduction.
    Ready,
    /// Cost-minus-one return edge `t -> s` is visible.
    BuildReturnEdgeReduction,
    /// Midpoint/artificial-star strict interior is visible.
    BuildInitialPoint,
    /// Exact finite forest population used to certify the bounded branch set.
    EnumerateForestPool,
    /// One concrete level/branch geometry is installed and made visible.
    InstallBranchRecord,
    /// Stable low-stretch/MWU-inspired branch collection is installed.
    BuildBranchCollection,
    /// Active forest is contracted into its explicit core.
    BuildCoreGraph,
    /// Deterministic sparse core and every embedding path are visible.
    BuildSpannerEmbedding,
    /// One geometric checkpoint of an evaluated fundamental-cycle candidate.
    InspectFundamentalCycle,
    /// Current gradient/length fundamental cycles were compared.
    QueryMinimumRatioCycle,
    /// The current chain failed the bounded approximation band.
    QueryFailure,
    /// Largest eligible level advances one branch.
    ShiftBranch,
    /// Levels below a shifted level are rebuilt.
    RebuildDeeperLevels,
    /// The source potential-reduction step was applied.
    PotentialReductionStep,
    /// Slowly changing coordinates were explicitly detected.
    DetectChangedCoordinates,
    /// A periodic source-style rebuild refreshes all levels.
    ScheduledRebuild,
    /// Bounded integer feasible-return circulations were enumerated.
    EnumerateFeasibleSet,
    /// A feasible fractional point entered the source additive-half gate.
    ConstructFinalPoint,
    /// Kang--Payor skipped an already-integral edge.
    RoundingIntegralEdge,
    /// A fractional edge joined two distinct forest components.
    RoundingLinkFractionalEdge,
    /// A fractional cycle was canceled in a non-increasing-cost direction.
    RoundingCancelFractionalCycle,
    /// The fractional forest proof completed with an integral circulation.
    FinishFlowRounding,
    /// Original flow, return flow, and min cut were checked.
    CheckCertificate,
    /// Certified maximum flow is public.
    Optimal,
}

/// Source family of the selected explicit candidate cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeterministicAlmostLinearCycleKind {
    /// Fundamental cycle of an active chain tree.
    Tree,
    /// Off-spanner core edge plus its explicit embedding path.
    Spanner,
}

/// Original-node projection including the source artificial initial-point edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAlmostLinearNodeState {
    /// Canonical original node.
    pub node: NodeIndex,
    /// The artificial star has index `n`.
    pub tree_parent: Option<usize>,
    /// Component ordinal in the selected forest.
    pub tree_component: usize,
    /// Membership in the exact terminal source-side cut.
    pub source_side: bool,
    /// `1`: star-to-node, `-1`: node-to-star, `0`: absent.
    pub artificial_direction: i8,
    /// Current strict-interior artificial flow.
    pub artificial_flow: DeterministicAlmostLinearScalar,
    /// Artificial-edge capacity, zero when absent.
    pub artificial_capacity: DeterministicAlmostLinearScalar,
    /// Bit mask of levels whose active tree contains the artificial edge.
    pub artificial_tree_level_mask: u64,
    /// Whether the artificial edge is in the selected forest.
    pub active_artificial_tree_edge: bool,
    /// Signed active-cycle membership on the artificial edge.
    pub active_artificial_sign: i8,
}

/// Original-edge projection of IPM, tree/core/spanner, and rounding state.
#[derive(Clone, Debug, Eq, PartialEq)]
// These flags are independent, concurrently visible projection layers rather
// than mutually exclusive solver states.
#[allow(clippy::struct_excessive_bools)]
pub struct DeterministicAlmostLinearEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Current strict-interior augmented-circulation flow.
    pub interior_flow: DeterministicAlmostLinearScalar,
    /// Current source gradient coordinate.
    pub gradient: DeterministicAlmostLinearScalar,
    /// Current positive source length coordinate.
    pub length: DeterministicAlmostLinearScalar,
    /// Bit mask of levels whose active tree contains the edge.
    pub tree_level_mask: u64,
    /// Bit mask of levels whose partial forest contains the edge.
    pub forest_level_mask: u64,
    /// Whether the edge is in the selected forest.
    pub active_tree_edge: bool,
    /// Whether the edge survives contraction into the active core.
    pub active_core_edge: bool,
    /// Whether the active deterministic core spanner contains the edge.
    pub active_spanner_edge: bool,
    /// Hops in the explicit active spanner embedding.
    pub embedding_hops: u64,
    /// Active embedding stretch over the current length coordinate.
    pub embedding_stretch: DeterministicAlmostLinearScalar,
    /// Signed active fundamental-cycle membership.
    pub active_cycle_sign: i8,
    /// Whether Detect refreshed this coordinate.
    pub changed_coordinate: bool,
    /// Source additive-half feasible point before deterministic rounding.
    pub final_point_flow: Option<BigRational>,
    /// Current exact coordinate during Kang--Payor rounding.
    pub rounding_flow: Option<BigRational>,
    /// Membership in the processed fractional-edge forest.
    pub rounding_forest_edge: bool,
    /// Signed membership in the active cost-nonincreasing fractional cycle.
    pub rounding_cycle_sign: i8,
    /// Exact rounded integral flow when available.
    pub final_flow: Option<u64>,
}

/// Exact bounded-work and source-operation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeterministicAlmostLinearMaxFlowMetrics {
    /// Exact original cuts inspected.
    pub enumerated_cuts: u64,
    /// Candidate forest subsets inspected.
    pub forest_subsets: u64,
    /// Forests in the exact finite population.
    pub forest_pool_size: u64,
    /// Deterministic branch records built.
    pub branch_records: u64,
    /// Contracted core graphs built.
    pub core_builds: u64,
    /// Explicit spanner embeddings built.
    pub spanner_embeddings: u64,
    /// Fundamental cycles evaluated.
    pub fundamental_cycles: u64,
    /// Queries inside the approximation band.
    pub successful_queries: u64,
    /// Explicit deterministic chain misses.
    pub query_failures: u64,
    /// Branch shifts performed by the game.
    pub branch_shifts: u64,
    /// Branch-index wraps performed by the game.
    pub branch_wraps: u64,
    /// Deeper-level rebuilds after a shift.
    pub deeper_rebuilds: u64,
    /// Completed source potential steps.
    pub potential_steps: u64,
    /// Coordinates refreshed by Detect.
    pub detected_coordinates: u64,
    /// Periodic whole-chain rebuilds.
    pub scheduled_rebuilds: u64,
    /// Original-edge assignments inspected by the final-point oracle.
    pub enumerated_assignments: u64,
    /// Feasible integer return-edge circulations found by the oracle.
    pub feasible_circulations: u64,
    /// Fractional edges processed by Kang--Payor rounding.
    pub rounding_processed_edges: u64,
    /// Fractional cycles canceled by Kang--Payor rounding.
    pub rounding_cycles: u64,
    /// Fractional coordinates made integral by cancellation.
    pub rounding_integralized_edges: u64,
    /// Independent terminal checks.
    pub certificate_checks: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete reversible state at one publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAlmostLinearMaxFlowSnapshot {
    /// Original-node projections.
    pub nodes: Vec<DeterministicAlmostLinearNodeState>,
    /// Original-edge projections.
    pub edges: Vec<DeterministicAlmostLinearEdgeState>,
    /// Paper parameter `1 / (1000 log(mU))`.
    pub alpha: DeterministicAlmostLinearScalar,
    /// Current source potential.
    pub potential: DeterministicAlmostLinearScalar,
    /// Current augmented min-cost gap above `F*`.
    pub cost_gap: DeterministicAlmostLinearScalar,
    /// Best ratio in the current active deterministic chain.
    pub selected_ratio: Option<DeterministicAlmostLinearScalar>,
    /// Best ratio over the exact finite forest pool.
    pub exact_pool_ratio: Option<DeterministicAlmostLinearScalar>,
    /// Work-edge ordinal that closes the selected cycle.
    pub selected_off_tree_edge: Option<u64>,
    /// Whether the selected cycle came from the tree or core-spanner family.
    pub selected_cycle_kind: Option<DeterministicAlmostLinearCycleKind>,
    /// Exact finite sampling-population size.
    pub forest_pool_size: u64,
    /// Number of explicit shifted-tree-chain levels.
    pub level_count: u64,
    /// Number of deterministic branches at each level.
    pub branch_count: u64,
    /// Current branch index at every level.
    pub active_branches: Vec<u64>,
    /// Completed branch wraps at every level.
    pub passes: Vec<u64>,
    /// Level of the active candidate or most recent shift.
    pub active_level: Option<u64>,
    /// Active contracted-core vertex count.
    pub core_vertices: u64,
    /// Active contracted-core edge count.
    pub core_edges: u64,
    /// Active deterministic spanner edge count.
    pub spanner_edges: u64,
    /// Active explicit embedding hop count.
    pub embedding_hops: u64,
    /// Completed potential steps.
    pub iteration: u64,
    /// Current whole-chain rebuild epoch.
    pub rebuild_epoch: u64,
    /// Strict-interior return-edge flow.
    pub return_flow: DeterministicAlmostLinearScalar,
    /// Return-edge capacity `mU`.
    pub return_capacity: u64,
    /// Return-edge gradient coordinate.
    pub return_gradient: DeterministicAlmostLinearScalar,
    /// Return-edge length coordinate.
    pub return_length: DeterministicAlmostLinearScalar,
    /// Bit mask of levels whose active tree contains the return edge.
    pub return_tree_level_mask: u64,
    /// Whether the return edge is in the selected forest.
    pub active_return_tree_edge: bool,
    /// Signed active-cycle membership on `t -> s`.
    pub active_return_sign: i8,
    /// Source additive-half return coordinate before deterministic rounding.
    pub final_point_return_flow: Option<BigRational>,
    /// Current return coordinate during Kang--Payor rounding.
    pub rounding_return_flow: Option<BigRational>,
    /// Whether the return edge is in the fractional-edge forest.
    pub rounding_return_forest_edge: bool,
    /// Signed return-edge membership in the active rounding cycle.
    pub rounding_return_sign: i8,
    /// Rounded return flow, equal to the max-flow value.
    pub final_return_flow: Option<u64>,
    /// Artificial edges in the source initial-point construction.
    pub artificial_edges: u64,
    /// Sum of strict-interior artificial flow.
    pub artificial_flow: DeterministicAlmostLinearScalar,
    /// Rounded artificial flow, zero when rounding succeeded.
    pub final_artificial_flow: Option<u64>,
    /// Exact augmented-cost gap of the deterministic final point.
    pub final_point_gap: Option<BigRational>,
    /// Source additive final-point threshold `1/2`.
    pub final_point_threshold: BigRational,
    /// Weight of the feasible-face barycenter in the final point.
    pub final_point_mix: Option<BigRational>,
    /// Most recently processed augmented edge during rounding.
    pub rounding_processed_edge: Option<EdgeId>,
    /// Exact maximum-flow target installed by bounded cut enumeration.
    pub target_value: u64,
    /// Current source/final-point/rounding boundary.
    pub stage: DeterministicAlmostLinearMaxFlowStage,
    /// Exact counters at this boundary.
    pub metrics: DeterministicAlmostLinearMaxFlowMetrics,
}

/// One atomic reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAlmostLinearMaxFlowTraceEvent {
    /// Stable event-vocabulary identity.
    pub catalog_id: &'static str,
    /// Boundary before the transition.
    pub before: DeterministicAlmostLinearMaxFlowSnapshot,
    /// Boundary after the transition.
    pub after: DeterministicAlmostLinearMaxFlowSnapshot,
}

/// Certified bounded solver result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAlmostLinearMaxFlowResult {
    /// Original-edge integral flows.
    pub flows: Vec<u64>,
    /// Independent max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Terminal public state.
    pub final_snapshot: DeterministicAlmostLinearMaxFlowSnapshot,
    /// Exact bounded-work counters.
    pub metrics: DeterministicAlmostLinearMaxFlowMetrics,
}

/// Result plus all reversible source and deterministic-rounding boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeterministicAlmostLinearMaxFlowTraceResult {
    /// Same certified result as the fast profile.
    pub result: DeterministicAlmostLinearMaxFlowResult,
    /// Ready boundary before the reduction.
    pub base_snapshot: DeterministicAlmostLinearMaxFlowSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<DeterministicAlmostLinearMaxFlowTraceEvent>,
    /// Terminal boundary, equal to the result snapshot.
    pub final_snapshot: DeterministicAlmostLinearMaxFlowSnapshot,
}

/// Admission, work-limit, numerical, certificate, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DeterministicAlmostLinearMaxFlowError {
    /// Input exceeds the bounded exact interactive band.
    #[error("deterministic almost-linear max-flow input exceeds admission limits")]
    AdmissionLimit,
    /// Input violates the max-flow/source-reduction requirements.
    #[error(
        "deterministic almost-linear max-flow requires distinct terminals, zero lower/supply, positive capacities, and no self-loops"
    )]
    GraphRequirement,
    /// Exact finite forest enumeration exceeded its ceiling.
    #[error("deterministic almost-linear max-flow forest pool exceeds bounded work limit")]
    ForestLimit,
    /// A forest or signed fundamental circulation was invalid.
    #[error("deterministic almost-linear max-flow forest/cycle invariant failed")]
    ForestInvariant,
    /// Checked integer arithmetic overflowed.
    #[error("deterministic almost-linear max-flow arithmetic overflow")]
    ArithmeticOverflow,
    /// A source scalar was non-finite or left the strict interior.
    #[error("deterministic almost-linear max-flow numerical invariant failed")]
    NumericalFailure,
    /// Exact final-point construction or deterministic rounding failed.
    #[error("deterministic almost-linear max-flow final-point rounding failed")]
    FinalPointRounding,
    /// The exact Kang--Payor rounding primitive rejected the circulation.
    #[error("deterministic almost-linear max-flow rounding failed: {0}")]
    Rounding(#[from] CostedFlowRoundingError),
    /// The augmented rounding model could not be constructed.
    #[error("deterministic almost-linear max-flow rounding model failed: {0}")]
    Model(#[from] FlowModelError),
    /// Independent max-flow validation rejected the final flow.
    #[error("deterministic almost-linear max-flow certificate failed")]
    Certificate(#[from] CertificateError),
    /// Event replay or terminal metadata contradicted the contract.
    #[error("deterministic almost-linear max-flow replay invariant failed")]
    TraceInvariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkEdgeKind {
    Original(usize),
    Return,
    Artificial(usize),
}

#[derive(Clone, Debug)]
struct WorkEdge {
    kind: WorkEdgeKind,
    from: usize,
    to: usize,
    lower: f64,
    upper: f64,
    cost: f64,
    flow: f64,
    gradient: f64,
    length: f64,
    approximate_gradient: f64,
    approximate_length: f64,
    changed: bool,
}

#[derive(Clone, Debug)]
struct Forest {
    edges: Vec<usize>,
}

#[derive(Clone, Debug)]
struct BranchGeometry {
    tree_index: usize,
    forest_edges: Vec<usize>,
    components: Vec<usize>,
    core_edges: Vec<usize>,
    spanner_edges: Vec<usize>,
    embeddings: Vec<Vec<usize>>,
    stretch: Vec<f64>,
}

#[derive(Clone, Debug)]
struct CycleCandidate {
    forest_index: usize,
    level: usize,
    branch: usize,
    off_tree_edge: usize,
    kind: DeterministicAlmostLinearCycleKind,
    signs: Vec<i8>,
    ratio: f64,
    numerator: f64,
}

#[derive(Clone, Debug)]
struct CycleEvaluationCheckpoint {
    candidate: CycleCandidate,
    metrics: DeterministicAlmostLinearMaxFlowMetrics,
}

#[derive(Clone, Debug)]
struct BranchCollectionCheckpoints {
    cycle_evaluations: Vec<CycleEvaluationCheckpoint>,
    branch_records: Vec<KernelState>,
}

#[derive(Clone, Copy)]
struct FundamentalCycleContext<'a> {
    level: usize,
    branch: usize,
    geometry: Option<&'a BranchGeometry>,
    nodes: usize,
    edges: &'a [WorkEdge],
}

#[derive(Clone, Debug)]
struct KernelState {
    star: usize,
    target: u64,
    return_capacity: u64,
    alpha: f64,
    optimum_cost: f64,
    work_edges: Vec<WorkEdge>,
    forests: Vec<Forest>,
    branches: Vec<Vec<BranchGeometry>>,
    active_branches: Vec<usize>,
    passes: Vec<u64>,
    active_level: Option<usize>,
    active_geometry: Option<BranchGeometry>,
    active: Option<CycleCandidate>,
    exact_pool_ratio: Option<f64>,
    iteration: u64,
    rebuild_epoch: u64,
    final_point_flows: Option<Vec<BigRational>>,
    final_point_return_flow: Option<BigRational>,
    final_point_gap: Option<BigRational>,
    final_point_mix: Option<BigRational>,
    rounding_flows: Option<Vec<BigRational>>,
    rounding_return_flow: Option<BigRational>,
    rounding_forest: Vec<bool>,
    rounding_cycle_signs: Vec<i8>,
    rounding_return_forest: bool,
    rounding_return_sign: i8,
    rounding_processed_edge: Option<EdgeId>,
    final_return_flow: Option<u64>,
    final_flows: Option<Vec<u64>>,
    metrics: DeterministicAlmostLinearMaxFlowMetrics,
}

#[derive(Clone, Debug)]
struct FeasibleReturnCirculation {
    original_flows: Vec<u64>,
    return_flow: u64,
}

#[derive(Clone, Debug)]
struct FeasibleReturnFace {
    circulations: Vec<FeasibleReturnCirculation>,
    original_flow_sums: Vec<u128>,
    return_flow_sum: u128,
}

#[derive(Clone, Debug)]
struct DeterministicFinalPoint {
    augmented_graph: FlowNetwork,
    original_to_augmented: Vec<usize>,
    return_index: usize,
    augmented_flows: Vec<BigRational>,
    original_flows: Vec<BigRational>,
    return_flow: BigRational,
    gap: BigRational,
    mix: BigRational,
}

/// Runs the project-owned bounded demonstrator without retaining trace events.
///
/// # Errors
///
/// Rejects invalid/beyond-band graphs, bounded-work or numerical failure, and
/// any result rejected by the independent certificate.
pub fn solve_deterministic_almost_linear_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DeterministicAlmostLinearMaxFlowResult, DeterministicAlmostLinearMaxFlowError> {
    build_trace(graph, source, sink, false).map(|trace| trace.result)
}

/// Traces every deterministic source, data-structure, and rounding boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_deterministic_almost_linear_max_flow`].
pub fn trace_deterministic_almost_linear_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DeterministicAlmostLinearMaxFlowTraceResult, DeterministicAlmostLinearMaxFlowError> {
    let trace = build_trace(graph, source, sink, true)?;
    check_deterministic_almost_linear_max_flow_trace(graph, source, sink, &trace)?;
    Ok(trace)
}

#[allow(clippy::too_many_lines)]
fn build_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
) -> Result<DeterministicAlmostLinearMaxFlowTraceResult, DeterministicAlmostLinearMaxFlowError> {
    validate_graph(graph, source, sink)?;
    let (target, cut_side, enumerated_cuts) = enumerate_min_cut(graph, source, sink)?;
    let mut state = initialize_kernel(graph, source, sink, target)?;
    state.metrics.enumerated_cuts = enumerated_cuts;
    let base_snapshot = snapshot(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::Ready,
    )?;
    let mut current = base_snapshot.clone();
    let mut events = record_trace.then(Vec::new);

    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::BuildReturnEdgeReduction,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::BuildInitialPoint,
        &mut current,
        &mut events,
    )?;

    state.forests =
        enumerate_spanning_forests(state.star + 1, &state.work_edges, &mut state.metrics)?;
    state.metrics.forest_pool_size = state.forests.len() as u64;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::EnumerateForestPool,
        &mut current,
        &mut events,
    )?;
    let branch_baseline = state.clone();
    let branch_checkpoints = build_branch_collection(&mut state, false)?;
    publish_cycle_checkpoints(
        graph,
        &branch_baseline,
        &cut_side,
        branch_checkpoints.cycle_evaluations,
        &mut current,
        &mut events,
    )?;
    publish_branch_record_checkpoints(
        graph,
        &cut_side,
        branch_checkpoints.branch_records,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::BuildBranchCollection,
        &mut current,
        &mut events,
    )?;
    prepare_active_geometry(&mut state)?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::BuildCoreGraph,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::BuildSpannerEmbedding,
        &mut current,
        &mut events,
    )?;

    while state.iteration < DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS {
        update_gradient_lengths(&mut state)?;
        let query_baseline = state.clone();
        let (successful, cycle_checkpoints) = query_tree_chain(&mut state)?;
        publish_cycle_checkpoints(
            graph,
            &query_baseline,
            &cut_side,
            cycle_checkpoints,
            &mut current,
            &mut events,
        )?;
        publish(
            graph,
            &state,
            &cut_side,
            DeterministicAlmostLinearMaxFlowStage::QueryMinimumRatioCycle,
            &mut current,
            &mut events,
        )?;
        if !successful {
            state.metrics.query_failures = state.metrics.query_failures.saturating_add(1);
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::QueryFailure,
                &mut current,
                &mut events,
            )?;
            let Some(shifted_level) = shift_largest_eligible_level(&mut state) else {
                break;
            };
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::ShiftBranch,
                &mut current,
                &mut events,
            )?;
            if shifted_level + 1 < DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS {
                rebuild_deeper_levels(&mut state, shifted_level)?;
                publish(
                    graph,
                    &state,
                    &cut_side,
                    DeterministicAlmostLinearMaxFlowStage::RebuildDeeperLevels,
                    &mut current,
                    &mut events,
                )?;
            }
            prepare_active_geometry(&mut state)?;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::BuildCoreGraph,
                &mut current,
                &mut events,
            )?;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::BuildSpannerEmbedding,
                &mut current,
                &mut events,
            )?;
            continue;
        }
        apply_potential_step(&mut state)?;
        publish(
            graph,
            &state,
            &cut_side,
            DeterministicAlmostLinearMaxFlowStage::PotentialReductionStep,
            &mut current,
            &mut events,
        )?;
        detect_changed_coordinates(&mut state)?;
        publish(
            graph,
            &state,
            &cut_side,
            DeterministicAlmostLinearMaxFlowStage::DetectChangedCoordinates,
            &mut current,
            &mut events,
        )?;
        if state.iteration % 2 == 0
            && state.iteration < DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS
        {
            state.rebuild_epoch = state.rebuild_epoch.saturating_add(1);
            state.metrics.scheduled_rebuilds = state.metrics.scheduled_rebuilds.saturating_add(1);
            state.active = None;
            state.active_level = None;
            state.active_geometry = None;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::ScheduledRebuild,
                &mut current,
                &mut events,
            )?;
            let branch_baseline = state.clone();
            let branch_checkpoints = build_branch_collection(&mut state, true)?;
            publish_cycle_checkpoints(
                graph,
                &branch_baseline,
                &cut_side,
                branch_checkpoints.cycle_evaluations,
                &mut current,
                &mut events,
            )?;
            publish_branch_record_checkpoints(
                graph,
                &cut_side,
                branch_checkpoints.branch_records,
                &mut current,
                &mut events,
            )?;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::BuildBranchCollection,
                &mut current,
                &mut events,
            )?;
            prepare_active_geometry(&mut state)?;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::BuildCoreGraph,
                &mut current,
                &mut events,
            )?;
            publish(
                graph,
                &state,
                &cut_side,
                DeterministicAlmostLinearMaxFlowStage::BuildSpannerEmbedding,
                &mut current,
                &mut events,
            )?;
        }
    }

    state.active = None;
    let face = enumerate_feasible_return_face(graph, source, sink, &mut state.metrics)?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::EnumerateFeasibleSet,
        &mut current,
        &mut events,
    )?;
    let final_point = construct_deterministic_final_point(
        graph,
        source,
        sink,
        state.return_capacity,
        target,
        &face,
    )?;
    state.final_point_flows = Some(final_point.original_flows.clone());
    state.final_point_return_flow = Some(final_point.return_flow.clone());
    state.final_point_gap = Some(final_point.gap.clone());
    state.final_point_mix = Some(final_point.mix.clone());
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::ConstructFinalPoint,
        &mut current,
        &mut events,
    )?;

    let required = vec![0_i128; final_point.augmented_graph.nodes().len()];
    let rounding = trace_costed_flow_rounding(
        &final_point.augmented_graph,
        &required,
        &final_point.augmented_flows,
    )?;
    let expected_initial_cost = -final_point.return_flow.clone();
    if rounding.result.initial_cost != expected_initial_cost
        || BigRational::from_integer(BigInt::from(target)) + &rounding.result.initial_cost
            != final_point.gap
    {
        return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
    }
    for event in &rounding.events {
        apply_rounding_snapshot(&mut state, &final_point, &event.after, &event.kind)?;
        let rounding_stage = match event.kind {
            CostedFlowRoundingEventKind::IntegralEdgeSkipped { .. } => {
                DeterministicAlmostLinearMaxFlowStage::RoundingIntegralEdge
            }
            CostedFlowRoundingEventKind::FractionalEdgeLinked { .. } => {
                DeterministicAlmostLinearMaxFlowStage::RoundingLinkFractionalEdge
            }
            CostedFlowRoundingEventKind::FractionalCycleCanceled { .. } => {
                DeterministicAlmostLinearMaxFlowStage::RoundingCancelFractionalCycle
            }
            CostedFlowRoundingEventKind::Completed => {
                let rounded = original_rounded_flows(&final_point, &rounding.result.flows)?;
                let return_flow = rounding
                    .result
                    .flows
                    .get(final_point.return_index)
                    .copied()
                    .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
                if return_flow != target || rounding.result.total_cost != -i128::from(target) {
                    return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
                }
                state.final_flows = Some(rounded);
                state.final_return_flow = Some(return_flow);
                DeterministicAlmostLinearMaxFlowStage::FinishFlowRounding
            }
        };
        publish(
            graph,
            &state,
            &cut_side,
            rounding_stage,
            &mut current,
            &mut events,
        )?;
    }
    let rounded_flows = state
        .final_flows
        .clone()
        .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
    let certificate = check_max_flow(graph, source, sink, &rounded_flows)?;
    if u64::try_from(certificate.value).ok() != Some(target) {
        return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
    }
    state.metrics.certificate_checks = state.metrics.certificate_checks.saturating_add(4);
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::CheckCertificate,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        DeterministicAlmostLinearMaxFlowStage::Optimal,
        &mut current,
        &mut events,
    )?;

    let final_snapshot = current;
    let result = DeterministicAlmostLinearMaxFlowResult {
        flows: rounded_flows,
        certificate,
        final_snapshot: final_snapshot.clone(),
        metrics: final_snapshot.metrics,
    };
    let trace = DeterministicAlmostLinearMaxFlowTraceResult {
        result,
        base_snapshot,
        events: events.unwrap_or_default(),
        final_snapshot,
    };
    Ok(trace)
}

/// Independently checks the published branch-game boundaries and the exact
/// terminal max-flow/min-cut certificate.
///
/// # Errors
///
/// Rejects a broken chain, branch/core/spanner drift, or non-optimal flow.
pub fn check_deterministic_almost_linear_max_flow_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &DeterministicAlmostLinearMaxFlowTraceResult,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    validate_graph(graph, source, sink)?;
    if trace.base_snapshot.stage != DeterministicAlmostLinearMaxFlowStage::Ready
        || trace.final_snapshot.stage != DeterministicAlmostLinearMaxFlowStage::Optimal
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.metrics != trace.final_snapshot.metrics
        || trace.events.len() > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS
    {
        #[cfg(test)]
        eprintln!(
            "invalid trace envelope: base={:?} final={:?} events={}",
            trace.base_snapshot.stage,
            trace.final_snapshot.stage,
            trace.events.len()
        );
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    if check_published_snapshot(graph, &trace.base_snapshot).is_err() {
        #[cfg(test)]
        eprintln!(
            "invalid published snapshot: {:?}",
            trace.base_snapshot.stage
        );
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    let mut previous = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != stage_catalog_id(event.after.stage)
            || event.before != *previous
            || !valid_transition(event.before.stage, event.after.stage)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
            || event.after.return_capacity != trace.base_snapshot.return_capacity
            || event.after.target_value != trace.base_snapshot.target_value
            || event.after.active_branches.len() != DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS
            || event.after.passes.len() != DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS
        {
            #[cfg(test)]
            eprintln!(
                "invalid event envelope: {:?} -> {:?}, catalog={}, transitions {} -> {}",
                event.before.stage,
                event.after.stage,
                event.catalog_id,
                event.before.metrics.state_transitions,
                event.after.metrics.state_transitions
            );
            return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
        }
        if check_published_snapshot(graph, &event.after).is_err() {
            #[cfg(test)]
            eprintln!("invalid published snapshot: {:?}", event.after.stage);
            return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
        }
        if check_published_transition(&event.before, &event.after).is_err() {
            #[cfg(test)]
            eprintln!(
                "invalid published transition: {:?} -> {:?}",
                event.before.stage, event.after.stage
            );
            return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
        }
        previous = &event.after;
    }
    if *previous != trace.final_snapshot
        || trace.events.len() as u64 != trace.final_snapshot.metrics.state_transitions
        || trace.final_snapshot.final_return_flow != Some(trace.final_snapshot.target_value)
        || trace.final_snapshot.final_artificial_flow != Some(0)
        || trace.final_snapshot.forest_pool_size == 0
        || trace.final_snapshot.level_count != DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS as u64
        || trace.final_snapshot.branch_count != DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES as u64
    {
        #[cfg(test)]
        eprintln!(
            "invalid trace terminal envelope: previous={:?} final={:?} events={} transitions={}",
            previous.stage,
            trace.final_snapshot.stage,
            trace.events.len(),
            trace.final_snapshot.metrics.state_transitions
        );
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    let certificate = check_max_flow(graph, source, sink, &trace.result.flows)?;
    if certificate != trace.result.certificate
        || u64::try_from(certificate.value).ok() != Some(trace.final_snapshot.target_value)
        || trace.final_snapshot.edges.len() != graph.edges().len()
    {
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    for (edge, flow) in trace.final_snapshot.edges.iter().zip(&trace.result.flows) {
        if !(edge.final_flow == Some(*flow)
            && edge.interior_flow.get().is_finite()
            && edge.gradient.get().is_finite()
            && edge.length.get().is_finite()
            && edge.length.get() > 0.0)
        {
            return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
        }
    }
    Ok(())
}

fn check_published_snapshot(
    graph: &FlowNetwork,
    snapshot: &DeterministicAlmostLinearMaxFlowSnapshot,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    if snapshot.nodes.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.nodes.iter().enumerate().any(|(index, state)| {
            state.node.as_usize() != index
                || state
                    .tree_parent
                    .is_some_and(|parent| parent > graph.nodes().len())
                || !matches!(state.artificial_direction, -1..=1)
                || !matches!(state.active_artificial_sign, -1..=1)
                || !state.artificial_flow.get().is_finite()
                || !state.artificial_capacity.get().is_finite()
                || state.artificial_flow.get() < 0.0
                || state.artificial_capacity.get() < 0.0
        })
        || snapshot
            .edges
            .iter()
            .zip(graph.edges())
            .any(|(state, edge)| {
                &state.edge != edge.id()
                    || !state.interior_flow.get().is_finite()
                    || !state.gradient.get().is_finite()
                    || !state.length.get().is_finite()
                    || !state.embedding_stretch.get().is_finite()
                    || state.length.get() <= 0.0
                    || state.embedding_stretch.get() < 0.0
                    || !matches!(state.active_cycle_sign, -1..=1)
                    || !matches!(state.rounding_cycle_sign, -1..=1)
                    || state.final_flow.is_some_and(|flow| flow > edge.capacity())
            })
        || !snapshot.alpha.get().is_finite()
        || snapshot.alpha.get() <= 0.0
        || !snapshot.potential.get().is_finite()
        || !snapshot.cost_gap.get().is_finite()
        || snapshot.cost_gap.get() < -NUMERICAL_TOLERANCE
        || snapshot
            .selected_ratio
            .is_some_and(|value| !value.get().is_finite())
        || snapshot
            .exact_pool_ratio
            .is_some_and(|value| !value.get().is_finite())
        || snapshot.active_branches.iter().any(|branch| {
            *branch
                >= u64::try_from(DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES).unwrap_or(u64::MAX)
        })
        || snapshot
            .passes
            .iter()
            .any(|pass| *pass > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_PASSES)
        || snapshot
            .active_level
            .is_some_and(|level| level >= snapshot.level_count)
        || snapshot.iteration > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS
        || !snapshot.return_flow.get().is_finite()
        || !snapshot.return_gradient.get().is_finite()
        || !snapshot.return_length.get().is_finite()
        || snapshot.return_length.get() <= 0.0
        || !matches!(snapshot.active_return_sign, -1..=1)
        || !matches!(snapshot.rounding_return_sign, -1..=1)
        || !snapshot.artificial_flow.get().is_finite()
        || snapshot.artificial_flow.get() < 0.0
        || snapshot.return_flow.get() < 0.0
        || snapshot.return_flow.get() > snapshot.return_capacity as f64 + NUMERICAL_TOLERANCE
        || snapshot.final_point_threshold != BigRational::new(BigInt::from(1), BigInt::from(2))
        || snapshot
            .final_point_gap
            .as_ref()
            .is_some_and(|gap| gap < &BigRational::zero() || gap >= &snapshot.final_point_threshold)
    {
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive stage-to-invariant table is clearer as one total match"
)]
fn check_published_transition(
    before: &DeterministicAlmostLinearMaxFlowSnapshot,
    after: &DeterministicAlmostLinearMaxFlowSnapshot,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    use DeterministicAlmostLinearMaxFlowStage as Stage;
    let metrics_do_not_decrease = after.metrics.enumerated_cuts >= before.metrics.enumerated_cuts
        && after.metrics.forest_subsets >= before.metrics.forest_subsets
        && after.metrics.forest_pool_size >= before.metrics.forest_pool_size
        && after.metrics.branch_records >= before.metrics.branch_records
        && after.metrics.core_builds >= before.metrics.core_builds
        && after.metrics.spanner_embeddings >= before.metrics.spanner_embeddings
        && after.metrics.fundamental_cycles >= before.metrics.fundamental_cycles
        && after.metrics.successful_queries >= before.metrics.successful_queries
        && after.metrics.query_failures >= before.metrics.query_failures
        && after.metrics.branch_shifts >= before.metrics.branch_shifts
        && after.metrics.branch_wraps >= before.metrics.branch_wraps
        && after.metrics.deeper_rebuilds >= before.metrics.deeper_rebuilds
        && after.metrics.potential_steps >= before.metrics.potential_steps
        && after.metrics.detected_coordinates >= before.metrics.detected_coordinates
        && after.metrics.scheduled_rebuilds >= before.metrics.scheduled_rebuilds
        && after.metrics.enumerated_assignments >= before.metrics.enumerated_assignments
        && after.metrics.feasible_circulations >= before.metrics.feasible_circulations
        && after.metrics.rounding_processed_edges >= before.metrics.rounding_processed_edges
        && after.metrics.rounding_cycles >= before.metrics.rounding_cycles
        && after.metrics.rounding_integralized_edges >= before.metrics.rounding_integralized_edges
        && after.metrics.certificate_checks >= before.metrics.certificate_checks;
    let stage_contract = match after.stage {
        Stage::EnumerateForestPool => after.forest_pool_size > 0,
        Stage::InstallBranchRecord => {
            let records_per_collection = u64::try_from(
                DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS
                    * DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES,
            )
            .unwrap_or(u64::MAX);
            let branch_count =
                u64::try_from(DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES).unwrap_or(u64::MAX);
            let ordinal = after.metrics.branch_records.saturating_sub(1) % records_per_collection;
            let expected_level = ordinal / branch_count;
            let expected_branch = ordinal % branch_count;
            after.metrics.branch_records == before.metrics.branch_records.saturating_add(1)
                && after.active_level == Some(expected_level)
                && usize::try_from(expected_level)
                    .ok()
                    .and_then(|level| after.active_branches.get(level))
                    .copied()
                    == Some(expected_branch)
                && after.core_vertices > 0
        }
        Stage::InspectFundamentalCycle => {
            after.selected_ratio.is_some()
                && after.selected_off_tree_edge.is_some()
                && after.selected_cycle_kind.is_some()
                && after.metrics.fundamental_cycles > before.metrics.fundamental_cycles
        }
        Stage::QueryMinimumRatioCycle => {
            after.selected_ratio.is_some() == after.selected_off_tree_edge.is_some()
                && after.selected_ratio.is_some() == after.selected_cycle_kind.is_some()
        }
        Stage::QueryFailure => after.metrics.query_failures > before.metrics.query_failures,
        Stage::ShiftBranch => after.metrics.branch_shifts > before.metrics.branch_shifts,
        Stage::RebuildDeeperLevels => {
            after.metrics.deeper_rebuilds > before.metrics.deeper_rebuilds
        }
        Stage::PotentialReductionStep => {
            after.iteration == before.iteration.saturating_add(1)
                && after.metrics.potential_steps > before.metrics.potential_steps
        }
        Stage::DetectChangedCoordinates => {
            after.iteration == before.iteration
                && after.metrics.detected_coordinates >= before.metrics.detected_coordinates
        }
        Stage::ScheduledRebuild => {
            after.rebuild_epoch == before.rebuild_epoch.saturating_add(1)
                && after.metrics.scheduled_rebuilds > before.metrics.scheduled_rebuilds
        }
        Stage::ConstructFinalPoint => {
            after.final_point_gap.is_some()
                && after.final_point_mix.is_some()
                && after.final_point_return_flow.is_some()
                && after
                    .edges
                    .iter()
                    .all(|edge| edge.final_point_flow.is_some())
        }
        Stage::RoundingIntegralEdge
        | Stage::RoundingLinkFractionalEdge
        | Stage::RoundingCancelFractionalCycle => {
            after.metrics.rounding_processed_edges >= before.metrics.rounding_processed_edges
        }
        Stage::FinishFlowRounding => {
            after
                .rounding_return_flow
                .as_ref()
                .is_some_and(BigRational::is_integer)
                && after.edges.iter().all(|edge| {
                    edge.rounding_flow
                        .as_ref()
                        .is_some_and(BigRational::is_integer)
                })
        }
        Stage::CheckCertificate => {
            after.metrics.certificate_checks > before.metrics.certificate_checks
        }
        Stage::Optimal => {
            after.final_return_flow == Some(after.target_value)
                && after.final_artificial_flow == Some(0)
                && after.edges.iter().all(|edge| edge.final_flow.is_some())
        }
        Stage::Ready
        | Stage::BuildReturnEdgeReduction
        | Stage::BuildInitialPoint
        | Stage::BuildCoreGraph
        | Stage::BuildSpannerEmbedding
        | Stage::EnumerateFeasibleSet => true,
        Stage::BuildBranchCollection => {
            after.metrics.branch_records == before.metrics.branch_records
                && after.active_level.is_none()
        }
    };
    if !metrics_do_not_decrease || !stage_contract {
        return Err(DeterministicAlmostLinearMaxFlowError::TraceInvariant);
    }
    Ok(())
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
    {
        return Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit);
    }
    if source == sink
        || graph.node(source).is_none()
        || graph.node(sink).is_none()
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.capacity() == 0 || edge.from() == edge.to())
    {
        return Err(DeterministicAlmostLinearMaxFlowError::GraphRequirement);
    }
    let assignments = graph.edges().iter().try_fold(1_u64, |count, edge| {
        count.checked_mul(edge.capacity().saturating_add(1))
    });
    if assignments.is_none_or(|count| count > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS)
    {
        return Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit);
    }
    Ok(())
}

fn enumerate_min_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(u64, Vec<bool>, u64), DeterministicAlmostLinearMaxFlowError> {
    let n = graph.nodes().len();
    let mut best = u128::MAX;
    let mut best_side = vec![false; n];
    let mut inspected = 0_u64;
    for mask in 0..(1_u128 << n) {
        if mask & (1_u128 << source.as_usize()) == 0 || mask & (1_u128 << sink.as_usize()) != 0 {
            continue;
        }
        inspected = inspected.saturating_add(1);
        let mut value = 0_u128;
        for edge in graph.edges() {
            let from = mask & (1_u128 << edge.from().as_usize()) != 0;
            let to = mask & (1_u128 << edge.to().as_usize()) != 0;
            if from && !to {
                value = value
                    .checked_add(u128::from(edge.capacity()))
                    .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
            }
        }
        if value < best {
            best = value;
            for (node, side) in best_side.iter_mut().enumerate() {
                *side = mask & (1_u128 << node) != 0;
            }
        }
    }
    Ok((
        u64::try_from(best)
            .map_err(|_| DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?,
        best_side,
        inspected,
    ))
}

fn enumerate_feasible_return_face(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut DeterministicAlmostLinearMaxFlowMetrics,
) -> Result<FeasibleReturnFace, DeterministicAlmostLinearMaxFlowError> {
    fn recurse(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        edge_index: usize,
        flows: &mut [u64],
        circulations: &mut Vec<FeasibleReturnCirculation>,
        metrics: &mut DeterministicAlmostLinearMaxFlowMetrics,
    ) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
        if edge_index < graph.edges().len() {
            for flow in 0..=graph.edges()[edge_index].capacity() {
                flows[edge_index] = flow;
                recurse(
                    graph,
                    source,
                    sink,
                    edge_index + 1,
                    flows,
                    circulations,
                    metrics,
                )?;
            }
            return Ok(());
        }
        metrics.enumerated_assignments = metrics
            .enumerated_assignments
            .checked_add(1)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        if metrics.enumerated_assignments > DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS {
            return Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit);
        }
        let mut divergence = vec![0_i128; graph.nodes().len()];
        for (edge, flow) in graph.edges().iter().zip(flows.iter().copied()) {
            divergence[edge.from().as_usize()] = divergence[edge.from().as_usize()]
                .checked_add(i128::from(flow))
                .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
            divergence[edge.to().as_usize()] = divergence[edge.to().as_usize()]
                .checked_sub(i128::from(flow))
                .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        }
        let value = divergence[source.as_usize()];
        if value < 0
            || divergence[sink.as_usize()] != -value
            || divergence.iter().enumerate().any(|(node, balance)| {
                node != source.as_usize() && node != sink.as_usize() && *balance != 0
            })
        {
            return Ok(());
        }
        circulations.push(FeasibleReturnCirculation {
            original_flows: flows.to_vec(),
            return_flow: u64::try_from(value)
                .map_err(|_| DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?,
        });
        Ok(())
    }

    let mut circulations = Vec::new();
    let mut flows = vec![0_u64; graph.edges().len()];
    recurse(
        graph,
        source,
        sink,
        0,
        &mut flows,
        &mut circulations,
        metrics,
    )?;
    if circulations.is_empty() {
        return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
    }
    metrics.feasible_circulations = u64::try_from(circulations.len())
        .map_err(|_| DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let mut original_flow_sums = vec![0_u128; graph.edges().len()];
    let mut return_flow_sum = 0_u128;
    for circulation in &circulations {
        for (sum, flow) in original_flow_sums
            .iter_mut()
            .zip(&circulation.original_flows)
        {
            *sum = sum
                .checked_add(u128::from(*flow))
                .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        }
        return_flow_sum = return_flow_sum
            .checked_add(u128::from(circulation.return_flow))
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    Ok(FeasibleReturnFace {
        circulations,
        original_flow_sums,
        return_flow_sum,
    })
}

fn construct_deterministic_final_point(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    return_capacity: u64,
    target: u64,
    face: &FeasibleReturnFace,
) -> Result<DeterministicFinalPoint, DeterministicAlmostLinearMaxFlowError> {
    let anchor = face
        .circulations
        .iter()
        .find(|circulation| circulation.return_flow == target)
        .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
    let face_size = u128::try_from(face.circulations.len())
        .map_err(|_| DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let target_sum = u128::from(target)
        .checked_mul(face_size)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let gap_sum = target_sum
        .checked_sub(face.return_flow_sum)
        .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
    let mut mix_denominator = 4_u128;
    while gap_sum
        .checked_mul(2)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?
        >= face_size
            .checked_mul(mix_denominator)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?
    {
        mix_denominator = mix_denominator
            .checked_mul(2)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    let denominator = face_size
        .checked_mul(mix_denominator)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let coordinate = |sum: u128,
                      anchor_value: u64|
     -> Result<BigRational, DeterministicAlmostLinearMaxFlowError> {
        let anchor_scaled = u128::from(anchor_value)
            .checked_mul(denominator)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        let anchor_face = u128::from(anchor_value)
            .checked_mul(face_size)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        let numerator = if sum >= anchor_face {
            anchor_scaled
                .checked_add(sum - anchor_face)
                .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?
        } else {
            anchor_scaled
                .checked_sub(anchor_face - sum)
                .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?
        };
        Ok(BigRational::new(
            BigInt::from(numerator),
            BigInt::from(denominator),
        ))
    };
    let original_flows = face
        .original_flow_sums
        .iter()
        .zip(&anchor.original_flows)
        .map(|(&sum, &flow)| coordinate(sum, flow))
        .collect::<Result<Vec<_>, _>>()?;
    let return_flow = coordinate(face.return_flow_sum, anchor.return_flow)?;
    let gap = BigRational::new(BigInt::from(gap_sum), BigInt::from(denominator));
    if gap < BigRational::zero() || gap >= BigRational::new(BigInt::one(), BigInt::from(2)) {
        return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
    }
    let mix = BigRational::new(BigInt::one(), BigInt::from(mix_denominator));
    let (augmented_graph, original_to_augmented, return_index) =
        build_rounding_network(graph, source, sink, return_capacity)?;
    let mut augmented_flows = vec![BigRational::zero(); augmented_graph.edges().len()];
    for (&index, flow) in original_to_augmented.iter().zip(&original_flows) {
        augmented_flows[index] = flow.clone();
    }
    augmented_flows[return_index] = return_flow.clone();
    Ok(DeterministicFinalPoint {
        augmented_graph,
        original_to_augmented,
        return_index,
        augmented_flows,
        original_flows,
        return_flow,
        gap,
        mix,
    })
}

fn build_rounding_network(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    return_capacity: u64,
) -> Result<(FlowNetwork, Vec<usize>, usize), DeterministicAlmostLinearMaxFlowError> {
    let mut unresolved = graph
        .edges()
        .iter()
        .map(|edge| UnresolvedFlowEdge {
            id: edge.id().clone(),
            from: graph.nodes()[edge.from().as_usize()].id().clone(),
            to: graph.nodes()[edge.to().as_usize()].id().clone(),
            lower: 0,
            capacity: edge.capacity(),
            cost: 0,
        })
        .collect::<Vec<_>>();
    let return_id = (0..=graph.edges().len())
        .find_map(|suffix| {
            let text = if suffix == 0 {
                "deterministic-rounding-return".to_owned()
            } else {
                format!("deterministic-rounding-return-{suffix}")
            };
            EdgeId::parse(&text).ok().filter(|candidate| {
                graph.edge_index(candidate).is_none()
                    && unresolved.iter().all(|edge| edge.id != *candidate)
            })
        })
        .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
    unresolved.push(UnresolvedFlowEdge {
        id: return_id.clone(),
        from: graph.nodes()[sink.as_usize()].id().clone(),
        to: graph.nodes()[source.as_usize()].id().clone(),
        lower: 0,
        capacity: return_capacity,
        cost: -1,
    });
    let augmented = FlowNetwork::new(graph.nodes().to_vec(), unresolved)?;
    let original_to_augmented = graph
        .edges()
        .iter()
        .map(|edge| {
            augmented
                .edge_index(edge.id())
                .map(crate::model::EdgeIndex::as_usize)
                .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_index = augmented
        .edge_index(&return_id)
        .map(crate::model::EdgeIndex::as_usize)
        .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
    Ok((augmented, original_to_augmented, return_index))
}

#[allow(clippy::too_many_lines)]
fn initialize_kernel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    target: u64,
) -> Result<KernelState, DeterministicAlmostLinearMaxFlowError> {
    let n = graph.nodes().len();
    let star = n;
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .ok_or(DeterministicAlmostLinearMaxFlowError::GraphRequirement)?;
    let return_capacity = (graph.edges().len() as u64)
        .checked_mul(maximum_capacity)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let mut work_edges = Vec::with_capacity(graph.edges().len() + n + 1);
    for (index, edge) in graph.edges().iter().enumerate() {
        work_edges.push(WorkEdge {
            kind: WorkEdgeKind::Original(index),
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            lower: 0.0,
            upper: edge.capacity() as f64,
            cost: 0.0,
            flow: edge.capacity() as f64 / 2.0,
            gradient: 0.0,
            length: 1.0,
            approximate_gradient: 0.0,
            approximate_length: 1.0,
            changed: false,
        });
    }
    work_edges.push(WorkEdge {
        kind: WorkEdgeKind::Return,
        from: sink.as_usize(),
        to: source.as_usize(),
        lower: 0.0,
        upper: return_capacity as f64,
        cost: -1.0,
        flow: return_capacity as f64 / 2.0,
        gradient: 0.0,
        length: 1.0,
        approximate_gradient: 0.0,
        approximate_length: 1.0,
        changed: false,
    });

    let mut divergence = vec![0.0; n];
    for edge in &work_edges {
        divergence[edge.from] += edge.flow;
        divergence[edge.to] -= edge.flow;
    }
    let work_bound = maximum_capacity.max(return_capacity) as f64;
    let artificial_cost = 4.0 * (graph.edges().len() + n + 1) as f64 * work_bound * work_bound;
    for (node, value) in divergence.into_iter().enumerate() {
        if value.abs() <= POSITIVE_FLOOR {
            continue;
        }
        let (from, to) = if value > 0.0 {
            (star, node)
        } else {
            (node, star)
        };
        let flow = value.abs();
        work_edges.push(WorkEdge {
            kind: WorkEdgeKind::Artificial(node),
            from,
            to,
            lower: 0.0,
            upper: 2.0 * flow,
            cost: artificial_cost,
            flow,
            gradient: 0.0,
            length: 1.0,
            approximate_gradient: 0.0,
            approximate_length: 1.0,
            changed: false,
        });
    }
    check_work_circulation(star + 1, &work_edges)?;
    let alpha = 1.0
        / (1000.0
            * ((work_edges.len() as f64) * work_bound.max(2.0))
                .ln()
                .max(1.0));
    let mut state = KernelState {
        star,
        target,
        return_capacity,
        alpha,
        optimum_cost: -(target as f64),
        work_edges,
        forests: Vec::new(),
        branches: vec![Vec::new(); DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS],
        active_branches: vec![0; DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS],
        passes: vec![0; DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS],
        active_level: None,
        active_geometry: None,
        active: None,
        exact_pool_ratio: None,
        iteration: 0,
        rebuild_epoch: 0,
        final_point_flows: None,
        final_point_return_flow: None,
        final_point_gap: None,
        final_point_mix: None,
        rounding_flows: None,
        rounding_return_flow: None,
        rounding_forest: vec![false; graph.edges().len()],
        rounding_cycle_signs: vec![0; graph.edges().len()],
        rounding_return_forest: false,
        rounding_return_sign: 0,
        rounding_processed_edge: None,
        final_return_flow: None,
        final_flows: None,
        metrics: DeterministicAlmostLinearMaxFlowMetrics::default(),
    };
    update_gradient_lengths(&mut state)?;
    Ok(state)
}

fn update_gradient_lengths(
    state: &mut KernelState,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let gap = work_objective(&state.work_edges) - state.optimum_cost;
    if !(gap.is_finite() && gap > 0.0) {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    let m = state.work_edges.len() as f64;
    for edge in &mut state.work_edges {
        let upper_slack = edge.upper - edge.flow;
        let lower_slack = edge.flow - edge.lower;
        if !(upper_slack > POSITIVE_FLOOR && lower_slack > POSITIVE_FLOOR) {
            return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
        }
        edge.length = upper_slack.powf(-1.0 - state.alpha) + lower_slack.powf(-1.0 - state.alpha);
        edge.gradient = 20.0 * m * edge.cost / gap
            + state.alpha * upper_slack.powf(-1.0 - state.alpha)
            - state.alpha * lower_slack.powf(-1.0 - state.alpha);
        if !(edge.length.is_finite() && edge.length > 0.0 && edge.gradient.is_finite()) {
            return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
        }
    }
    Ok(())
}

fn work_objective(edges: &[WorkEdge]) -> f64 {
    edges.iter().map(|edge| edge.cost * edge.flow).sum()
}

fn work_potential(
    state: &KernelState,
) -> Result<(f64, f64), DeterministicAlmostLinearMaxFlowError> {
    let gap = work_objective(&state.work_edges) - state.optimum_cost;
    if !(gap.is_finite() && gap > 0.0) {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    let barrier: f64 = state
        .work_edges
        .iter()
        .map(|edge| {
            (edge.upper - edge.flow).powf(-state.alpha)
                + (edge.flow - edge.lower).powf(-state.alpha)
        })
        .sum();
    let potential = 20.0 * state.work_edges.len() as f64 * gap.ln() + barrier;
    if !potential.is_finite() {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    Ok((potential, gap))
}

fn check_work_circulation(
    nodes: usize,
    edges: &[WorkEdge],
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let mut divergence = vec![0.0; nodes];
    for edge in edges {
        if !(edge.flow > edge.lower && edge.flow < edge.upper) {
            return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
        }
        divergence[edge.from] += edge.flow;
        divergence[edge.to] -= edge.flow;
    }
    if divergence
        .into_iter()
        .any(|value| value.abs() > NUMERICAL_TOLERANCE)
    {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    Ok(())
}

fn enumerate_spanning_forests(
    nodes: usize,
    edges: &[WorkEdge],
    metrics: &mut DeterministicAlmostLinearMaxFlowMetrics,
) -> Result<Vec<Forest>, DeterministicAlmostLinearMaxFlowError> {
    let target_components = graph_component_count(nodes, edges);
    let target_edges = nodes.saturating_sub(target_components);
    let mut forests = Vec::new();
    let mut selected = Vec::with_capacity(target_edges);
    enumerate_forest_subsets(
        nodes,
        edges,
        target_components,
        target_edges,
        0,
        &mut selected,
        &mut forests,
        metrics,
    )?;
    if forests.is_empty() {
        return Err(DeterministicAlmostLinearMaxFlowError::ForestInvariant);
    }
    Ok(forests)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_forest_subsets(
    nodes: usize,
    edges: &[WorkEdge],
    target_components: usize,
    target_edges: usize,
    next: usize,
    selected: &mut Vec<usize>,
    forests: &mut Vec<Forest>,
    metrics: &mut DeterministicAlmostLinearMaxFlowMetrics,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    if forests.len() >= DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS {
        return Err(DeterministicAlmostLinearMaxFlowError::ForestLimit);
    }
    if selected.len() == target_edges {
        metrics.forest_subsets = metrics.forest_subsets.saturating_add(1);
        let mut dsu = Dsu::new(nodes);
        for edge_index in selected.iter().copied() {
            let edge = &edges[edge_index];
            if !dsu.union(edge.from, edge.to) {
                return Ok(());
            }
        }
        if dsu.component_count() == target_components {
            forests.push(Forest {
                edges: selected.clone(),
            });
        }
        return Ok(());
    }
    let needed = target_edges - selected.len();
    if edges.len().saturating_sub(next) < needed {
        return Ok(());
    }
    for edge_index in next..=edges.len() - needed {
        selected.push(edge_index);
        enumerate_forest_subsets(
            nodes,
            edges,
            target_components,
            target_edges,
            edge_index + 1,
            selected,
            forests,
            metrics,
        )?;
        selected.pop();
    }
    Ok(())
}

fn graph_component_count(nodes: usize, edges: &[WorkEdge]) -> usize {
    let mut dsu = Dsu::new(nodes);
    for edge in edges {
        dsu.union(edge.from, edge.to);
    }
    dsu.component_count()
}

fn build_branch_collection(
    state: &mut KernelState,
    reset_game: bool,
) -> Result<BranchCollectionCheckpoints, DeterministicAlmostLinearMaxFlowError> {
    if state.forests.is_empty() {
        return Err(DeterministicAlmostLinearMaxFlowError::ForestInvariant);
    }
    let mut scored = Vec::with_capacity(state.forests.len());
    let mut exact: Option<CycleCandidate> = None;
    let mut cycle_evaluations = Vec::new();
    for (index, forest) in state.forests.iter().enumerate() {
        let score = forest_stretch_score(forest, state.star + 1, &state.work_edges)?;
        let candidate = best_fundamental_cycle(
            index,
            forest,
            FundamentalCycleContext {
                level: 0,
                branch: 0,
                geometry: None,
                nodes: state.star + 1,
                edges: &state.work_edges,
            },
            &mut state.metrics,
            &mut cycle_evaluations,
        )?;
        if let Some(candidate) = candidate
            && exact
                .as_ref()
                .is_none_or(|incumbent| candidate_better(&candidate, incumbent))
        {
            exact = Some(candidate);
        }
        scored.push((score, index));
    }
    scored.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                state.forests[left.1]
                    .edges
                    .cmp(&state.forests[right.1].edges)
            })
    });
    let exact_tree = exact
        .as_ref()
        .map_or(scored[0].1, |candidate| candidate.forest_index);
    state.exact_pool_ratio = exact.as_ref().map(|candidate| candidate.ratio);
    let rebuild_epoch = usize::try_from(state.rebuild_epoch)
        .map_err(|_| DeterministicAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let previous_active_branches = state.active_branches.clone();
    state.branches.iter_mut().for_each(Vec::clear);
    state.active_branches.fill(0);
    state.active = None;
    let mut branch_records = Vec::with_capacity(
        DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS * DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES,
    );
    for level in 0..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS {
        for branch in 0..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES {
            let index = if branch + 1 == DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES {
                exact_tree
            } else {
                let offset = (level * DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES
                    + branch
                    + rebuild_epoch)
                    % scored.len();
                scored[offset].1
            };
            let geometry = build_branch_geometry(state, index, level)?;
            state.branches[level].push(geometry.clone());
            state.active_level = Some(level);
            state.active_branches[level] = branch;
            state.active_geometry = Some(geometry);
            state.metrics.branch_records = state.metrics.branch_records.saturating_add(1);
            branch_records.push(state.clone());
        }
    }
    if reset_game {
        state.active_branches.fill(0);
        state.passes.fill(0);
    } else {
        state.active_branches = previous_active_branches;
    }
    state.active = None;
    state.active_level = None;
    state.active_geometry = None;
    Ok(BranchCollectionCheckpoints {
        cycle_evaluations,
        branch_records,
    })
}

fn forest_stretch_score(
    forest: &Forest,
    nodes: usize,
    edges: &[WorkEdge],
) -> Result<f64, DeterministicAlmostLinearMaxFlowError> {
    let mut tree_membership = vec![false; edges.len()];
    let mut adjacency = vec![Vec::new(); nodes];
    for edge_index in forest.edges.iter().copied() {
        tree_membership[edge_index] = true;
        let edge = &edges[edge_index];
        adjacency[edge.from].push((edge.to, edge_index, 1_i8));
        adjacency[edge.to].push((edge.from, edge_index, -1_i8));
    }
    let mut score = 0.0;
    for (index, edge) in edges.iter().enumerate() {
        if tree_membership[index] {
            continue;
        }
        if let Some(path) = tree_path(edge.from, edge.to, &adjacency) {
            let path_length: f64 = path
                .iter()
                .map(|(path_edge, _)| edges[*path_edge].length)
                .sum();
            score += path_length / edge.length.max(POSITIVE_FLOOR);
        }
    }
    if score.is_finite() {
        Ok(score)
    } else {
        Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure)
    }
}

fn build_branch_geometry(
    state: &mut KernelState,
    tree_index: usize,
    level: usize,
) -> Result<BranchGeometry, DeterministicAlmostLinearMaxFlowError> {
    let tree = state
        .forests
        .get(tree_index)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?;
    let mut removable = tree.edges.clone();
    removable.sort_by(|left, right| {
        state.work_edges[*right]
            .length
            .partial_cmp(&state.work_edges[*left].length)
            .unwrap_or(Ordering::Equal)
            .then_with(|| right.cmp(left))
    });
    let remove = (level + 1).min(removable.len().saturating_sub(1));
    let removed = &removable[..remove];
    let forest_edges = tree
        .edges
        .iter()
        .copied()
        .filter(|edge| !removed.contains(edge))
        .collect::<Vec<_>>();
    let components = component_projection(state.star + 1, &state.work_edges, &forest_edges);
    let core_edges = state
        .work_edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| (components[edge.from] != components[edge.to]).then_some(index))
        .collect::<Vec<_>>();
    let component_count = components.iter().copied().max().unwrap_or(0) + 1;
    let mut order = core_edges.clone();
    order.sort_by(|left, right| {
        state.work_edges[*left]
            .length
            .partial_cmp(&state.work_edges[*right].length)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.cmp(right))
    });
    let mut dsu = Dsu::new(component_count);
    let mut spanner_edges = Vec::new();
    for edge_index in order {
        let edge = &state.work_edges[edge_index];
        if dsu.union(components[edge.from], components[edge.to]) {
            spanner_edges.push(edge_index);
        }
    }
    let mut adjacency = vec![Vec::new(); component_count];
    for edge_index in &spanner_edges {
        let edge = &state.work_edges[*edge_index];
        let from = components[edge.from];
        let to = components[edge.to];
        adjacency[from].push((to, *edge_index, 1_i8));
        adjacency[to].push((from, *edge_index, -1_i8));
    }
    let mut embeddings = vec![Vec::new(); state.work_edges.len()];
    let mut stretch = vec![0.0; state.work_edges.len()];
    for edge_index in &core_edges {
        let edge = &state.work_edges[*edge_index];
        let path = tree_path(components[edge.from], components[edge.to], &adjacency)
            .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?;
        embeddings[*edge_index] = path.iter().map(|(path_edge, _)| *path_edge).collect();
        let embedded_length: f64 = path
            .iter()
            .map(|(path_edge, _)| state.work_edges[*path_edge].length)
            .sum();
        stretch[*edge_index] = embedded_length / edge.length.max(POSITIVE_FLOOR);
        if !stretch[*edge_index].is_finite() {
            return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
        }
        state.metrics.spanner_embeddings = state.metrics.spanner_embeddings.saturating_add(1);
    }
    state.metrics.core_builds = state.metrics.core_builds.saturating_add(1);
    Ok(BranchGeometry {
        tree_index,
        forest_edges,
        components,
        core_edges,
        spanner_edges,
        embeddings,
        stretch,
    })
}

fn component_projection(nodes: usize, edges: &[WorkEdge], selected: &[usize]) -> Vec<usize> {
    let mut dsu = Dsu::new(nodes);
    for edge_index in selected {
        let edge = &edges[*edge_index];
        dsu.union(edge.from, edge.to);
    }
    let mut roots = Vec::<usize>::new();
    (0..nodes)
        .map(|node| {
            let root = dsu.find(node);
            roots
                .iter()
                .position(|value| *value == root)
                .unwrap_or_else(|| {
                    roots.push(root);
                    roots.len() - 1
                })
        })
        .collect()
}

fn prepare_active_geometry(
    state: &mut KernelState,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let level = state
        .active_level
        .unwrap_or(DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS - 1);
    let branch = state.active_branches[level];
    state.active_geometry = Some(
        state.branches[level]
            .get(branch)
            .cloned()
            .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?,
    );
    Ok(())
}

fn query_tree_chain(
    state: &mut KernelState,
) -> Result<(bool, Vec<CycleEvaluationCheckpoint>), DeterministicAlmostLinearMaxFlowError> {
    let mut exact: Option<CycleCandidate> = None;
    let mut checkpoints = Vec::new();
    for (index, forest) in state.forests.iter().enumerate() {
        let best = best_fundamental_cycle(
            index,
            forest,
            FundamentalCycleContext {
                level: 0,
                branch: 0,
                geometry: None,
                nodes: state.star + 1,
                edges: &state.work_edges,
            },
            &mut state.metrics,
            &mut checkpoints,
        )?;
        if let Some(candidate) = &best
            && exact
                .as_ref()
                .is_none_or(|incumbent| candidate_better(candidate, incumbent))
        {
            exact = Some(candidate.clone());
        }
    }
    let mut chain_best: Option<CycleCandidate> = None;
    for level in 0..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS {
        let branch = state.active_branches[level];
        let geometry = &state.branches[level][branch];
        let forest = &state.forests[geometry.tree_index];
        if let Some(candidate) = best_fundamental_cycle(
            geometry.tree_index,
            forest,
            FundamentalCycleContext {
                level,
                branch,
                geometry: Some(geometry),
                nodes: state.star + 1,
                edges: &state.work_edges,
            },
            &mut state.metrics,
            &mut checkpoints,
        )? && chain_best
            .as_ref()
            .is_none_or(|incumbent| candidate_better(&candidate, incumbent))
        {
            chain_best = Some(candidate);
        }
    }
    let Some(exact) = exact else {
        state.active = None;
        state.exact_pool_ratio = None;
        return Ok((false, checkpoints));
    };
    state.exact_pool_ratio = Some(exact.ratio);
    let approximation = 4.0 * (state.work_edges.len() as f64).ln().max(1.0);
    let threshold = exact.ratio / approximation;
    let successful = chain_best
        .as_ref()
        .is_some_and(|candidate| candidate.ratio <= threshold + NUMERICAL_TOLERANCE);
    if let Some(candidate) = &chain_best {
        state.active_level = Some(candidate.level);
        state.active_geometry = Some(state.branches[candidate.level][candidate.branch].clone());
    }
    state.active = chain_best;
    if successful {
        state.metrics.successful_queries = state.metrics.successful_queries.saturating_add(1);
    }
    Ok((successful, checkpoints))
}

fn best_fundamental_cycle(
    forest_index: usize,
    forest: &Forest,
    context: FundamentalCycleContext<'_>,
    metrics: &mut DeterministicAlmostLinearMaxFlowMetrics,
    checkpoints: &mut Vec<CycleEvaluationCheckpoint>,
) -> Result<Option<CycleCandidate>, DeterministicAlmostLinearMaxFlowError> {
    let FundamentalCycleContext {
        level,
        branch,
        geometry,
        nodes,
        edges,
    } = context;
    let mut tree_membership = vec![false; edges.len()];
    let mut adjacency = vec![Vec::new(); nodes];
    for edge_index in forest.edges.iter().copied() {
        tree_membership[edge_index] = true;
        let edge = &edges[edge_index];
        adjacency[edge.from].push((edge.to, edge_index, 1_i8));
        adjacency[edge.to].push((edge.from, edge_index, -1_i8));
    }
    let mut best: Option<CycleCandidate> = None;
    for (off_tree, edge) in edges.iter().enumerate() {
        if tree_membership[off_tree] {
            continue;
        }
        let Some(path) = tree_path(edge.to, edge.from, &adjacency) else {
            continue;
        };
        metrics.fundamental_cycles = metrics.fundamental_cycles.saturating_add(1);
        let mut signs = vec![0_i8; edges.len()];
        signs[off_tree] = 1;
        for (path_edge, sign) in path {
            signs[path_edge] = sign;
        }
        let (mut numerator, denominator) = cycle_value(edges, &signs)?;
        if numerator > 0.0 {
            for sign in &mut signs {
                *sign = -*sign;
            }
            numerator = -numerator;
        }
        let candidate = CycleCandidate {
            forest_index,
            level,
            branch,
            off_tree_edge: off_tree,
            kind: geometry.map_or(DeterministicAlmostLinearCycleKind::Tree, |geometry| {
                if geometry.core_edges.contains(&off_tree)
                    && !geometry.spanner_edges.contains(&off_tree)
                {
                    DeterministicAlmostLinearCycleKind::Spanner
                } else {
                    DeterministicAlmostLinearCycleKind::Tree
                }
            }),
            signs,
            ratio: numerator / denominator,
            numerator,
        };
        if metrics.fundamental_cycles.is_power_of_two() {
            checkpoints.push(CycleEvaluationCheckpoint {
                candidate: candidate.clone(),
                metrics: *metrics,
            });
        }
        if numerator >= -NUMERICAL_TOLERANCE {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|incumbent| candidate_better(&candidate, incumbent))
        {
            best = Some(candidate);
        }
    }
    Ok(best)
}

fn tree_path(
    start: usize,
    target: usize,
    adjacency: &[Vec<(usize, usize, i8)>],
) -> Option<Vec<(usize, i8)>> {
    let mut parent = vec![None; adjacency.len()];
    let mut queue = VecDeque::from([start]);
    parent[start] = Some((start, usize::MAX, 0_i8));
    while let Some(node) = queue.pop_front() {
        if node == target {
            break;
        }
        for &(next, edge, sign) in &adjacency[node] {
            if parent[next].is_none() {
                parent[next] = Some((node, edge, sign));
                queue.push_back(next);
            }
        }
    }
    parent[target]?;
    let mut reversed = Vec::new();
    let mut node = target;
    while node != start {
        let (previous, edge, sign) = parent[node]?;
        reversed.push((edge, sign));
        node = previous;
    }
    reversed.reverse();
    Some(reversed)
}

fn cycle_value(
    edges: &[WorkEdge],
    signs: &[i8],
) -> Result<(f64, f64), DeterministicAlmostLinearMaxFlowError> {
    let nodes = edges
        .iter()
        .map(|edge| edge.from.max(edge.to))
        .max()
        .unwrap_or(0)
        + 1;
    let mut divergence = vec![0_i32; nodes];
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (edge, sign) in edges.iter().zip(signs) {
        if !matches!(*sign, -1..=1) {
            return Err(DeterministicAlmostLinearMaxFlowError::ForestInvariant);
        }
        numerator += edge.gradient * f64::from(*sign);
        denominator += edge.length * f64::from(sign.abs());
        divergence[edge.from] += i32::from(*sign);
        divergence[edge.to] -= i32::from(*sign);
    }
    if denominator <= 0.0 || divergence.into_iter().any(|value| value != 0) {
        return Err(DeterministicAlmostLinearMaxFlowError::ForestInvariant);
    }
    Ok((numerator, denominator))
}

fn candidate_better(left: &CycleCandidate, right: &CycleCandidate) -> bool {
    left.ratio
        .partial_cmp(&right.ratio)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.forest_index.cmp(&right.forest_index))
        .then_with(|| left.signs.cmp(&right.signs))
        == Ordering::Less
}

fn apply_potential_step(
    state: &mut KernelState,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let candidate = state
        .active
        .clone()
        .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?;
    let (before, _) = work_potential(state)?;
    let kappa = (-candidate.ratio).min(1.0 / 16.0);
    if !(kappa > 0.0 && candidate.numerator < 0.0) {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    // Theorem 3.1: eta * <g,Delta> = -kappa^2 / 50.
    let mut eta = kappa * kappa / (50.0 * candidate.numerator.abs());
    let feasible_eta = state
        .work_edges
        .iter()
        .zip(&candidate.signs)
        .filter(|(_, sign)| **sign != 0)
        .map(|(edge, sign)| {
            if *sign > 0 {
                (edge.upper - edge.flow) * 0.05
            } else {
                (edge.flow - edge.lower) * 0.05
            }
        })
        .fold(f64::INFINITY, f64::min);
    eta = eta.min(feasible_eta);
    if !(eta.is_finite() && eta > 0.0) {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    for (edge, sign) in state.work_edges.iter_mut().zip(&candidate.signs) {
        edge.flow += eta * f64::from(*sign);
    }
    check_work_circulation(state.star + 1, &state.work_edges)?;
    let (after, _) = work_potential(state)?;
    if after > before + NUMERICAL_TOLERANCE {
        return Err(DeterministicAlmostLinearMaxFlowError::NumericalFailure);
    }
    state.iteration = state.iteration.saturating_add(1);
    state.metrics.potential_steps = state.metrics.potential_steps.saturating_add(1);
    Ok(())
}

fn detect_changed_coordinates(
    state: &mut KernelState,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    update_gradient_lengths(state)?;
    for edge in &mut state.work_edges {
        let length_change = ((edge.length / edge.approximate_length) - 1.0).abs();
        let gradient_change =
            (edge.gradient - edge.approximate_gradient).abs() / edge.length.max(POSITIVE_FLOOR);
        edge.changed = length_change > CHANGE_THRESHOLD || gradient_change > CHANGE_THRESHOLD;
        if edge.changed {
            edge.approximate_length = edge.length;
            edge.approximate_gradient = edge.gradient;
            state.metrics.detected_coordinates =
                state.metrics.detected_coordinates.saturating_add(1);
        }
    }
    Ok(())
}

fn shift_largest_eligible_level(state: &mut KernelState) -> Option<usize> {
    let level = (0..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS)
        .rev()
        .find(|level| state.passes[*level] < DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_PASSES)?;
    let next = state.active_branches[level].saturating_add(1);
    if next == DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES {
        state.active_branches[level] = 0;
        state.passes[level] = state.passes[level].saturating_add(1);
        state.metrics.branch_wraps = state.metrics.branch_wraps.saturating_add(1);
    } else {
        state.active_branches[level] = next;
    }
    state.metrics.branch_shifts = state.metrics.branch_shifts.saturating_add(1);
    state.active_level = Some(level);
    state.active = None;
    Some(level)
}

fn rebuild_deeper_levels(
    state: &mut KernelState,
    shifted_level: usize,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let rotation = state.active_branches[shifted_level];
    for level in shifted_level + 1..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS {
        if state.branches[level].is_empty() {
            return Err(DeterministicAlmostLinearMaxFlowError::ForestInvariant);
        }
        let length = state.branches[level].len();
        state.branches[level].rotate_left((rotation + level) % length);
        state.active_branches[level] = 0;
        state.passes[level] = 0;
        state.metrics.deeper_rebuilds = state.metrics.deeper_rebuilds.saturating_add(1);
    }
    state.active = None;
    state.active_level = Some(shifted_level);
    Ok(())
}

fn apply_rounding_snapshot(
    state: &mut KernelState,
    final_point: &DeterministicFinalPoint,
    snapshot: &CostedFlowRoundingSnapshot,
    kind: &CostedFlowRoundingEventKind,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    state.rounding_flows = Some(
        final_point
            .original_to_augmented
            .iter()
            .map(|&index| snapshot.flows[index].clone())
            .collect(),
    );
    state.rounding_return_flow = Some(snapshot.flows[final_point.return_index].clone());
    state.rounding_forest = final_point
        .original_to_augmented
        .iter()
        .map(|&index| snapshot.fractional_forest[index])
        .collect();
    state.rounding_return_forest = snapshot.fractional_forest[final_point.return_index];
    state.rounding_cycle_signs.fill(0);
    state.rounding_return_sign = 0;
    state.rounding_processed_edge = match kind {
        CostedFlowRoundingEventKind::IntegralEdgeSkipped { edge }
        | CostedFlowRoundingEventKind::FractionalEdgeLinked { edge } => Some(edge.clone()),
        CostedFlowRoundingEventKind::FractionalCycleCanceled {
            inserted_edge,
            cycle,
            ..
        } => {
            for arc in cycle {
                let index = final_point
                    .augmented_graph
                    .edge_index(&arc.edge)
                    .map(crate::model::EdgeIndex::as_usize)
                    .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)?;
                if index == final_point.return_index {
                    state.rounding_return_sign = arc.direction;
                } else if let Some(original) = final_point
                    .original_to_augmented
                    .iter()
                    .position(|candidate| *candidate == index)
                {
                    state.rounding_cycle_signs[original] = arc.direction;
                } else {
                    return Err(DeterministicAlmostLinearMaxFlowError::FinalPointRounding);
                }
            }
            Some(inserted_edge.clone())
        }
        CostedFlowRoundingEventKind::Completed => None,
    };
    state.metrics.rounding_processed_edges = snapshot.metrics.processed_edges;
    state.metrics.rounding_cycles = snapshot.metrics.canceled_cycles;
    state.metrics.rounding_integralized_edges = snapshot.metrics.integralized_edges;
    Ok(())
}

fn original_rounded_flows(
    final_point: &DeterministicFinalPoint,
    augmented: &[u64],
) -> Result<Vec<u64>, DeterministicAlmostLinearMaxFlowError> {
    final_point
        .original_to_augmented
        .iter()
        .map(|&index| {
            augmented
                .get(index)
                .copied()
                .ok_or(DeterministicAlmostLinearMaxFlowError::FinalPointRounding)
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
fn snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
    cut_side: &[bool],
    boundary: DeterministicAlmostLinearMaxFlowStage,
) -> Result<DeterministicAlmostLinearMaxFlowSnapshot, DeterministicAlmostLinearMaxFlowError> {
    let active_forest = state
        .active_geometry
        .as_ref()
        .map(|geometry| geometry.tree_index);
    let active_signs = state
        .active
        .as_ref()
        .map(|candidate| candidate.signs.as_slice());
    let mut tree_level_mask = vec![0_u64; state.work_edges.len()];
    let mut forest_level_mask = vec![0_u64; state.work_edges.len()];
    for level in 0..DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS {
        let Some(collection) = state.branches.get(level) else {
            continue;
        };
        let Some(geometry) = collection.get(state.active_branches[level]) else {
            continue;
        };
        for edge in &state.forests[geometry.tree_index].edges {
            tree_level_mask[*edge] |= 1_u64 << level;
        }
        for edge in &geometry.forest_edges {
            forest_level_mask[*edge] |= 1_u64 << level;
        }
    }
    let (parents, tree_components) = active_forest.map_or_else(
        || (vec![None; state.star + 1], (0..=state.star).collect()),
        |index| forest_projection(state.star + 1, &state.work_edges, &state.forests[index]),
    );
    let components = state
        .active_geometry
        .as_ref()
        .map_or(tree_components, |geometry| geometry.components.clone());
    let final_flows = state.final_flows.as_deref();
    let final_point_flows = state.final_point_flows.as_deref();
    let rounding_flows = state.rounding_flows.as_deref();
    let mut nodes = Vec::with_capacity(graph.nodes().len());
    for node in graph.node_indices() {
        let node_index = node.as_usize();
        let artificial = state
            .work_edges
            .iter()
            .enumerate()
            .find(|(_, edge)| edge.kind == WorkEdgeKind::Artificial(node_index));
        let (direction, flow, capacity, tree_mask, active_tree, sign) =
            artificial.map_or((0, 0.0, 0.0, 0, false, 0), |(index, edge)| {
                (
                    if edge.from == state.star { 1 } else { -1 },
                    edge.flow,
                    edge.upper,
                    tree_level_mask[index],
                    active_forest
                        .is_some_and(|forest| state.forests[forest].edges.contains(&index)),
                    active_signs.map_or(0, |signs| signs[index]),
                )
            });
        nodes.push(DeterministicAlmostLinearNodeState {
            node,
            tree_parent: parents[node_index],
            tree_component: components[node_index],
            source_side: cut_side[node_index],
            artificial_direction: direction,
            artificial_flow: DeterministicAlmostLinearScalar::try_new(flow)?,
            artificial_capacity: DeterministicAlmostLinearScalar::try_new(capacity)?,
            artificial_tree_level_mask: tree_mask,
            active_artificial_tree_edge: active_tree,
            active_artificial_sign: sign,
        });
    }
    let mut edges = Vec::with_capacity(graph.edges().len());
    for (original, graph_edge) in graph.edges().iter().enumerate() {
        let (work_index, work_edge) = state
            .work_edges
            .iter()
            .enumerate()
            .find(|(_, edge)| edge.kind == WorkEdgeKind::Original(original))
            .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?;
        edges.push(DeterministicAlmostLinearEdgeState {
            edge: graph_edge.id().clone(),
            interior_flow: DeterministicAlmostLinearScalar::try_new(work_edge.flow)?,
            gradient: DeterministicAlmostLinearScalar::try_new(work_edge.gradient)?,
            length: DeterministicAlmostLinearScalar::try_new(work_edge.length)?,
            tree_level_mask: tree_level_mask[work_index],
            forest_level_mask: forest_level_mask[work_index],
            active_tree_edge: active_forest
                .is_some_and(|index| state.forests[index].edges.contains(&work_index)),
            active_core_edge: state
                .active_geometry
                .as_ref()
                .is_some_and(|geometry| geometry.core_edges.contains(&work_index)),
            active_spanner_edge: state
                .active_geometry
                .as_ref()
                .is_some_and(|geometry| geometry.spanner_edges.contains(&work_index)),
            embedding_hops: state
                .active_geometry
                .as_ref()
                .map_or(0, |geometry| geometry.embeddings[work_index].len() as u64),
            embedding_stretch: DeterministicAlmostLinearScalar::try_new(
                state
                    .active_geometry
                    .as_ref()
                    .map_or(0.0, |geometry| geometry.stretch[work_index]),
            )?,
            active_cycle_sign: active_signs.map_or(0, |signs| signs[work_index]),
            changed_coordinate: work_edge.changed,
            final_point_flow: final_point_flows.map(|flows| flows[original].clone()),
            rounding_flow: rounding_flows.map(|flows| flows[original].clone()),
            rounding_forest_edge: state.rounding_forest[original],
            rounding_cycle_sign: state.rounding_cycle_signs[original],
            final_flow: final_flows.map(|flows| flows[original]),
        });
    }
    let return_index = state
        .work_edges
        .iter()
        .position(|edge| edge.kind == WorkEdgeKind::Return)
        .ok_or(DeterministicAlmostLinearMaxFlowError::ForestInvariant)?;
    let return_edge = &state.work_edges[return_index];
    let artificial_edges = state
        .work_edges
        .iter()
        .filter(|edge| matches!(edge.kind, WorkEdgeKind::Artificial(_)))
        .count();
    let artificial_flow: f64 = state
        .work_edges
        .iter()
        .filter(|edge| matches!(edge.kind, WorkEdgeKind::Artificial(_)))
        .map(|edge| edge.flow)
        .sum();
    let (potential, gap) = work_potential(state)?;
    Ok(DeterministicAlmostLinearMaxFlowSnapshot {
        nodes,
        edges,
        alpha: DeterministicAlmostLinearScalar::try_new(state.alpha)?,
        potential: DeterministicAlmostLinearScalar::try_new(potential)?,
        cost_gap: DeterministicAlmostLinearScalar::try_new(gap)?,
        selected_ratio: state
            .active
            .as_ref()
            .map(|candidate| DeterministicAlmostLinearScalar::try_new(candidate.ratio))
            .transpose()?,
        exact_pool_ratio: state
            .exact_pool_ratio
            .map(DeterministicAlmostLinearScalar::try_new)
            .transpose()?,
        selected_off_tree_edge: state
            .active
            .as_ref()
            .map(|candidate| candidate.off_tree_edge as u64),
        selected_cycle_kind: state.active.as_ref().map(|candidate| candidate.kind),
        forest_pool_size: state.forests.len() as u64,
        level_count: DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_LEVELS as u64,
        branch_count: DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_BRANCHES as u64,
        active_branches: state
            .active_branches
            .iter()
            .map(|value| *value as u64)
            .collect(),
        passes: state.passes.clone(),
        active_level: state.active_level.map(|value| value as u64),
        core_vertices: state.active_geometry.as_ref().map_or(0, |geometry| {
            geometry.components.iter().copied().max().unwrap_or(0) as u64 + 1
        }),
        core_edges: state
            .active_geometry
            .as_ref()
            .map_or(0, |geometry| geometry.core_edges.len() as u64),
        spanner_edges: state
            .active_geometry
            .as_ref()
            .map_or(0, |geometry| geometry.spanner_edges.len() as u64),
        embedding_hops: state.active_geometry.as_ref().map_or(0, |geometry| {
            geometry.embeddings.iter().map(Vec::len).sum::<usize>() as u64
        }),
        iteration: state.iteration,
        rebuild_epoch: state.rebuild_epoch,
        return_flow: DeterministicAlmostLinearScalar::try_new(return_edge.flow)?,
        return_capacity: state.return_capacity,
        return_gradient: DeterministicAlmostLinearScalar::try_new(return_edge.gradient)?,
        return_length: DeterministicAlmostLinearScalar::try_new(return_edge.length)?,
        return_tree_level_mask: tree_level_mask[return_index],
        active_return_tree_edge: active_forest
            .is_some_and(|forest| state.forests[forest].edges.contains(&return_index)),
        active_return_sign: active_signs.map_or(0, |signs| signs[return_index]),
        final_point_return_flow: state.final_point_return_flow.clone(),
        rounding_return_flow: state.rounding_return_flow.clone(),
        rounding_return_forest_edge: state.rounding_return_forest,
        rounding_return_sign: state.rounding_return_sign,
        final_return_flow: state.final_return_flow,
        artificial_edges: artificial_edges as u64,
        artificial_flow: DeterministicAlmostLinearScalar::try_new(artificial_flow)?,
        final_artificial_flow: final_flows.map(|_| 0),
        final_point_gap: state.final_point_gap.clone(),
        final_point_threshold: BigRational::new(BigInt::one(), BigInt::from(2)),
        final_point_mix: state.final_point_mix.clone(),
        rounding_processed_edge: state.rounding_processed_edge.clone(),
        target_value: state.target,
        stage: boundary,
        metrics: state.metrics,
    })
}

fn publish(
    graph: &FlowNetwork,
    state: &KernelState,
    cut_side: &[bool],
    boundary: DeterministicAlmostLinearMaxFlowStage,
    current: &mut DeterministicAlmostLinearMaxFlowSnapshot,
    events: &mut Option<Vec<DeterministicAlmostLinearMaxFlowTraceEvent>>,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    if current.metrics.state_transitions
        >= DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS as u64
    {
        return Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit);
    }
    let mut after = snapshot(graph, state, cut_side, boundary)?;
    after.metrics.state_transitions = current.metrics.state_transitions.saturating_add(1);
    if let Some(events) = events {
        events.push(DeterministicAlmostLinearMaxFlowTraceEvent {
            catalog_id: stage_catalog_id(boundary),
            before: current.clone(),
            after: after.clone(),
        });
    }
    *current = after;
    Ok(())
}

fn publish_cycle_checkpoints(
    graph: &FlowNetwork,
    baseline: &KernelState,
    cut_side: &[bool],
    checkpoints: Vec<CycleEvaluationCheckpoint>,
    current: &mut DeterministicAlmostLinearMaxFlowSnapshot,
    events: &mut Option<Vec<DeterministicAlmostLinearMaxFlowTraceEvent>>,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    let mut checkpoint_state = baseline.clone();
    checkpoint_state.active_geometry = None;
    checkpoint_state.exact_pool_ratio = None;
    for checkpoint in checkpoints {
        checkpoint_state.active_level = Some(checkpoint.candidate.level);
        checkpoint_state.active = Some(checkpoint.candidate);
        checkpoint_state.metrics = checkpoint.metrics;
        publish(
            graph,
            &checkpoint_state,
            cut_side,
            DeterministicAlmostLinearMaxFlowStage::InspectFundamentalCycle,
            current,
            events,
        )?;
    }
    Ok(())
}

fn publish_branch_record_checkpoints(
    graph: &FlowNetwork,
    cut_side: &[bool],
    checkpoints: Vec<KernelState>,
    current: &mut DeterministicAlmostLinearMaxFlowSnapshot,
    events: &mut Option<Vec<DeterministicAlmostLinearMaxFlowTraceEvent>>,
) -> Result<(), DeterministicAlmostLinearMaxFlowError> {
    for checkpoint in checkpoints {
        publish(
            graph,
            &checkpoint,
            cut_side,
            DeterministicAlmostLinearMaxFlowStage::InstallBranchRecord,
            current,
            events,
        )?;
    }
    Ok(())
}

fn forest_projection(
    nodes: usize,
    edges: &[WorkEdge],
    forest: &Forest,
) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut adjacency = vec![Vec::new(); nodes];
    for edge_index in &forest.edges {
        let edge = &edges[*edge_index];
        adjacency[edge.from].push(edge.to);
        adjacency[edge.to].push(edge.from);
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut parents = vec![None; nodes];
    let mut components = vec![usize::MAX; nodes];
    let mut component = 0;
    for root in 0..nodes {
        if components[root] != usize::MAX {
            continue;
        }
        components[root] = component;
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for next in &adjacency[node] {
                if components[*next] == usize::MAX {
                    components[*next] = component;
                    parents[*next] = Some(node);
                    queue.push_back(*next);
                }
            }
        }
        component += 1;
    }
    (parents, components)
}

const fn stage_catalog_id(stage: DeterministicAlmostLinearMaxFlowStage) -> &'static str {
    use DeterministicAlmostLinearMaxFlowStage as Stage;
    match stage {
        Stage::Ready => "deterministic-almost-linear-max-flow-oracle-demonstrator.ready",
        Stage::BuildReturnEdgeReduction => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.return-edge"
        }
        Stage::BuildInitialPoint => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.initial-point"
        }
        Stage::EnumerateForestPool => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.forest-pool"
        }
        Stage::InstallBranchRecord => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.install-branch-record"
        }
        Stage::BuildBranchCollection => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.build-branches"
        }
        Stage::BuildCoreGraph => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.build-core"
        }
        Stage::BuildSpannerEmbedding => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.build-spanner"
        }
        Stage::InspectFundamentalCycle => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle"
        }
        Stage::QueryMinimumRatioCycle => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.query-cycle"
        }
        Stage::QueryFailure => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.query-failure"
        }
        Stage::ShiftBranch => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.shift-branch"
        }
        Stage::RebuildDeeperLevels => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.rebuild-deeper"
        }
        Stage::PotentialReductionStep => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.potential-step"
        }
        Stage::DetectChangedCoordinates => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.detect"
        }
        Stage::ScheduledRebuild => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.scheduled-rebuild"
        }
        Stage::EnumerateFeasibleSet => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.enumerate-feasible-set"
        }
        Stage::ConstructFinalPoint => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.construct-final-point"
        }
        Stage::RoundingIntegralEdge => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.round-integral-edge"
        }
        Stage::RoundingLinkFractionalEdge => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.link-fractional-edge"
        }
        Stage::RoundingCancelFractionalCycle => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.cancel-fractional-cycle"
        }
        Stage::FinishFlowRounding => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.finish-flow-rounding"
        }
        Stage::CheckCertificate => {
            "deterministic-almost-linear-max-flow-oracle-demonstrator.check-certificate"
        }
        Stage::Optimal => "deterministic-almost-linear-max-flow-oracle-demonstrator.optimal",
    }
}

#[allow(clippy::unnested_or_patterns)]
const fn valid_transition(
    before: DeterministicAlmostLinearMaxFlowStage,
    after: DeterministicAlmostLinearMaxFlowStage,
) -> bool {
    use DeterministicAlmostLinearMaxFlowStage as Stage;
    matches!(
        (before, after),
        (Stage::Ready, Stage::BuildReturnEdgeReduction)
            | (Stage::BuildReturnEdgeReduction, Stage::BuildInitialPoint)
            | (Stage::BuildInitialPoint, Stage::EnumerateForestPool)
            | (
                Stage::EnumerateForestPool | Stage::ScheduledRebuild,
                Stage::InstallBranchRecord
            )
            | (Stage::InstallBranchRecord, Stage::InstallBranchRecord)
            | (Stage::InstallBranchRecord, Stage::BuildBranchCollection)
            | (
                Stage::BuildBranchCollection | Stage::ScheduledRebuild | Stage::RebuildDeeperLevels,
                Stage::BuildCoreGraph
            )
            | (Stage::BuildCoreGraph, Stage::BuildSpannerEmbedding)
            | (
                Stage::EnumerateForestPool
                    | Stage::BuildSpannerEmbedding
                    | Stage::DetectChangedCoordinates
                    | Stage::ScheduledRebuild,
                Stage::InspectFundamentalCycle
            )
            | (
                Stage::InspectFundamentalCycle,
                Stage::InspectFundamentalCycle
            )
            | (
                Stage::InspectFundamentalCycle,
                Stage::BuildBranchCollection
                    | Stage::InstallBranchRecord
                    | Stage::ScheduledRebuild
                    | Stage::QueryMinimumRatioCycle
            )
            | (
                Stage::BuildSpannerEmbedding | Stage::DetectChangedCoordinates,
                Stage::QueryMinimumRatioCycle
            )
            | (
                Stage::QueryMinimumRatioCycle,
                Stage::PotentialReductionStep | Stage::QueryFailure
            )
            | (
                Stage::QueryFailure,
                Stage::ShiftBranch | Stage::EnumerateFeasibleSet
            )
            | (
                Stage::ShiftBranch,
                Stage::RebuildDeeperLevels | Stage::BuildCoreGraph
            )
            | (
                Stage::DetectChangedCoordinates,
                Stage::ScheduledRebuild | Stage::EnumerateFeasibleSet
            )
            | (
                Stage::PotentialReductionStep,
                Stage::DetectChangedCoordinates
            )
            | (Stage::EnumerateFeasibleSet, Stage::ConstructFinalPoint)
            | (
                Stage::ConstructFinalPoint
                    | Stage::RoundingIntegralEdge
                    | Stage::RoundingLinkFractionalEdge
                    | Stage::RoundingCancelFractionalCycle,
                Stage::RoundingIntegralEdge
                    | Stage::RoundingLinkFractionalEdge
                    | Stage::RoundingCancelFractionalCycle
                    | Stage::FinishFlowRounding
            )
            | (Stage::FinishFlowRounding, Stage::CheckCertificate)
            | (Stage::CheckCertificate, Stage::Optimal)
    )
}

#[derive(Clone, Debug)]
struct Dsu {
    parent: Vec<usize>,
    rank: Vec<u8>,
    components: usize,
}

impl Dsu {
    fn new(nodes: usize) -> Self {
        Self {
            parent: (0..nodes).collect(),
            rank: vec![0; nodes],
            components: nodes,
        }
    }
    fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            self.parent[node] = self.find(self.parent[node]);
        }
        self.parent[node]
    }
    fn union(&mut self, left: usize, right: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        if self.rank[left_root] < self.rank[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        if self.rank[left_root] == self.rank[right_root] {
            self.rank[left_root] = self.rank[left_root].saturating_add(1);
        }
        self.components = self.components.saturating_sub(1);
        true
    }
    const fn component_count(&self) -> usize {
        self.components
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::solve_dinic;

    fn fixture() -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edge = |id: &str, from: &str, to: &str, capacity: u64| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost: 0,
        };
        let graph = FlowNetwork::new(
            nodes,
            vec![
                edge("e0", "s", "a", 3),
                edge("e1", "s", "b", 2),
                edge("e2", "a", "b", 1),
                edge("e3", "a", "t", 2),
                edge("e4", "b", "t", 3),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        (graph, source, sink)
    }

    fn work_rich_fixture() -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "c", "d", "e", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edge = |id: &str, from: &str, to: &str| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity: 1,
            cost: 0,
        };
        let graph = FlowNetwork::new(
            nodes,
            vec![
                edge("sa", "s", "a"),
                edge("sb", "s", "b"),
                edge("ac", "a", "c"),
                edge("bc", "b", "c"),
                edge("cd", "c", "d"),
                edge("ce", "c", "e"),
                edge("dt", "d", "t"),
                edge("et", "e", "t"),
            ],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn work_rich_trace_preserves_geometric_cycle_checkpoints() {
        let (graph, source, sink) = work_rich_fixture();
        let trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        let checkpoints = trace
            .events
            .iter()
            .filter(|event| {
                event.after.stage == DeterministicAlmostLinearMaxFlowStage::InspectFundamentalCycle
            })
            .count();
        let required = u64::BITS
            - trace
                .final_snapshot
                .metrics
                .fundamental_cycles
                .leading_zeros();
        assert!(checkpoints >= required as usize);
    }

    #[test]
    fn reduction_tree_chain_additive_half_final_point_and_rounding_are_certified() {
        let (graph, source, sink) = fixture();
        let trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        assert_eq!(trace.result.certificate.value, 5);
        assert_eq!(trace.final_snapshot.final_return_flow, Some(5));
        assert_eq!(trace.final_snapshot.final_artificial_flow, Some(0));
        assert!(trace.final_snapshot.forest_pool_size > 1);
        assert!(trace.final_snapshot.metrics.fundamental_cycles > 0);
        assert!(trace.final_snapshot.metrics.potential_steps > 0);
        assert!(trace.final_snapshot.metrics.enumerated_assignments > 0);
        assert!(trace.final_snapshot.metrics.feasible_circulations > 0);
        assert!(trace.final_snapshot.metrics.rounding_processed_edges > 0);
        assert!(trace.final_snapshot.metrics.rounding_cycles > 0);
        assert!(trace.final_snapshot.metrics.rounding_integralized_edges > 0);
        assert_eq!(
            trace.final_snapshot.final_point_threshold,
            BigRational::new(BigInt::from(1), BigInt::from(2))
        );
        assert!(
            trace
                .final_snapshot
                .final_point_gap
                .as_ref()
                .is_some_and(|gap| gap >= &BigRational::zero()
                    && gap < &trace.final_snapshot.final_point_threshold)
        );
        assert!(
            trace
                .final_snapshot
                .rounding_return_flow
                .as_ref()
                .is_some_and(BigRational::is_integer)
        );
        assert!(trace.final_snapshot.edges.iter().all(|edge| {
            edge.rounding_flow
                .as_ref()
                .is_some_and(BigRational::is_integer)
        }));
        assert!(trace.events.iter().any(|event| event.after.stage
            == DeterministicAlmostLinearMaxFlowStage::RoundingLinkFractionalEdge));
        assert!(trace.events.iter().any(|event| event.after.stage
            == DeterministicAlmostLinearMaxFlowStage::RoundingCancelFractionalCycle));
        check_deterministic_almost_linear_max_flow_trace(&graph, source, sink, &trace)
            .expect("check");
    }

    #[test]
    fn fast_internal_run_does_not_retain_trace_events() {
        let (graph, source, sink) = fixture();
        let fast = build_trace(&graph, source, sink, false).expect("fast internal run");
        let trace = build_trace(&graph, source, sink, true).expect("trace internal run");
        assert!(fast.events.is_empty());
        assert!(!trace.events.is_empty());
        assert_eq!(fast.result, trace.result);
    }

    #[test]
    fn deterministic_reexecution_is_bit_exact() {
        let (graph, source, sink) = fixture();
        let left = trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("left");
        let right =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("right");
        assert_eq!(left, right);
    }

    #[test]
    fn core_spanner_and_branch_collection_are_explicit() {
        let (graph, source, sink) = fixture();
        let trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        assert!(trace.final_snapshot.metrics.branch_records >= 6);
        assert!(trace.final_snapshot.metrics.core_builds >= 6);
        assert!(trace.final_snapshot.metrics.spanner_embeddings > 0);
        assert_eq!(trace.final_snapshot.active_branches.len(), 2);
        assert_eq!(trace.final_snapshot.passes.len(), 2);
        let records = trace
            .events
            .iter()
            .filter(|event| {
                event.after.stage == DeterministicAlmostLinearMaxFlowStage::InstallBranchRecord
            })
            .collect::<Vec<_>>();
        assert_eq!(
            records.len() as u64,
            trace.final_snapshot.metrics.branch_records
        );
        assert!(records.iter().all(|event| {
            event.after.metrics.branch_records
                == event.before.metrics.branch_records.saturating_add(1)
                && event.after.active_level.is_some()
                && event.after.core_vertices > 0
                && (event.after.edges.iter().any(|edge| {
                    edge.active_tree_edge || edge.active_core_edge || edge.active_spanner_edge
                }) || event
                    .after
                    .nodes
                    .iter()
                    .any(|node| node.active_artificial_tree_edge))
        }));
        assert!(
            trace
                .events
                .iter()
                .filter(|event| {
                    event.after.stage
                        == DeterministicAlmostLinearMaxFlowStage::BuildBranchCollection
                })
                .all(|event| {
                    event.after.metrics.branch_records == event.before.metrics.branch_records
                })
        );
    }

    #[test]
    fn tampered_terminal_flow_is_rejected() {
        let (graph, source, sink) = fixture();
        let mut trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        trace.result.flows[0] = 0;
        assert!(
            check_deterministic_almost_linear_max_flow_trace(&graph, source, sink, &trace).is_err()
        );
    }

    #[test]
    fn shift_game_uses_largest_eligible_level_and_rebuilds_deeper_levels() {
        let (graph, source, sink) = fixture();
        let (target, _, _) = enumerate_min_cut(&graph, source, sink).expect("cut");
        let mut state = initialize_kernel(&graph, source, sink, target).expect("initial state");
        state.forests =
            enumerate_spanning_forests(state.star + 1, &state.work_edges, &mut state.metrics)
                .expect("forests");
        build_branch_collection(&mut state, true).expect("branches");
        state.active_branches = vec![2, 2];
        state.passes = vec![0, 1];
        assert_eq!(shift_largest_eligible_level(&mut state), Some(1));
        assert_eq!(state.active_branches, vec![2, 0]);
        assert_eq!(state.passes, vec![0, 2]);
        assert_eq!(shift_largest_eligible_level(&mut state), Some(0));
        assert_eq!(state.active_branches, vec![0, 0]);
        assert_eq!(state.passes, vec![1, 2]);
        rebuild_deeper_levels(&mut state, 0).expect("deeper rebuild");
        assert_eq!(state.passes, vec![1, 0]);
        assert_eq!(state.metrics.branch_wraps, 2);
        assert_eq!(state.metrics.deeper_rebuilds, 1);
    }

    #[test]
    fn tampered_branch_boundary_is_rejected_by_chain_validation() {
        let (graph, source, sink) = fixture();
        let mut trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        let event = trace
            .events
            .iter_mut()
            .find(|event| {
                event.after.stage == DeterministicAlmostLinearMaxFlowStage::InstallBranchRecord
            })
            .expect("branch record boundary");
        event.after.active_branches[0] = 2;
        assert!(
            check_deterministic_almost_linear_max_flow_trace(&graph, source, sink, &trace).is_err()
        );
    }

    #[test]
    fn return_and_artificial_reduction_are_visible_then_rounded() {
        let (graph, source, sink) = fixture();
        let trace =
            trace_deterministic_almost_linear_max_flow(&graph, source, sink).expect("trace");
        let initial = trace
            .events
            .iter()
            .find(|event| {
                event.after.stage == DeterministicAlmostLinearMaxFlowStage::BuildInitialPoint
            })
            .map(|event| &event.after)
            .expect("initial point");
        assert!(initial.return_capacity >= initial.target_value);
        assert!(initial.artificial_edges > 0);
        assert!(initial.artificial_flow.get() > 0.0);
        assert_eq!(trace.final_snapshot.final_artificial_flow, Some(0));
        assert_eq!(
            trace.final_snapshot.final_return_flow,
            Some(trace.final_snapshot.target_value)
        );
    }

    #[test]
    fn thirty_two_small_capacity_instances_match_dinic() {
        let (base, source, sink) = fixture();
        for mask in 0_u64..32 {
            let edges = base
                .edges()
                .iter()
                .enumerate()
                .map(|(index, edge)| UnresolvedFlowEdge {
                    id: edge.id().clone(),
                    from: base.node(edge.from()).expect("from").id().clone(),
                    to: base.node(edge.to()).expect("to").id().clone(),
                    lower: 0,
                    capacity: 1 + ((mask >> index) & 1),
                    cost: 0,
                })
                .collect();
            let graph = FlowNetwork::new(base.nodes().to_vec(), edges).expect("graph");
            let expected = solve_dinic(&graph, source, sink).expect("Dinic");
            let actual = solve_deterministic_almost_linear_max_flow(&graph, source, sink)
                .expect("deterministic");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "mask={mask}"
            );
            check_max_flow(&graph, source, sink, &actual.flows).expect("certificate");
        }
    }

    #[test]
    fn rejects_graph_above_node_admission_before_enumeration() {
        let nodes = (0..=DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_NODES)
            .map(|index| FlowNode::new(NodeId::parse(&format!("v{index}")).expect("node id"), 0))
            .collect::<Vec<_>>();
        let graph = FlowNetwork::new(
            nodes,
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("e").expect("edge"),
                from: NodeId::parse("v0").expect("from"),
                to: NodeId::parse("v1").expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("v0").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("v1").expect("sink id"))
            .expect("sink");
        assert_eq!(
            solve_deterministic_almost_linear_max_flow(&graph, source, sink),
            Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit)
        );
    }

    #[test]
    fn rejects_zero_capacity() {
        let (graph, source, sink) = fixture();
        let mut edges = graph
            .edges()
            .iter()
            .map(|edge| UnresolvedFlowEdge {
                id: edge.id().clone(),
                from: graph.node(edge.from()).expect("from").id().clone(),
                to: graph.node(edge.to()).expect("to").id().clone(),
                lower: edge.lower(),
                capacity: edge.capacity(),
                cost: edge.cost(),
            })
            .collect::<Vec<_>>();
        edges[0].capacity = 0;
        let zero = FlowNetwork::new(graph.nodes().to_vec(), edges).expect("zero graph");
        assert_eq!(
            solve_deterministic_almost_linear_max_flow(&zero, source, sink),
            Err(DeterministicAlmostLinearMaxFlowError::GraphRequirement)
        );
    }

    #[test]
    fn rejects_final_point_assignment_product_above_budget() {
        let nodes = vec![
            FlowNode::new(NodeId::parse("s").expect("source id"), 0),
            FlowNode::new(NodeId::parse("t").expect("sink id"), 0),
        ];
        let graph = FlowNetwork::new(
            nodes,
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("edge"),
                from: NodeId::parse("s").expect("from"),
                to: NodeId::parse("t").expect("to"),
                lower: 0,
                capacity: DETERMINISTIC_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS,
                cost: 0,
            }],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        assert_eq!(
            solve_deterministic_almost_linear_max_flow(&graph, source, sink),
            Err(DeterministicAlmostLinearMaxFlowError::AdmissionLimit)
        );
    }
}
