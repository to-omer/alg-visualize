//! Dinic blocking-flow kernel over stable residual identities.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for Dinic.
pub const DINIC_MAX_NODES: usize = 2_000;
/// Conservative interactive edge limit for Dinic.
pub const DINIC_MAX_EDGES: usize = 20_000;

/// Exact operation counts from the deterministic blocking-flow kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DinicMetrics {
    /// Level-BFS invocations, including the final unsuccessful search.
    pub bfs_runs: u64,
    /// Positive residual arcs inspected by BFS, level-graph construction, or DFS.
    pub residual_arc_scans: u128,
    /// Completed blocking flows for reachable level graphs.
    pub blocking_flow_phases: u64,
    /// Successful path augmentations inside blocking-flow phases.
    pub augmentations: u64,
}

/// Certified canonical Dinic result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DinicResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: DinicMetrics,
}

/// Certified Dinic result with a complete reversible event sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DinicTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: DinicResult,
    /// Replay boundary before the first level BFS.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the verified optimal phase.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Dinic construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DinicError {
    /// Input exceeds the practical admission band for this algorithm.
    #[error("graph exceeds Dinic admission limits")]
    AdmissionLimit,
    /// A specialized complexity preset received a graph outside its domain.
    #[error("Dinic graph requirement is not satisfied: {0}")]
    GraphRequirement(&'static str),
    /// Lower-bound circulation construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final solver-independent certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact operation counters exceeded their declared domain.
    #[error("Dinic metric overflow")]
    MetricOverflow,
    /// A level path contradicted the materialized level graph.
    #[error("Dinic level-graph invariant failed")]
    LevelGraphInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves lower-bounded single-source/sink maximum flow using blocking flows.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible lower bounds, residual invariant
/// failure, metric overflow, or a result rejected by the independent certificate.
pub fn solve_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::General,
        &mut feasibility,
    )
}

/// Solves Dinic after proving that every original edge has unit capacity.
///
/// # Errors
///
/// Returns [`DinicError::GraphRequirement`] unless every lower bound is zero
/// and every upper capacity is one, then returns the same failures as
/// [`solve_dinic`].
pub fn solve_unit_capacity_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::UnitCapacity,
        &mut feasibility,
    )
}

/// Solves the unit-network Dinic preset after validating its degree contract.
///
/// # Errors
///
/// Requires unit capacities and, for every nonterminal vertex, either
/// in-degree one or out-degree one. It then returns the same failures as
/// [`solve_dinic`].
pub fn solve_unit_network_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::UnitNetwork,
        &mut feasibility,
    )
}

/// Closed set of Dinic execution presets with distinct graph contracts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DinicExecutionPreset {
    /// General lower-bounded maximum flow.
    General,
    /// Unit-capacity specialization.
    UnitCapacity,
    /// Unit-network specialization.
    UnitNetwork,
}

/// Solves one Dinic preset while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_dinic_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DinicExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<DinicResult, DinicError> {
    validate_dinic_preset(graph, source, sink, preset)?;
    solve_dinic_internal(graph, source, sink, false, feasibility).map(|run| run.result)
}

/// Solves Dinic while recording level, augmentation, and blocking-flow events.
///
/// # Errors
///
/// Returns the same failures as [`solve_dinic`], plus trace invariant failures.
pub fn trace_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicTraceResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::General,
        &mut feasibility,
    )
}

/// Traces one Dinic preset while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_dinic_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DinicExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<DinicTraceResult, DinicError> {
    validate_dinic_preset(graph, source, sink, preset)?;
    let run = solve_dinic_internal(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(DinicError::LevelGraphInvariant)?;
    Ok(DinicTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces the unit-capacity Dinic preset after validating its graph contract.
///
/// # Errors
///
/// Returns the same requirement and execution failures as
/// [`solve_unit_capacity_dinic`], plus reversible trace failures.
pub fn trace_unit_capacity_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicTraceResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::UnitCapacity,
        &mut feasibility,
    )
}

/// Traces the unit-network Dinic preset after validating its graph contract.
///
/// # Errors
///
/// Returns the same requirement and execution failures as
/// [`solve_unit_network_dinic`], plus reversible trace failures.
pub fn trace_unit_network_dinic(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DinicTraceResult, DinicError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_dinic_preset_with_feasibility(
        graph,
        source,
        sink,
        DinicExecutionPreset::UnitNetwork,
        &mut feasibility,
    )
}

fn validate_dinic_preset(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DinicExecutionPreset,
) -> Result<(), DinicError> {
    match preset {
        DinicExecutionPreset::General => Ok(()),
        DinicExecutionPreset::UnitCapacity => validate_unit_capacity_dinic_graph(graph),
        DinicExecutionPreset::UnitNetwork => validate_unit_network_dinic_graph(graph, source, sink),
    }
}

/// Validates the graph domain behind the unit-capacity complexity claim.
///
/// # Errors
///
/// Rejects any nonzero lower bound or non-unit upper capacity.
pub fn validate_unit_capacity_dinic_graph(graph: &FlowNetwork) -> Result<(), DinicError> {
    if graph
        .edges()
        .iter()
        .all(|edge| edge.lower() == 0 && edge.capacity() == 1)
    {
        Ok(())
    } else {
        Err(DinicError::GraphRequirement("unit capacity"))
    }
}

/// Validates the graph domain behind the unit-network complexity claim.
///
/// # Errors
///
/// Requires unit capacities and, for each nonterminal vertex, in-degree one or
/// out-degree one.
pub fn validate_unit_network_dinic_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), DinicError> {
    validate_unit_capacity_dinic_graph(graph)?;
    if graph.nodes().iter().all(|node| {
        let Some(index) = graph.node_index(node.id()) else {
            return false;
        };
        index == source
            || index == sink
            || graph.incoming_edges(index).len() == 1
            || graph.outgoing_edges(index).len() == 1
    }) {
        Ok(())
    } else {
        Err(DinicError::GraphRequirement("unit network"))
    }
}

struct DinicInternalRun {
    result: DinicResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_dinic_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<DinicInternalRun, DinicError> {
    if graph.nodes().len() > DINIC_MAX_NODES || graph.edges().len() > DINIC_MAX_EDGES {
        return Err(DinicError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut metrics = DinicMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, metrics, with_trace)?;

    loop {
        metrics.bfs_runs = metrics
            .bfs_runs
            .checked_add(1)
            .ok_or(DinicError::MetricOverflow)?;
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "dinic.level-bfs",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "dinic:begin-level-bfs",
            },
            DinicTraceView::level_bfs_start(graph, source),
        )?;
        let level = build_levels(&state, source, &mut metrics, recorder.as_mut())?;
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "dinic.publish-level-graph",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "dinic:publish-level-graph",
            },
            DinicTraceView::from_level(&level),
        )?;
        if level.distances[sink.as_usize()].is_none() {
            break;
        }
        run_blocking_flow(
            graph,
            &mut state,
            source,
            sink,
            &level,
            &mut metrics,
            recorder.as_mut(),
        )?;
    }

    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "dinic.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dinic:return-max-flow-min-cut",
        },
        DinicTraceView::empty(graph),
    )?;
    let result = DinicResult {
        flows,
        certificate,
        metrics,
    };
    Ok(DinicInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn run_blocking_flow(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    level: &DinicLevelGraph,
    metrics: &mut DinicMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), DinicError> {
    let mut recorder = recorder;
    record_trace(
        recorder.as_deref_mut(),
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "dinic.blocking-flow",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "dinic:begin-blocking-flow",
        },
        DinicTraceView::from_level(level),
    )?;
    let adjacency = build_level_adjacency(state, level, metrics, recorder.as_deref_mut())?;
    let mut current = vec![0_usize; graph.nodes().len()];
    while let Some(path) = next_level_path(
        state,
        source,
        sink,
        &level.distances,
        &adjacency,
        &mut current,
        metrics,
        recorder.as_deref_mut(),
    )? {
        for prefix_length in 1..=path.len() {
            record_trace(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                FlowTraceEventMetadata {
                    catalog_id: "dinic.extend-level-path-prefix",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "dinic:extend-admissible-level-path-prefix",
                },
                DinicTraceView {
                    distances: level.distances.clone(),
                    search_order: level.search_order.clone(),
                    path: path[..prefix_length].to_vec(),
                },
            )?;
        }
        let bottleneck = path
            .iter()
            .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
            .min()
            .ok_or(DinicError::LevelGraphInvariant)?;
        state.augment(&path, bottleneck)?;
        metrics.augmentations = metrics
            .augmentations
            .checked_add(1)
            .ok_or(DinicError::MetricOverflow)?;
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "dinic.augment",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "dinic:augment-admissible-path",
            },
            DinicTraceView {
                distances: level.distances.clone(),
                search_order: level.search_order.clone(),
                path,
            },
        )?;
    }
    metrics.blocking_flow_phases = metrics
        .blocking_flow_phases
        .checked_add(1)
        .ok_or(DinicError::MetricOverflow)?;
    record_trace(
        recorder,
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "dinic.complete-blocking-flow",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "dinic:complete-blocking-flow",
        },
        DinicTraceView::from_level(level),
    )?;
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: DinicMetrics,
    record_trace: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_trace {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct DinicTraceView {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl DinicTraceView {
    fn level_bfs_start(graph: &FlowNetwork, source: NodeIndex) -> Self {
        let mut distances = vec![None; graph.nodes().len()];
        distances[source.as_usize()] = Some(0);
        Self {
            distances,
            search_order: vec![source],
            path: Vec::new(),
        }
    }

    fn from_level(level: &DinicLevelGraph) -> Self {
        Self {
            distances: level.distances.clone(),
            search_order: level.search_order.clone(),
            path: Vec::new(),
        }
    }

    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            distances: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: DinicMetrics,
    metadata: FlowTraceEventMetadata,
    view: DinicTraceView,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let local_path_focus = (metadata.catalog_id == "dinic.extend-level-path-prefix").then(|| {
        view.path
            .last()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .into_iter()
            .collect::<Vec<_>>()
    });
    let level_bfs_focus = (metadata.catalog_id == "dinic.level-bfs")
        .then(|| {
            view.search_order
                .first()
                .and_then(|&node| graph.node(node))
                .map(|node| FlowTraceEntityRef::Node(node.id().clone()))
        })
        .flatten();
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.distances,
        view.search_order,
        view.path,
        Vec::new(),
        trace_metrics(metrics),
    );
    if let Some(focus) = local_path_focus {
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, None, focus)
    } else if let Some(focus) = level_bfs_focus {
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, None, vec![focus])
    } else {
        recorder.record_transition(metadata, &snapshot)
    }
}

const fn trace_metrics(metrics: DinicMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.bfs_runs as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: metrics.blocking_flow_phases as u128,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
    }
}

struct DinicLevelGraph {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

fn build_levels(
    state: &ResidualState<'_>,
    source: NodeIndex,
    metrics: &mut DinicMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<DinicLevelGraph, DinicError> {
    let mut recorder = recorder;
    let mut distances: Vec<Option<i128>> = vec![None; state.graph().nodes().len()];
    let mut search_order = vec![source];
    let mut queue = VecDeque::from([source]);
    distances[source.as_usize()] = Some(0_i128);
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            increment_scans(metrics, recorder.as_deref_mut(), &arc.id)?;
            if distances[arc.to.as_usize()].is_some() {
                continue;
            }
            let distance = distances[node.as_usize()]
                .and_then(|value| value.checked_add(1))
                .ok_or(DinicError::MetricOverflow)?;
            distances[arc.to.as_usize()] = Some(distance);
            search_order.push(arc.to);
            queue.push_back(arc.to);
        }
    }
    Ok(DinicLevelGraph {
        distances,
        search_order,
    })
}

fn build_level_adjacency(
    state: &ResidualState<'_>,
    level: &DinicLevelGraph,
    metrics: &mut DinicMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Vec<Vec<ResidualArcId>>, DinicError> {
    let mut recorder = recorder;
    let mut adjacency = vec![Vec::new(); state.graph().nodes().len()];
    for &node in &level.search_order {
        let Some(from_level) = level.distances[node.as_usize()] else {
            return Err(DinicError::LevelGraphInvariant);
        };
        for arc in state.outgoing_arcs(node) {
            increment_scans(metrics, recorder.as_deref_mut(), &arc.id)?;
            if level.distances[arc.to.as_usize()] == from_level.checked_add(1) {
                adjacency[node.as_usize()].push(arc.id);
            }
        }
    }
    Ok(adjacency)
}

#[allow(clippy::too_many_arguments)]
fn next_level_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    levels: &[Option<i128>],
    adjacency: &[Vec<ResidualArcId>],
    current: &mut [usize],
    metrics: &mut DinicMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Option<Vec<ResidualArcId>>, DinicError> {
    let mut recorder = recorder;
    let mut node_stack = vec![source];
    let mut path = Vec::new();
    loop {
        let node = *node_stack.last().ok_or(DinicError::LevelGraphInvariant)?;
        if node == sink {
            return Ok(Some(path));
        }
        let node_index = node.as_usize();
        let candidates = adjacency
            .get(node_index)
            .ok_or(DinicError::LevelGraphInvariant)?;
        let mut advanced = false;
        while current[node_index] < candidates.len() {
            let id = candidates[current[node_index]].clone();
            increment_scans(metrics, recorder.as_deref_mut(), &id)?;
            let arc = state.arc(&id).ok_or(DinicError::LevelGraphInvariant)?;
            if arc.capacity > 0
                && levels[arc.to.as_usize()]
                    == levels[node_index].and_then(|level| level.checked_add(1))
            {
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
            return Ok(None);
        }
        node_stack.pop();
        path.pop().ok_or(DinicError::LevelGraphInvariant)?;
        let parent = *node_stack.last().ok_or(DinicError::LevelGraphInvariant)?;
        current[parent.as_usize()] += 1;
    }
}

fn increment_scans(
    metrics: &mut DinicMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    arc: &ResidualArcId,
) -> Result<(), DinicError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(DinicError::MetricOverflow)?;
    if let Some(recorder) = recorder {
        recorder.record_metric_observation(
            FlowTraceEventMetadata {
                catalog_id: "dinic.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "dinic:inspect-residual-arc",
            },
            FlowTraceMetricId::ResidualArcScans,
            FlowTraceEntityRef::ResidualArc(arc.clone()),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(edges: &[(&str, &str, &str, u64, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let node_ids = ["a", "b", "s", "t"]
            .into_iter()
            .map(|id| NodeId::parse(id).expect("node id"))
            .collect::<Vec<_>>();
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .cloned()
                .map(|id| FlowNode::new(id, 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, lower, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("valid graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn blocking_flow_matches_edmonds_karp_and_uses_one_reachable_level_graph() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 10),
            ("sb", "s", "b", 0, 10),
            ("at", "a", "t", 0, 10),
            ("bt", "b", "t", 0, 10),
            ("ab", "a", "b", 0, 1),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
        let result = solve_dinic(&graph, source, sink).expect("Dinic maximum");

        assert_eq!(result.certificate, expected.certificate);
        assert_eq!(result.flows, expected.flows);
        assert_eq!(result.metrics.blocking_flow_phases, 1);
        assert_eq!(result.metrics.bfs_runs, 2);
        assert_eq!(result.metrics.augmentations, 2);
    }

    #[test]
    fn preserves_lower_bounds_parallel_edges_opposites_and_self_loops() {
        let (graph, source, sink) = network(&[
            ("lower", "s", "a", 2, 3),
            ("parallel", "s", "a", 0, 4),
            ("out", "a", "t", 2, 7),
            ("opposite", "a", "s", 0, 1),
            ("loop", "a", "a", 1, 5),
        ]);
        let result = solve_dinic(&graph, source, sink).expect("maximum flow");

        assert_eq!(result.certificate.value, 7);
        assert_eq!(result.certificate.cut_bound, 7);
    }

    #[test]
    fn specialized_presets_enforce_unit_capacity_and_unit_network_contracts() {
        let (non_unit, source, sink) = network(&[("st", "s", "t", 0, 2)]);
        assert_eq!(
            solve_unit_capacity_dinic(&non_unit, source, sink),
            Err(DinicError::GraphRequirement("unit capacity"))
        );

        let (unit_network, source, sink) = network(&[
            ("sa", "s", "a", 0, 1),
            ("ab", "a", "b", 0, 1),
            ("bt", "b", "t", 0, 1),
        ]);
        let result = solve_unit_network_dinic(&unit_network, source, sink)
            .expect("unit network is accepted");
        assert_eq!(result.certificate.value, 1);

        let (not_unit_network, source, sink) = network(&[
            ("sa", "s", "a", 0, 1),
            ("sb", "s", "b", 0, 1),
            ("ba", "b", "a", 0, 1),
            ("ab", "a", "b", 0, 1),
            ("at", "a", "t", 0, 1),
            ("bt", "b", "t", 0, 1),
        ]);
        assert_eq!(
            solve_unit_network_dinic(&not_unit_network, source, sink),
            Err(DinicError::GraphRequirement("unit network"))
        );
    }

    #[test]
    fn trace_replays_both_directions_and_matches_fast_result() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 5),
            ("sb", "s", "b", 0, 4),
            ("ab", "a", "b", 0, 2),
            ("at", "a", "t", 0, 3),
            ("bt", "b", "t", 0, 6),
        ]);
        let fast = solve_dinic(&graph, source, sink).expect("fast result");
        let traced = trace_dinic(&graph, source, sink).expect("trace result");

        assert_eq!(traced.result, fast);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dinic.level-bfs")
        );
        let first_level_bfs = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "dinic.level-bfs")
            .expect("first level BFS event");
        assert_eq!(
            first_level_bfs.entity_refs,
            vec![FlowTraceEntityRef::Node(
                graph.node(source).expect("source node").id().clone()
            )]
        );
        let mut first_level_snapshot = traced.base_snapshot.clone();
        apply_trace_event(
            &graph,
            &mut first_level_snapshot,
            first_level_bfs,
            FlowTraceDirection::Forward,
        )
        .expect("first level BFS replay");
        assert_eq!(first_level_snapshot.node_labels[source.as_usize()], Some(0));
        assert_eq!(
            first_level_snapshot.search_order,
            vec![graph.node(source).expect("source node").id().clone()]
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dinic.blocking-flow")
        );
        let scan_events = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "dinic.inspect-residual-arc")
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scan_events.len()).expect("scan event count"),
            fast.metrics.residual_arc_scans
        );
        assert!(scan_events.iter().all(|event| {
            event.entity_refs.len() == 1
                && matches!(event.entity_refs[0], FlowTraceEntityRef::ResidualArc(_))
                && event.patches.iter().any(|patch| {
                    matches!(
                        patch,
                        crate::trace::FlowTracePatch::Metric {
                            metric: FlowTraceMetricId::ResidualArcScans,
                            before,
                            after,
                        } if *after == before.saturating_add(1)
                    )
                })
        }));

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        assert_eq!(
            replay.metrics.blocking_flow_phases,
            u128::from(fast.metrics.blocking_flow_phases)
        );
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_the_independent_edmonds_karp_value() {
        const ARCS: [(&str, &str, &str); 10] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("st", "s", "t"),
            ("as", "a", "s"),
            ("ab", "a", "b"),
            ("at", "a", "t"),
            ("bs", "b", "s"),
            ("ba", "b", "a"),
            ("bt", "b", "t"),
            ("ta", "t", "a"),
        ];
        for seed in 0_u64..64 {
            let edges = ARCS
                .iter()
                .enumerate()
                .map(|(index, &(id, from, to))| {
                    let capacity = (seed
                        .wrapping_mul(17)
                        .wrapping_add((index as u64).wrapping_mul(29)))
                        % 11;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&edges);
            let expected =
                solve_edmonds_karp(&graph, source, sink).expect("reference maximum flow");
            let actual = solve_dinic(&graph, source, sink).expect("Dinic maximum flow");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "capacity fixture {seed}"
            );
            assert_eq!(actual.certificate.value, actual.certificate.cut_bound);
        }
    }
}
