//! Hopcroft–Karp phases over an explicit native bipartite-matching model.

use std::collections::VecDeque;

use thiserror::Error;

use crate::bipartite::{BipartiteMatchingGraph, BipartiteModelError};
use crate::certificate::{
    BipartiteMatchingCertificate, CertificateError, check_bipartite_matching,
};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for the native matching model.
pub const HOPCROFT_KARP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge limit, including optional adapter edges.
pub const HOPCROFT_KARP_MAX_EDGES: usize = 20_000;
/// Explicit scan ceiling guarding eager trace and malformed-state regressions.
pub const HOPCROFT_KARP_MAX_EDGE_SCANS: u128 = 20_000_000;
/// Explicit state-transition ceiling guarding eager trace materialization.
pub const HOPCROFT_KARP_MAX_STATE_TRANSITIONS: u64 = 100_000;

/// Exact counters from the deterministic stable-ID kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HopcroftKarpMetrics {
    /// Alternating BFS runs, including the final unsuccessful search.
    pub bfs_runs: u64,
    /// Compatibility edges inspected by BFS or DFS.
    pub edge_scans: u128,
    /// Reachable shortest-path phases completed.
    pub phases: u64,
    /// Vertex-disjoint shortest augmenting paths applied.
    pub augmentations: u64,
    /// Free-left roots submitted to layered DFS.
    pub dfs_roots: u64,
}

/// Certified canonical maximum-cardinality matching.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HopcroftKarpResult {
    /// Unit-flow projection in canonical original-edge order.
    pub flows: Vec<u64>,
    /// Independently reconstructed matching and minimum-cover certificate.
    pub certificate: BipartiteMatchingCertificate,
    /// Deterministic kernel counters.
    pub metrics: HopcroftKarpMetrics,
}

/// Certified result with a complete reversible phase/augmentation trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HopcroftKarpTraceResult {
    /// Same result produced by the fast profile.
    pub result: HopcroftKarpResult,
    /// Replay boundary before the first alternating BFS.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible events.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the independent certificate succeeds.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Native matching construction, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HopcroftKarpError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds Hopcroft-Karp admission limits")]
    AdmissionLimit,
    /// Exact scan or transition budget was exceeded.
    #[error("Hopcroft-Karp work limit exceeded")]
    WorkLimit,
    /// Explicit partition or flow-adapter validation failed.
    #[error(transparent)]
    Model(#[from] BipartiteModelError),
    /// Unit-flow mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent matching/cover certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact counter arithmetic overflowed.
    #[error("Hopcroft-Karp metric overflow")]
    MetricOverflow,
    /// Layered path or pair arrays contradicted the phase invariant.
    #[error("Hopcroft-Karp layered matching invariant failed")]
    Invariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves a validated native bipartite-matching model.
///
/// # Errors
///
/// Rejects out-of-band models, invalid partitions/adapters, bounded-work
/// exhaustion, invariant failure, or an independently rejected final result.
pub fn solve_hopcroft_karp(
    graph: &FlowNetwork,
    left: &[String],
    right: &[String],
    adapter: Option<(&str, &str)>,
) -> Result<HopcroftKarpResult, HopcroftKarpError> {
    solve_internal(graph, left, right, adapter, false).map(|run| run.result)
}

/// Solves while recording alternating BFS, atomic augmentation, and phase events.
///
/// # Errors
///
/// Returns the same failures as [`solve_hopcroft_karp`], plus reversible trace
/// construction failures.
pub fn trace_hopcroft_karp(
    graph: &FlowNetwork,
    left: &[String],
    right: &[String],
    adapter: Option<(&str, &str)>,
) -> Result<HopcroftKarpTraceResult, HopcroftKarpError> {
    let run = solve_internal(graph, left, right, adapter, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(HopcroftKarpError::Invariant)?;
    Ok(HopcroftKarpTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: HopcroftKarpResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    left: &[String],
    right: &[String],
    adapter: Option<(&str, &str)>,
    with_trace: bool,
) -> Result<InternalRun, HopcroftKarpError> {
    if graph.nodes().len() > HOPCROFT_KARP_MAX_NODES
        || graph.edges().len() > HOPCROFT_KARP_MAX_EDGES
    {
        return Err(HopcroftKarpError::AdmissionLimit);
    }
    let model = BipartiteMatchingGraph::new(graph, left, right, adapter)?;
    let mut kernel = HopcroftKarpKernel::new(graph, &model, with_trace)?;
    kernel.solve(graph, &model)?;
    kernel.finish(graph, &model)
}

struct HopcroftKarpKernel<'graph> {
    pair_by_left: Vec<Option<usize>>,
    pair_by_right: Vec<Option<usize>>,
    state: ResidualState<'graph>,
    metrics: HopcroftKarpMetrics,
    transitions: u64,
    recorder: Option<FlowTraceRecorder<'graph>>,
}

impl<'graph> HopcroftKarpKernel<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
        with_trace: bool,
    ) -> Result<Self, HopcroftKarpError> {
        let state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
        let metrics = HopcroftKarpMetrics::default();
        let recorder = start_trace(graph, &state, metrics, with_trace)?;
        Ok(Self {
            pair_by_left: vec![None; model.left.len()],
            pair_by_right: vec![None; model.right.len()],
            state,
            metrics,
            transitions: 0,
            recorder,
        })
    }

    fn solve(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
    ) -> Result<(), HopcroftKarpError> {
        loop {
            self.metrics.bfs_runs = self
                .metrics
                .bfs_runs
                .checked_add(1)
                .ok_or(HopcroftKarpError::MetricOverflow)?;
            let mut layers = build_layers(
                model,
                &self.pair_by_left,
                &self.pair_by_right,
                &mut self.metrics,
            )?;
            self.record(
                graph,
                FlowTraceEventMetadata {
                    catalog_id: "hopcroft-karp.level-bfs",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "hopcroft-karp:build-shortest-alternating-layers",
                },
                &layers,
                Vec::new(),
                layers.shortest_actual_length(),
            )?;
            if layers.shortest_units.is_none() {
                return Ok(());
            }
            self.augment_phase(graph, model, &mut layers)?;
            self.metrics.phases = self
                .metrics
                .phases
                .checked_add(1)
                .ok_or(HopcroftKarpError::MetricOverflow)?;
            self.record(
                graph,
                FlowTraceEventMetadata {
                    catalog_id: "hopcroft-karp.phase-complete",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "hopcroft-karp:complete-maximal-disjoint-shortest-set",
                },
                &layers,
                Vec::new(),
                Some(i128::from(self.metrics.phases)),
            )?;
        }
    }

    fn augment_phase(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
        layers: &mut LayerView,
    ) -> Result<(), HopcroftKarpError> {
        let roots = self
            .pair_by_left
            .iter()
            .enumerate()
            .filter_map(|(left, pair)| pair.is_none().then_some(left))
            .collect::<Vec<_>>();
        let mut current = vec![0; model.left.len()];
        for root in roots {
            if self.pair_by_left[root].is_some() || layers.left_distance[root].is_none() {
                continue;
            }
            self.metrics.dfs_roots = self
                .metrics
                .dfs_roots
                .checked_add(1)
                .ok_or(HopcroftKarpError::MetricOverflow)?;
            let mut search = LayeredDfs {
                model,
                shortest_units: layers.shortest_units.ok_or(HopcroftKarpError::Invariant)?,
                pair_by_left: &self.pair_by_left,
                pair_by_right: &self.pair_by_right,
                distance: &mut layers.left_distance,
                current: &mut current,
                metrics: &mut self.metrics,
            };
            let Some(unmatched_path) = find_layered_path(&mut search, root)? else {
                continue;
            };
            self.record_alternating_path_prefixes(graph, model, layers, &unmatched_path)?;
            self.apply_augmentation(graph, model, layers, &unmatched_path)?;
        }
        Ok(())
    }

    fn record_alternating_path_prefixes(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
        layers: &LayerView,
        unmatched_path: &[usize],
    ) -> Result<(), HopcroftKarpError> {
        let residual_path = residual_path(model, &self.pair_by_right, unmatched_path, graph)?;
        for prefix_length in 1..=residual_path.len() {
            self.record(
                graph,
                FlowTraceEventMetadata {
                    catalog_id: "hopcroft-karp.extend-alternating-path",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "hopcroft-karp:extend-layered-alternating-path",
                },
                layers,
                residual_path[..prefix_length].to_vec(),
                Some(i128::try_from(prefix_length).map_err(|_| HopcroftKarpError::MetricOverflow)?),
            )?;
        }
        Ok(())
    }

    fn apply_augmentation(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
        layers: &LayerView,
        unmatched_path: &[usize],
    ) -> Result<(), HopcroftKarpError> {
        let residual_path = residual_path(model, &self.pair_by_right, unmatched_path, graph)?;
        self.state.augment(&residual_path, 1)?;
        apply_matching_path(
            model,
            unmatched_path,
            &mut self.pair_by_left,
            &mut self.pair_by_right,
        )?;
        let expected = model.flows_from_pairs(graph, &self.pair_by_left, &self.pair_by_right)?;
        if self.state.flows() != expected {
            return Err(HopcroftKarpError::Invariant);
        }
        self.metrics.augmentations = self
            .metrics
            .augmentations
            .checked_add(1)
            .ok_or(HopcroftKarpError::MetricOverflow)?;
        self.record(
            graph,
            FlowTraceEventMetadata {
                catalog_id: "hopcroft-karp.augment",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "hopcroft-karp:symmetric-difference-shortest-path",
            },
            layers,
            residual_path,
            Some(i128::from(self.metrics.augmentations)),
        )
    }

    fn record(
        &mut self,
        graph: &'graph FlowNetwork,
        metadata: FlowTraceEventMetadata,
        layers: &LayerView,
        path: Vec<ResidualArcId>,
        detail: Option<i128>,
    ) -> Result<(), HopcroftKarpError> {
        record_trace(
            self.recorder.as_mut(),
            graph,
            &self.state,
            self.metrics,
            metadata,
            layers,
            path,
            detail,
            &mut self.transitions,
        )
    }

    fn finish(
        mut self,
        graph: &'graph FlowNetwork,
        model: &BipartiteMatchingGraph,
    ) -> Result<InternalRun, HopcroftKarpError> {
        let flows = self.state.flows().to_vec();
        let certificate = check_bipartite_matching(graph, model, &flows)?;
        let final_layers = build_certificate_view(model, &self.pair_by_left, &self.pair_by_right)?;
        self.record(
            graph,
            FlowTraceEventMetadata {
                catalog_id: "hopcroft-karp.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "hopcroft-karp:return-matching-and-minimum-cover",
            },
            &final_layers,
            Vec::new(),
            Some(i128::from(certificate.cardinality)),
        )?;
        Ok(InternalRun {
            result: HopcroftKarpResult {
                flows,
                certificate,
                metrics: self.metrics,
            },
            trace: self.recorder.map(FlowTraceRecorder::finish),
        })
    }
}

struct LayerView {
    left_distance: Vec<Option<u32>>,
    node_labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    shortest_units: Option<u32>,
}

impl LayerView {
    fn shortest_actual_length(&self) -> Option<i128> {
        self.shortest_units
            .and_then(|units| units.checked_mul(2))
            .and_then(|length| length.checked_sub(1))
            .map(i128::from)
    }
}

fn build_layers(
    model: &BipartiteMatchingGraph,
    pair_by_left: &[Option<usize>],
    pair_by_right: &[Option<usize>],
    metrics: &mut HopcroftKarpMetrics,
) -> Result<LayerView, HopcroftKarpError> {
    let node_count = model
        .left
        .iter()
        .chain(&model.right)
        .map(|node| node.as_usize())
        .max()
        .and_then(|index| index.checked_add(1))
        .ok_or(HopcroftKarpError::Invariant)?;
    let adapter_max = model
        .source
        .into_iter()
        .chain(model.sink)
        .map(NodeIndex::as_usize)
        .max()
        .and_then(|index| index.checked_add(1))
        .unwrap_or(0);
    let mut node_labels = vec![None; node_count.max(adapter_max)];
    let mut left_distance: Vec<Option<u32>> = vec![None; model.left.len()];
    let mut search_order = Vec::new();
    let mut queue = VecDeque::new();
    for (left, pair) in pair_by_left.iter().enumerate() {
        if pair.is_none() {
            left_distance[left] = Some(0);
            node_labels[model.left[left].as_usize()] = Some(0);
            search_order.push(model.left[left]);
            queue.push_back(left);
        }
    }
    let mut shortest_units: Option<u32> = None;
    while let Some(left) = queue.pop_front() {
        let distance = left_distance[left].ok_or(HopcroftKarpError::Invariant)?;
        if shortest_units.is_some_and(|shortest| distance >= shortest) {
            continue;
        }
        for &ordinal in &model.adjacency[left] {
            increment_scans(metrics)?;
            if pair_by_left[left] == Some(ordinal) {
                continue;
            }
            let edge = model
                .compatibility_edges
                .get(ordinal)
                .ok_or(HopcroftKarpError::Invariant)?;
            let right_label = i128::from(
                distance
                    .checked_mul(2)
                    .and_then(|value| value.checked_add(1))
                    .ok_or(HopcroftKarpError::MetricOverflow)?,
            );
            if node_labels[model.right[edge.right].as_usize()].is_none() {
                node_labels[model.right[edge.right].as_usize()] = Some(right_label);
                search_order.push(model.right[edge.right]);
            }
            let next_distance = distance
                .checked_add(1)
                .ok_or(HopcroftKarpError::MetricOverflow)?;
            let Some(matched) = pair_by_right[edge.right] else {
                shortest_units =
                    Some(shortest_units.map_or(next_distance, |old| old.min(next_distance)));
                continue;
            };
            if shortest_units.is_some_and(|shortest| next_distance >= shortest) {
                continue;
            }
            let next_left = model
                .compatibility_edges
                .get(matched)
                .ok_or(HopcroftKarpError::Invariant)?
                .left;
            if left_distance[next_left].is_none() {
                left_distance[next_left] = Some(next_distance);
                node_labels[model.left[next_left].as_usize()] = Some(i128::from(
                    next_distance
                        .checked_mul(2)
                        .ok_or(HopcroftKarpError::MetricOverflow)?,
                ));
                search_order.push(model.left[next_left]);
                queue.push_back(next_left);
            }
        }
    }
    Ok(LayerView {
        left_distance,
        node_labels,
        search_order,
        shortest_units,
    })
}

struct LayeredDfs<'search> {
    model: &'search BipartiteMatchingGraph,
    shortest_units: u32,
    pair_by_left: &'search [Option<usize>],
    pair_by_right: &'search [Option<usize>],
    distance: &'search mut [Option<u32>],
    current: &'search mut [usize],
    metrics: &'search mut HopcroftKarpMetrics,
}

fn find_layered_path(
    search: &mut LayeredDfs<'_>,
    root: usize,
) -> Result<Option<Vec<usize>>, HopcroftKarpError> {
    if search.distance.get(root).and_then(|value| *value).is_none() {
        return Ok(None);
    }
    let mut left_stack = vec![root];
    let mut unmatched_path = Vec::new();
    loop {
        let left = *left_stack.last().ok_or(HopcroftKarpError::Invariant)?;
        let left_distance = search.distance[left].ok_or(HopcroftKarpError::Invariant)?;
        let mut advanced = false;
        while search.current[left] < search.model.adjacency[left].len() {
            let ordinal = search.model.adjacency[left][search.current[left]];
            search.current[left] = search.current[left]
                .checked_add(1)
                .ok_or(HopcroftKarpError::MetricOverflow)?;
            increment_scans(search.metrics)?;
            if search.pair_by_left[left] == Some(ordinal) {
                continue;
            }
            let edge = search
                .model
                .compatibility_edges
                .get(ordinal)
                .ok_or(HopcroftKarpError::Invariant)?;
            if let Some(matched) = search.pair_by_right[edge.right] {
                let next_left = search
                    .model
                    .compatibility_edges
                    .get(matched)
                    .ok_or(HopcroftKarpError::Invariant)?
                    .left;
                if search.distance[next_left] == left_distance.checked_add(1) {
                    unmatched_path.push(ordinal);
                    left_stack.push(next_left);
                    advanced = true;
                    break;
                }
                continue;
            }
            if left_distance.checked_add(1) == Some(search.shortest_units) {
                unmatched_path.push(ordinal);
                return Ok(Some(unmatched_path));
            }
        }
        if advanced {
            continue;
        }
        search.distance[left] = None;
        left_stack.pop();
        if left_stack.is_empty() {
            return Ok(None);
        }
        unmatched_path.pop().ok_or(HopcroftKarpError::Invariant)?;
    }
}

fn residual_path(
    model: &BipartiteMatchingGraph,
    pair_by_right: &[Option<usize>],
    unmatched_path: &[usize],
    graph: &FlowNetwork,
) -> Result<Vec<ResidualArcId>, HopcroftKarpError> {
    let first = *unmatched_path.first().ok_or(HopcroftKarpError::Invariant)?;
    let first_edge = model
        .compatibility_edges
        .get(first)
        .ok_or(HopcroftKarpError::Invariant)?;
    let mut path = Vec::with_capacity(unmatched_path.len().saturating_mul(2).saturating_add(2));
    if let Some(source_edge) = model.source_edges.get(first_edge.left) {
        path.push(residual_id(
            graph,
            *source_edge,
            ResidualDirection::Forward,
        )?);
    }
    for (position, &ordinal) in unmatched_path.iter().enumerate() {
        let edge = model
            .compatibility_edges
            .get(ordinal)
            .ok_or(HopcroftKarpError::Invariant)?;
        path.push(residual_id(graph, edge.edge, ResidualDirection::Forward)?);
        if position + 1 < unmatched_path.len() {
            let matched = pair_by_right[edge.right].ok_or(HopcroftKarpError::Invariant)?;
            let matched = model
                .compatibility_edges
                .get(matched)
                .ok_or(HopcroftKarpError::Invariant)?;
            path.push(residual_id(
                graph,
                matched.edge,
                ResidualDirection::Reverse,
            )?);
        } else if let Some(sink_edge) = model.sink_edges.get(edge.right) {
            path.push(residual_id(graph, *sink_edge, ResidualDirection::Forward)?);
        }
    }
    Ok(path)
}

fn apply_matching_path(
    model: &BipartiteMatchingGraph,
    unmatched_path: &[usize],
    pair_by_left: &mut [Option<usize>],
    pair_by_right: &mut [Option<usize>],
) -> Result<(), HopcroftKarpError> {
    for &ordinal in unmatched_path {
        let edge = model
            .compatibility_edges
            .get(ordinal)
            .ok_or(HopcroftKarpError::Invariant)?;
        pair_by_left[edge.left] = Some(ordinal);
        pair_by_right[edge.right] = Some(ordinal);
    }
    Ok(())
}

fn residual_id(
    graph: &FlowNetwork,
    edge: crate::model::EdgeIndex,
    direction: ResidualDirection,
) -> Result<ResidualArcId, HopcroftKarpError> {
    graph
        .edge(edge)
        .map(|edge| ResidualArcId::new(edge.id().clone(), direction))
        .ok_or(HopcroftKarpError::Invariant)
}

fn build_certificate_view(
    model: &BipartiteMatchingGraph,
    pair_by_left: &[Option<usize>],
    pair_by_right: &[Option<usize>],
) -> Result<LayerView, HopcroftKarpError> {
    let mut metrics = HopcroftKarpMetrics::default();
    build_layers(model, pair_by_left, pair_by_right, &mut metrics)
}

fn start_trace<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: HopcroftKarpMetrics,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !enabled {
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

#[expect(
    clippy::too_many_arguments,
    reason = "trace projection keeps all state explicit"
)]
fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: HopcroftKarpMetrics,
    metadata: FlowTraceEventMetadata,
    view: &LayerView,
    path: Vec<ResidualArcId>,
    detail: Option<i128>,
    transitions: &mut u64,
) -> Result<(), HopcroftKarpError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    *transitions = transitions
        .checked_add(1)
        .ok_or(HopcroftKarpError::MetricOverflow)?;
    if *transitions > HOPCROFT_KARP_MAX_STATE_TRANSITIONS {
        return Err(HopcroftKarpError::WorkLimit);
    }
    let exact_focus = (metadata.catalog_id == "hopcroft-karp.extend-alternating-path")
        .then(|| path.last().cloned())
        .flatten()
        .map(|arc| vec![FlowTraceEntityRef::ResidualArc(arc)]);
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        padded_labels(graph, &view.node_labels),
        view.search_order.clone(),
        path,
        Vec::new(),
        trace_metrics(metrics),
    );
    let detail = detail.map(|value| {
        let label = match metadata.catalog_id {
            "hopcroft-karp.level-bfs" => "shortest-length",
            "hopcroft-karp.extend-alternating-path" => "path-prefix-length",
            "hopcroft-karp.augment" | "hopcroft-karp.optimal" => "cardinality",
            _ => "phase",
        };
        (label, value)
    });
    if let Some(focus) = exact_focus {
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
    }
    Ok(())
}

fn padded_labels(graph: &FlowNetwork, labels: &[Option<i128>]) -> Vec<Option<i128>> {
    let mut result = vec![None; graph.nodes().len()];
    for (target, source) in result.iter_mut().zip(labels) {
        *target = *source;
    }
    result
}

const fn trace_metrics(metrics: HopcroftKarpMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.bfs_runs as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.edge_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.dfs_roots as u128,
        scaling_phases: 0,
        blocking_flow_phases: metrics.phases as u128,
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

fn increment_scans(metrics: &mut HopcroftKarpMetrics) -> Result<(), HopcroftKarpError> {
    metrics.edge_scans = metrics
        .edge_scans
        .checked_add(1)
        .ok_or(HopcroftKarpError::MetricOverflow)?;
    if metrics.edge_scans > HOPCROFT_KARP_MAX_EDGE_SCANS {
        return Err(HopcroftKarpError::WorkLimit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_dinic;
    use crate::certificate::check_bipartite_matching;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn adapter_graph(
        left_count: usize,
        right_count: usize,
        compatibility: &[(usize, usize)],
    ) -> (FlowNetwork, Vec<String>, Vec<String>) {
        let left = (0..left_count)
            .map(|index| format!("l{index:02}"))
            .collect::<Vec<_>>();
        let right = (0..right_count)
            .map(|index| format!("r{index:02}"))
            .collect::<Vec<_>>();
        let mut nodes = vec!["s".to_owned(), "t".to_owned()];
        nodes.extend(left.iter().cloned());
        nodes.extend(right.iter().cloned());
        let mut edges = Vec::new();
        for (index, id) in left.iter().enumerate() {
            edges.push(unit_edge(&format!("a-source-{index:02}"), "s", id));
        }
        for &(left_index, right_index) in compatibility {
            edges.push(unit_edge(
                &format!("b-match-{left_index:02}-{right_index:02}"),
                &left[left_index],
                &right[right_index],
            ));
        }
        for (index, id) in right.iter().enumerate() {
            edges.push(unit_edge(&format!("c-sink-{index:02}"), id, "t"));
        }
        let graph = FlowNetwork::new(
            nodes
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(&id).expect("node id"), 0))
                .collect(),
            edges,
        )
        .expect("adapter graph");
        (graph, left, right)
    }

    fn unit_edge(id: &str, from: &str, to: &str) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower: 0,
            capacity: 1,
            cost: 0,
        }
    }

    #[test]
    fn shortest_disjoint_phases_grow_from_length_one_to_three() {
        let (graph, left, right) = adapter_graph(2, 2, &[(0, 0), (0, 1), (1, 0)]);
        let result =
            solve_hopcroft_karp(&graph, &left, &right, Some(("s", "t"))).expect("maximum matching");

        assert_eq!(result.certificate.cardinality, 2);
        assert_eq!(result.certificate.cover_left.len(), 2);
        assert!(result.certificate.cover_right.is_empty());
        assert_eq!(result.metrics.phases, 2);
        assert_eq!(result.metrics.bfs_runs, 3);
        assert_eq!(result.metrics.augmentations, 2);

        let traced =
            trace_hopcroft_karp(&graph, &left, &right, Some(("s", "t"))).expect("matching trace");
        let lengths = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "hopcroft-karp.level-bfs")
            .filter_map(|event| event.detail.as_ref().map(|detail| detail.value))
            .collect::<Vec<_>>();
        assert_eq!(lengths, [1, 3]);
    }

    #[test]
    fn fast_and_trace_replay_are_identical_in_both_directions() {
        let (graph, left, right) =
            adapter_graph(4, 4, &[(0, 0), (0, 1), (1, 0), (2, 2), (2, 3), (3, 2)]);
        let fast =
            solve_hopcroft_karp(&graph, &left, &right, Some(("s", "t"))).expect("fast matching");
        let traced =
            trace_hopcroft_karp(&graph, &left, &right, Some(("s", "t"))).expect("matching trace");
        assert_eq!(traced.result, fast);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "hopcroft-karp.phase-complete")
        );

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn all_three_by_three_graphs_match_independent_unit_flow_certificates() {
        let candidates = (0..3)
            .flat_map(|left| (0..3).map(move |right| (left, right)))
            .collect::<Vec<_>>();
        for mask in 0_u16..(1_u16 << candidates.len()) {
            let compatibility = candidates
                .iter()
                .enumerate()
                .filter_map(|(bit, &edge)| ((mask >> bit) & 1 == 1).then_some(edge))
                .collect::<Vec<_>>();
            let (graph, left, right) = adapter_graph(3, 3, &compatibility);
            let actual = solve_hopcroft_karp(&graph, &left, &right, Some(("s", "t")))
                .expect("matching result");
            let source = graph
                .node_index(&NodeId::parse("s").expect("source id"))
                .expect("source");
            let sink = graph
                .node_index(&NodeId::parse("t").expect("sink id"))
                .expect("sink");
            let reference = solve_dinic(&graph, source, sink).expect("unit-flow maximum");
            assert_eq!(
                i128::from(actual.certificate.cardinality),
                reference.certificate.value,
                "compatibility mask {mask}"
            );
            assert_eq!(actual.flows, reference.flows, "stable mask {mask}");
        }
    }

    #[test]
    fn independent_checker_rejects_repeated_left_endpoint() {
        let (graph, left, right) = adapter_graph(2, 2, &[(0, 0), (0, 1), (1, 1)]);
        let model = BipartiteMatchingGraph::new(&graph, &left, &right, Some(("s", "t")))
            .expect("matching model");
        let mut flows = vec![0; graph.edges().len()];
        for compatibility in model.compatibility_edges.iter().take(2) {
            flows[compatibility.edge.as_usize()] = 1;
        }
        assert_eq!(
            check_bipartite_matching(&graph, &model, &flows),
            Err(CertificateError::InvalidMatching)
        );
    }

    #[test]
    fn native_graph_without_auxiliary_terminals_is_supported() {
        let nodes = ["l0", "l1", "r0", "r1"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let graph = FlowNetwork::new(
            nodes,
            vec![
                unit_edge("e0", "l0", "r0"),
                unit_edge("e1", "l0", "r1"),
                unit_edge("e2", "l1", "r1"),
            ],
        )
        .expect("native graph");
        let left = vec!["l0".to_owned(), "l1".to_owned()];
        let right = vec!["r0".to_owned(), "r1".to_owned()];
        let result = solve_hopcroft_karp(&graph, &left, &right, None).expect("native matching");
        assert_eq!(result.certificate.cardinality, 2);
        assert_eq!(result.flows.iter().sum::<u64>(), 2);
    }
}
