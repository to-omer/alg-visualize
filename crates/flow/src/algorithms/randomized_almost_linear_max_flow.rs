//! Project-owned randomized max-flow oracle demonstrator.
//!
//! Chen et al. reduce directed `s-t` maximum flow to min-cost circulation by
//! adding a `t -> s` edge of cost `-1`, and solve the circulation with a
//! potential-reduction IPM whose subproblems are undirected minimum-ratio
//! cycles. Their data structure samples low-stretch tree chains, detects
//! slowly-changing gradient/length coordinates, and rebuilds under adaptive
//! queries. This endpoint displays a bounded source prefix around those
//! operations; it is neither a source component nor the paper's solver.
//!
//! Before the prefix, a project cut oracle supplies the exact scalar target
//! used to initialize the optimum objective, log gap, and source potential.
//! It deliberately does not claim the paper's
//! almost-linear running time. The
//! finite bounded spanning-forest population is enumerated, so uniform seeded
//! sampling has an exact inspectable miss probability. After a bounded number
//! of literal potential steps, a separate project optimum-vector oracle
//! materializes the paper's isolation-based final-point precondition; coordinate rounding
//! produces the integral flow and an independent max-flow/min-cut certificate
//! checks it.

#![allow(clippy::cast_precision_loss)]

use std::cmp::Ordering;
use std::collections::VecDeque;

use num_bigint::BigUint;
use num_traits::ToPrimitive;
use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowNetwork, NodeIndex};

/// Original-node admission ceiling.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES: usize = 8;
/// Original-edge admission ceiling.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES: usize = 10;
/// Exact finite spanning-forest population ceiling.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS: usize = 250_000;
/// Public reversible-boundary ceiling.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS: usize = 4_096;
/// Literal potential-reduction steps before exact bounded repair.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS: u64 = 8;
/// Consecutive failed sampling epochs before repair.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_REBUILDS: u64 = 3;
/// Maximum original-edge assignments inspected by the bounded final-point oracle.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS: u64 = 100_000;
/// Maximum independent isolation attempts before the explicit failure boundary.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ISOLATION_ATTEMPTS: usize = 16;
/// Deterministic seed used by the catalog executable.
pub const RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED: u64 = 0x434b_4c50_4d43_4631;

const POSITIVE_FLOOR: f64 = 1.0e-12;
const NUMERICAL_TOLERANCE: f64 = 1.0e-8;
const CHANGE_THRESHOLD: f64 = 0.01;

/// Finite replay-safe scalar stored by exact IEEE-754 bit identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RandomizedAlmostLinearScalar(u64);

impl RandomizedAlmostLinearScalar {
    fn try_new(value: f64) -> Result<Self, RandomizedAlmostLinearMaxFlowError> {
        if !value.is_finite() {
            return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
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

/// Reduced exact probability that all uniform forest draws miss a good tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearProbability {
    /// Nonnegative reduced numerator.
    pub numerator: u128,
    /// Positive reduced denominator.
    pub denominator: u128,
}

impl RandomizedAlmostLinearProbability {
    fn new(numerator: u128, denominator: u128) -> Result<Self, RandomizedAlmostLinearMaxFlowError> {
        if denominator == 0 || numerator > denominator {
            return Err(RandomizedAlmostLinearMaxFlowError::ForestInvariant);
        }
        let divisor = gcd_u128(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }
}

const fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

/// Source-level and bounded-repair publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomizedAlmostLinearMaxFlowStage {
    /// Valid graph before the reduction.
    Ready,
    /// Cost-minus-one return edge `t -> s` is visible.
    BuildReturnEdgeReduction,
    /// Midpoint/artificial-star strict interior is visible.
    BuildInitialPoint,
    /// Exact finite spanning-forest population is installed.
    EnumerateForestPool,
    /// Seeded forests were sampled with replacement.
    SampleTreeChain,
    /// One geometric checkpoint of an evaluated fundamental-cycle candidate.
    InspectFundamentalCycle,
    /// Current gradient/length fundamental cycles were compared.
    QueryMinimumRatioCycle,
    /// The sampled set missed the bounded approximation band.
    SamplingFailure,
    /// The source potential-reduction step was applied.
    PotentialReductionStep,
    /// Slowly changing coordinates were explicitly detected.
    DetectChangedCoordinates,
    /// The sampled hierarchy was rebuilt.
    RebuildTreeChain,
    /// One geometric checkpoint of the bounded integral assignment enumeration.
    InspectFeasibleAssignment,
    /// Integral feasible flows of the bounded return-edge reduction were enumerated.
    EnumerateFeasibleSet,
    /// Independent isolation perturbations were sampled.
    SampleIsolationCosts,
    /// The unique perturbed optimum was selected.
    SelectIsolatedOptimum,
    /// A feasible point inside the source final-point accuracy gate was constructed.
    ConstructFinalPoint,
    /// Every original and return-edge coordinate was rounded to the nearest integer.
    RoundNearestInteger,
    /// Original flow, return flow, and min cut were checked.
    CheckCertificate,
    /// Certified maximum flow is public.
    Optimal,
}

/// Original-node projection including the source artificial initial-point edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearNodeState {
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
    pub artificial_flow: RandomizedAlmostLinearScalar,
    /// Artificial-edge capacity, zero when absent.
    pub artificial_capacity: RandomizedAlmostLinearScalar,
    /// Number of sampled forests containing the artificial edge.
    pub artificial_tree_memberships: u64,
    /// Whether the artificial edge is in the selected forest.
    pub active_artificial_tree_edge: bool,
    /// Signed active-cycle membership on the artificial edge.
    pub active_artificial_sign: i8,
}

/// Original-edge projection of IPM, sampled-tree, detection, and repair state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Current strict-interior augmented-circulation flow.
    pub interior_flow: RandomizedAlmostLinearScalar,
    /// Current source gradient coordinate.
    pub gradient: RandomizedAlmostLinearScalar,
    /// Current positive source length coordinate.
    pub length: RandomizedAlmostLinearScalar,
    /// Number of sampled forests containing the edge.
    pub sampled_tree_memberships: u64,
    /// Whether the edge is in the selected forest.
    pub active_tree_edge: bool,
    /// Signed active fundamental-cycle membership.
    pub active_cycle_sign: i8,
    /// Whether Detect refreshed this coordinate.
    pub changed_coordinate: bool,
    /// Independent isolation draw on this original coordinate.
    pub isolation_draw: u64,
    /// Feasible near-optimal final-point coordinate when available.
    pub final_point_flow: Option<RandomizedAlmostLinearScalar>,
    /// Exact rounded integral flow when available.
    pub final_flow: Option<u64>,
}

/// Exact bounded-work and source-operation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMaxFlowMetrics {
    /// Exact original cuts inspected.
    pub enumerated_cuts: u64,
    /// Candidate forest subsets inspected.
    pub forest_subsets: u64,
    /// Forests in the exact finite population.
    pub forest_pool_size: u64,
    /// Seeded forest draws.
    pub sampled_forests: u64,
    /// Fundamental cycles evaluated.
    pub fundamental_cycles: u64,
    /// Queries inside the approximation band.
    pub successful_queries: u64,
    /// Explicit finite-population misses.
    pub sampling_failures: u64,
    /// Completed source potential steps.
    pub potential_steps: u64,
    /// Coordinates refreshed by Detect.
    pub detected_coordinates: u64,
    /// Tree-chain rebuilds.
    pub rebuilds: u64,
    /// Original-edge assignments inspected by final-point enumeration.
    pub enumerated_assignments: u64,
    /// Feasible integral return-edge circulations retained.
    pub feasible_flows: u64,
    /// Independent isolation attempts.
    pub isolation_attempts: u64,
    /// Coordinates rounded by the source final-point rule.
    pub rounding_operations: u64,
    /// Independent terminal checks.
    pub certificate_checks: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete reversible state at one publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMaxFlowSnapshot {
    /// Original-node projections.
    pub nodes: Vec<RandomizedAlmostLinearNodeState>,
    /// Original-edge projections.
    pub edges: Vec<RandomizedAlmostLinearEdgeState>,
    /// Fixed run seed.
    pub seed: u64,
    /// `SplitMix64` draws consumed so far.
    pub random_draws: u64,
    /// Paper parameter `1 / (1000 log(mU))`.
    pub alpha: RandomizedAlmostLinearScalar,
    /// Current source potential.
    pub potential: RandomizedAlmostLinearScalar,
    /// Current augmented min-cost gap above `F*`.
    pub cost_gap: RandomizedAlmostLinearScalar,
    /// Best ratio in the current sampled set.
    pub selected_ratio: Option<RandomizedAlmostLinearScalar>,
    /// Best ratio over the exact finite forest pool.
    pub exact_pool_ratio: Option<RandomizedAlmostLinearScalar>,
    /// Exact probability that the current number of uniform draws all miss.
    pub miss_probability: RandomizedAlmostLinearProbability,
    /// Exact finite sampling-population size.
    pub forest_pool_size: u64,
    /// Draws per sampled tree chain.
    pub sample_count: u64,
    /// Completed potential steps.
    pub iteration: u64,
    /// Current rebuild epoch.
    pub rebuild_epoch: u64,
    /// Strict-interior return-edge flow.
    pub return_flow: RandomizedAlmostLinearScalar,
    /// Return-edge capacity `mU`.
    pub return_capacity: u64,
    /// Return-edge gradient coordinate.
    pub return_gradient: RandomizedAlmostLinearScalar,
    /// Return-edge length coordinate.
    pub return_length: RandomizedAlmostLinearScalar,
    /// Number of sampled forests containing the return edge.
    pub return_tree_memberships: u64,
    /// Whether the return edge is in the selected forest.
    pub active_return_tree_edge: bool,
    /// Signed active-cycle membership on `t -> s`.
    pub active_return_sign: i8,
    /// Independent isolation draw on the return edge.
    pub return_isolation_draw: u64,
    /// Near-optimal final-point return flow.
    pub final_point_return_flow: Option<RandomizedAlmostLinearScalar>,
    /// Rounded return flow, equal to the max-flow value.
    pub final_return_flow: Option<u64>,
    /// Artificial edges in the source initial-point construction.
    pub artificial_edges: u64,
    /// Sum of strict-interior artificial flow.
    pub artificial_flow: RandomizedAlmostLinearScalar,
    /// Rounded artificial flow, zero after source final-point recovery.
    pub final_artificial_flow: Option<u64>,
    /// Isolation scale `D = 4 M^2 U^2` for the `M`-edge reduction.
    pub isolation_scale: u128,
    /// One-based successful isolation attempt, zero before sampling.
    pub isolation_attempt: u64,
    /// Probability bound that every completed isolation attempt failed.
    pub isolation_failure_probability: RandomizedAlmostLinearProbability,
    /// Scaled objective of the selected unique perturbed optimum.
    pub isolated_objective: Option<i128>,
    /// Source final-point additive accuracy threshold.
    pub final_point_threshold: RandomizedAlmostLinearScalar,
    /// Verified perturbed-objective gap of the constructed final point.
    pub final_point_gap: Option<RandomizedAlmostLinearScalar>,
    /// Convex mixing coefficient used by the bounded final-point oracle.
    pub final_point_mix: Option<RandomizedAlmostLinearScalar>,
    /// Exact maximum-flow target installed by bounded cut enumeration.
    pub target_value: u64,
    /// Current source/repair boundary.
    pub stage: RandomizedAlmostLinearMaxFlowStage,
    /// Exact counters at this boundary.
    pub metrics: RandomizedAlmostLinearMaxFlowMetrics,
}

/// One atomic reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMaxFlowTraceEvent {
    /// Stable event-vocabulary identity.
    pub catalog_id: &'static str,
    /// Boundary before the transition.
    pub before: RandomizedAlmostLinearMaxFlowSnapshot,
    /// Boundary after the transition.
    pub after: RandomizedAlmostLinearMaxFlowSnapshot,
}

/// Certified bounded solver result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMaxFlowResult {
    /// Original-edge integral flows.
    pub flows: Vec<u64>,
    /// Independent max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Terminal public state.
    pub final_snapshot: RandomizedAlmostLinearMaxFlowSnapshot,
    /// Exact bounded-work counters.
    pub metrics: RandomizedAlmostLinearMaxFlowMetrics,
}

/// Result plus all reversible source and repair boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizedAlmostLinearMaxFlowTraceResult {
    /// Same certified result as the fast profile.
    pub result: RandomizedAlmostLinearMaxFlowResult,
    /// Ready boundary before the reduction.
    pub base_snapshot: RandomizedAlmostLinearMaxFlowSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<RandomizedAlmostLinearMaxFlowTraceEvent>,
    /// Terminal boundary, equal to the result snapshot.
    pub final_snapshot: RandomizedAlmostLinearMaxFlowSnapshot,
}

/// Admission, work-limit, numerical, certificate, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RandomizedAlmostLinearMaxFlowError {
    /// Input exceeds the bounded exact interactive band.
    #[error("randomized almost-linear max-flow input exceeds admission limits")]
    AdmissionLimit,
    /// Input violates the max-flow/source-reduction requirements.
    #[error(
        "randomized almost-linear max-flow requires distinct terminals, zero lower/supply, positive capacities, and no self-loops"
    )]
    GraphRequirement,
    /// Exact finite forest enumeration exceeded its ceiling.
    #[error("randomized almost-linear max-flow forest pool exceeds bounded work limit")]
    ForestLimit,
    /// A forest or signed fundamental circulation was invalid.
    #[error("randomized almost-linear max-flow forest/cycle invariant failed")]
    ForestInvariant,
    /// Checked integer arithmetic overflowed.
    #[error("randomized almost-linear max-flow arithmetic overflow")]
    ArithmeticOverflow,
    /// A source scalar was non-finite or left the strict interior.
    #[error("randomized almost-linear max-flow numerical invariant failed")]
    NumericalFailure,
    /// Isolation did not produce a unique perturbed optimum within the explicit cap.
    #[error("randomized almost-linear max-flow isolation attempts exhausted")]
    IsolationExhausted,
    /// The bounded final-point construction or coordinate rounding failed.
    #[error("randomized almost-linear max-flow final-point rounding failed")]
    FinalPointRounding,
    /// Independent max-flow validation rejected the final flow.
    #[error("randomized almost-linear max-flow certificate failed")]
    Certificate(#[from] CertificateError),
    /// Event replay or terminal metadata contradicted the contract.
    #[error("randomized almost-linear max-flow replay invariant failed")]
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
struct CycleCandidate {
    forest_index: usize,
    signs: Vec<i8>,
    ratio: f64,
    numerator: f64,
}

#[derive(Clone, Debug)]
struct CycleEvaluationCheckpoint {
    candidate: CycleCandidate,
    metrics: RandomizedAlmostLinearMaxFlowMetrics,
}

#[derive(Clone, Debug)]
struct KernelState {
    star: usize,
    target: u64,
    return_capacity: u64,
    seed: u64,
    rng: SplitMix64,
    alpha: f64,
    optimum_cost: f64,
    work_edges: Vec<WorkEdge>,
    forests: Vec<Forest>,
    sampled: Vec<usize>,
    active: Option<CycleCandidate>,
    exact_pool_ratio: Option<f64>,
    miss_probability: RandomizedAlmostLinearProbability,
    sample_count: usize,
    iteration: u64,
    rebuild_epoch: u64,
    isolation_draws: Vec<u64>,
    return_isolation_draw: u64,
    isolation_scale: u128,
    isolation_attempt: u64,
    isolation_failure_probability: RandomizedAlmostLinearProbability,
    isolated_objective: Option<i128>,
    final_point_flows: Option<Vec<f64>>,
    final_point_return_flow: Option<f64>,
    final_point_gap: Option<f64>,
    final_point_mix: Option<f64>,
    final_flows: Option<Vec<u64>>,
    assignment_projection: Option<Vec<u64>>,
    metrics: RandomizedAlmostLinearMaxFlowMetrics,
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
struct IsolationOutcome {
    original_draws: Vec<u64>,
    return_draw: u64,
    scale: u128,
    attempt: u64,
    objective: i128,
    isolated: FeasibleReturnCirculation,
}

#[derive(Clone, Debug)]
struct FinalPointOutcome {
    original_flows: Vec<f64>,
    return_flow: f64,
    gap: f64,
    mix: f64,
    rounded_flows: Vec<u64>,
    rounded_return_flow: u64,
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
    state: u64,
    draws: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self {
            state: seed,
            draws: 0,
        }
    }
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.draws = self.draws.saturating_add(1);
        value ^ (value >> 31)
    }
}

/// Solves with the catalog's deterministic seed.
///
/// # Errors
///
/// Rejects invalid/beyond-band graphs, bounded-work or numerical failure, and
/// any result rejected by the independent certificate.
pub fn solve_randomized_almost_linear_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<RandomizedAlmostLinearMaxFlowResult, RandomizedAlmostLinearMaxFlowError> {
    solve_randomized_almost_linear_max_flow_with_seed(
        graph,
        source,
        sink,
        RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED,
    )
}

/// Solves with an explicit replayable seed.
///
/// # Errors
///
/// Returns the same failures as [`solve_randomized_almost_linear_max_flow`].
pub fn solve_randomized_almost_linear_max_flow_with_seed(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    seed: u64,
) -> Result<RandomizedAlmostLinearMaxFlowResult, RandomizedAlmostLinearMaxFlowError> {
    run_internal(graph, source, sink, seed, false).map(|trace| trace.result)
}

/// Traces every source and repair boundary with the catalog seed.
///
/// # Errors
///
/// Returns the same failures as [`solve_randomized_almost_linear_max_flow`].
pub fn trace_randomized_almost_linear_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<RandomizedAlmostLinearMaxFlowTraceResult, RandomizedAlmostLinearMaxFlowError> {
    trace_randomized_almost_linear_max_flow_with_seed(
        graph,
        source,
        sink,
        RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED,
    )
}

/// Traces every source and repair boundary with an explicit seed.
///
/// # Errors
///
/// Returns the same failures as [`solve_randomized_almost_linear_max_flow`].
#[allow(clippy::too_many_lines)]
pub fn trace_randomized_almost_linear_max_flow_with_seed(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    seed: u64,
) -> Result<RandomizedAlmostLinearMaxFlowTraceResult, RandomizedAlmostLinearMaxFlowError> {
    let trace = run_internal(graph, source, sink, seed, true)?;
    check_randomized_almost_linear_max_flow_trace(graph, source, sink, &trace)?;
    Ok(trace)
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    seed: u64,
    record_trace: bool,
) -> Result<RandomizedAlmostLinearMaxFlowTraceResult, RandomizedAlmostLinearMaxFlowError> {
    validate_graph(graph, source, sink)?;
    let (target, cut_side, enumerated_cuts) = enumerate_min_cut(graph, source, sink)?;
    let mut state = initialize_kernel(graph, source, sink, target, seed)?;
    state.metrics.enumerated_cuts = enumerated_cuts;
    let base_snapshot = snapshot(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::Ready,
    )?;
    let mut current = base_snapshot.clone();
    let mut events = record_trace.then(Vec::new);

    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::BuildReturnEdgeReduction,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::BuildInitialPoint,
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
        RandomizedAlmostLinearMaxFlowStage::EnumerateForestPool,
        &mut current,
        &mut events,
    )?;
    sample_tree_chain(&mut state)?;
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::SampleTreeChain,
        &mut current,
        &mut events,
    )?;

    while state.iteration < RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS {
        update_gradient_lengths(&mut state)?;
        let (successful, cycle_checkpoints) = query_tree_chain(&mut state)?;
        let selected = state.active.clone();
        let exact_pool_ratio = state.exact_pool_ratio;
        let miss_probability = state.miss_probability;
        let query_metrics = state.metrics;
        state.exact_pool_ratio = None;
        state.miss_probability = RandomizedAlmostLinearProbability::new(1, 1)?;
        for checkpoint in cycle_checkpoints {
            state.active = Some(checkpoint.candidate);
            state.metrics = checkpoint.metrics;
            publish(
                graph,
                &state,
                &cut_side,
                RandomizedAlmostLinearMaxFlowStage::InspectFundamentalCycle,
                &mut current,
                &mut events,
            )?;
        }
        state.active = selected;
        state.exact_pool_ratio = exact_pool_ratio;
        state.miss_probability = miss_probability;
        state.metrics = query_metrics;
        publish(
            graph,
            &state,
            &cut_side,
            RandomizedAlmostLinearMaxFlowStage::QueryMinimumRatioCycle,
            &mut current,
            &mut events,
        )?;
        if !successful {
            state.metrics.sampling_failures = state.metrics.sampling_failures.saturating_add(1);
            publish(
                graph,
                &state,
                &cut_side,
                RandomizedAlmostLinearMaxFlowStage::SamplingFailure,
                &mut current,
                &mut events,
            )?;
            if state.rebuild_epoch >= RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_REBUILDS {
                break;
            }
            rebuild_tree_chain(&mut state)?;
            publish(
                graph,
                &state,
                &cut_side,
                RandomizedAlmostLinearMaxFlowStage::RebuildTreeChain,
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
            RandomizedAlmostLinearMaxFlowStage::PotentialReductionStep,
            &mut current,
            &mut events,
        )?;
        detect_changed_coordinates(&mut state)?;
        publish(
            graph,
            &state,
            &cut_side,
            RandomizedAlmostLinearMaxFlowStage::DetectChangedCoordinates,
            &mut current,
            &mut events,
        )?;
        if state.iteration % 3 == 0
            && state.iteration < RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_IPM_STEPS
        {
            rebuild_tree_chain(&mut state)?;
            publish(
                graph,
                &state,
                &cut_side,
                RandomizedAlmostLinearMaxFlowStage::RebuildTreeChain,
                &mut current,
                &mut events,
            )?;
        }
    }

    state.active = None;
    let mut face_metrics = state.metrics;
    let face = enumerate_feasible_return_face(
        graph,
        source,
        sink,
        &mut face_metrics,
        &mut |assignment, observed_metrics| {
            if !observed_metrics.enumerated_assignments.is_power_of_two() {
                return Ok(());
            }
            state.assignment_projection = Some(assignment.to_vec());
            state.metrics = observed_metrics;
            publish(
                graph,
                &state,
                &cut_side,
                RandomizedAlmostLinearMaxFlowStage::InspectFeasibleAssignment,
                &mut current,
                &mut events,
            )
        },
    )?;
    state.assignment_projection = None;
    state.metrics = face_metrics;
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::EnumerateFeasibleSet,
        &mut current,
        &mut events,
    )?;
    let isolation = isolate_return_circulation(graph, &face, &mut state)?;
    if isolation.isolated.return_flow != target {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    state.isolation_draws.clone_from(&isolation.original_draws);
    state.return_isolation_draw = isolation.return_draw;
    state.isolation_scale = isolation.scale;
    state.isolation_attempt = isolation.attempt;
    state.isolation_failure_probability = isolation_failure_bound(isolation.attempt)?;
    state.isolated_objective = Some(isolation.objective);
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::SampleIsolationCosts,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::SelectIsolatedOptimum,
        &mut current,
        &mut events,
    )?;

    let final_point = construct_final_point(graph, source, sink, &face, &isolation)?;
    state.final_point_flows = Some(final_point.original_flows.clone());
    state.final_point_return_flow = Some(final_point.return_flow);
    state.final_point_gap = Some(final_point.gap);
    state.final_point_mix = Some(final_point.mix);
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::ConstructFinalPoint,
        &mut current,
        &mut events,
    )?;

    if final_point.rounded_return_flow != target
        || final_point.rounded_flows != isolation.isolated.original_flows
    {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    state.metrics.rounding_operations = u64::try_from(graph.edges().len() + 1)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    state.final_flows = Some(final_point.rounded_flows.clone());
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::RoundNearestInteger,
        &mut current,
        &mut events,
    )?;

    let certificate = check_max_flow(graph, source, sink, &final_point.rounded_flows)?;
    if u64::try_from(certificate.value).ok() != Some(target) {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    state.metrics.certificate_checks = state.metrics.certificate_checks.saturating_add(4);
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::CheckCertificate,
        &mut current,
        &mut events,
    )?;
    publish(
        graph,
        &state,
        &cut_side,
        RandomizedAlmostLinearMaxFlowStage::Optimal,
        &mut current,
        &mut events,
    )?;

    let final_snapshot = current;
    let result = RandomizedAlmostLinearMaxFlowResult {
        flows: final_point.rounded_flows,
        certificate,
        final_snapshot: final_snapshot.clone(),
        metrics: final_snapshot.metrics,
    };
    let trace = RandomizedAlmostLinearMaxFlowTraceResult {
        result,
        base_snapshot,
        events: events.unwrap_or_default(),
        final_snapshot,
    };
    Ok(trace)
}

/// Independently checks replay, return reduction, finite-pool metadata, and
/// the exact terminal max-flow/min-cut certificate.
///
/// # Errors
///
/// Rejects a broken chain, invalid probability, or non-optimal terminal flow.
pub fn check_randomized_almost_linear_max_flow_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &RandomizedAlmostLinearMaxFlowTraceResult,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    validate_graph(graph, source, sink)?;
    if trace.base_snapshot.stage != RandomizedAlmostLinearMaxFlowStage::Ready
        || trace.final_snapshot.stage != RandomizedAlmostLinearMaxFlowStage::Optimal
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.metrics != trace.final_snapshot.metrics
        || trace.events.len() > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS
        || !valid_snapshot_publication(graph, &trace.base_snapshot)
    {
        return Err(RandomizedAlmostLinearMaxFlowError::TraceInvariant);
    }
    let mut previous = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != stage_catalog_id(event.after.stage)
            || event.before != *previous
            || !valid_transition(event.before.stage, event.after.stage)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
            || event.after.seed != trace.base_snapshot.seed
            || event.after.return_capacity != trace.base_snapshot.return_capacity
            || event.after.target_value != trace.base_snapshot.target_value
            || event.after.miss_probability.denominator == 0
            || event.after.miss_probability.numerator > event.after.miss_probability.denominator
            || !valid_snapshot_publication(graph, &event.after)
        {
            return Err(RandomizedAlmostLinearMaxFlowError::TraceInvariant);
        }
        previous = &event.after;
    }
    if *previous != trace.final_snapshot
        || trace.events.len() as u64 != trace.final_snapshot.metrics.state_transitions
        || trace.final_snapshot.final_return_flow != Some(trace.final_snapshot.target_value)
        || trace.final_snapshot.final_artificial_flow != Some(0)
        || trace.final_snapshot.final_point_return_flow.is_none()
        || trace.final_snapshot.final_point_gap.is_none()
        || trace.final_snapshot.final_point_mix.is_none()
        || trace.final_snapshot.isolation_attempt == 0
        || trace.final_snapshot.isolated_objective.is_none()
        || trace
            .final_snapshot
            .isolation_failure_probability
            .denominator
            == 0
        || trace.final_snapshot.isolation_failure_probability.numerator
            > trace
                .final_snapshot
                .isolation_failure_probability
                .denominator
        || trace.final_snapshot.forest_pool_size == 0
        || trace.final_snapshot.sample_count == 0
    {
        return Err(RandomizedAlmostLinearMaxFlowError::TraceInvariant);
    }
    let certificate = check_max_flow(graph, source, sink, &trace.result.flows)?;
    if certificate != trace.result.certificate
        || u64::try_from(certificate.value).ok() != Some(trace.final_snapshot.target_value)
        || trace.final_snapshot.edges.len() != graph.edges().len()
    {
        return Err(RandomizedAlmostLinearMaxFlowError::TraceInvariant);
    }
    for (edge, flow) in trace.final_snapshot.edges.iter().zip(&trace.result.flows) {
        if !(edge.final_flow == Some(*flow)
            && edge.isolation_draw > 0
            && edge.final_point_flow.is_some()
            && edge.interior_flow.get().is_finite()
            && edge.gradient.get().is_finite()
            && edge.length.get().is_finite()
            && edge.length.get() > 0.0)
        {
            return Err(RandomizedAlmostLinearMaxFlowError::TraceInvariant);
        }
    }
    Ok(())
}

fn valid_snapshot_publication(
    graph: &FlowNetwork,
    snapshot: &RandomizedAlmostLinearMaxFlowSnapshot,
) -> bool {
    use RandomizedAlmostLinearMaxFlowStage as Stage;
    let isolation_ready = matches!(
        snapshot.stage,
        Stage::SampleIsolationCosts
            | Stage::SelectIsolatedOptimum
            | Stage::ConstructFinalPoint
            | Stage::RoundNearestInteger
            | Stage::CheckCertificate
            | Stage::Optimal
    );
    let final_point_ready = matches!(
        snapshot.stage,
        Stage::ConstructFinalPoint
            | Stage::RoundNearestInteger
            | Stage::CheckCertificate
            | Stage::Optimal
    );
    let rounded = matches!(
        snapshot.stage,
        Stage::RoundNearestInteger | Stage::CheckCertificate | Stage::Optimal
    );
    if snapshot.edges.len() != graph.edges().len()
        || isolation_ready != (snapshot.isolation_attempt > 0)
        || isolation_ready != snapshot.isolated_objective.is_some()
        || isolation_ready != (snapshot.return_isolation_draw > 0)
        || final_point_ready != snapshot.final_point_return_flow.is_some()
        || final_point_ready != snapshot.final_point_gap.is_some()
        || final_point_ready != snapshot.final_point_mix.is_some()
        || rounded != snapshot.final_return_flow.is_some()
        || rounded != snapshot.final_artificial_flow.is_some()
        || snapshot.isolation_attempt
            > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ISOLATION_ATTEMPTS as u64
        || snapshot.isolation_failure_probability.denominator == 0
        || snapshot.isolation_failure_probability.numerator
            > snapshot.isolation_failure_probability.denominator
        || snapshot
            .final_point_gap
            .is_some_and(|gap| gap.get() < 0.0 || gap.get() > snapshot.final_point_threshold.get())
        || snapshot
            .final_point_mix
            .is_some_and(|mix| !(0.0..=0.25).contains(&mix.get()))
    {
        return false;
    }
    snapshot
        .edges
        .iter()
        .zip(graph.edges())
        .all(|(edge, graph_edge)| {
            isolation_ready == (edge.isolation_draw > 0)
                && final_point_ready == edge.final_point_flow.is_some()
                && rounded == edge.final_flow.is_some()
                && edge.final_point_flow.is_none_or(|flow| {
                    flow.get().is_finite()
                        && flow.get() >= 0.0
                        && flow.get() <= graph_edge.capacity() as f64
                })
                && (!rounded
                    || edge
                        .final_point_flow
                        .zip(edge.final_flow)
                        .is_some_and(|(point, flow)| point.get().round().to_u64() == Some(flow)))
        })
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_EDGES
    {
        return Err(RandomizedAlmostLinearMaxFlowError::AdmissionLimit);
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
        return Err(RandomizedAlmostLinearMaxFlowError::GraphRequirement);
    }
    let assignments = graph.edges().iter().try_fold(1_u64, |count, edge| {
        count.checked_mul(edge.capacity().saturating_add(1))
    });
    if assignments.is_none_or(|count| count > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS) {
        return Err(RandomizedAlmostLinearMaxFlowError::AdmissionLimit);
    }
    Ok(())
}

fn enumerate_min_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(u64, Vec<bool>, u64), RandomizedAlmostLinearMaxFlowError> {
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
                    .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
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
        u64::try_from(best).map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
        best_side,
        inspected,
    ))
}

fn isolation_scale(
    graph: &FlowNetwork,
    return_capacity: u64,
) -> Result<u128, RandomizedAlmostLinearMaxFlowError> {
    let reduction_edges = u128::try_from(graph.edges().len() + 1)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let congestion_bound = u128::from(
        return_capacity.max(
            graph
                .edges()
                .iter()
                .map(crate::model::FlowEdge::capacity)
                .max()
                .unwrap_or(1),
        ),
    );
    4_u128
        .checked_mul(reduction_edges)
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
}

fn enumerate_feasible_return_face(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut RandomizedAlmostLinearMaxFlowMetrics,
    observe: &mut impl FnMut(
        &[u64],
        RandomizedAlmostLinearMaxFlowMetrics,
    ) -> Result<(), RandomizedAlmostLinearMaxFlowError>,
) -> Result<FeasibleReturnFace, RandomizedAlmostLinearMaxFlowError> {
    let mut circulations = Vec::new();
    let mut flows = vec![0_u64; graph.edges().len()];
    enumerate_flow_assignments(
        graph,
        source,
        sink,
        0,
        &mut flows,
        &mut circulations,
        metrics,
        observe,
    )?;
    if circulations.is_empty() {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    metrics.feasible_flows = u64::try_from(circulations.len())
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let mut original_flow_sums = vec![0_u128; graph.edges().len()];
    let mut return_flow_sum = 0_u128;
    for circulation in &circulations {
        for (sum, flow) in original_flow_sums
            .iter_mut()
            .zip(&circulation.original_flows)
        {
            *sum = sum
                .checked_add(u128::from(*flow))
                .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        }
        return_flow_sum = return_flow_sum
            .checked_add(u128::from(circulation.return_flow))
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    Ok(FeasibleReturnFace {
        circulations,
        original_flow_sums,
        return_flow_sum,
    })
}

#[allow(clippy::too_many_arguments)]
fn enumerate_flow_assignments(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    edge_index: usize,
    flows: &mut [u64],
    circulations: &mut Vec<FeasibleReturnCirculation>,
    metrics: &mut RandomizedAlmostLinearMaxFlowMetrics,
    observe: &mut impl FnMut(
        &[u64],
        RandomizedAlmostLinearMaxFlowMetrics,
    ) -> Result<(), RandomizedAlmostLinearMaxFlowError>,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if edge_index < graph.edges().len() {
        for flow in 0..=graph.edges()[edge_index].capacity() {
            flows[edge_index] = flow;
            enumerate_flow_assignments(
                graph,
                source,
                sink,
                edge_index + 1,
                flows,
                circulations,
                metrics,
                observe,
            )?;
        }
        return Ok(());
    }
    metrics.enumerated_assignments = metrics.enumerated_assignments.saturating_add(1);
    if metrics.enumerated_assignments > RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ASSIGNMENTS {
        return Err(RandomizedAlmostLinearMaxFlowError::AdmissionLimit);
    }
    observe(flows, *metrics)?;
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for (edge, flow) in graph.edges().iter().zip(flows.iter().copied()) {
        divergence[edge.from().as_usize()] += i128::from(flow);
        divergence[edge.to().as_usize()] -= i128::from(flow);
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
            .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
    });
    Ok(())
}

fn isolate_return_circulation(
    graph: &FlowNetwork,
    face: &FeasibleReturnFace,
    state: &mut KernelState,
) -> Result<IsolationOutcome, RandomizedAlmostLinearMaxFlowError> {
    let reduction_edges = u64::try_from(graph.edges().len() + 1)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let maximum_draw = 2_u64
        .checked_mul(reduction_edges)
        .and_then(|value| value.checked_mul(state.return_capacity))
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    for attempt in 1..=RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_ISOLATION_ATTEMPTS {
        let original_draws = (0..graph.edges().len())
            .map(|_| state.rng.next() % maximum_draw + 1)
            .collect::<Vec<_>>();
        let return_draw = state.rng.next() % maximum_draw + 1;
        state.metrics.isolation_attempts = u64::try_from(attempt)
            .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        let mut best_objective = i128::MAX;
        let mut best_index = 0_usize;
        let mut ties = 0_usize;
        for (index, circulation) in face.circulations.iter().enumerate() {
            let objective = isolated_objective(
                circulation,
                &original_draws,
                return_draw,
                state.isolation_scale,
            )?;
            match objective.cmp(&best_objective) {
                Ordering::Less => {
                    best_objective = objective;
                    best_index = index;
                    ties = 1;
                }
                Ordering::Equal => ties = ties.saturating_add(1),
                Ordering::Greater => {}
            }
        }
        if ties == 1 {
            return Ok(IsolationOutcome {
                original_draws,
                return_draw,
                scale: state.isolation_scale,
                attempt: u64::try_from(attempt)
                    .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
                objective: best_objective,
                isolated: face.circulations[best_index].clone(),
            });
        }
    }
    Err(RandomizedAlmostLinearMaxFlowError::IsolationExhausted)
}

fn isolated_objective(
    circulation: &FeasibleReturnCirculation,
    original_draws: &[u64],
    return_draw: u64,
    scale: u128,
) -> Result<i128, RandomizedAlmostLinearMaxFlowError> {
    let scale = i128::try_from(scale)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let return_flow = i128::from(circulation.return_flow);
    let mut objective = scale
        .checked_mul(return_flow)
        .and_then(i128::checked_neg)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    objective = objective
        .checked_add(
            i128::from(return_draw)
                .checked_mul(return_flow)
                .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
        )
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    for (&draw, &flow) in original_draws.iter().zip(&circulation.original_flows) {
        objective = objective
            .checked_add(
                i128::from(draw)
                    .checked_mul(i128::from(flow))
                    .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
            )
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    Ok(objective)
}

fn isolation_failure_bound(
    attempts: u64,
) -> Result<RandomizedAlmostLinearProbability, RandomizedAlmostLinearMaxFlowError> {
    let exponent = u32::try_from(attempts)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let denominator = 2_u128
        .checked_pow(exponent)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    RandomizedAlmostLinearProbability::new(1, denominator)
}

fn source_final_point_threshold_denominator(
    graph: &FlowNetwork,
    return_capacity: u64,
) -> Result<u128, RandomizedAlmostLinearMaxFlowError> {
    let reduction_edges = u128::try_from(graph.edges().len() + 1)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let congestion_bound = u128::from(return_capacity.max(1));
    12_u128
        .checked_mul(reduction_edges)
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(reduction_edges))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .and_then(|value| value.checked_mul(congestion_bound))
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
}

fn source_final_point_threshold(
    graph: &FlowNetwork,
    return_capacity: u64,
) -> Result<f64, RandomizedAlmostLinearMaxFlowError> {
    Ok(1.0 / source_final_point_threshold_denominator(graph, return_capacity)? as f64)
}

// Keep the isolation-gap algebra, convex mixing, exact coordinate checks, and
// rounding precondition together in the order used by the source argument.
#[allow(clippy::too_many_lines)]
fn construct_final_point(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    face: &FeasibleReturnFace,
    isolation: &IsolationOutcome,
) -> Result<FinalPointOutcome, RandomizedAlmostLinearMaxFlowError> {
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(1);
    let return_capacity = maximum_capacity.saturating_mul(graph.edges().len() as u64);
    let threshold_denominator = source_final_point_threshold_denominator(graph, return_capacity)?;
    let threshold = 1.0 / threshold_denominator as f64;
    let isolated = &isolation.isolated;
    let face_size = u128::try_from(face.circulations.len())
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let objective_delta_sum = face.circulations.iter().try_fold(
        0_u128,
        |sum, circulation| -> Result<u128, RandomizedAlmostLinearMaxFlowError> {
            let objective = isolated_objective(
                circulation,
                &isolation.original_draws,
                isolation.return_draw,
                isolation.scale,
            )?;
            let delta = objective
                .checked_sub(isolation.objective)
                .filter(|value| *value >= 0)
                .ok_or(RandomizedAlmostLinearMaxFlowError::FinalPointRounding)?;
            sum.checked_add(
                u128::try_from(delta)
                    .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?,
            )
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
        },
    )?;
    let mut coordinate_deltas = Vec::with_capacity(graph.edges().len() + 1);
    for (&sum, &flow) in face.original_flow_sums.iter().zip(&isolated.original_flows) {
        let isolated_sum = u128::from(flow)
            .checked_mul(face_size)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        coordinate_deltas.push(sum.abs_diff(isolated_sum));
    }
    let isolated_return_sum = u128::from(isolated.return_flow)
        .checked_mul(face_size)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    coordinate_deltas.push(face.return_flow_sum.abs_diff(isolated_return_sum));
    let mut mix_denominator = 4_u128;
    loop {
        let accuracy_left =
            BigUint::from(objective_delta_sum) * BigUint::from(threshold_denominator);
        let accuracy_right = BigUint::from(face_size)
            * BigUint::from(isolation.scale)
            * BigUint::from(mix_denominator);
        let rounding_safe = coordinate_deltas.iter().all(|delta| {
            BigUint::from(4_u8) * BigUint::from(*delta)
                < BigUint::from(face_size) * BigUint::from(mix_denominator)
        });
        if accuracy_left <= accuracy_right && rounding_safe {
            break;
        }
        mix_denominator = mix_denominator
            .checked_mul(2)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    let point_denominator = face_size
        .checked_mul(mix_denominator)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let original_point_numerators = face
        .original_flow_sums
        .iter()
        .zip(&isolated.original_flows)
        .map(|(&sum, &flow)| {
            final_point_numerator(sum, u128::from(flow), face_size, point_denominator)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_point_numerator = final_point_numerator(
        face.return_flow_sum,
        u128::from(isolated.return_flow),
        face_size,
        point_denominator,
    )?;
    verify_final_point_circulation(
        graph,
        source,
        sink,
        &original_point_numerators,
        return_point_numerator,
        point_denominator,
    )?;
    let rounded_flows = original_point_numerators
        .iter()
        .map(|numerator| round_nonnegative_rational(*numerator, point_denominator))
        .collect::<Result<Vec<_>, _>>()?;
    let rounded_return_flow =
        round_nonnegative_rational(return_point_numerator, point_denominator)?;
    if rounded_flows != isolated.original_flows || rounded_return_flow != isolated.return_flow {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    let original_flows = original_point_numerators
        .iter()
        .map(|numerator| *numerator as f64 / point_denominator as f64)
        .collect::<Vec<_>>();
    let return_flow = return_point_numerator as f64 / point_denominator as f64;
    let gap_denominator = face_size
        .checked_mul(mix_denominator)
        .and_then(|value| value.checked_mul(isolation.scale))
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let gap = objective_delta_sum as f64 / gap_denominator as f64;
    let mix = 1.0 / mix_denominator as f64;
    if !(threshold.is_finite()
        && threshold > 0.0
        && gap.is_finite()
        && gap >= 0.0
        && gap <= threshold)
    {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    if original_flows.iter().any(|flow| !flow.is_finite())
        || !return_flow.is_finite()
        || return_flow < 0.0
    {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    Ok(FinalPointOutcome {
        original_flows,
        return_flow,
        gap,
        mix,
        rounded_flows,
        rounded_return_flow,
    })
}

fn verify_final_point_circulation(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    original_numerators: &[u128],
    return_numerator: u128,
    denominator: u128,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if original_numerators.len() != graph.edges().len() || denominator == 0 {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for (edge, &numerator) in graph.edges().iter().zip(original_numerators) {
        let upper_numerator = u128::from(edge.capacity())
            .checked_mul(denominator)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        if numerator > upper_numerator {
            return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
        }
        let signed = i128::try_from(numerator)
            .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        divergence[edge.from().as_usize()] = divergence[edge.from().as_usize()]
            .checked_add(signed)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        divergence[edge.to().as_usize()] = divergence[edge.to().as_usize()]
            .checked_sub(signed)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    }
    let signed_return = i128::try_from(return_numerator)
        .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    if divergence[source.as_usize()] != signed_return
        || divergence[sink.as_usize()]
            != signed_return
                .checked_neg()
                .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?
        || divergence.iter().enumerate().any(|(node, balance)| {
            node != source.as_usize() && node != sink.as_usize() && *balance != 0
        })
    {
        return Err(RandomizedAlmostLinearMaxFlowError::FinalPointRounding);
    }
    Ok(())
}

fn final_point_numerator(
    coordinate_sum: u128,
    isolated_coordinate: u128,
    face_size: u128,
    point_denominator: u128,
) -> Result<u128, RandomizedAlmostLinearMaxFlowError> {
    let base = isolated_coordinate
        .checked_mul(point_denominator)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    let anchor_sum = isolated_coordinate
        .checked_mul(face_size)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
    if coordinate_sum >= anchor_sum {
        base.checked_add(coordinate_sum - anchor_sum)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
    } else {
        base.checked_sub(anchor_sum - coordinate_sum)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
    }
}

fn round_nonnegative_rational(
    numerator: u128,
    denominator: u128,
) -> Result<u64, RandomizedAlmostLinearMaxFlowError> {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = if remainder
        .checked_mul(2)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?
        >= denominator
    {
        quotient
            .checked_add(1)
            .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?
    } else {
        quotient
    };
    u64::try_from(rounded).map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)
}

#[allow(clippy::too_many_lines)]
fn initialize_kernel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    target: u64,
    seed: u64,
) -> Result<KernelState, RandomizedAlmostLinearMaxFlowError> {
    let n = graph.nodes().len();
    let star = n;
    let maximum_capacity = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .ok_or(RandomizedAlmostLinearMaxFlowError::GraphRequirement)?;
    let return_capacity = (graph.edges().len() as u64)
        .checked_mul(maximum_capacity)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
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
    // Six draws keep `250_000^6` inside `u128`, so the displayed miss
    // probability remains an exact fraction throughout the admitted band.
    let sample_count = match n {
        0..=3 => 4,
        4..=5 => 5,
        _ => 6,
    };
    let mut state = KernelState {
        star,
        target,
        return_capacity,
        seed,
        rng: SplitMix64::new(seed),
        alpha,
        optimum_cost: -(target as f64),
        work_edges,
        forests: Vec::new(),
        sampled: Vec::new(),
        active: None,
        exact_pool_ratio: None,
        miss_probability: RandomizedAlmostLinearProbability::new(1, 1)?,
        sample_count,
        iteration: 0,
        rebuild_epoch: 0,
        isolation_draws: vec![0; graph.edges().len()],
        return_isolation_draw: 0,
        isolation_scale: isolation_scale(graph, return_capacity)?,
        isolation_attempt: 0,
        isolation_failure_probability: RandomizedAlmostLinearProbability::new(1, 1)?,
        isolated_objective: None,
        final_point_flows: None,
        final_point_return_flow: None,
        final_point_gap: None,
        final_point_mix: None,
        final_flows: None,
        assignment_projection: None,
        metrics: RandomizedAlmostLinearMaxFlowMetrics::default(),
    };
    update_gradient_lengths(&mut state)?;
    Ok(state)
}

fn update_gradient_lengths(
    state: &mut KernelState,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    let gap = work_objective(&state.work_edges) - state.optimum_cost;
    if !(gap.is_finite() && gap > 0.0) {
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
    }
    let m = state.work_edges.len() as f64;
    for edge in &mut state.work_edges {
        let upper_slack = edge.upper - edge.flow;
        let lower_slack = edge.flow - edge.lower;
        if !(upper_slack > POSITIVE_FLOOR && lower_slack > POSITIVE_FLOOR) {
            return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
        }
        edge.length = upper_slack.powf(-1.0 - state.alpha) + lower_slack.powf(-1.0 - state.alpha);
        edge.gradient = 20.0 * m * edge.cost / gap
            + state.alpha * upper_slack.powf(-1.0 - state.alpha)
            - state.alpha * lower_slack.powf(-1.0 - state.alpha);
        if !(edge.length.is_finite() && edge.length > 0.0 && edge.gradient.is_finite()) {
            return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
        }
    }
    Ok(())
}

fn work_objective(edges: &[WorkEdge]) -> f64 {
    edges.iter().map(|edge| edge.cost * edge.flow).sum()
}

fn work_potential(state: &KernelState) -> Result<(f64, f64), RandomizedAlmostLinearMaxFlowError> {
    let gap = work_objective(&state.work_edges) - state.optimum_cost;
    if !(gap.is_finite() && gap > 0.0) {
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
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
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
    }
    Ok((potential, gap))
}

fn check_work_circulation(
    nodes: usize,
    edges: &[WorkEdge],
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    let mut divergence = vec![0.0; nodes];
    for edge in edges {
        if !(edge.flow > edge.lower && edge.flow < edge.upper) {
            return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
        }
        divergence[edge.from] += edge.flow;
        divergence[edge.to] -= edge.flow;
    }
    if divergence
        .into_iter()
        .any(|value| value.abs() > NUMERICAL_TOLERANCE)
    {
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
    }
    Ok(())
}

fn enumerate_spanning_forests(
    nodes: usize,
    edges: &[WorkEdge],
    metrics: &mut RandomizedAlmostLinearMaxFlowMetrics,
) -> Result<Vec<Forest>, RandomizedAlmostLinearMaxFlowError> {
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
        return Err(RandomizedAlmostLinearMaxFlowError::ForestInvariant);
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
    metrics: &mut RandomizedAlmostLinearMaxFlowMetrics,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if forests.len() >= RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_FORESTS {
        return Err(RandomizedAlmostLinearMaxFlowError::ForestLimit);
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

fn sample_tree_chain(state: &mut KernelState) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if state.forests.is_empty() {
        return Err(RandomizedAlmostLinearMaxFlowError::ForestInvariant);
    }
    state.sampled.clear();
    for _ in 0..state.sample_count {
        let forest = usize::try_from(state.rng.next() % state.forests.len() as u64)
            .map_err(|_| RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        state.sampled.push(forest);
        state.metrics.sampled_forests = state.metrics.sampled_forests.saturating_add(1);
    }
    Ok(())
}

fn query_tree_chain(
    state: &mut KernelState,
) -> Result<(bool, Vec<CycleEvaluationCheckpoint>), RandomizedAlmostLinearMaxFlowError> {
    let mut exact: Option<CycleCandidate> = None;
    let mut forest_best = Vec::with_capacity(state.forests.len());
    let mut checkpoints = Vec::new();
    for (index, forest) in state.forests.iter().enumerate() {
        let best = best_fundamental_cycle(
            index,
            forest,
            state.star + 1,
            &state.work_edges,
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
        forest_best.push(best);
    }
    let mut sampled_best: Option<CycleCandidate> = None;
    for forest_index in state.sampled.iter().copied() {
        if let Some(candidate) = &forest_best[forest_index]
            && sampled_best
                .as_ref()
                .is_none_or(|incumbent| candidate_better(candidate, incumbent))
        {
            sampled_best = Some(candidate.clone());
        }
    }
    let Some(exact) = exact else {
        state.active = None;
        state.exact_pool_ratio = None;
        state.miss_probability = RandomizedAlmostLinearProbability::new(0, 1)?;
        return Ok((false, checkpoints));
    };
    state.exact_pool_ratio = Some(exact.ratio);
    let approximation = 4.0 * state.work_edges.len() as f64;
    let threshold = exact.ratio / approximation;
    let good_forests = forest_best
        .iter()
        .filter(|candidate| {
            candidate
                .as_ref()
                .is_some_and(|value| value.ratio <= threshold + NUMERICAL_TOLERANCE)
        })
        .count();
    let bad = state.forests.len().saturating_sub(good_forests) as u128;
    let total = state.forests.len() as u128;
    state.miss_probability = RandomizedAlmostLinearProbability::new(
        checked_pow_u128(bad, state.sample_count)?,
        checked_pow_u128(total, state.sample_count)?,
    )?;
    let successful = sampled_best
        .as_ref()
        .is_some_and(|candidate| candidate.ratio <= threshold + NUMERICAL_TOLERANCE);
    state.active = sampled_best;
    if successful {
        state.metrics.successful_queries = state.metrics.successful_queries.saturating_add(1);
    }
    Ok((successful, checkpoints))
}

fn checked_pow_u128(
    mut base: u128,
    mut exponent: usize,
) -> Result<u128, RandomizedAlmostLinearMaxFlowError> {
    let mut result = 1_u128;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result
                .checked_mul(base)
                .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base
                .checked_mul(base)
                .ok_or(RandomizedAlmostLinearMaxFlowError::ArithmeticOverflow)?;
        }
    }
    Ok(result)
}

fn best_fundamental_cycle(
    forest_index: usize,
    forest: &Forest,
    nodes: usize,
    edges: &[WorkEdge],
    metrics: &mut RandomizedAlmostLinearMaxFlowMetrics,
    checkpoints: &mut Vec<CycleEvaluationCheckpoint>,
) -> Result<Option<CycleCandidate>, RandomizedAlmostLinearMaxFlowError> {
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
) -> Result<(f64, f64), RandomizedAlmostLinearMaxFlowError> {
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
            return Err(RandomizedAlmostLinearMaxFlowError::ForestInvariant);
        }
        numerator += edge.gradient * f64::from(*sign);
        denominator += edge.length * f64::from(sign.abs());
        divergence[edge.from] += i32::from(*sign);
        divergence[edge.to] -= i32::from(*sign);
    }
    if denominator <= 0.0 || divergence.into_iter().any(|value| value != 0) {
        return Err(RandomizedAlmostLinearMaxFlowError::ForestInvariant);
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

fn apply_potential_step(state: &mut KernelState) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    let candidate = state
        .active
        .clone()
        .ok_or(RandomizedAlmostLinearMaxFlowError::ForestInvariant)?;
    let (before, _) = work_potential(state)?;
    let kappa = (-candidate.ratio).min(1.0 / 16.0);
    if !(kappa > 0.0 && candidate.numerator < 0.0) {
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
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
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
    }
    for (edge, sign) in state.work_edges.iter_mut().zip(&candidate.signs) {
        edge.flow += eta * f64::from(*sign);
    }
    check_work_circulation(state.star + 1, &state.work_edges)?;
    let (after, _) = work_potential(state)?;
    if after > before + NUMERICAL_TOLERANCE {
        return Err(RandomizedAlmostLinearMaxFlowError::NumericalFailure);
    }
    state.iteration = state.iteration.saturating_add(1);
    state.metrics.potential_steps = state.metrics.potential_steps.saturating_add(1);
    Ok(())
}

fn detect_changed_coordinates(
    state: &mut KernelState,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
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

fn rebuild_tree_chain(state: &mut KernelState) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    state.rebuild_epoch = state.rebuild_epoch.saturating_add(1);
    state.metrics.rebuilds = state.metrics.rebuilds.saturating_add(1);
    state.active = None;
    sample_tree_chain(state)
}

#[allow(clippy::too_many_lines)]
fn snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
    cut_side: &[bool],
    boundary: RandomizedAlmostLinearMaxFlowStage,
) -> Result<RandomizedAlmostLinearMaxFlowSnapshot, RandomizedAlmostLinearMaxFlowError> {
    let active_forest = state
        .active
        .as_ref()
        .map(|candidate| candidate.forest_index);
    let active_signs = state
        .active
        .as_ref()
        .map(|candidate| candidate.signs.as_slice());
    let mut memberships = vec![0_u64; state.work_edges.len()];
    for forest_index in &state.sampled {
        for edge in &state.forests[*forest_index].edges {
            memberships[*edge] = memberships[*edge].saturating_add(1);
        }
    }
    let (parents, components) = active_forest.map_or_else(
        || (vec![None; state.star + 1], (0..=state.star).collect()),
        |index| forest_projection(state.star + 1, &state.work_edges, &state.forests[index]),
    );
    let final_point_flows = state.final_point_flows.as_deref();
    let final_flows = state.final_flows.as_deref();
    let mut nodes = Vec::with_capacity(graph.nodes().len());
    for node in graph.node_indices() {
        let node_index = node.as_usize();
        let artificial = state
            .work_edges
            .iter()
            .enumerate()
            .find(|(_, edge)| edge.kind == WorkEdgeKind::Artificial(node_index));
        let (direction, flow, capacity, tree_memberships, active_tree, sign) =
            artificial.map_or((0, 0.0, 0.0, 0, false, 0), |(index, edge)| {
                (
                    if edge.from == state.star { 1 } else { -1 },
                    edge.flow,
                    edge.upper,
                    memberships[index],
                    active_forest
                        .is_some_and(|forest| state.forests[forest].edges.contains(&index)),
                    active_signs.map_or(0, |signs| signs[index]),
                )
            });
        nodes.push(RandomizedAlmostLinearNodeState {
            node,
            tree_parent: parents[node_index],
            tree_component: components[node_index],
            source_side: cut_side[node_index],
            artificial_direction: direction,
            artificial_flow: RandomizedAlmostLinearScalar::try_new(flow)?,
            artificial_capacity: RandomizedAlmostLinearScalar::try_new(capacity)?,
            artificial_tree_memberships: tree_memberships,
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
            .ok_or(RandomizedAlmostLinearMaxFlowError::ForestInvariant)?;
        edges.push(RandomizedAlmostLinearEdgeState {
            edge: graph_edge.id().clone(),
            interior_flow: RandomizedAlmostLinearScalar::try_new(
                state
                    .assignment_projection
                    .as_ref()
                    .map_or(work_edge.flow, |assignment| assignment[original] as f64),
            )?,
            gradient: RandomizedAlmostLinearScalar::try_new(work_edge.gradient)?,
            length: RandomizedAlmostLinearScalar::try_new(work_edge.length)?,
            sampled_tree_memberships: memberships[work_index],
            active_tree_edge: active_forest
                .is_some_and(|index| state.forests[index].edges.contains(&work_index)),
            active_cycle_sign: active_signs.map_or(0, |signs| signs[work_index]),
            changed_coordinate: work_edge.changed,
            isolation_draw: state.isolation_draws[original],
            final_point_flow: final_point_flows
                .map(|flows| RandomizedAlmostLinearScalar::try_new(flows[original]))
                .transpose()?,
            final_flow: final_flows.map(|flows| flows[original]),
        });
    }
    let return_index = state
        .work_edges
        .iter()
        .position(|edge| edge.kind == WorkEdgeKind::Return)
        .ok_or(RandomizedAlmostLinearMaxFlowError::ForestInvariant)?;
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
    Ok(RandomizedAlmostLinearMaxFlowSnapshot {
        nodes,
        edges,
        seed: state.seed,
        random_draws: state.rng.draws,
        alpha: RandomizedAlmostLinearScalar::try_new(state.alpha)?,
        potential: RandomizedAlmostLinearScalar::try_new(potential)?,
        cost_gap: RandomizedAlmostLinearScalar::try_new(gap)?,
        selected_ratio: state
            .active
            .as_ref()
            .map(|candidate| RandomizedAlmostLinearScalar::try_new(candidate.ratio))
            .transpose()?,
        exact_pool_ratio: state
            .exact_pool_ratio
            .map(RandomizedAlmostLinearScalar::try_new)
            .transpose()?,
        miss_probability: state.miss_probability,
        forest_pool_size: state.forests.len() as u64,
        sample_count: state.sample_count as u64,
        iteration: state.iteration,
        rebuild_epoch: state.rebuild_epoch,
        return_flow: RandomizedAlmostLinearScalar::try_new(return_edge.flow)?,
        return_capacity: state.return_capacity,
        return_gradient: RandomizedAlmostLinearScalar::try_new(return_edge.gradient)?,
        return_length: RandomizedAlmostLinearScalar::try_new(return_edge.length)?,
        return_tree_memberships: memberships[return_index],
        active_return_tree_edge: active_forest
            .is_some_and(|forest| state.forests[forest].edges.contains(&return_index)),
        active_return_sign: active_signs.map_or(0, |signs| signs[return_index]),
        return_isolation_draw: state.return_isolation_draw,
        final_point_return_flow: state
            .final_point_return_flow
            .map(RandomizedAlmostLinearScalar::try_new)
            .transpose()?,
        final_return_flow: final_flows.map(|_| state.target),
        artificial_edges: artificial_edges as u64,
        artificial_flow: RandomizedAlmostLinearScalar::try_new(artificial_flow)?,
        final_artificial_flow: final_flows.map(|_| 0),
        isolation_scale: state.isolation_scale,
        isolation_attempt: state.isolation_attempt,
        isolation_failure_probability: state.isolation_failure_probability,
        isolated_objective: state.isolated_objective,
        final_point_threshold: RandomizedAlmostLinearScalar::try_new(
            source_final_point_threshold(graph, state.return_capacity)?,
        )?,
        final_point_gap: state
            .final_point_gap
            .map(RandomizedAlmostLinearScalar::try_new)
            .transpose()?,
        final_point_mix: state
            .final_point_mix
            .map(RandomizedAlmostLinearScalar::try_new)
            .transpose()?,
        target_value: state.target,
        stage: boundary,
        metrics: state.metrics,
    })
}

fn publish(
    graph: &FlowNetwork,
    state: &KernelState,
    cut_side: &[bool],
    boundary: RandomizedAlmostLinearMaxFlowStage,
    current: &mut RandomizedAlmostLinearMaxFlowSnapshot,
    events: &mut Option<Vec<RandomizedAlmostLinearMaxFlowTraceEvent>>,
) -> Result<(), RandomizedAlmostLinearMaxFlowError> {
    if current.metrics.state_transitions
        >= RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_MAX_TRACE_EVENTS as u64
    {
        return Err(RandomizedAlmostLinearMaxFlowError::AdmissionLimit);
    }
    let mut after = snapshot(graph, state, cut_side, boundary)?;
    after.metrics.state_transitions = current.metrics.state_transitions.saturating_add(1);
    if let Some(events) = events {
        events.push(RandomizedAlmostLinearMaxFlowTraceEvent {
            catalog_id: stage_catalog_id(boundary),
            before: current.clone(),
            after: after.clone(),
        });
    }
    *current = after;
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

const fn stage_catalog_id(stage: RandomizedAlmostLinearMaxFlowStage) -> &'static str {
    use RandomizedAlmostLinearMaxFlowStage as Stage;
    match stage {
        Stage::Ready => "randomized-almost-linear-max-flow-oracle-demonstrator.ready",
        Stage::BuildReturnEdgeReduction => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.return-edge"
        }
        Stage::BuildInitialPoint => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.initial-point"
        }
        Stage::EnumerateForestPool => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.forest-pool"
        }
        Stage::SampleTreeChain => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.sample-chain"
        }
        Stage::InspectFundamentalCycle => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle"
        }
        Stage::QueryMinimumRatioCycle => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.query-cycle"
        }
        Stage::SamplingFailure => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.sampling-failure"
        }
        Stage::PotentialReductionStep => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.potential-step"
        }
        Stage::DetectChangedCoordinates => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.detect"
        }
        Stage::RebuildTreeChain => "randomized-almost-linear-max-flow-oracle-demonstrator.rebuild",
        Stage::InspectFeasibleAssignment => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.inspect-feasible-assignment"
        }
        Stage::EnumerateFeasibleSet => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.enumerate-feasible-set"
        }
        Stage::SampleIsolationCosts => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.sample-isolation-costs"
        }
        Stage::SelectIsolatedOptimum => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.select-isolated-optimum"
        }
        Stage::ConstructFinalPoint => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.construct-final-point"
        }
        Stage::RoundNearestInteger => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.round-nearest-integer"
        }
        Stage::CheckCertificate => {
            "randomized-almost-linear-max-flow-oracle-demonstrator.check-certificate"
        }
        Stage::Optimal => "randomized-almost-linear-max-flow-oracle-demonstrator.optimal",
    }
}

#[allow(clippy::unnested_or_patterns)]
const fn valid_transition(
    before: RandomizedAlmostLinearMaxFlowStage,
    after: RandomizedAlmostLinearMaxFlowStage,
) -> bool {
    use RandomizedAlmostLinearMaxFlowStage as Stage;
    matches!(
        (before, after),
        (Stage::Ready, Stage::BuildReturnEdgeReduction)
            | (Stage::BuildReturnEdgeReduction, Stage::BuildInitialPoint)
            | (Stage::BuildInitialPoint, Stage::EnumerateForestPool)
            | (Stage::EnumerateForestPool, Stage::SampleTreeChain)
            | (
                Stage::SampleTreeChain | Stage::DetectChangedCoordinates | Stage::RebuildTreeChain,
                Stage::InspectFundamentalCycle
            )
            | (
                Stage::InspectFundamentalCycle,
                Stage::InspectFundamentalCycle
            )
            | (
                Stage::InspectFundamentalCycle,
                Stage::QueryMinimumRatioCycle
            )
            | (
                Stage::SampleTreeChain | Stage::DetectChangedCoordinates | Stage::RebuildTreeChain,
                Stage::QueryMinimumRatioCycle
            )
            | (
                Stage::QueryMinimumRatioCycle,
                Stage::PotentialReductionStep | Stage::SamplingFailure
            )
            | (
                Stage::SamplingFailure | Stage::DetectChangedCoordinates,
                Stage::RebuildTreeChain
            )
            | (
                Stage::SamplingFailure | Stage::DetectChangedCoordinates | Stage::RebuildTreeChain,
                Stage::InspectFeasibleAssignment
            )
            | (
                Stage::InspectFeasibleAssignment,
                Stage::InspectFeasibleAssignment
            )
            | (
                Stage::InspectFeasibleAssignment,
                Stage::EnumerateFeasibleSet
            )
            | (
                Stage::PotentialReductionStep,
                Stage::DetectChangedCoordinates
            )
            | (Stage::EnumerateFeasibleSet, Stage::SampleIsolationCosts)
            | (Stage::SampleIsolationCosts, Stage::SelectIsolatedOptimum)
            | (Stage::SelectIsolatedOptimum, Stage::ConstructFinalPoint)
            | (Stage::ConstructFinalPoint, Stage::RoundNearestInteger)
            | (Stage::RoundNearestInteger, Stage::CheckCertificate)
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

    #[test]
    fn reduction_tree_chain_isolation_and_final_point_rounding_are_certified() {
        let (graph, source, sink) = fixture();
        let trace = trace_randomized_almost_linear_max_flow(&graph, source, sink).expect("trace");
        assert_eq!(trace.result.certificate.value, 5);
        assert_eq!(trace.final_snapshot.final_return_flow, Some(5));
        assert_eq!(trace.final_snapshot.final_artificial_flow, Some(0));
        assert!(trace.final_snapshot.forest_pool_size > 1);
        assert!(trace.final_snapshot.metrics.fundamental_cycles > 0);
        assert!(trace.final_snapshot.metrics.potential_steps > 0);
        assert!(trace.final_snapshot.metrics.enumerated_assignments > 0);
        assert!(trace.final_snapshot.metrics.feasible_flows > 0);
        assert!(trace.final_snapshot.metrics.isolation_attempts > 0);
        assert_eq!(
            trace.final_snapshot.metrics.rounding_operations,
            graph.edges().len() as u64 + 1
        );
        assert!(
            trace
                .final_snapshot
                .final_point_gap
                .is_some_and(|gap| gap.get() <= trace.final_snapshot.final_point_threshold.get())
        );
        for stage in [
            RandomizedAlmostLinearMaxFlowStage::EnumerateFeasibleSet,
            RandomizedAlmostLinearMaxFlowStage::SampleIsolationCosts,
            RandomizedAlmostLinearMaxFlowStage::SelectIsolatedOptimum,
            RandomizedAlmostLinearMaxFlowStage::ConstructFinalPoint,
            RandomizedAlmostLinearMaxFlowStage::RoundNearestInteger,
        ] {
            assert!(trace.events.iter().any(|event| event.after.stage == stage));
        }
        check_randomized_almost_linear_max_flow_trace(&graph, source, sink, &trace).expect("check");
    }

    #[test]
    fn fast_internal_run_does_not_retain_trace_events() {
        let (graph, source, sink) = fixture();
        let fast = run_internal(
            &graph,
            source,
            sink,
            RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED,
            false,
        )
        .expect("fast internal run");
        let trace = run_internal(
            &graph,
            source,
            sink,
            RANDOMIZED_ALMOST_LINEAR_MAX_FLOW_DEFAULT_SEED,
            true,
        )
        .expect("trace internal run");
        assert!(fast.events.is_empty());
        assert!(!trace.events.is_empty());
        assert_eq!(fast.result, trace.result);
    }

    #[test]
    fn final_flow_is_published_only_after_nearest_integer_rounding() {
        let (graph, source, sink) = fixture();
        let trace = trace_randomized_almost_linear_max_flow(&graph, source, sink).expect("trace");
        for event in &trace.events {
            let rounded = matches!(
                event.after.stage,
                RandomizedAlmostLinearMaxFlowStage::RoundNearestInteger
                    | RandomizedAlmostLinearMaxFlowStage::CheckCertificate
                    | RandomizedAlmostLinearMaxFlowStage::Optimal
            );
            assert_eq!(
                event
                    .after
                    .edges
                    .iter()
                    .all(|edge| edge.final_flow.is_some()),
                rounded
            );
        }
    }

    #[test]
    fn all_small_four_node_edge_subsets_round_to_an_independently_certified_max_flow() {
        let candidates = [
            ("sa", "s", "a", 1),
            ("sb", "s", "b", 2),
            ("ab", "a", "b", 1),
            ("ba", "b", "a", 2),
            ("at", "a", "t", 1),
            ("bt", "b", "t", 2),
        ];
        for mask in 1_u64..(1_u64 << candidates.len()) {
            let nodes = ["s", "a", "b", "t"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect();
            let edges = candidates
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, &(id, from, to, capacity))| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("from"),
                    to: NodeId::parse(to).expect("to"),
                    lower: 0,
                    capacity,
                    cost: 0,
                })
                .collect();
            let graph = FlowNetwork::new(nodes, edges).expect("subset graph");
            let source = graph
                .node_index(&NodeId::parse("s").expect("s"))
                .expect("source");
            let sink = graph
                .node_index(&NodeId::parse("t").expect("t"))
                .expect("sink");
            let result = solve_randomized_almost_linear_max_flow(&graph, source, sink)
                .expect("bounded source final-point solve");
            let certificate = check_max_flow(&graph, source, sink, &result.flows)
                .expect("independent certificate");
            assert_eq!(certificate, result.certificate, "mask {mask:#08b}");
        }
    }

    #[test]
    fn fixed_seed_replays_bit_exactly() {
        let (graph, source, sink) = fixture();
        let left = trace_randomized_almost_linear_max_flow_with_seed(&graph, source, sink, 7)
            .expect("left");
        let right = trace_randomized_almost_linear_max_flow_with_seed(&graph, source, sink, 7)
            .expect("right");
        assert_eq!(left, right);
    }

    #[test]
    fn distinct_seeds_preserve_the_exact_answer() {
        let (graph, source, sink) = fixture();
        let left = trace_randomized_almost_linear_max_flow_with_seed(&graph, source, sink, 11)
            .expect("left");
        let right = trace_randomized_almost_linear_max_flow_with_seed(&graph, source, sink, 29)
            .expect("right");
        assert_eq!(left.result.flows, right.result.flows);
        assert_eq!(left.result.certificate, right.result.certificate);
        assert_ne!(left.base_snapshot.seed, right.base_snapshot.seed);
    }

    #[test]
    fn finite_population_probability_is_normalized() {
        let (graph, source, sink) = fixture();
        let trace = trace_randomized_almost_linear_max_flow(&graph, source, sink).expect("trace");
        for event in &trace.events {
            assert!(event.after.miss_probability.denominator > 0);
            assert!(
                event.after.miss_probability.numerator <= event.after.miss_probability.denominator
            );
        }
    }

    #[test]
    fn tampered_terminal_flow_is_rejected() {
        let (graph, source, sink) = fixture();
        let mut trace =
            trace_randomized_almost_linear_max_flow(&graph, source, sink).expect("trace");
        trace.result.flows[0] = 0;
        assert!(
            check_randomized_almost_linear_max_flow_trace(&graph, source, sink, &trace).is_err()
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
            solve_randomized_almost_linear_max_flow(&zero, source, sink),
            Err(RandomizedAlmostLinearMaxFlowError::GraphRequirement)
        );
    }
}
