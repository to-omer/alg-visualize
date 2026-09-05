//! Exact successive-shortest-augmenting-path minimum-cost maximum flow.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostMaxFlowCertificate, check_min_cost_max_flow,
    check_residual_min_cost_optimality,
};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot, residual_arc_entity_refs, residual_path_node_order,
};

/// Conservative interactive admission limit for SSAP.
pub const SSAP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge admission limit for SSAP.
pub const SSAP_MAX_EDGES: usize = 20_000;
/// Deterministic guard against pseudo-polynomial augmentation counts.
pub const SSAP_MAX_AUGMENTATIONS: u64 = 1_000_000;
/// Deterministic guard against pathological residual scanning.
pub const SSAP_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;

/// Exact deterministic counters from successive shortest augmenting paths.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SuccessiveShortestAugmentingPathMetrics {
    /// Successful source-to-sink residual augmentations.
    pub augmentations: u64,
    /// Complete reduced-cost Dijkstra searches, including the final failed one.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled across all searches.
    pub settled_nodes: u128,
    /// Positive residual arcs inspected by Dijkstra.
    pub residual_arc_scans: u128,
    /// Dual-potential update phases after successful searches.
    pub potential_updates: u64,
}

/// Certified canonical minimum-cost maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessiveShortestAugmentingPathResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut and minimum-cost certificates.
    pub certificate: MinCostMaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: SuccessiveShortestAugmentingPathMetrics,
}

/// Certified SSAP result with reversible shortest-path and augmentation events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessiveShortestAugmentingPathTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: SuccessiveShortestAugmentingPathResult,
    /// Replay boundary at the source-defined zero flow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after both independent certificates pass.
    pub final_snapshot: FlowTraceSnapshot,
}

/// SSAP construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SuccessiveShortestAugmentingPathError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds successive shortest augmenting path admission limits")]
    AdmissionLimit,
    /// The source-defined zero-flow domain was violated.
    #[error("successive shortest augmenting path requires zero lower bounds and zero supplies")]
    GraphRequirement,
    /// Source and sink are absent or equal.
    #[error("invalid successive shortest augmenting path terminals")]
    InvalidTerminals,
    /// A deterministic work ceiling was reached.
    #[error("successive shortest augmenting path work limit reached")]
    WorkLimit,
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Initial negative-cycle compatibility or final certificates failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("successive shortest augmenting path arithmetic overflow")]
    ArithmeticOverflow,
    /// A shortest-path predecessor chain was inconsistent.
    #[error("successive shortest augmenting path predecessor invariant failed")]
    PredecessorInvariant,
    /// A positive residual arc violated dual feasibility.
    #[error("successive shortest augmenting path encountered a negative reduced cost")]
    NegativeReducedCost,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves lexicographic minimum-cost maximum flow by shortest augmenting paths.
///
/// This source-defined variant starts at the feasible zero flow, so every lower
/// bound and node supply must be zero. A solver-independent all-component
/// Bellman–Ford check rejects initial negative cycles and reconstructs feasible
/// potentials. Each iteration chooses a minimum reduced-cost source-to-sink
/// path, breaking cost ties by fewer arcs, augments its full bottleneck, and
/// halts exactly when the sink is unreachable.
///
/// # Errors
///
/// Rejects admission, graph-domain, terminal, negative-cycle, reduced-cost,
/// arithmetic, residual, work-limit, predecessor, or certificate failures.
pub fn solve_successive_shortest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SuccessiveShortestAugmentingPathResult, SuccessiveShortestAugmentingPathError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves SSAP while recording prices, shortest paths, and the final no-path cut.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace invariant failures.
pub fn trace_successive_shortest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SuccessiveShortestAugmentingPathTraceResult, SuccessiveShortestAugmentingPathError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)?;
    Ok(SuccessiveShortestAugmentingPathTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: SuccessiveShortestAugmentingPathResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct WorkingState<'graph> {
    residual: ResidualState<'graph>,
    potentials: Vec<i128>,
    metrics: SuccessiveShortestAugmentingPathMetrics,
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace_enabled: bool,
) -> Result<InternalRun, SuccessiveShortestAugmentingPathError> {
    validate_input(graph, source, sink)?;
    let zero = vec![0_u64; graph.edges().len()];
    let potentials = check_residual_min_cost_optimality(graph, &zero)?;
    let mut work = WorkingState {
        residual: ResidualState::from_flows(graph, &zero)?,
        potentials,
        metrics: SuccessiveShortestAugmentingPathMetrics::default(),
    };
    let mut recorder = start_trace_recorder(graph, &work, trace_enabled)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        FlowTraceEventMetadata {
            catalog_id: "successive-shortest-augmenting-path.initial-potentials",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "ssap:initialize-feasible-potentials",
        },
        TraceView::potentials(&work.potentials),
        None,
    )?;

    augment_until_unreachable(graph, source, sink, &mut work, &mut recorder)?;

    let flows = work.residual.flows().to_vec();
    let certificate = check_min_cost_max_flow(graph, source, sink, &flows)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        FlowTraceEventMetadata {
            catalog_id: "successive-shortest-augmenting-path.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "ssap:return-minimum-cost-maximum-flow",
        },
        TraceView::empty(graph),
        Some(("flow-value", certificate.max_flow.value)),
    )?;
    Ok(InternalRun {
        result: SuccessiveShortestAugmentingPathResult {
            flows,
            certificate,
            metrics: work.metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn augment_until_unreachable(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    work: &mut WorkingState<'_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), SuccessiveShortestAugmentingPathError> {
    loop {
        if work.metrics.augmentations >= SSAP_MAX_AUGMENTATIONS {
            return Err(SuccessiveShortestAugmentingPathError::WorkLimit);
        }
        let search = shortest_path(
            &work.residual,
            source,
            sink,
            &work.potentials,
            &mut work.metrics,
            recorder.as_mut(),
            graph,
        )?;
        let path_cost = search
            .path
            .as_deref()
            .map(|path| residual_path_cost(&work.residual, path))
            .transpose()?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            FlowTraceEventMetadata {
                catalog_id: if search.path.is_some() {
                    "successive-shortest-augmenting-path.shortest-path"
                } else {
                    "successive-shortest-augmenting-path.no-augmenting-path"
                },
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: if search.path.is_some() {
                    "ssap:dijkstra-reduced-costs"
                } else {
                    "ssap:halt-when-sink-unreachable"
                },
            },
            TraceView::search(&search),
            path_cost.map(|value| ("path-cost", value)),
        )?;
        let Some(ref path) = search.path else {
            break;
        };
        let delta = record_path_preparation(graph, work, recorder, &search, path)?;
        update_potentials(
            &mut work.potentials,
            &search.distances,
            sink,
            &mut work.metrics,
        )?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            FlowTraceEventMetadata {
                catalog_id: "successive-shortest-augmenting-path.update-potentials",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "ssap:update-potentials",
            },
            TraceView {
                labels: work.potentials.iter().copied().map(Some).collect(),
                search_order: search.settled_order.clone(),
                path: path.clone(),
            },
            None,
        )?;
        work.residual.augment(path, delta)?;
        work.metrics.augmentations = work
            .metrics
            .augmentations
            .checked_add(1)
            .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
        validate_reduced_costs(&work.residual, &work.potentials)?;
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            FlowTraceEventMetadata {
                catalog_id: "successive-shortest-augmenting-path.augment",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "ssap:augment-path-bottleneck",
            },
            TraceView {
                labels: work.potentials.iter().copied().map(Some).collect(),
                search_order: search.settled_order,
                path: path.clone(),
            },
            Some(("delta", i128::from(delta))),
        )?;
    }
    Ok(())
}

fn record_path_preparation(
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    search: &Search,
    path: &[ResidualArcId],
) -> Result<u64, SuccessiveShortestAugmentingPathError> {
    for prefix_length in 1..=path.len() {
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            FlowTraceEventMetadata {
                catalog_id: "successive-shortest-augmenting-path.reconstruct-path",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "ssap:follow-predecessor-edge",
            },
            TraceView {
                labels: search.distances.clone(),
                search_order: search.settled_order.clone(),
                path: path[..prefix_length].to_vec(),
            },
            Some(("path-edges", prefix_length as i128)),
        )?;
    }
    let delta = path_bottleneck(&work.residual, path)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        FlowTraceEventMetadata {
            catalog_id: "successive-shortest-augmenting-path.bottleneck",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "ssap:measure-path-bottleneck",
        },
        TraceView {
            labels: search.distances.clone(),
            search_order: search.settled_order.clone(),
            path: path.to_vec(),
        },
        Some(("bottleneck", i128::from(delta))),
    )?;
    Ok(delta)
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), SuccessiveShortestAugmentingPathError> {
    if graph.nodes().len() > SSAP_MAX_NODES || graph.edges().len() > SSAP_MAX_EDGES {
        return Err(SuccessiveShortestAugmentingPathError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(SuccessiveShortestAugmentingPathError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(SuccessiveShortestAugmentingPathError::GraphRequirement);
    }
    Ok(())
}

struct Search {
    path: Option<Vec<ResidualArcId>>,
    distances: Vec<Option<i128>>,
    settled_order: Vec<NodeIndex>,
}

fn shortest_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    potentials: &[i128],
    metrics: &mut SuccessiveShortestAugmentingPathMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
) -> Result<Search, SuccessiveShortestAugmentingPathError> {
    let node_count = state.graph().nodes().len();
    if potentials.len() != node_count {
        return Err(SuccessiveShortestAugmentingPathError::PredecessorInvariant);
    }
    metrics.dijkstra_runs = metrics
        .dijkstra_runs
        .checked_add(1)
        .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
    let mut distances = vec![None; node_count];
    let mut hops = vec![usize::MAX; node_count];
    let mut predecessor = vec![None; node_count];
    let mut settled = vec![false; node_count];
    let mut settled_order = Vec::new();
    let mut heap = BinaryHeap::new();
    distances[source.as_usize()] = Some(0);
    hops[source.as_usize()] = 0;
    heap.push(Reverse((0_i128, 0_usize, source)));

    while let Some(Reverse((distance, hop_count, node))) = heap.pop() {
        if settled[node.as_usize()]
            || distances[node.as_usize()] != Some(distance)
            || hops[node.as_usize()] != hop_count
        {
            continue;
        }
        settled[node.as_usize()] = true;
        settled_order.push(node);
        metrics.settled_nodes = metrics
            .settled_nodes
            .checked_add(1)
            .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
        record_settled_node(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            &distances,
            node,
        )?;
        for arc in state.outgoing_arcs(node) {
            metrics.residual_arc_scans = metrics
                .residual_arc_scans
                .checked_add(1)
                .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
            if metrics.residual_arc_scans > SSAP_MAX_RESIDUAL_ARC_SCANS {
                return Err(SuccessiveShortestAugmentingPathError::WorkLimit);
            }
            let reduced = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
            if reduced < 0 {
                return Err(SuccessiveShortestAugmentingPathError::NegativeReducedCost);
            }
            let candidate_distance = distance
                .checked_add(reduced)
                .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
            let candidate_hops = hop_count
                .checked_add(1)
                .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
            let current =
                distances[arc.to.as_usize()].map(|value| (value, hops[arc.to.as_usize()]));
            let improved = current.is_none_or(|value| (candidate_distance, candidate_hops) < value);
            let arc_id = arc.id.clone();
            if improved {
                distances[arc.to.as_usize()] = Some(candidate_distance);
                hops[arc.to.as_usize()] = candidate_hops;
                predecessor[arc.to.as_usize()] = Some(arc_id.clone());
                heap.push(Reverse((candidate_distance, candidate_hops, arc.to)));
            }
            record_search_trace(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                FlowTraceEventMetadata {
                    catalog_id: "successive-shortest-augmenting-path.inspect-residual-arc",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "ssap:relax-one-reduced-cost-arc",
                },
                TraceView {
                    labels: distances.clone(),
                    search_order: settled_order.clone(),
                    path: vec![arc_id],
                },
                Some(("improved", i128::from(improved))),
            )?;
        }
    }

    finish_search(
        state,
        source,
        sink,
        node_count,
        &predecessor,
        distances,
        settled_order,
    )
}

fn finish_search(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    node_count: usize,
    predecessor: &[Option<ResidualArcId>],
    distances: Vec<Option<i128>>,
    settled_order: Vec<NodeIndex>,
) -> Result<Search, SuccessiveShortestAugmentingPathError> {
    let path = distances[sink.as_usize()]
        .map(|_| reconstruct_path(state, source, sink, node_count, predecessor))
        .transpose()?;
    Ok(Search {
        path,
        distances,
        settled_order,
    })
}

fn record_settled_node(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: SuccessiveShortestAugmentingPathMetrics,
    distances: &[Option<i128>],
    node: NodeIndex,
) -> Result<(), SuccessiveShortestAugmentingPathError> {
    let focus = vec![FlowTraceEntityRef::Node(
        graph
            .node(node)
            .ok_or(FlowTraceError::MissingEntity)?
            .id()
            .clone(),
    )];
    record_search_trace_with_focus(
        recorder,
        graph,
        state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "successive-shortest-augmenting-path.settle-node",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "ssap:settle-minimum-distance-node",
        },
        TraceView {
            labels: distances.to_vec(),
            search_order: vec![node],
            path: Vec::new(),
        },
        Some(("node", node.as_usize() as i128)),
        Some(focus),
    )
    .map_err(Into::into)
}

fn reconstruct_path(
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    node_count: usize,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, SuccessiveShortestAugmentingPathError> {
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor[cursor.as_usize()]
            .clone()
            .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)?;
        let arc = state
            .arc(&id)
            .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)?;
        if arc.to != cursor || arc.capacity == 0 {
            return Err(SuccessiveShortestAugmentingPathError::PredecessorInvariant);
        }
        cursor = arc.from;
        reversed.push(id);
        if reversed.len() > node_count {
            return Err(SuccessiveShortestAugmentingPathError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

fn update_potentials(
    potentials: &mut [i128],
    distances: &[Option<i128>],
    sink: NodeIndex,
    metrics: &mut SuccessiveShortestAugmentingPathMetrics,
) -> Result<(), SuccessiveShortestAugmentingPathError> {
    if potentials.len() != distances.len() {
        return Err(SuccessiveShortestAugmentingPathError::PredecessorInvariant);
    }
    let cutoff = distances[sink.as_usize()]
        .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)?;
    for (potential, distance) in potentials.iter_mut().zip(distances) {
        let adjustment = distance.map_or(cutoff, |value| value.min(cutoff));
        *potential = potential
            .checked_add(adjustment)
            .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
    }
    metrics.potential_updates = metrics
        .potential_updates
        .checked_add(1)
        .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
    Ok(())
}

fn residual_path_cost(
    state: &ResidualState<'_>,
    path: &[ResidualArcId],
) -> Result<i128, SuccessiveShortestAugmentingPathError> {
    path.iter().try_fold(0_i128, |sum, id| {
        let arc = state
            .arc(id)
            .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)?;
        sum.checked_add(arc.cost)
            .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)
    })
}

fn path_bottleneck(
    state: &ResidualState<'_>,
    path: &[ResidualArcId],
) -> Result<u64, SuccessiveShortestAugmentingPathError> {
    path.iter()
        .map(|id| {
            state
                .arc(id)
                .filter(|arc| arc.capacity > 0)
                .map(|arc| arc.capacity)
                .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(SuccessiveShortestAugmentingPathError::PredecessorInvariant)
}

fn validate_reduced_costs(
    state: &ResidualState<'_>,
    potentials: &[i128],
) -> Result<(), SuccessiveShortestAugmentingPathError> {
    for node in state.graph().node_indices() {
        for arc in state.outgoing_arcs(node) {
            let reduced = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(SuccessiveShortestAugmentingPathError::ArithmeticOverflow)?;
            if reduced < 0 {
                return Err(SuccessiveShortestAugmentingPathError::NegativeReducedCost);
            }
        }
    }
    Ok(())
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
        Vec::new(),
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
    fn potentials(potentials: &[i128]) -> Self {
        Self {
            labels: potentials.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }

    fn search(search: &Search) -> Self {
        Self {
            labels: search.distances.clone(),
            search_order: search.settled_order.clone(),
            path: search.path.clone().unwrap_or_default(),
        }
    }

    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
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
    record_search_trace(
        recorder,
        graph,
        &work.residual,
        work.metrics,
        metadata,
        view,
        detail,
    )
}

fn record_search_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    residual: &ResidualState<'_>,
    metrics: SuccessiveShortestAugmentingPathMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let focus_arcs = if metadata.catalog_id == "successive-shortest-augmenting-path.bottleneck" {
        view.path
            .iter()
            .filter_map(|id| residual.arc(id).map(|arc| (arc.capacity, id)))
            .min()
            .map(|(_, id)| id.clone())
            .into_iter()
            .collect::<Vec<_>>()
    } else if metadata.minimum_granularity == TraceGranularityV1::Micro {
        view.path.last().cloned().into_iter().collect::<Vec<_>>()
    } else {
        view.path.clone()
    };
    let focus = Some(residual_arc_entity_refs(graph, residual, &focus_arcs)?);
    record_search_trace_with_focus(
        recorder, graph, residual, metrics, metadata, view, detail, focus,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_search_trace_with_focus(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    residual: &ResidualState<'_>,
    metrics: SuccessiveShortestAugmentingPathMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
    focus: Option<Vec<FlowTraceEntityRef>>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let search_order = if view.path.is_empty() {
        view.search_order
    } else {
        residual_path_node_order(residual, &view.path)?
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        residual,
        view.labels,
        search_order,
        view.path,
        Vec::new(),
        trace_metrics(metrics),
    );
    if let Some(focus) = focus {
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, detail)
    }
}

const fn trace_metrics(metrics: SuccessiveShortestAugmentingPathMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.dijkstra_runs as u128,
        scaling_phases: 0,
        blocking_flow_phases: 0,
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
    use crate::algorithms::solve_edmonds_karp;
    use crate::certificate::{check_min_cost_flow, fixed_flow_divergences};
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTraceMetricId, FlowTracePatch, apply_trace_event};

    use super::*;

    fn network(nodes: &[&str], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
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
        .expect("valid network")
    }

    fn terminals(graph: &FlowNetwork) -> (NodeIndex, NodeIndex) {
        (
            graph
                .node_index(&NodeId::parse("s").expect("source id"))
                .expect("source"),
            graph
                .node_index(&NodeId::parse("t").expect("sink id"))
                .expect("sink"),
        )
    }

    #[test]
    fn augments_cheapest_path_then_higher_cost_path_and_certifies_both_objectives() {
        let graph = network(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 0, 2, 2),
                ("at", "a", "t", 0, 2, 3),
                ("sb", "s", "b", 0, 1, 0),
                ("bt", "b", "t", 0, 1, 2),
            ],
        );
        let (source, sink) = terminals(&graph);
        let traced = trace_successive_shortest_augmenting_path(&graph, source, sink).expect("SSAP");

        assert_eq!(traced.result.certificate.max_flow.value, 3);
        assert_eq!(traced.result.certificate.max_flow.cut_bound, 3);
        assert_eq!(traced.result.certificate.min_cost.total_cost, 12);
        assert_eq!(traced.result.metrics.augmentations, 2);
        assert_eq!(traced.result.metrics.dijkstra_runs, 3);
        let path_costs = traced
            .events
            .iter()
            .filter(|event| event.catalog_id.ends_with(".shortest-path"))
            .filter_map(|event| event.detail.as_ref().map(|detail| detail.value))
            .collect::<Vec<_>>();
        assert_eq!(path_costs, [2, 5]);
    }

    #[test]
    fn equal_cost_prefers_fewer_arcs_before_stable_residual_order() {
        let graph = network(
            &["s", "a", "t"],
            &[
                ("direct", "s", "t", 0, 1, 2),
                ("sa", "s", "a", 0, 1, 1),
                ("at", "a", "t", 0, 1, 1),
            ],
        );
        let (source, sink) = terminals(&graph);
        let traced = trace_successive_shortest_augmenting_path(&graph, source, sink).expect("SSAP");
        let first = traced
            .events
            .iter()
            .find(|event| event.catalog_id.ends_with(".shortest-path"))
            .expect("first path");
        let active = first
            .patches
            .iter()
            .filter(|patch| matches!(patch, crate::trace::FlowTracePatch::ActivePath { .. }))
            .count();
        assert_eq!(active, 1);
        assert_eq!(first.detail.as_ref().map(|detail| detail.value), Some(2));
    }

    #[test]
    fn negative_edge_without_cycle_uses_feasible_initial_prices() {
        let graph = network(
            &["s", "a", "t"],
            &[
                ("sa", "s", "a", 0, 2, -4),
                ("at", "a", "t", 0, 2, 1),
                ("st", "s", "t", 0, 1, 0),
            ],
        );
        let (source, sink) = terminals(&graph);
        let result = solve_successive_shortest_augmenting_path(&graph, source, sink).expect("SSAP");
        assert_eq!(result.certificate.max_flow.value, 3);
        assert_eq!(result.certificate.min_cost.total_cost, -6);
    }

    #[test]
    fn rejects_nonzero_lower_bound_and_initial_negative_cycle() {
        let lower = network(&["s", "t"], &[("st", "s", "t", 1, 2, 0)]);
        let (source, sink) = terminals(&lower);
        assert_eq!(
            solve_successive_shortest_augmenting_path(&lower, source, sink),
            Err(SuccessiveShortestAugmentingPathError::GraphRequirement)
        );

        let cycle = network(
            &["s", "t", "x"],
            &[("st", "s", "t", 0, 1, 0), ("loop", "x", "x", 0, 1, -1)],
        );
        let (source, sink) = terminals(&cycle);
        assert_eq!(
            solve_successive_shortest_augmenting_path(&cycle, source, sink),
            Err(SuccessiveShortestAugmentingPathError::Certificate(
                CertificateError::NegativeCycle
            ))
        );
    }

    #[test]
    fn trace_replays_in_both_directions_and_matches_fast_profile() {
        let graph = network(
            &["s", "a", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 2, 1),
                ("st", "s", "t", 0, 1, 5),
            ],
        );
        let (source, sink) = terminals(&graph);
        let fast = solve_successive_shortest_augmenting_path(&graph, source, sink).expect("fast");
        let traced =
            trace_successive_shortest_augmenting_path(&graph, source, sink).expect("trace");
        assert_eq!(traced.result, fast);
        let scan_events = traced
            .events
            .iter()
            .filter(|event| {
                event.catalog_id == "successive-shortest-augmenting-path.inspect-residual-arc"
            })
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scan_events.len()).expect("scan count fits u128"),
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

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn deterministic_small_graphs_match_independent_max_flow_and_fixed_value_cost() {
        for mask in 0_u64..32 {
            let cost = |value: u64| i64::try_from(value & 3).expect("cost is at most three");
            let mut edges = vec![
                ("sa", "s", "a", 0, 1 + (mask & 1), cost(mask)),
                ("sb", "s", "b", 0, 1 + ((mask >> 1) & 1), cost(mask >> 2)),
                ("at", "a", "t", 0, 1 + ((mask >> 2) & 1), cost(mask >> 1)),
                ("bt", "b", "t", 0, 1 + ((mask >> 3) & 1), cost(mask >> 3)),
            ];
            if mask & 16 != 0 {
                edges.push(("ab", "a", "b", 0, 1, 1));
            }
            let graph = network(&["s", "a", "b", "t"], &edges);
            let (source, sink) = terminals(&graph);
            let actual = solve_successive_shortest_augmenting_path(&graph, source, sink)
                .expect("SSAP fixture");
            let maximum = solve_edmonds_karp(&graph, source, sink).expect("max flow fixture");
            assert_eq!(actual.certificate.max_flow.value, maximum.certificate.value);
            let target = fixed_flow_divergences(
                &graph,
                source,
                sink,
                u64::try_from(maximum.certificate.value).expect("nonnegative value"),
            )
            .expect("target");
            check_min_cost_flow(&graph, &target, &actual.flows).expect("minimum cost certificate");
        }
    }
}
