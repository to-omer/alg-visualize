//! Restricted-primal minimum-cost flow with Dinitz blocking phases.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow,
    check_residual_min_cost_optimality, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowEdge, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot, residual_arc_entity_refs, residual_path_node_order,
};

/// Conservative node limit for the explicit restricted-primal trace kernel.
pub const BLOCKING_PRIMAL_DUAL_MAX_NODES: usize = 256;
/// Conservative edge limit for the explicit restricted-primal trace kernel.
pub const BLOCKING_PRIMAL_DUAL_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling for positive residual-arc inspections.
pub const BLOCKING_PRIMAL_DUAL_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;
/// Deterministic ceiling for searches, price changes, blocking phases, and augmentations.
pub const BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS: u64 = 100_000;

/// Exact counters from the restricted-primal blocking-flow kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlockingPrimalDualMetrics {
    /// Equality-subnetwork level searches, including unsuccessful searches.
    pub admissible_bfs_runs: u64,
    /// Reduced-cost multi-source Dijkstra searches.
    pub slack_searches: u64,
    /// Nodes permanently settled by slack searches.
    pub settled_nodes: u128,
    /// Positive residual arcs inspected by searches and current-arc traversal.
    pub residual_arc_scans: u128,
    /// Dual-price tightening phases.
    pub potential_updates: u64,
    /// Completed Dinitz blocking flows on a shortest equality level graph.
    pub blocking_flow_phases: u64,
    /// Successful equality-path augmentations inside blocking-flow phases.
    pub augmentations: u64,
}

/// Certified canonical blocking-flow primal-dual result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockingPrimalDualResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and feasible dual prices.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic restricted-primal operation counters.
    pub metrics: BlockingPrimalDualMetrics,
}

/// Certified result plus reversible restricted-primal events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockingPrimalDualTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: BlockingPrimalDualResult,
    /// Replay boundary at the lower-bound pseudoflow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Exact reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent optimality certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Restricted-primal construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BlockingPrimalDualError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds blocking-flow primal-dual admission limits")]
    AdmissionLimit,
    /// A deterministic work ceiling was reached.
    #[error("blocking-flow primal-dual work limit reached")]
    WorkLimit,
    /// A feasibility precheck proved the requested balances impossible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Initial dual compatibility or final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("blocking-flow primal-dual arithmetic overflow")]
    ArithmeticOverflow,
    /// Feasibility and the restricted-primal residual searches disagreed.
    #[error("blocking-flow primal-dual could not route a feasible remaining imbalance")]
    MissingPath,
    /// A level, current-arc, or path relation contradicted the published state.
    #[error("blocking-flow primal-dual level-graph invariant failed")]
    LevelGraphInvariant,
    /// A positive residual arc had negative reduced cost under visible prices.
    #[error("blocking-flow primal-dual encountered a negative reduced cost")]
    NegativeReducedCost,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by restricted-primal max-flow phases.
///
/// Prices remain dual feasible. At fixed prices, the equality subnetwork contains
/// exactly the positive residual arcs with zero reduced cost. The implementation
/// computes a maximum restricted primal flow by successive Dinitz blocking flows;
/// only when no equality path joins any surplus to any deficit are prices tightened
/// by a capped multi-source reduced-cost shortest-path label.
///
/// # Errors
///
/// Rejects admission, infeasibility, an initially nonoptimal lower pseudoflow,
/// arithmetic or work limits, residual invariants, or certificate failure.
pub fn solve_blocking_primal_dual(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<BlockingPrimalDualResult, BlockingPrimalDualError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_blocking_primal_dual_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPrimalDualResult, BlockingPrimalDualError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Traces equality-level searches, dual tightening, and blocking augmentations.
///
/// # Errors
///
/// Returns the same failures as [`solve_blocking_primal_dual`] plus trace failures.
pub fn trace_blocking_primal_dual(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<BlockingPrimalDualTraceResult, BlockingPrimalDualError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
    Ok(BlockingPrimalDualTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces blocking-flow primal--dual while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_blocking_primal_dual_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPrimalDualTraceResult, BlockingPrimalDualError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
    Ok(BlockingPrimalDualTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: BlockingPrimalDualResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct WorkingState<'graph> {
    residual: ResidualState<'graph>,
    remaining: Vec<i128>,
    potentials: Vec<i128>,
    metrics: BlockingPrimalDualMetrics,
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
) -> Result<InternalRun, BlockingPrimalDualError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, trace_enabled, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, BlockingPrimalDualError> {
    let mut work = initialize(graph, required_divergence, feasibility)?;
    let mut recorder = start_trace_recorder(graph, &work, trace_enabled)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        metadata(
            "blocking-flow-primal-dual.initialize-dual",
            TraceGranularityV1::Phase,
            "blocking-primal-dual:initialize-feasible-prices",
        ),
        TraceView::potentials(&work),
        None,
    )?;

    while has_surplus(&work.remaining) {
        validate_work(&work.metrics)?;
        let levels = equality_levels(graph, &mut work, recorder.as_mut())?;
        record_trace(
            recorder.as_mut(),
            graph,
            &work,
            metadata(
                "blocking-flow-primal-dual.build-admissible-levels",
                TraceGranularityV1::Phase,
                "blocking-primal-dual:bfs-equality-subnetwork",
            ),
            TraceView::levels(&levels),
            levels.target_level.map(|value| ("sink-level", value)),
        )?;

        if let Some(target_level) = levels.target_level {
            run_blocking_flow(graph, &mut work, &levels, target_level, recorder.as_mut())?;
        } else {
            let slack = shortest_slacks(graph, &mut work, recorder.as_mut())?;
            let cutoff = slack.cutoff.ok_or(BlockingPrimalDualError::MissingPath)?;
            if cutoff <= 0 {
                return Err(BlockingPrimalDualError::LevelGraphInvariant);
            }
            record_trace(
                recorder.as_mut(),
                graph,
                &work,
                metadata(
                    "blocking-flow-primal-dual.shortest-slack-labels",
                    TraceGranularityV1::Phase,
                    "blocking-primal-dual:compute-multi-source-slacks",
                ),
                TraceView::slacks(&slack),
                Some(("cutoff", cutoff)),
            )?;
            tighten_prices(&mut work, &slack.distances, cutoff)?;
            validate_reduced_costs(graph, &mut work, recorder.as_mut())?;
            record_trace(
                recorder.as_mut(),
                graph,
                &work,
                metadata(
                    "blocking-flow-primal-dual.tighten-dual",
                    TraceGranularityV1::Phase,
                    "blocking-primal-dual:tighten-prices-to-next-equality-arc",
                ),
                TraceView::potentials_with_order(&work, slack.search_order),
                Some(("cutoff", cutoff)),
            )?;
        }
    }

    if work.remaining.iter().any(|&value| value != 0) {
        return Err(BlockingPrimalDualError::MissingPath);
    }
    let flows = work.residual.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        metadata(
            "blocking-flow-primal-dual.optimal",
            TraceGranularityV1::Phase,
            "blocking-primal-dual:return-complementary-optimum",
        ),
        TraceView::potentials(&work),
        None,
    )?;
    Ok(InternalRun {
        result: BlockingPrimalDualResult {
            flows,
            certificate,
            metrics: work.metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn initialize<'graph>(
    graph: &'graph FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<WorkingState<'graph>, BlockingPrimalDualError> {
    if graph.nodes().len() > BLOCKING_PRIMAL_DUAL_MAX_NODES
        || graph.edges().len() > BLOCKING_PRIMAL_DUAL_MAX_EDGES
    {
        return Err(BlockingPrimalDualError::AdmissionLimit);
    }
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let potentials = check_residual_min_cost_optimality(graph, &lower_flows)?;
    let current = divergences(graph, &lower_flows)?;
    if current.len() != required_divergence.len() {
        return Err(BlockingPrimalDualError::MissingPath);
    }
    let remaining = required_divergence
        .iter()
        .zip(current)
        .map(|(&required, actual)| {
            required
                .checked_sub(actual)
                .ok_or(BlockingPrimalDualError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorkingState {
        residual: ResidualState::from_flows(graph, &lower_flows)?,
        remaining,
        potentials,
        metrics: BlockingPrimalDualMetrics::default(),
    })
}

fn has_surplus(remaining: &[i128]) -> bool {
    remaining.iter().any(|&value| value > 0)
}

struct EqualityLevels {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    target_level: Option<i128>,
}

fn equality_levels(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<EqualityLevels, BlockingPrimalDualError> {
    work.metrics.admissible_bfs_runs = checked_increment(work.metrics.admissible_bfs_runs)?;
    let node_count = work.residual.graph().nodes().len();
    let mut distances: Vec<Option<i128>> = vec![None; node_count];
    let mut search_order = Vec::new();
    let mut queue = VecDeque::new();
    for node in work.residual.graph().node_indices() {
        if work.remaining[node.as_usize()] > 0 {
            distances[node.as_usize()] = Some(0);
            search_order.push(node);
            queue.push_back(node);
        }
    }
    let mut target_level = None;
    while let Some(node) = queue.pop_front() {
        let level =
            distances[node.as_usize()].ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
        if target_level.is_some_and(|target| level >= target) {
            continue;
        }
        for arc in work.residual.outgoing_arcs(node) {
            increment_scans(&mut work.metrics)?;
            let discovered = reduced_cost(&arc, &work.potentials)? == 0
                && distances[arc.to.as_usize()].is_none();
            if discovered {
                let next_level = level
                    .checked_add(1)
                    .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
                distances[arc.to.as_usize()] = Some(next_level);
                search_order.push(arc.to);
                if work.remaining[arc.to.as_usize()] < 0 {
                    target_level =
                        Some(target_level.map_or(next_level, |old: i128| old.min(next_level)));
                }
                queue.push_back(arc.to);
            }
            publish_arc_scan(
                graph,
                work,
                recorder.as_deref_mut(),
                &arc.id,
                distances.clone(),
                "blocking-flow-primal-dual.inspect-equality-arc",
                "blocking-primal-dual:inspect-one-equality-residual-arc",
                Some(("discovered", i128::from(discovered))),
            )?;
        }
    }
    validate_work(&work.metrics)?;
    Ok(EqualityLevels {
        distances,
        search_order,
        target_level,
    })
}

struct SlackSearch {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    cutoff: Option<i128>,
}

fn shortest_slacks(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<SlackSearch, BlockingPrimalDualError> {
    work.metrics.slack_searches = checked_increment(work.metrics.slack_searches)?;
    let mut distances = vec![None; work.residual.graph().nodes().len()];
    let mut settled = vec![false; distances.len()];
    let mut heap = BinaryHeap::new();
    let mut search_order = Vec::new();
    for node in work.residual.graph().node_indices() {
        if work.remaining[node.as_usize()] > 0 {
            distances[node.as_usize()] = Some(0);
            heap.push(Reverse((0_i128, node)));
        }
    }
    while let Some(Reverse((distance, node))) = heap.pop() {
        if settled[node.as_usize()] || distances[node.as_usize()] != Some(distance) {
            continue;
        }
        settled[node.as_usize()] = true;
        search_order.push(node);
        work.metrics.settled_nodes = work
            .metrics
            .settled_nodes
            .checked_add(1)
            .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
        for arc in work.residual.outgoing_arcs(node) {
            increment_scans(&mut work.metrics)?;
            let reduced = reduced_cost(&arc, &work.potentials)?;
            let candidate = distance
                .checked_add(reduced)
                .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
            let improved = distances[arc.to.as_usize()].is_none_or(|old| candidate < old);
            if improved {
                distances[arc.to.as_usize()] = Some(candidate);
                heap.push(Reverse((candidate, arc.to)));
            }
            publish_arc_scan(
                graph,
                work,
                recorder.as_deref_mut(),
                &arc.id,
                distances.clone(),
                "blocking-flow-primal-dual.inspect-slack-arc",
                "blocking-primal-dual:inspect-one-slack-residual-arc",
                Some(("improved", i128::from(improved))),
            )?;
        }
    }
    let cutoff = work
        .residual
        .graph()
        .node_indices()
        .filter(|node| work.remaining[node.as_usize()] < 0)
        .filter_map(|node| distances[node.as_usize()])
        .min();
    validate_work(&work.metrics)?;
    Ok(SlackSearch {
        distances,
        search_order,
        cutoff,
    })
}

fn tighten_prices(
    work: &mut WorkingState<'_>,
    distances: &[Option<i128>],
    cutoff: i128,
) -> Result<(), BlockingPrimalDualError> {
    if distances.len() != work.potentials.len() {
        return Err(BlockingPrimalDualError::LevelGraphInvariant);
    }
    for (potential, distance) in work.potentials.iter_mut().zip(distances) {
        let adjustment = distance.map_or(cutoff, |value| value.min(cutoff));
        *potential = potential
            .checked_add(adjustment)
            .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    }
    work.metrics.potential_updates = checked_increment(work.metrics.potential_updates)?;
    validate_work(&work.metrics)
}

fn run_blocking_flow(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    levels: &EqualityLevels,
    target_level: i128,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), BlockingPrimalDualError> {
    let adjacency =
        build_level_adjacency(graph, work, levels, target_level, recorder.as_deref_mut())?;
    let mut current = vec![0_usize; graph.nodes().len()];
    let mut phase_augmentations = 0_u64;
    while let Some((source, sink, path)) = next_level_path(
        graph,
        work,
        levels,
        target_level,
        &adjacency,
        &mut current,
        recorder.as_deref_mut(),
    )? {
        let amount = augment_path(work, source, sink, &path)?;
        phase_augmentations = checked_increment(phase_augmentations)?;
        validate_reduced_costs(graph, work, recorder.as_deref_mut())?;
        record_trace(
            recorder.as_deref_mut(),
            graph,
            work,
            metadata(
                "blocking-flow-primal-dual.augment-admissible-path",
                TraceGranularityV1::Operation,
                "blocking-primal-dual:augment-level-path",
            ),
            TraceView::path(work, levels, path),
            Some(("delta", i128::from(amount))),
        )?;
    }
    if phase_augmentations == 0 {
        return Err(BlockingPrimalDualError::LevelGraphInvariant);
    }
    work.metrics.blocking_flow_phases = checked_increment(work.metrics.blocking_flow_phases)?;
    validate_work(&work.metrics)?;
    record_trace(
        recorder,
        graph,
        work,
        metadata(
            "blocking-flow-primal-dual.complete-blocking-flow",
            TraceGranularityV1::Phase,
            "blocking-primal-dual:complete-shortest-level-blocking-flow",
        ),
        TraceView::levels(levels),
        Some(("sink-level", target_level)),
    )?;
    Ok(())
}

fn build_level_adjacency(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    levels: &EqualityLevels,
    target_level: i128,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Vec<Vec<ResidualArcId>>, BlockingPrimalDualError> {
    let mut adjacency = vec![Vec::new(); work.residual.graph().nodes().len()];
    for &node in &levels.search_order {
        let from_level = levels.distances[node.as_usize()]
            .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
        if from_level >= target_level {
            continue;
        }
        for arc in work.residual.outgoing_arcs(node) {
            increment_scans(&mut work.metrics)?;
            let admissible = reduced_cost(&arc, &work.potentials)? == 0
                && levels.distances[arc.to.as_usize()] == from_level.checked_add(1);
            if admissible {
                adjacency[node.as_usize()].push(arc.id.clone());
            }
            publish_arc_scan(
                graph,
                work,
                recorder.as_deref_mut(),
                &arc.id,
                levels.distances.clone(),
                "blocking-flow-primal-dual.inspect-level-arc",
                "blocking-primal-dual:materialize-one-level-residual-arc",
                Some(("admissible", i128::from(admissible))),
            )?;
        }
    }
    validate_work(&work.metrics)?;
    Ok(adjacency)
}

fn next_level_path(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    levels: &EqualityLevels,
    target_level: i128,
    adjacency: &[Vec<ResidualArcId>],
    current: &mut [usize],
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Option<(NodeIndex, NodeIndex, Vec<ResidualArcId>)>, BlockingPrimalDualError> {
    let sources = work
        .residual
        .graph()
        .node_indices()
        .filter(|node| work.remaining[node.as_usize()] > 0)
        .collect::<Vec<_>>();
    for source in sources {
        let mut node_stack = vec![source];
        let mut path = Vec::new();
        loop {
            let node = *node_stack
                .last()
                .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
            if levels.distances[node.as_usize()] == Some(target_level)
                && work.remaining[node.as_usize()] < 0
            {
                return Ok(Some((source, node, path)));
            }
            let node_index = node.as_usize();
            let candidates = adjacency
                .get(node_index)
                .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
            let mut advanced = false;
            while current[node_index] < candidates.len() {
                let id = candidates[current[node_index]].clone();
                increment_scans(&mut work.metrics)?;
                let arc = work
                    .residual
                    .arc(&id)
                    .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
                let admissible = arc.capacity > 0
                    && reduced_cost(&arc, &work.potentials)? == 0
                    && levels.distances[arc.to.as_usize()]
                        == levels.distances[node_index].and_then(|value| value.checked_add(1));
                publish_arc_scan(
                    graph,
                    work,
                    recorder.as_deref_mut(),
                    &id,
                    levels.distances.clone(),
                    "blocking-flow-primal-dual.inspect-level-arc",
                    "blocking-primal-dual:advance-current-level-arc",
                    Some(("admissible", i128::from(admissible))),
                )?;
                if admissible {
                    node_stack.push(arc.to);
                    path.push(id);
                    advanced = true;
                    break;
                }
                current[node_index] += 1;
            }
            if advanced {
                continue;
            }
            if node == source {
                break;
            }
            node_stack.pop();
            path.pop()
                .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
            let parent = *node_stack
                .last()
                .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
            current[parent.as_usize()] += 1;
        }
    }
    validate_work(&work.metrics)?;
    Ok(None)
}

fn augment_path(
    work: &mut WorkingState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    path: &[ResidualArcId],
) -> Result<u64, BlockingPrimalDualError> {
    let bottleneck = path
        .iter()
        .map(|id| {
            work.residual
                .arc(id)
                .map(|arc| arc.capacity)
                .ok_or(BlockingPrimalDualError::LevelGraphInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
    let supply = u128::try_from(work.remaining[source.as_usize()])
        .map_err(|_| BlockingPrimalDualError::ArithmeticOverflow)?;
    let demand = work.remaining[sink.as_usize()]
        .checked_neg()
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    let amount = u64::try_from(u128::from(bottleneck).min(supply).min(demand))
        .map_err(|_| BlockingPrimalDualError::ArithmeticOverflow)?;
    if amount == 0 {
        return Err(BlockingPrimalDualError::LevelGraphInvariant);
    }
    work.residual.augment(path, amount)?;
    work.remaining[source.as_usize()] = work.remaining[source.as_usize()]
        .checked_sub(i128::from(amount))
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    work.remaining[sink.as_usize()] = work.remaining[sink.as_usize()]
        .checked_add(i128::from(amount))
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    work.metrics.augmentations = checked_increment(work.metrics.augmentations)?;
    validate_work(&work.metrics)?;
    Ok(amount)
}

fn reduced_cost(
    arc: &crate::residual::ResidualArc,
    potentials: &[i128],
) -> Result<i128, BlockingPrimalDualError> {
    let value = arc
        .cost
        .checked_add(potentials[arc.from.as_usize()])
        .and_then(|partial| partial.checked_sub(potentials[arc.to.as_usize()]))
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    if value < 0 {
        return Err(BlockingPrimalDualError::NegativeReducedCost);
    }
    Ok(value)
}

fn validate_reduced_costs(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), BlockingPrimalDualError> {
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            increment_scans(&mut work.metrics)?;
            reduced_cost(&arc, &work.potentials)?;
            publish_arc_scan(
                graph,
                work,
                recorder.as_deref_mut(),
                &arc.id,
                work.potentials.iter().copied().map(Some).collect(),
                "blocking-flow-primal-dual.inspect-validation-arc",
                "blocking-primal-dual:validate-one-reduced-cost-arc",
                None,
            )?;
        }
    }
    validate_work(&work.metrics)
}

fn increment_scans(metrics: &mut BlockingPrimalDualMetrics) -> Result<(), BlockingPrimalDualError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > BLOCKING_PRIMAL_DUAL_MAX_RESIDUAL_ARC_SCANS {
        return Err(BlockingPrimalDualError::WorkLimit);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish_arc_scan(
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    arc_id: &ResidualArcId,
    labels: Vec<Option<i128>>,
    catalog_id: &'static str,
    pseudocode_line: &'static str,
    detail: Option<(&'static str, i128)>,
) -> Result<(), BlockingPrimalDualError> {
    let arc = work
        .residual
        .arc(arc_id)
        .ok_or(BlockingPrimalDualError::LevelGraphInvariant)?;
    record_trace(
        recorder,
        graph,
        work,
        metadata(catalog_id, TraceGranularityV1::Micro, pseudocode_line),
        TraceView {
            labels,
            search_order: vec![arc.from, arc.to],
            path: vec![arc_id.clone()],
        },
        detail,
    )
    .map_err(Into::into)
}

fn checked_increment(value: u64) -> Result<u64, BlockingPrimalDualError> {
    value
        .checked_add(1)
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)
}

fn validate_work(metrics: &BlockingPrimalDualMetrics) -> Result<(), BlockingPrimalDualError> {
    let transitions = metrics
        .admissible_bfs_runs
        .checked_add(metrics.slack_searches)
        .and_then(|value| value.checked_add(metrics.potential_updates))
        .and_then(|value| value.checked_add(metrics.blocking_flow_phases))
        .and_then(|value| value.checked_add(metrics.augmentations))
        .ok_or(BlockingPrimalDualError::ArithmeticOverflow)?;
    if transitions > BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS
        || metrics.residual_arc_scans > BLOCKING_PRIMAL_DUAL_MAX_RESIDUAL_ARC_SCANS
    {
        return Err(BlockingPrimalDualError::WorkLimit);
    }
    Ok(())
}

fn metadata(
    catalog_id: &'static str,
    minimum_granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
) -> FlowTraceEventMetadata {
    FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    }
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    work: &WorkingState<'_>,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !enabled {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        work.remaining.clone(),
        trace_metrics(work.metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl TraceView {
    fn potentials(work: &WorkingState<'_>) -> Self {
        Self::potentials_with_order(work, Vec::new())
    }

    fn potentials_with_order(work: &WorkingState<'_>, search_order: Vec<NodeIndex>) -> Self {
        Self {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order,
            path: Vec::new(),
        }
    }

    fn levels(levels: &EqualityLevels) -> Self {
        Self {
            labels: levels.distances.clone(),
            search_order: levels.search_order.clone(),
            path: Vec::new(),
        }
    }

    fn slacks(slack: &SlackSearch) -> Self {
        Self {
            labels: slack.distances.clone(),
            search_order: slack.search_order.clone(),
            path: Vec::new(),
        }
    }

    fn path(work: &WorkingState<'_>, levels: &EqualityLevels, path: Vec<ResidualArcId>) -> Self {
        Self {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order: levels.search_order.clone(),
            path,
        }
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    if recorder.event_count()
        >= usize::try_from(BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS)
            .unwrap_or(usize::MAX)
            .saturating_add(2)
    {
        return Err(FlowTraceError::EventLimit);
    }
    // The complete admissible path belongs to the typed snapshot overlay.  The
    // event focus owns one source primitive: a saturated bottleneck arc after
    // augmentation (or the first path arc when no arc saturated).  Publishing
    // every path endpoint as Detail focus duplicates the overlay and turns a
    // long path into an apparent whole-graph selection.
    let focus_arcs = view
        .path
        .iter()
        .filter(|arc| {
            work.residual
                .arc(arc)
                .is_some_and(|residual_arc| residual_arc.capacity == 0)
        })
        .min()
        .or_else(|| view.path.first())
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let search_order = if view.path.is_empty() {
        view.search_order
    } else {
        residual_path_node_order(&work.residual, &view.path)?
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        view.labels,
        search_order,
        view.path,
        work.remaining.clone(),
        trace_metrics(work.metrics),
    );
    recorder.record_transition_with_detail_and_focus(
        metadata,
        &snapshot,
        detail,
        residual_arc_entity_refs(graph, &work.residual, &focus_arcs)?,
    )
}

const fn trace_metrics(metrics: BlockingPrimalDualMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.admissible_bfs_runs as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.slack_searches as u128,
        scaling_phases: 0,
        blocking_flow_phases: metrics.blocking_flow_phases as u128,
        relabels: metrics.potential_updates as u128,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.settled_nodes,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::{solve_potential_dijkstra_ssp, solve_simple_cycle_canceling};
    use crate::certificate::fixed_flow_divergences;
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
        .expect("network")
    }

    #[test]
    fn batches_equal_price_paths_in_one_blocking_phase() {
        let graph = network(
            &[("s", 0), ("a", 0), ("b", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 4, 1),
                ("at", "a", "t", 0, 4, 2),
                ("sb", "s", "b", 0, 4, 2),
                ("bt", "b", "t", 0, 4, 1),
                ("st", "s", "t", 0, 8, 9),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("id"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("id"))
            .expect("t");
        let target = fixed_flow_divergences(&graph, source, sink, 8).expect("target");
        let result = solve_blocking_primal_dual(&graph, &target).expect("blocking primal-dual");
        let ssp = solve_potential_dijkstra_ssp(&graph, &target).expect("SSP");
        assert_eq!(result.certificate.total_cost, 24);
        assert_eq!(result.certificate, ssp.certificate);
        assert_eq!(result.metrics.potential_updates, 1);
        assert_eq!(result.metrics.blocking_flow_phases, 1);
        assert_eq!(result.metrics.augmentations, 2);
    }

    #[test]
    fn supports_lower_bounds_parallel_opposite_arcs_and_multiple_balances() {
        let graph = network(
            &[("p", 0), ("q", 0), ("r", 0), ("x", 0)],
            &[
                ("pq0", "p", "q", 1, 4, -2),
                ("pq1", "p", "q", 0, 3, 1),
                ("qp", "q", "p", 0, 2, 4),
                ("qr", "q", "r", 0, 5, 2),
                ("px", "p", "x", 0, 4, 3),
                ("xr", "x", "r", 0, 4, 0),
                ("xx", "x", "x", 0, 2, 5),
            ],
        );
        let target = vec![4, 1, -4, -1];
        let actual = solve_blocking_primal_dual(&graph, &target).expect("blocking primal-dual");
        let cycle = solve_simple_cycle_canceling(&graph, &target).expect("cycle canceling");
        assert_eq!(actual.certificate.total_cost, cycle.certificate.total_cost);
        assert_eq!(actual.certificate.total_cost, 5);
    }

    #[test]
    fn trace_replays_both_directions_and_exposes_restricted_primal_phases() {
        let graph = network(
            &[("s", 0), ("a", 0), ("b", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 3, 1),
                ("sb", "s", "b", 0, 3, 1),
                ("bt", "b", "t", 0, 3, 1),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("id"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("id"))
            .expect("t");
        let target = fixed_flow_divergences(&graph, source, sink, 6).expect("target");
        let traced = trace_blocking_primal_dual(&graph, &target).expect("trace");
        assert_eq!(
            traced.result,
            solve_blocking_primal_dual(&graph, &target).expect("fast result")
        );
        for suffix in [
            "initialize-dual",
            "shortest-slack-labels",
            "tighten-dual",
            "build-admissible-levels",
            "augment-admissible-path",
            "complete-blocking-flow",
            "optimal",
        ] {
            assert!(traced.events.iter().any(|event| {
                event.catalog_id == format!("blocking-flow-primal-dual.{suffix}")
            }));
        }
        let augment_count = traced
            .events
            .iter()
            .filter(|event| event.catalog_id.ends_with("augment-admissible-path"))
            .count();
        assert_eq!(augment_count, 2);

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward");
            if event.catalog_id.ends_with("augment-admissible-path") {
                let prices = replay
                    .node_labels
                    .iter()
                    .copied()
                    .collect::<Option<Vec<_>>>()
                    .expect("augmentation publishes dual prices");
                let residual = ResidualState::from_flows(&graph, &replay.flows)
                    .expect("replayed residual state");
                assert!(!replay.active_path.is_empty());
                for id in &replay.active_path {
                    let arc = residual.arc(id).expect("selected residual arc");
                    let reduced =
                        arc.cost + prices[arc.from.as_usize()] - prices[arc.to.as_usize()];
                    assert_eq!(reduced, 0, "every published admissible arc is tight");
                }
            }
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn rejects_a_finite_negative_cycle_in_an_unrelated_component() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("st", "s", "t", 0, 1, 0),
                ("xy", "x", "y", 0, 1, -2),
                ("yx", "y", "x", 0, 1, 1),
            ],
        );
        assert!(matches!(
            solve_blocking_primal_dual(&graph, &[1, -1, 0, 0]),
            Err(BlockingPrimalDualError::Certificate(
                CertificateError::NegativeCycle
            ))
        ));
    }

    #[test]
    fn deterministic_acyclic_family_matches_the_independent_ssp_kernel() {
        for seed in 0_u64..64 {
            let node_ids = (0..6).map(|index| format!("n{index}")).collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0_usize..6 {
                for to in (from + 1)..6 {
                    let mixed = seed
                        .wrapping_mul(0x9e37_79b9)
                        .wrapping_add((from as u64 + 1) * 37)
                        .wrapping_add((to as u64 + 1) * 101);
                    let capacity = 1 + mixed % 4;
                    let cost = i64::try_from((mixed >> 3) % 9).expect("cost band") - 4;
                    edges.push((
                        format!("e-{from}-{to}"),
                        node_ids[from].clone(),
                        node_ids[to].clone(),
                        capacity,
                        cost,
                    ));
                }
            }
            let graph = FlowNetwork::new(
                node_ids
                    .iter()
                    .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                    .collect(),
                edges
                    .into_iter()
                    .map(|(id, from, to, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(&id).expect("edge id"),
                        from: NodeId::parse(&from).expect("tail"),
                        to: NodeId::parse(&to).expect("head"),
                        lower: 0,
                        capacity,
                        cost,
                    })
                    .collect(),
            )
            .expect("acyclic network");
            let source = graph
                .node_index(&NodeId::parse("n0").expect("source id"))
                .expect("source");
            let sink = graph
                .node_index(&NodeId::parse("n5").expect("sink id"))
                .expect("sink");
            let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");
            let blocking =
                solve_blocking_primal_dual(&graph, &target).expect("blocking primal-dual");
            let ssp = solve_potential_dijkstra_ssp(&graph, &target).expect("independent SSP");
            assert_eq!(
                blocking.certificate.total_cost, ssp.certificate.total_cost,
                "seed {seed}"
            );
            assert_eq!(
                blocking,
                solve_blocking_primal_dual(&graph, &target).expect("deterministic rerun")
            );
        }
    }

    #[test]
    fn enforces_the_small_interactive_admission_band() {
        let admitted_nodes = FlowNetwork::new(
            (0..BLOCKING_PRIMAL_DUAL_MAX_NODES)
                .map(|index| {
                    FlowNode::new(NodeId::parse(&format!("n{index:03}")).expect("node id"), 0)
                })
                .collect(),
            Vec::new(),
        )
        .expect("boundary node graph");
        assert!(
            solve_blocking_primal_dual(&admitted_nodes, &vec![0; admitted_nodes.nodes().len()])
                .is_ok()
        );

        let rejected_nodes = FlowNetwork::new(
            (0..=BLOCKING_PRIMAL_DUAL_MAX_NODES)
                .map(|index| {
                    FlowNode::new(NodeId::parse(&format!("n{index:03}")).expect("node id"), 0)
                })
                .collect(),
            Vec::new(),
        )
        .expect("large edgeless graph");
        assert_eq!(
            solve_blocking_primal_dual(&rejected_nodes, &vec![0; rejected_nodes.nodes().len()]),
            Err(BlockingPrimalDualError::AdmissionLimit)
        );

        let edge_boundary = |count: usize| {
            FlowNetwork::new(
                vec![FlowNode::new(NodeId::parse("x").expect("node id"), 0)],
                (0..count)
                    .map(|index| UnresolvedFlowEdge {
                        id: EdgeId::parse(&format!("loop-{index:04}")).expect("edge id"),
                        from: NodeId::parse("x").expect("tail"),
                        to: NodeId::parse("x").expect("head"),
                        lower: 0,
                        capacity: 1,
                        cost: 0,
                    })
                    .collect(),
            )
            .expect("parallel self-loop graph")
        };
        let admitted_edges = edge_boundary(BLOCKING_PRIMAL_DUAL_MAX_EDGES);
        assert!(solve_blocking_primal_dual(&admitted_edges, &[0]).is_ok());
        let rejected_edges = edge_boundary(BLOCKING_PRIMAL_DUAL_MAX_EDGES + 1);
        assert_eq!(
            solve_blocking_primal_dual(&rejected_edges, &[0]),
            Err(BlockingPrimalDualError::AdmissionLimit)
        );
    }

    #[test]
    fn work_limits_accept_the_exact_boundary_and_reject_the_next_unit() {
        let mut scans = BlockingPrimalDualMetrics {
            residual_arc_scans: BLOCKING_PRIMAL_DUAL_MAX_RESIDUAL_ARC_SCANS,
            ..BlockingPrimalDualMetrics::default()
        };
        assert!(validate_work(&scans).is_ok());
        assert_eq!(
            increment_scans(&mut scans),
            Err(BlockingPrimalDualError::WorkLimit)
        );

        let exact_transitions = BlockingPrimalDualMetrics {
            admissible_bfs_runs: BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS,
            ..BlockingPrimalDualMetrics::default()
        };
        assert!(validate_work(&exact_transitions).is_ok());
        let too_many_transitions = BlockingPrimalDualMetrics {
            admissible_bfs_runs: BLOCKING_PRIMAL_DUAL_MAX_STATE_TRANSITIONS + 1,
            ..BlockingPrimalDualMetrics::default()
        };
        assert_eq!(
            validate_work(&too_many_transitions),
            Err(BlockingPrimalDualError::WorkLimit)
        );
    }
}
