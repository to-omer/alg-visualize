//! Bounded exact undirected minimum-ratio-cycle primitive.
//!
//! Chen et al. reduce their potential-reduction IPM to repeatedly finding a
//! circulation `delta` that approximately minimizes
//!
//! `g^T delta / ||diag(l) delta||_1` subject to `B^T delta = 0`.
//!
//! This interactive realization maps an original edge's signed `cost` to the
//! source gradient `g_e` and its positive `capacity` to the source length
//! `l_e`.  It treats each stored arc as an arbitrary orientation of an
//! undirected edge.  Because the admitted graph is deliberately tiny, the
//! visible oracle examines all ternary edge vectors and keeps exactly the
//! connected, degree-two circulations.  A structurally independent DFS cycle
//! enumerator checks the selected optimum.  This is one exact source-level
//! primitive; it does not claim the paper's randomized dynamic data structure
//! or its end-to-end almost-linear maximum-flow bound.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use thiserror::Error;

use crate::model::{EdgeId, FlowNetwork, NodeIndex};

/// Conservative vertex limit for exhaustive interactive checking.
pub const MINIMUM_RATIO_CYCLE_MAX_NODES: usize = 8;
/// Conservative edge limit; `3^11 = 177147` candidate sign vectors.
pub const MINIMUM_RATIO_CYCLE_MAX_EDGES: usize = 11;
/// Positive source length ceiling.
pub const MINIMUM_RATIO_CYCLE_MAX_LENGTH: u64 = 1_000_000;
/// Absolute source gradient ceiling.
pub const MINIMUM_RATIO_CYCLE_MAX_ABS_GRADIENT: i64 = 1_000_000;
/// Deterministic ceiling on ternary sign vectors.
pub const MINIMUM_RATIO_CYCLE_MAX_ENUMERATED_VECTORS: u64 = 177_147;
/// Deterministic ceiling on the independent DFS edge expansions.
pub const MINIMUM_RATIO_CYCLE_MAX_DFS_EXPANSIONS: u64 = 1_000_000;
/// Trace ceiling, including candidate and best-update boundaries.
pub const MINIMUM_RATIO_CYCLE_MAX_TRACE_EVENTS: usize = 2_048;
const MINIMUM_RATIO_CYCLE_TRACE_DENSE_VECTOR_PREFIX: u64 = 16;
const MINIMUM_RATIO_CYCLE_TRACE_VECTOR_BLOCK_MAX: u64 = 256;

/// Exact signed ratio with a positive denominator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleRational {
    /// Signed numerator.
    pub numerator: i128,
    /// Positive denominator.
    pub denominator: u128,
}

impl MinimumRatioCycleRational {
    fn new(numerator: i128, denominator: u128) -> Result<Self, MinimumRatioCycleError> {
        if denominator == 0 {
            return Err(MinimumRatioCycleError::CycleInvariant);
        }
        let divisor = gcd_u128(numerator.unsigned_abs(), denominator);
        let reduced_denominator = denominator / divisor;
        let signed_divisor =
            i128::try_from(divisor).map_err(|_| MinimumRatioCycleError::ArithmeticOverflow)?;
        Ok(Self {
            numerator: numerator / signed_divisor,
            denominator: reduced_denominator,
        })
    }

    fn compare(self, other: Self) -> Result<Ordering, MinimumRatioCycleError> {
        let left = self
            .numerator
            .checked_mul(
                i128::try_from(other.denominator)
                    .map_err(|_| MinimumRatioCycleError::ArithmeticOverflow)?,
            )
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        let right = other
            .numerator
            .checked_mul(
                i128::try_from(self.denominator)
                    .map_err(|_| MinimumRatioCycleError::ArithmeticOverflow)?,
            )
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        Ok(left.cmp(&right))
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

/// One signed original edge in the selected circulation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleArc {
    /// Stable original-edge identity.
    pub edge: EdgeId,
    /// `1` follows the stored orientation and `-1` opposes it.
    pub sign: i8,
}

/// Source-level publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinimumRatioCycleStage {
    /// Valid input, before interpreting gradient and length fields.
    Ready,
    /// `cost -> g` and `capacity -> l` were published.
    MapGradientLength,
    /// A canonical spanning forest and fundamental-cycle count were built.
    BuildSpanningForest,
    /// One geometrically spaced ternary vector is being tested for circulation structure.
    InspectVector,
    /// One exact simple-cycle ratio was evaluated.
    EvaluateCycle,
    /// The incumbent minimum ratio changed.
    UpdateBest,
    /// The selected signed vector passed cycle-space conservation checks.
    VerifyCycleSpace,
    /// The independent DFS cycle oracle agreed.
    CheckExhaustiveOracle,
    /// The primitive certificate is complete; no maximum-flow claim is made.
    Complete,
}

/// One canonical node at a reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleNodeState {
    /// Canonical original node.
    pub node: NodeIndex,
    /// Deterministic connected-component ordinal.
    pub component: usize,
    /// Canonical forest parent.
    pub parent: Option<NodeIndex>,
    /// Forest depth.
    pub depth: usize,
    /// Signed incidence balance of the visible candidate.
    pub candidate_balance: i32,
    /// Whether the node belongs to the visible candidate cycle.
    pub on_candidate: bool,
    /// Whether the node belongs to the selected cycle.
    pub on_selected: bool,
}

/// One original edge with source inputs and cycle annotations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleEdgeState {
    /// Stable original edge.
    pub edge: EdgeId,
    /// Source gradient `g_e`, mapped from `cost`.
    pub gradient: i64,
    /// Positive source length `l_e`, mapped from `capacity`.
    pub length: u64,
    /// Membership in the deterministic spanning forest.
    pub tree_edge: bool,
    /// Visible candidate sign in `{-1,0,1}`.
    pub candidate_sign: i8,
    /// Selected optimum sign in `{-1,0,1}`.
    pub selected_sign: i8,
    /// Visible signed contribution `g_e delta_e`.
    pub numerator_contribution: i128,
    /// Visible absolute contribution `l_e |delta_e|`.
    pub denominator_contribution: u128,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumRatioCycleMetrics {
    /// Edge inspections used to build and root the spanning forest.
    pub forest_edge_scans: u128,
    /// Dimension `m - n + components` of the cycle space.
    pub fundamental_cycles: u64,
    /// Ternary sign vectors inspected by the visible exact oracle.
    pub enumerated_vectors: u64,
    /// Connected degree-two circulations evaluated.
    pub simple_cycles: u64,
    /// Exact ratio comparisons.
    pub ratio_comparisons: u64,
    /// Strict or deterministic tie-breaking incumbent changes.
    pub best_updates: u64,
    /// Independent DFS edge expansions.
    pub dfs_expansions: u64,
    /// Terminal cycle-space and oracle checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleSnapshot {
    /// Canonical node projections.
    pub nodes: Vec<MinimumRatioCycleNodeState>,
    /// Canonical edge projections.
    pub edges: Vec<MinimumRatioCycleEdgeState>,
    /// Current candidate ratio, absent outside candidate inspection.
    pub candidate_ratio: Option<MinimumRatioCycleRational>,
    /// Best ratio found so far, absent for an acyclic graph.
    pub best_ratio: Option<MinimumRatioCycleRational>,
    /// Number of selected cycle edges.
    pub selected_edge_count: usize,
    /// Largest absolute visible incidence imbalance.
    pub maximum_absolute_balance: u32,
    /// Current source-level boundary.
    pub stage: MinimumRatioCycleStage,
    /// Exact bounded work counters.
    pub metrics: MinimumRatioCycleMetrics,
}

/// One reversible primitive transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleTraceEvent {
    /// Stable revision-owned event identity.
    pub catalog_id: &'static str,
    /// Boundary before the transition.
    pub before: MinimumRatioCycleSnapshot,
    /// Boundary after the transition.
    pub after: MinimumRatioCycleSnapshot,
}

/// Independently checked primitive result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleResult {
    /// Selected signed simple cycle; empty iff the graph is acyclic.
    pub cycle: Vec<MinimumRatioCycleArc>,
    /// Exact optimum ratio; absent iff the graph is acyclic.
    pub ratio: Option<MinimumRatioCycleRational>,
    /// Terminal state.
    pub final_snapshot: MinimumRatioCycleSnapshot,
    /// Exact bounded work counters.
    pub metrics: MinimumRatioCycleMetrics,
}

/// Result plus all reversible search and checking boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleTraceResult {
    /// Same checked result as the fast profile.
    pub result: MinimumRatioCycleResult,
    /// Ready state before input mapping.
    pub base_snapshot: MinimumRatioCycleSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<MinimumRatioCycleTraceEvent>,
    /// Terminal boundary, equal to `result.final_snapshot`.
    pub final_snapshot: MinimumRatioCycleSnapshot,
}

/// Admission, arithmetic, certificate, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MinimumRatioCycleError {
    /// Input exceeds the bounded exact interactive band.
    #[error("minimum-ratio-cycle input exceeds admission limits")]
    AdmissionLimit,
    /// Input cannot be interpreted as the source undirected ratio instance.
    #[error(
        "minimum-ratio-cycle requires zero lowers/supplies, positive bounded lengths, bounded gradients, and no self-loops"
    )]
    GraphRequirement,
    /// A checked integer operation overflowed.
    #[error("minimum-ratio-cycle arithmetic overflow")]
    ArithmeticOverflow,
    /// A candidate or selected vector was not a signed simple circulation.
    #[error("minimum-ratio-cycle circulation invariant failed")]
    CycleInvariant,
    /// The visible oracle and independent DFS oracle disagreed.
    #[error("minimum-ratio-cycle exact oracle disagreement")]
    OracleDisagreement,
    /// A deterministic work or trace ceiling was reached.
    #[error("minimum-ratio-cycle deterministic work ceiling reached")]
    WorkLimit,
    /// A supplied trace differs from deterministic replay.
    #[error("minimum-ratio-cycle trace verification failed")]
    TraceVerification,
}

/// Solves one bounded exact source-level minimum-ratio-cycle primitive.
///
/// # Errors
///
/// Rejects unsupported/oversized instances, checked arithmetic failure, work
/// ceiling exhaustion, or any independent oracle disagreement.
pub fn solve_minimum_ratio_cycle(
    graph: &FlowNetwork,
) -> Result<MinimumRatioCycleResult, MinimumRatioCycleError> {
    run_internal(graph, false).map(|run| run.result)
}

/// Records mapping, forest, cycle, incumbent, and certificate boundaries.
///
/// # Errors
///
/// Returns the same errors as [`solve_minimum_ratio_cycle`] or replay failure.
pub fn trace_minimum_ratio_cycle(
    graph: &FlowNetwork,
) -> Result<MinimumRatioCycleTraceResult, MinimumRatioCycleError> {
    let run = run_internal(graph, true)?;
    let trace = MinimumRatioCycleTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_minimum_ratio_cycle_trace(graph, &trace)?;
    Ok(trace)
}

/// Checks continuity, source-level stage identity, certificate, and replay.
///
/// # Errors
///
/// Rejects any malformed state or disagreement with a fresh deterministic run.
pub fn check_minimum_ratio_cycle_trace(
    graph: &FlowNetwork,
    trace: &MinimumRatioCycleTraceResult,
) -> Result<(), MinimumRatioCycleError> {
    validate_graph(graph)?;
    validate_base(graph, &trace.base_snapshot)?;
    if trace.events.len() < 5 || trace.events.len() > MINIMUM_RATIO_CYCLE_MAX_TRACE_EVENTS {
        return Err(MinimumRatioCycleError::TraceVerification);
    }
    let mut previous = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != previous {
            return Err(MinimumRatioCycleError::TraceVerification);
        }
        previous = &event.after;
    }
    if previous != &trace.final_snapshot
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.result.metrics != trace.final_snapshot.metrics
        || trace.events.first().map(|event| event.after.stage)
            != Some(MinimumRatioCycleStage::MapGradientLength)
        || trace.events.get(1).map(|event| event.after.stage)
            != Some(MinimumRatioCycleStage::BuildSpanningForest)
        || trace.events.last().map(|event| event.after.stage)
            != Some(MinimumRatioCycleStage::Complete)
    {
        return Err(MinimumRatioCycleError::TraceVerification);
    }
    let stages = trace
        .events
        .iter()
        .map(|event| event.after.stage)
        .collect::<Vec<_>>();
    let verify_index = stages
        .iter()
        .position(|stage| *stage == MinimumRatioCycleStage::VerifyCycleSpace)
        .ok_or(MinimumRatioCycleError::TraceVerification)?;
    if stages.get(verify_index + 1) != Some(&MinimumRatioCycleStage::CheckExhaustiveOracle)
        || stages.get(verify_index + 2) != Some(&MinimumRatioCycleStage::Complete)
        || stages[..verify_index].iter().skip(2).any(|stage| {
            !matches!(
                stage,
                MinimumRatioCycleStage::InspectVector
                    | MinimumRatioCycleStage::EvaluateCycle
                    | MinimumRatioCycleStage::UpdateBest
            )
        })
    {
        return Err(MinimumRatioCycleError::TraceVerification);
    }
    validate_terminal(graph, &trace.result)?;
    let replay = run_internal(graph, true)?;
    if replay.base_snapshot != trace.base_snapshot
        || replay.events != trace.events
        || replay.result != trace.result
    {
        return Err(MinimumRatioCycleError::TraceVerification);
    }
    Ok(())
}

struct InternalRun {
    result: MinimumRatioCycleResult,
    base_snapshot: MinimumRatioCycleSnapshot,
    events: Vec<MinimumRatioCycleTraceEvent>,
}

struct Recorder {
    current: MinimumRatioCycleSnapshot,
    events: Vec<MinimumRatioCycleTraceEvent>,
    enabled: bool,
}

impl Recorder {
    fn emit<F>(&mut self, catalog_id: &'static str, update: F) -> Result<(), MinimumRatioCycleError>
    where
        F: FnOnce(&mut MinimumRatioCycleSnapshot) -> Result<(), MinimumRatioCycleError>,
    {
        let before = self.current.clone();
        update(&mut self.current)?;
        self.current.metrics.state_transitions = self
            .current
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        if self.enabled {
            if self.events.len() >= MINIMUM_RATIO_CYCLE_MAX_TRACE_EVENTS {
                return Err(MinimumRatioCycleError::WorkLimit);
            }
            self.events.push(MinimumRatioCycleTraceEvent {
                catalog_id,
                before,
                after: self.current.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Candidate {
    signs: Vec<i8>,
    ratio: MinimumRatioCycleRational,
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    record_events: bool,
) -> Result<InternalRun, MinimumRatioCycleError> {
    validate_graph(graph)?;
    let base_snapshot = base_snapshot(graph);
    let mut recorder = Recorder {
        current: base_snapshot.clone(),
        events: Vec::new(),
        enabled: record_events,
    };
    recorder.emit("minimum-ratio-cycle.map-gradient-length", |snapshot| {
        snapshot.stage = MinimumRatioCycleStage::MapGradientLength;
        Ok(())
    })?;

    let forest = build_spanning_forest(graph)?;
    recorder.emit("minimum-ratio-cycle.build-spanning-forest", |snapshot| {
        snapshot.stage = MinimumRatioCycleStage::BuildSpanningForest;
        snapshot.nodes.clone_from(&forest.nodes);
        for (state, &tree_edge) in snapshot.edges.iter_mut().zip(&forest.tree_edges) {
            state.tree_edge = tree_edge;
        }
        snapshot.metrics.forest_edge_scans = forest.edge_scans;
        snapshot.metrics.fundamental_cycles = u64::try_from(forest.fundamental_cycles)
            .map_err(|_| MinimumRatioCycleError::ArithmeticOverflow)?;
        Ok(())
    })?;

    let edge_count = graph.edges().len();
    let vector_count = checked_pow3(edge_count)?;
    if vector_count > MINIMUM_RATIO_CYCLE_MAX_ENUMERATED_VECTORS {
        return Err(MinimumRatioCycleError::WorkLimit);
    }
    let mut best: Option<Candidate> = None;
    let mut simple_cycles = 0_u64;
    let mut comparisons = 0_u64;
    let mut best_updates = 0_u64;
    let mut last_vector_checkpoint = 0_u64;
    for code in 1..vector_count {
        let signs = decode_ternary(code, edge_count);
        if code <= MINIMUM_RATIO_CYCLE_TRACE_DENSE_VECTOR_PREFIX
            || code.saturating_sub(last_vector_checkpoint)
                >= MINIMUM_RATIO_CYCLE_TRACE_VECTOR_BLOCK_MAX
            || code + 1 == vector_count
        {
            recorder.emit(
                "minimum-ratio-cycle.inspect-vector-checkpoint",
                |snapshot| {
                    snapshot.stage = MinimumRatioCycleStage::InspectVector;
                    apply_sign_vector(graph, snapshot, &signs, None, best.as_ref());
                    snapshot.metrics.enumerated_vectors = code;
                    snapshot.metrics.simple_cycles = simple_cycles;
                    snapshot.metrics.ratio_comparisons = comparisons;
                    snapshot.metrics.best_updates = best_updates;
                    Ok(())
                },
            )?;
            last_vector_checkpoint = code;
        }
        if signs.iter().find(|&&sign| sign != 0) != Some(&1) {
            continue;
        }
        let Some(candidate) = candidate_from_signs(graph, signs)? else {
            continue;
        };
        simple_cycles = simple_cycles
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        recorder.emit("minimum-ratio-cycle.evaluate-cycle", |snapshot| {
            snapshot.stage = MinimumRatioCycleStage::EvaluateCycle;
            apply_candidate(graph, snapshot, Some(&candidate), best.as_ref());
            snapshot.metrics.enumerated_vectors = code;
            snapshot.metrics.simple_cycles = simple_cycles;
            snapshot.metrics.ratio_comparisons = comparisons;
            snapshot.metrics.best_updates = best_updates;
            Ok(())
        })?;

        let replace = match &best {
            None => true,
            Some(current) => {
                comparisons = comparisons
                    .checked_add(1)
                    .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
                match candidate.ratio.compare(current.ratio)? {
                    Ordering::Less => true,
                    Ordering::Equal => candidate.signs < current.signs,
                    Ordering::Greater => false,
                }
            }
        };
        if replace {
            best_updates = best_updates
                .checked_add(1)
                .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
            best = Some(candidate.clone());
            recorder.emit("minimum-ratio-cycle.update-best", |snapshot| {
                snapshot.stage = MinimumRatioCycleStage::UpdateBest;
                apply_candidate(graph, snapshot, Some(&candidate), best.as_ref());
                snapshot.metrics.enumerated_vectors = code;
                snapshot.metrics.simple_cycles = simple_cycles;
                snapshot.metrics.ratio_comparisons = comparisons;
                snapshot.metrics.best_updates = best_updates;
                Ok(())
            })?;
        }
    }
    let inspected_vectors = vector_count.saturating_sub(1);
    recorder.emit("minimum-ratio-cycle.verify-cycle-space", |snapshot| {
        snapshot.stage = MinimumRatioCycleStage::VerifyCycleSpace;
        apply_candidate(graph, snapshot, None, best.as_ref());
        snapshot.metrics.enumerated_vectors = inspected_vectors;
        snapshot.metrics.simple_cycles = simple_cycles;
        snapshot.metrics.ratio_comparisons = comparisons;
        snapshot.metrics.best_updates = best_updates;
        snapshot.metrics.certificate_checks = 1;
        Ok(())
    })?;

    let mut dfs_expansions = 0_u64;
    let dfs_best = enumerate_best_with_dfs(graph, &mut dfs_expansions)?;
    if dfs_best != best {
        return Err(MinimumRatioCycleError::OracleDisagreement);
    }
    recorder.emit("minimum-ratio-cycle.check-dfs-oracle", |snapshot| {
        snapshot.stage = MinimumRatioCycleStage::CheckExhaustiveOracle;
        snapshot.metrics.dfs_expansions = dfs_expansions;
        snapshot.metrics.certificate_checks = 2;
        Ok(())
    })?;
    recorder.emit("minimum-ratio-cycle.complete-primitive", |snapshot| {
        snapshot.stage = MinimumRatioCycleStage::Complete;
        Ok(())
    })?;

    let cycle = best
        .as_ref()
        .map(|candidate| cycle_arcs(graph, &candidate.signs))
        .unwrap_or_default();
    let ratio = best.as_ref().map(|candidate| candidate.ratio);
    let metrics = recorder.current.metrics;
    let result = MinimumRatioCycleResult {
        cycle,
        ratio,
        final_snapshot: recorder.current,
        metrics,
    };
    validate_terminal(graph, &result)?;
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn validate_graph(graph: &FlowNetwork) -> Result<(), MinimumRatioCycleError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > MINIMUM_RATIO_CYCLE_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > MINIMUM_RATIO_CYCLE_MAX_EDGES
    {
        return Err(MinimumRatioCycleError::AdmissionLimit);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.capacity() == 0
                || edge.capacity() > MINIMUM_RATIO_CYCLE_MAX_LENGTH
                || edge.cost().unsigned_abs()
                    > u64::try_from(MINIMUM_RATIO_CYCLE_MAX_ABS_GRADIENT)
                        .expect("positive constant")
                || edge.from() == edge.to()
        })
    {
        return Err(MinimumRatioCycleError::GraphRequirement);
    }
    Ok(())
}

fn base_snapshot(graph: &FlowNetwork) -> MinimumRatioCycleSnapshot {
    MinimumRatioCycleSnapshot {
        nodes: graph
            .nodes()
            .iter()
            .enumerate()
            .map(|(index, _)| MinimumRatioCycleNodeState {
                node: NodeIndex::try_from_usize(index).expect("admitted node index"),
                component: index,
                parent: None,
                depth: 0,
                candidate_balance: 0,
                on_candidate: false,
                on_selected: false,
            })
            .collect(),
        edges: graph
            .edges()
            .iter()
            .map(|edge| MinimumRatioCycleEdgeState {
                edge: edge.id().clone(),
                gradient: edge.cost(),
                length: edge.capacity(),
                tree_edge: false,
                candidate_sign: 0,
                selected_sign: 0,
                numerator_contribution: 0,
                denominator_contribution: 0,
            })
            .collect(),
        candidate_ratio: None,
        best_ratio: None,
        selected_edge_count: 0,
        maximum_absolute_balance: 0,
        stage: MinimumRatioCycleStage::Ready,
        metrics: MinimumRatioCycleMetrics::default(),
    }
}

fn validate_base(
    graph: &FlowNetwork,
    snapshot: &MinimumRatioCycleSnapshot,
) -> Result<(), MinimumRatioCycleError> {
    if snapshot != &base_snapshot(graph) {
        return Err(MinimumRatioCycleError::TraceVerification);
    }
    Ok(())
}

struct ForestState {
    nodes: Vec<MinimumRatioCycleNodeState>,
    tree_edges: Vec<bool>,
    fundamental_cycles: usize,
    edge_scans: u128,
}

fn build_spanning_forest(graph: &FlowNetwork) -> Result<ForestState, MinimumRatioCycleError> {
    let node_count = graph.nodes().len();
    let mut union = UnionFind::new(node_count);
    let mut tree_edges = vec![false; graph.edges().len()];
    let mut scans = 0_u128;
    for (index, edge) in graph.edges().iter().enumerate() {
        scans = scans
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        if union.join(edge.from().as_usize(), edge.to().as_usize()) {
            tree_edges[index] = true;
        }
    }
    let mut adjacency = vec![Vec::<(usize, usize)>::new(); node_count];
    for (index, edge) in graph.edges().iter().enumerate() {
        if tree_edges[index] {
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            adjacency[from].push((to, index));
            adjacency[to].push((from, index));
            scans = scans
                .checked_add(2)
                .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        }
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut seen = vec![false; node_count];
    let mut nodes = base_snapshot(graph).nodes;
    let mut component = 0_usize;
    for root in 0..node_count {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        nodes[root].component = component;
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &(next, _) in &adjacency[node] {
                scans = scans
                    .checked_add(1)
                    .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
                if !seen[next] {
                    seen[next] = true;
                    nodes[next].component = component;
                    nodes[next].parent =
                        Some(NodeIndex::try_from_usize(node).expect("admitted parent node index"));
                    nodes[next].depth = nodes[node]
                        .depth
                        .checked_add(1)
                        .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
                    queue.push_back(next);
                }
            }
        }
        component = component
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
    }
    let fundamental_cycles = graph
        .edges()
        .len()
        .checked_add(component)
        .and_then(|value| value.checked_sub(node_count))
        .ok_or(MinimumRatioCycleError::CycleInvariant)?;
    Ok(ForestState {
        nodes,
        tree_edges,
        fundamental_cycles,
        edge_scans: scans,
    })
}

fn checked_pow3(exponent: usize) -> Result<u64, MinimumRatioCycleError> {
    (0..exponent).try_fold(1_u64, |value, _| {
        value
            .checked_mul(3)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)
    })
}

fn decode_ternary(mut code: u64, edge_count: usize) -> Vec<i8> {
    let mut signs = vec![0_i8; edge_count];
    for sign in &mut signs {
        *sign = match code % 3 {
            0 => 0,
            1 => 1,
            _ => -1,
        };
        code /= 3;
    }
    signs
}

fn candidate_from_signs(
    graph: &FlowNetwork,
    mut signs: Vec<i8>,
) -> Result<Option<Candidate>, MinimumRatioCycleError> {
    if signs.len() != graph.edges().len() || signs.iter().all(|&sign| sign == 0) {
        return Ok(None);
    }
    let node_count = graph.nodes().len();
    let mut balance = vec![0_i32; node_count];
    let mut degree = vec![0_u8; node_count];
    let mut adjacency = vec![Vec::<usize>::new(); node_count];
    let mut numerator = 0_i128;
    let mut denominator = 0_u128;
    for (index, (&sign, edge)) in signs.iter().zip(graph.edges()).enumerate() {
        if sign == 0 {
            continue;
        }
        if !matches!(sign, -1 | 1) {
            return Err(MinimumRatioCycleError::CycleInvariant);
        }
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        balance[from] -= i32::from(sign);
        balance[to] += i32::from(sign);
        degree[from] = degree[from]
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        degree[to] = degree[to]
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        adjacency[from].push(to);
        adjacency[to].push(from);
        numerator = numerator
            .checked_add(i128::from(edge.cost()) * i128::from(sign))
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        denominator = denominator
            .checked_add(u128::from(edge.capacity()))
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        let _ = index;
    }
    if balance.iter().any(|&value| value != 0)
        || degree.iter().any(|&value| value != 0 && value != 2)
    {
        return Ok(None);
    }
    let Some(start) = degree.iter().position(|&value| value != 0) else {
        return Ok(None);
    };
    let mut seen = vec![false; node_count];
    seen[start] = true;
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    if degree
        .iter()
        .enumerate()
        .any(|(node, &value)| value != 0 && !seen[node])
    {
        return Ok(None);
    }
    if numerator > 0 {
        for sign in &mut signs {
            *sign = -*sign;
        }
        numerator = -numerator;
    } else if numerator == 0 {
        let reversed = signs.iter().map(|sign| -*sign).collect::<Vec<_>>();
        if reversed < signs {
            signs = reversed;
        }
    }
    Ok(Some(Candidate {
        signs,
        ratio: MinimumRatioCycleRational::new(numerator, denominator)?,
    }))
}

fn apply_candidate(
    graph: &FlowNetwork,
    snapshot: &mut MinimumRatioCycleSnapshot,
    candidate: Option<&Candidate>,
    best: Option<&Candidate>,
) {
    let candidate_signs = candidate.map_or_else(
        || vec![0_i8; graph.edges().len()],
        |candidate| candidate.signs.clone(),
    );
    apply_sign_vector(
        graph,
        snapshot,
        &candidate_signs,
        candidate.map(|candidate| candidate.ratio),
        best,
    );
}

fn apply_sign_vector(
    graph: &FlowNetwork,
    snapshot: &mut MinimumRatioCycleSnapshot,
    candidate_signs: &[i8],
    candidate_ratio: Option<MinimumRatioCycleRational>,
    best: Option<&Candidate>,
) {
    debug_assert_eq!(candidate_signs.len(), graph.edges().len());
    let selected_signs = best.map_or_else(
        || vec![0_i8; graph.edges().len()],
        |candidate| candidate.signs.clone(),
    );
    let mut balances = vec![0_i32; graph.nodes().len()];
    for ((state, edge), (&candidate_sign, &selected_sign)) in snapshot
        .edges
        .iter_mut()
        .zip(graph.edges())
        .zip(candidate_signs.iter().zip(&selected_signs))
    {
        state.candidate_sign = candidate_sign;
        state.selected_sign = selected_sign;
        state.numerator_contribution = i128::from(edge.cost()) * i128::from(candidate_sign);
        state.denominator_contribution = if candidate_sign == 0 {
            0
        } else {
            u128::from(edge.capacity())
        };
        balances[edge.from().as_usize()] -= i32::from(candidate_sign);
        balances[edge.to().as_usize()] += i32::from(candidate_sign);
    }
    for (index, node) in snapshot.nodes.iter_mut().enumerate() {
        node.candidate_balance = balances[index];
        node.on_candidate = graph
            .edges()
            .iter()
            .zip(candidate_signs)
            .any(|(edge, &sign)| {
                sign != 0 && (edge.from().as_usize() == index || edge.to().as_usize() == index)
            });
        node.on_selected = graph
            .edges()
            .iter()
            .zip(&selected_signs)
            .any(|(edge, &sign)| {
                sign != 0 && (edge.from().as_usize() == index || edge.to().as_usize() == index)
            });
    }
    snapshot.maximum_absolute_balance = balances
        .iter()
        .map(|value| value.unsigned_abs())
        .max()
        .unwrap_or(0);
    snapshot.candidate_ratio = candidate_ratio;
    snapshot.best_ratio = best.map(|candidate| candidate.ratio);
    snapshot.selected_edge_count = selected_signs.iter().filter(|&&sign| sign != 0).count();
}

fn cycle_arcs(graph: &FlowNetwork, signs: &[i8]) -> Vec<MinimumRatioCycleArc> {
    graph
        .edges()
        .iter()
        .zip(signs)
        .filter(|(_, sign)| **sign != 0)
        .map(|(edge, &sign)| MinimumRatioCycleArc {
            edge: edge.id().clone(),
            sign,
        })
        .collect()
}

fn enumerate_best_with_dfs(
    graph: &FlowNetwork,
    expansions: &mut u64,
) -> Result<Option<Candidate>, MinimumRatioCycleError> {
    let node_count = graph.nodes().len();
    let mut cycles = BTreeSet::<Vec<i8>>::new();
    for start in 0..node_count {
        let mut visited = vec![false; node_count];
        visited[start] = true;
        let mut used_edges = vec![false; graph.edges().len()];
        let mut signs = vec![0_i8; graph.edges().len()];
        dfs_cycles(
            graph,
            start,
            start,
            &mut visited,
            &mut used_edges,
            &mut signs,
            0,
            &mut cycles,
            expansions,
        )?;
    }
    let mut best: Option<Candidate> = None;
    for signs in cycles {
        let candidate =
            candidate_from_signs(graph, signs)?.ok_or(MinimumRatioCycleError::CycleInvariant)?;
        let replace = match &best {
            None => true,
            Some(current) => match candidate.ratio.compare(current.ratio)? {
                Ordering::Less => true,
                Ordering::Equal => candidate.signs < current.signs,
                Ordering::Greater => false,
            },
        };
        if replace {
            best = Some(candidate);
        }
    }
    Ok(best)
}

#[allow(clippy::too_many_arguments)]
fn dfs_cycles(
    graph: &FlowNetwork,
    start: usize,
    current: usize,
    visited: &mut [bool],
    used_edges: &mut [bool],
    signs: &mut [i8],
    path_edges: usize,
    cycles: &mut BTreeSet<Vec<i8>>,
    expansions: &mut u64,
) -> Result<(), MinimumRatioCycleError> {
    for (edge_index, edge) in graph.edges().iter().enumerate() {
        if used_edges[edge_index] {
            continue;
        }
        let (next, sign) = if edge.from().as_usize() == current {
            (edge.to().as_usize(), 1_i8)
        } else if edge.to().as_usize() == current {
            (edge.from().as_usize(), -1_i8)
        } else {
            continue;
        };
        *expansions = expansions
            .checked_add(1)
            .ok_or(MinimumRatioCycleError::ArithmeticOverflow)?;
        if *expansions > MINIMUM_RATIO_CYCLE_MAX_DFS_EXPANSIONS {
            return Err(MinimumRatioCycleError::WorkLimit);
        }
        if next == start {
            if path_edges >= 1 {
                signs[edge_index] = sign;
                if let Some(candidate) = candidate_from_signs(graph, signs.to_vec())? {
                    cycles.insert(candidate.signs);
                }
                signs[edge_index] = 0;
            }
            continue;
        }
        if next < start || visited[next] || path_edges + 1 >= graph.nodes().len() {
            continue;
        }
        visited[next] = true;
        used_edges[edge_index] = true;
        signs[edge_index] = sign;
        dfs_cycles(
            graph,
            start,
            next,
            visited,
            used_edges,
            signs,
            path_edges + 1,
            cycles,
            expansions,
        )?;
        signs[edge_index] = 0;
        used_edges[edge_index] = false;
        visited[next] = false;
    }
    Ok(())
}

fn validate_terminal(
    graph: &FlowNetwork,
    result: &MinimumRatioCycleResult,
) -> Result<(), MinimumRatioCycleError> {
    let snapshot = &result.final_snapshot;
    if snapshot.stage != MinimumRatioCycleStage::Complete
        || snapshot.nodes.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.metrics != result.metrics
        || snapshot.metrics.enumerated_vectors != checked_pow3(graph.edges().len())? - 1
        || snapshot.metrics.certificate_checks != 2
        || snapshot.maximum_absolute_balance != 0
        || snapshot.best_ratio != result.ratio
        || snapshot.selected_edge_count != result.cycle.len()
        || snapshot.candidate_ratio.is_some()
    {
        return Err(MinimumRatioCycleError::CycleInvariant);
    }
    let signs = graph
        .edges()
        .iter()
        .map(|edge| {
            result
                .cycle
                .iter()
                .find(|arc| arc.edge == *edge.id())
                .map_or(0, |arc| arc.sign)
        })
        .collect::<Vec<_>>();
    if result.ratio.is_none() != result.cycle.is_empty() {
        return Err(MinimumRatioCycleError::CycleInvariant);
    }
    let selected = candidate_from_signs(graph, signs)?;
    match (&selected, result.ratio) {
        (None, None) => {}
        (Some(candidate), Some(ratio)) if candidate.ratio == ratio => {}
        _ => return Err(MinimumRatioCycleError::CycleInvariant),
    }
    for ((state, edge), sign) in snapshot.edges.iter().zip(graph.edges()).zip(
        selected
            .as_ref()
            .map_or_else(|| vec![0; graph.edges().len()], |value| value.signs.clone()),
    ) {
        if state.edge != *edge.id()
            || state.gradient != edge.cost()
            || state.length != edge.capacity()
            || state.candidate_sign != 0
            || state.selected_sign != sign
            || state.numerator_contribution != 0
            || state.denominator_contribution != 0
        {
            return Err(MinimumRatioCycleError::CycleInvariant);
        }
    }
    let mut expansions = 0;
    if enumerate_best_with_dfs(graph, &mut expansions)? != selected
        || expansions != snapshot.metrics.dfs_expansions
    {
        return Err(MinimumRatioCycleError::OracleDisagreement);
    }
    Ok(())
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, value: usize) -> usize {
        if self.parent[value] != value {
            self.parent[value] = self.find(self.parent[value]);
        }
        self.parent[value]
    }

    fn join(&mut self, left: usize, right: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        if self.rank[left_root] < self.rank[right_root]
            || (self.rank[left_root] == self.rank[right_root] && left_root > right_root)
        {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        if self.rank[left_root] == self.rank[right_root] {
            self.rank[left_root] += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(edges: &[(&str, usize, usize, u64, i64)], nodes: usize) -> FlowNetwork {
        FlowNetwork::new(
            (0..nodes)
                .map(|index| {
                    FlowNode::new(
                        NodeId::parse(&format!("v{index}")).expect("node identity"),
                        0,
                    )
                })
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, length, gradient)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge identity"),
                    from: NodeId::parse(&format!("v{from}")).expect("from identity"),
                    to: NodeId::parse(&format!("v{to}")).expect("to identity"),
                    lower: 0,
                    capacity: length,
                    cost: gradient,
                })
                .collect(),
        )
        .expect("valid test graph")
    }

    #[test]
    fn selects_exact_best_ratio_and_signs() {
        let graph = graph(
            &[
                ("ab", 0, 1, 2, 4),
                ("bc", 1, 2, 1, -2),
                ("ca", 2, 0, 1, -1),
                ("bd", 1, 3, 1, 8),
                ("dc", 3, 2, 1, 0),
            ],
            4,
        );
        let result = solve_minimum_ratio_cycle(&graph).expect("solve");
        let ratio = result.ratio.expect("cycle");
        assert_eq!(
            ratio,
            MinimumRatioCycleRational {
                numerator: -10,
                denominator: 3
            }
        );
        assert_eq!(result.cycle.len(), 3);
        assert_eq!(result.final_snapshot.maximum_absolute_balance, 0);
    }

    #[test]
    fn trace_contains_search_best_and_both_certificates() {
        let graph = graph(
            &[("ab", 0, 1, 1, 3), ("bc", 1, 2, 2, 1), ("ca", 2, 0, 1, -8)],
            3,
        );
        let trace = trace_minimum_ratio_cycle(&graph).expect("trace");
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.after.stage == MinimumRatioCycleStage::EvaluateCycle)
        );
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.after.stage == MinimumRatioCycleStage::UpdateBest)
        );
        assert_eq!(
            trace.result.ratio,
            Some(MinimumRatioCycleRational {
                numerator: -1,
                denominator: 1
            })
        );
        check_minimum_ratio_cycle_trace(&graph, &trace).expect("replay");
        assert_eq!(
            trace.result,
            solve_minimum_ratio_cycle(&graph).expect("fast")
        );
        let vector_checkpoints = trace
            .events
            .iter()
            .filter(|event| event.after.stage == MinimumRatioCycleStage::InspectVector)
            .map(|event| event.after.metrics.enumerated_vectors)
            .collect::<Vec<_>>();
        assert_eq!(vector_checkpoints.first(), Some(&1));
        assert_eq!(
            vector_checkpoints.last(),
            Some(&trace.result.metrics.enumerated_vectors)
        );
        assert!(vector_checkpoints.windows(2).all(|pair| {
            pair[1].saturating_sub(pair[0]) <= MINIMUM_RATIO_CYCLE_TRACE_VECTOR_BLOCK_MAX
        }));
        let verification = trace
            .events
            .iter()
            .position(|event| event.after.stage == MinimumRatioCycleStage::VerifyCycleSpace)
            .expect("cycle-space verification");
        assert_eq!(
            trace.events[verification - 1]
                .after
                .metrics
                .enumerated_vectors,
            trace.events[verification].after.metrics.enumerated_vectors,
        );
    }

    #[test]
    fn reports_acyclic_instance_without_inventing_ratio() {
        let graph = graph(&[("ab", 0, 1, 1, -5), ("bc", 1, 2, 2, 9)], 3);
        let result = solve_minimum_ratio_cycle(&graph).expect("forest is valid");
        assert!(result.cycle.is_empty());
        assert_eq!(result.ratio, None);
        assert_eq!(result.metrics.fundamental_cycles, 0);
    }

    #[test]
    fn supports_parallel_edge_two_cycle() {
        let graph = graph(&[("a", 0, 1, 2, 7), ("b", 0, 1, 3, -3)], 2);
        let result = solve_minimum_ratio_cycle(&graph).expect("parallel cycle");
        assert_eq!(
            result.ratio,
            Some(MinimumRatioCycleRational {
                numerator: -2,
                denominator: 1
            })
        );
        assert_eq!(result.cycle.len(), 2);
    }

    #[test]
    fn rejects_bad_source_mapping_and_limits() {
        let base = graph(&[("a", 0, 1, 1, 0)], 2);
        let bad = FlowNetwork::new(
            base.nodes().to_vec(),
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("loop").expect("edge"),
                from: NodeId::parse("v0").expect("from"),
                to: NodeId::parse("v0").expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("model permits explicit loop");
        assert_eq!(
            solve_minimum_ratio_cycle(&bad),
            Err(MinimumRatioCycleError::GraphRequirement)
        );

        let oversized = graph(
            &(0..12)
                .map(|index| {
                    (
                        Box::leak(format!("e{index}").into_boxed_str()) as &str,
                        index % 2,
                        (index + 1) % 2,
                        1,
                        0,
                    )
                })
                .collect::<Vec<_>>(),
            2,
        );
        assert_eq!(
            solve_minimum_ratio_cycle(&oversized),
            Err(MinimumRatioCycleError::AdmissionLimit)
        );
    }

    #[test]
    fn detects_tampered_trace() {
        let graph = graph(
            &[("ab", 0, 1, 1, 2), ("bc", 1, 2, 1, 3), ("ca", 2, 0, 1, -9)],
            3,
        );
        let mut trace = trace_minimum_ratio_cycle(&graph).expect("trace");
        trace.events[2].after.maximum_absolute_balance = u32::MAX;
        assert_eq!(
            check_minimum_ratio_cycle_trace(&graph, &trace),
            Err(MinimumRatioCycleError::TraceVerification)
        );
    }
}
