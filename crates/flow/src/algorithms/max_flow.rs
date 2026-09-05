//! Edmonds–Karp fast path over stable residual identities.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot, residual_arc_entity_refs, residual_path_node_order,
};

/// Conservative interactive admission limit for Edmonds–Karp.
pub const EDMONDS_KARP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge admission limit for Edmonds–Karp.
pub const EDMONDS_KARP_MAX_EDGES: usize = 20_000;
/// Hard ceiling for positive residual-arc inspections in one interactive run.
pub const EDMONDS_KARP_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for successful augmentations in one interactive run.
pub const EDMONDS_KARP_MAX_AUGMENTATIONS: u64 = 10_000;

/// Exact operation counts from the deterministic fast kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EdmondsKarpMetrics {
    /// BFS invocations, including the final unsuccessful search.
    pub bfs_runs: u64,
    /// Positive residual arcs inspected by BFS.
    pub residual_arc_scans: u128,
    /// Successful path augmentations.
    pub augmentations: u64,
}

/// Certified canonical maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdmondsKarpResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: EdmondsKarpMetrics,
}

/// Certified Edmonds–Karp result with a complete reversible event sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdmondsKarpTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: EdmondsKarpResult,
    /// Replay boundary before the first BFS phase.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the verified optimal phase.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Edmonds–Karp construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EdmondsKarpError {
    /// Input exceeds the practical admission band for this algorithm.
    #[error("graph exceeds Edmonds-Karp admission limits")]
    AdmissionLimit,
    /// A deterministic execution work ceiling was reached.
    #[error("Edmonds-Karp work limit reached")]
    WorkLimit,
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
    #[error("Edmonds-Karp metric overflow")]
    MetricOverflow,
    /// A predecessor chain contradicted BFS reachability.
    #[error("Edmonds-Karp predecessor invariant failed")]
    PredecessorInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves lower-bounded single-source/sink maximum flow with stable BFS ties.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible lower bounds, residual invariant
/// failure, metric overflow, or a result that fails the independent certificate.
pub fn solve_edmonds_karp(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<EdmondsKarpResult, EdmondsKarpError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_edmonds_karp_with_feasibility(graph, source, sink, &mut feasibility)
}

/// Solves Edmonds--Karp while explicitly publishing auxiliary feasibility work
/// to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_edmonds_karp_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<EdmondsKarpResult, EdmondsKarpError> {
    solve_edmonds_karp_internal(graph, source, sink, false, feasibility).map(|run| run.result)
}

/// Solves Edmonds–Karp while recording every phase and augmentation.
///
/// # Errors
///
/// Returns the same construction/certificate failures as the fast profile,
/// plus a trace invariant failure if a reversible transaction is inconsistent.
pub fn trace_edmonds_karp(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<EdmondsKarpTraceResult, EdmondsKarpError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_edmonds_karp_with_feasibility(graph, source, sink, &mut feasibility)
}

/// Traces Edmonds--Karp while explicitly publishing auxiliary feasibility work
/// to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_edmonds_karp_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<EdmondsKarpTraceResult, EdmondsKarpError> {
    let run = solve_edmonds_karp_internal(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(EdmondsKarpError::PredecessorInvariant)?;
    Ok(EdmondsKarpTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct EdmondsKarpInternalRun {
    result: EdmondsKarpResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_edmonds_karp_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<EdmondsKarpInternalRun, EdmondsKarpError> {
    if graph.nodes().len() > EDMONDS_KARP_MAX_NODES || graph.edges().len() > EDMONDS_KARP_MAX_EDGES
    {
        return Err(EdmondsKarpError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut metrics = EdmondsKarpMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, metrics, record_trace)?;
    loop {
        metrics.bfs_runs = metrics
            .bfs_runs
            .checked_add(1)
            .ok_or(EdmondsKarpError::MetricOverflow)?;
        record_edmonds_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "edmonds-karp.bfs",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "edmonds-karp:search-shortest-augmenting-path",
            },
            EdmondsKarpTraceView::bfs_start(graph, source),
            Some(("run", i128::from(metrics.bfs_runs))),
        )?;
        let search =
            shortest_augmenting_path(&state, source, sink, &mut metrics, recorder.as_mut(), graph)?;
        record_edmonds_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "edmonds-karp.bfs-complete",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "edmonds-karp:finish-shortest-path-search",
            },
            EdmondsKarpTraceView {
                distances: search.distances.clone(),
                search_order: search.search_order.clone(),
                path: Vec::new(),
            },
            Some(("reachable", search.search_order.len() as i128)),
        )?;
        let Some(path) = search.path else {
            break;
        };
        augment_edmonds_karp_path(
            &mut recorder,
            graph,
            &mut state,
            &mut metrics,
            search.distances,
            search.search_order,
            path,
        )?;
    }
    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_edmonds_trace(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "edmonds-karp.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "edmonds-karp:return-max-flow-min-cut",
        },
        EdmondsKarpTraceView::empty(graph),
        Some(("cut", certificate.cut_bound)),
    )?;
    let result = EdmondsKarpResult {
        flows,
        certificate,
        metrics,
    };
    Ok(EdmondsKarpInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn augment_edmonds_karp_path(
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    metrics: &mut EdmondsKarpMetrics,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
) -> Result<(), EdmondsKarpError> {
    for prefix_length in 1..=path.len() {
        record_edmonds_trace(
            recorder.as_mut(),
            graph,
            state,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "edmonds-karp.reconstruct-path",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "edmonds-karp:follow-predecessor-edge",
            },
            EdmondsKarpTraceView {
                distances: distances.clone(),
                search_order: search_order.clone(),
                path: path[..prefix_length].to_vec(),
            },
            Some(("path-edges", prefix_length as i128)),
        )?;
    }
    let bottleneck = path
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(EdmondsKarpError::PredecessorInvariant)?;
    if metrics.augmentations >= EDMONDS_KARP_MAX_AUGMENTATIONS {
        return Err(EdmondsKarpError::WorkLimit);
    }
    record_edmonds_trace(
        recorder.as_mut(),
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "edmonds-karp.bottleneck",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "edmonds-karp:measure-path-bottleneck",
        },
        EdmondsKarpTraceView {
            distances: distances.clone(),
            search_order: search_order.clone(),
            path: path.clone(),
        },
        Some(("bottleneck", i128::from(bottleneck))),
    )?;
    state.augment(&path, bottleneck)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(EdmondsKarpError::MetricOverflow)?;
    record_edmonds_trace(
        recorder.as_mut(),
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "edmonds-karp.augment",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "edmonds-karp:augment-by-bottleneck",
        },
        EdmondsKarpTraceView {
            distances,
            search_order,
            path,
        },
        Some(("amount", i128::from(bottleneck))),
    )?;
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: EdmondsKarpMetrics,
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

struct EdmondsKarpTraceView {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl EdmondsKarpTraceView {
    fn bfs_start(graph: &FlowNetwork, source: NodeIndex) -> Self {
        let mut distances = vec![None; graph.nodes().len()];
        distances[source.as_usize()] = Some(0);
        Self {
            distances,
            search_order: vec![source],
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

fn record_edmonds_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: EdmondsKarpMetrics,
    metadata: FlowTraceEventMetadata,
    view: EdmondsKarpTraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let focus_arcs = if metadata.catalog_id == "edmonds-karp.bottleneck" {
        view.path
            .iter()
            .filter_map(|id| state.arc(id).map(|arc| (arc.capacity, id)))
            .min()
            .map(|(_, id)| id.clone())
            .into_iter()
            .collect::<Vec<_>>()
    } else if metadata.minimum_granularity == TraceGranularityV1::Micro {
        view.path.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        view.path.clone()
    };
    let focus_node = if metadata.catalog_id == "edmonds-karp.bfs" {
        view.search_order
            .first()
            .and_then(|&node| graph.node(node))
            .map(|node| FlowTraceEntityRef::Node(node.id().clone()))
    } else {
        None
    };
    let search_order = if view.path.is_empty() {
        view.search_order
    } else {
        residual_path_node_order(state, &view.path)?
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.distances,
        search_order,
        view.path,
        Vec::new(),
        trace_metrics(metrics),
    );
    let mut focus = residual_arc_entity_refs(graph, state, &focus_arcs)?;
    focus.extend(focus_node);
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)
}

const fn trace_metrics(metrics: EdmondsKarpMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.bfs_runs as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: 0,
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

struct EdmondsKarpSearch {
    path: Option<Vec<ResidualArcId>>,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

fn shortest_augmenting_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    metrics: &mut EdmondsKarpMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
) -> Result<EdmondsKarpSearch, EdmondsKarpError> {
    let mut predecessor = vec![None; state.graph().nodes().len()];
    let mut visited = vec![false; state.graph().nodes().len()];
    let mut distances = vec![None; state.graph().nodes().len()];
    let mut search_order = vec![source];
    let mut queue = VecDeque::from([source]);
    visited[source.as_usize()] = true;
    distances[source.as_usize()] = Some(0_i128);
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            count_residual_arc_scan(metrics)?;
            let discovered = !visited[arc.to.as_usize()];
            if discovered {
                visited[arc.to.as_usize()] = true;
                let arc_id = arc.id.clone();
                predecessor[arc.to.as_usize()] = Some(arc_id.clone());
                distances[arc.to.as_usize()] =
                    distances[node.as_usize()].and_then(|distance| distance.checked_add(1));
                search_order.push(arc.to);
            }
            let arc_id = arc.id.clone();
            record_edmonds_trace(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                FlowTraceEventMetadata {
                    catalog_id: "edmonds-karp.inspect-residual-arc",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "edmonds-karp:inspect-residual-arc",
                },
                EdmondsKarpTraceView {
                    distances: distances.clone(),
                    search_order: search_order.clone(),
                    path: vec![arc_id],
                },
                Some(("discovered", i128::from(discovered))),
            )?;
            if !discovered {
                continue;
            }
            if arc.to == sink {
                return Ok(EdmondsKarpSearch {
                    path: Some(reconstruct_path(state, source, sink, &predecessor)?),
                    distances,
                    search_order,
                });
            }
            queue.push_back(arc.to);
        }
    }
    Ok(EdmondsKarpSearch {
        path: None,
        distances,
        search_order,
    })
}

fn count_residual_arc_scan(metrics: &mut EdmondsKarpMetrics) -> Result<(), EdmondsKarpError> {
    if metrics.residual_arc_scans >= EDMONDS_KARP_MAX_RESIDUAL_ARC_SCANS {
        return Err(EdmondsKarpError::WorkLimit);
    }
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(EdmondsKarpError::MetricOverflow)?;
    Ok(())
}

fn reconstruct_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, EdmondsKarpError> {
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(EdmondsKarpError::PredecessorInvariant)?;
        let arc = state
            .arc(&id)
            .ok_or(EdmondsKarpError::PredecessorInvariant)?;
        if arc.to != cursor {
            return Err(EdmondsKarpError::PredecessorInvariant);
        }
        reversed.push(id);
        cursor = arc.from;
        if reversed.len() > state.graph().nodes().len() {
            return Err(EdmondsKarpError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_dinic;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTraceMetricId, FlowTracePatch, apply_trace_event};

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
    fn computes_classic_max_flow_and_final_failed_bfs() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 10),
            ("sb", "s", "b", 0, 10),
            ("at", "a", "t", 0, 10),
            ("bt", "b", "t", 0, 10),
            ("ab", "a", "b", 0, 1),
        ]);
        let result = solve_edmonds_karp(&graph, source, sink).expect("maximum flow");

        assert_eq!(result.certificate.value, 20);
        assert_eq!(result.metrics.augmentations, 2);
        assert_eq!(result.metrics.bfs_runs, 3);
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
        let result = solve_edmonds_karp(&graph, source, sink).expect("maximum flow");

        assert_eq!(result.certificate.value, 7);
        assert_eq!(result.certificate.cut_bound, 7);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the trace contract test checks granularity, work deltas, ordering, and bidirectional replay together"
    )]
    fn trace_replays_both_directions_and_matches_fast_result() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 5),
            ("sb", "s", "b", 0, 4),
            ("ab", "a", "b", 0, 2),
            ("at", "a", "t", 0, 3),
            ("bt", "b", "t", 0, 6),
        ]);
        let fast = solve_edmonds_karp(&graph, source, sink).expect("fast result");
        let traced = trace_edmonds_karp(&graph, source, sink).expect("trace result");

        assert_eq!(traced.result, fast);
        let source_id = graph.nodes()[source.as_usize()].id().clone();
        let first = traced.events.first().expect("BFS-start event");
        assert_eq!(first.catalog_id, "edmonds-karp.bfs");
        assert_eq!(
            first.entity_refs,
            vec![FlowTraceEntityRef::Node(source_id.clone())]
        );
        let mut bfs_start = traced.base_snapshot.clone();
        apply_trace_event(&graph, &mut bfs_start, first, FlowTraceDirection::Forward)
            .expect("BFS-start event replays");
        assert_eq!(bfs_start.search_order, vec![source_id]);
        assert_eq!(bfs_start.node_labels[source.as_usize()], Some(0));
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "edmonds-karp.bfs")
        );
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "edmonds-karp.bfs-complete"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.label == "reachable" && detail.value > 0)
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "edmonds-karp.inspect-residual-arc"
                && event.minimum_granularity == TraceGranularityV1::Micro
                && event.entity_refs.len() >= 2
        }));
        let scan_events = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "edmonds-karp.inspect-residual-arc")
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scan_events.len()).expect("scan event count fits u128"),
            traced.result.metrics.residual_arc_scans
        );
        assert!(scan_events.iter().all(|event| {
            event.patches.iter().any(|patch| {
                matches!(
                    patch,
                    FlowTracePatch::Metric {
                        metric: FlowTraceMetricId::ResidualArcScans,
                        before,
                        after,
                    } if after.checked_sub(*before) == Some(1)
                )
            })
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "edmonds-karp.reconstruct-path"
                && event.minimum_granularity == TraceGranularityV1::Micro
        }));
        assert!(traced.events.iter().all(|event| {
            event.catalog_id != "edmonds-karp.bfs-complete"
                || event.minimum_granularity == TraceGranularityV1::Phase
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "edmonds-karp.bottleneck"
                && event.minimum_granularity == TraceGranularityV1::Micro
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.label == "bottleneck" && detail.value > 0)
        }));
        assert!(traced.events.iter().all(|event| {
            event.minimum_granularity != TraceGranularityV1::Operation
                || event.catalog_id == "edmonds-karp.augment"
        }));
        let bottleneck_index = traced
            .events
            .iter()
            .position(|event| event.catalog_id == "edmonds-karp.bottleneck")
            .expect("precommit bottleneck event");
        let augment_index = traced
            .events
            .iter()
            .position(|event| event.catalog_id == "edmonds-karp.augment")
            .expect("commit event");
        assert!(bottleneck_index < augment_index);
        assert!(
            traced.events[bottleneck_index]
                .patches
                .iter()
                .all(|patch| !matches!(patch, crate::trace::FlowTracePatch::EdgeFlow { .. }))
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "edmonds-karp.augment")
        );

        assert_trace_replay(&graph, sink, &traced, &fast);
    }

    fn assert_trace_replay(
        graph: &FlowNetwork,
        sink: NodeIndex,
        traced: &EdmondsKarpTraceResult,
        fast: &EdmondsKarpResult,
    ) {
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
            if event.catalog_id == "edmonds-karp.augment" {
                assert_eq!(
                    replay.node_labels[sink.as_usize()],
                    Some(replay.active_path.len() as i128)
                );
            }
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn stable_bfs_ties_ignore_declaration_order() {
        let edges = [
            ("sa", "s", "a", 0, 5),
            ("sb", "s", "b", 0, 5),
            ("at", "a", "t", 0, 5),
            ("bt", "b", "t", 0, 5),
            ("ab", "a", "b", 0, 1),
        ];
        let mut reversed = edges;
        reversed.reverse();
        let (left, left_source, left_sink) = network(&edges);
        let (right, right_source, right_sink) = network(&reversed);
        let left = trace_edmonds_karp(&left, left_source, left_sink).expect("left trace");
        let right = trace_edmonds_karp(&right, right_source, right_sink).expect("right trace");

        assert_eq!(left.result, right.result);
        assert_eq!(left.events, right.events);
        assert_eq!(left.final_snapshot, right.final_snapshot);
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_dinic() {
        for mask in 0_u64..64 {
            let capacity = |shift: u32| 1 + ((mask >> shift) & 3);
            let (graph, source, sink) = network(&[
                ("sa", "s", "a", 0, capacity(0)),
                ("sb", "s", "b", 0, capacity(1)),
                ("ab", "a", "b", 0, capacity(2)),
                ("ba", "b", "a", 0, capacity(3)),
                ("at", "a", "t", 0, capacity(4)),
                ("bt", "b", "t", 0, capacity(5)),
            ]);
            let expected = solve_dinic(&graph, source, sink).expect("independent solver");
            let actual = solve_edmonds_karp(&graph, source, sink).expect("Edmonds-Karp");
            assert_eq!(actual.certificate, expected.certificate, "mask={mask}");
            assert_eq!(actual.metrics.bfs_runs, actual.metrics.augmentations + 1);
        }
    }
}
