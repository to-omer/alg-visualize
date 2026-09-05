//! Ahuja--Orlin distance-directed maximum flow with an exact shortest-path tree.
//!
//! This is the paper's DD2 variant.  Every augmentation follows the unique
//! source-to-sink path in a sink-rooted shortest-path in-tree.  Saturated or
//! label-invalidated tree arcs are repaired incrementally with stable current
//! arcs.  The scaling preset rebuilds the exact tree for each integral residual
//! capacity threshold and otherwise uses the same update-tree kernel.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative node limit for observable shortest-path-tree repair.
pub const DISTANCE_DIRECTED_MAX_NODES: usize = 256;
/// Conservative edge limit for observable shortest-path-tree repair.
pub const DISTANCE_DIRECTED_MAX_EDGES: usize = 2_048;
/// Hard ceiling for positive eligible residual-arc inspections.
pub const DISTANCE_DIRECTED_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;
/// Hard ceiling for augment, invalidation, replacement, and relabel transitions.
pub const DISTANCE_DIRECTED_MAX_STATE_TRANSITIONS: u64 = 250_000;
/// Hard ceiling for eager semantic trace events.
pub const DISTANCE_DIRECTED_MAX_TRACE_EVENTS: usize = 100_000;

/// Exact counters from the DD2 shortest-path-tree kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DistanceDirectedMetrics {
    /// Reverse breadth-first constructions of an exact eligible tree.
    pub reverse_bfs_runs: u64,
    /// Capacity thresholds entered by the scaling preset; zero for DD2.
    pub scaling_phases: u64,
    /// Positive threshold-eligible residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Successful source-to-sink tree-path augmentations.
    pub augmentations: u64,
    /// Completed update-tree repairs after augmentation.
    pub tree_repairs: u64,
    /// Tree arcs processed after saturation or an ancestor relabel.
    pub invalid_tree_arcs: u64,
    /// Invalid tree arcs replaced without relabeling their tail.
    pub tree_arc_replacements: u64,
    /// Strict exact-distance label increases.
    pub relabels: u64,
    /// Nodes proved unable to reach the sink at the current threshold.
    pub node_deletions: u64,
    /// Tree-path arcs saturated by augmentation.
    pub saturated_tree_arcs: u64,
    /// Child tree arcs invalidated by a parent-node relabel.
    pub cascading_invalidations: u64,
    /// Rejected current-arc candidates.
    pub current_arc_advances: u128,
    /// Transitions charged to the deterministic work ceiling.
    pub state_transitions: u64,
}

/// Certified exact maximum flow produced by DD2 or its scaling preset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistanceDirectedResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact shortest-path-tree counters.
    pub metrics: DistanceDirectedMetrics,
}

/// DD2 result with reversible exact-tree and update-tree boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistanceDirectedTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: DistanceDirectedResult,
    /// Boundary before the first exact tree is constructed.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete semantic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Independently certified optimal boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// DD2 construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DistanceDirectedError {
    /// Input exceeds the bounded interactive implementation band.
    #[error("graph exceeds distance-directed admission limits")]
    AdmissionLimit,
    /// DD2 is exposed for the paper's zero-feasible two-terminal domain.
    #[error("distance-directed DD2 requires zero lower bounds and distinct terminals")]
    GraphRequirement,
    /// Residual mutation or reconstruction failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final solver-independent certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Checked arithmetic or an exact counter exceeded its domain.
    #[error("distance-directed exact arithmetic overflow")]
    ArithmeticOverflow,
    /// A deterministic scan, transition, or trace ceiling was reached.
    #[error("distance-directed deterministic work limit reached")]
    WorkLimit,
    /// Exact labels, tree parents, current arcs, or repair queue disagreed.
    #[error("distance-directed shortest-path-tree invariant failed")]
    Invariant,
}

/// Solves exact maximum flow with Ahuja--Orlin DD2.
///
/// # Errors
///
/// Rejects nonzero lower bounds, identical terminals, out-of-band input,
/// deterministic work exhaustion, invariant failure, or failed certification.
pub fn solve_distance_directed_dd2(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DistanceDirectedResult, DistanceDirectedError> {
    solve_internal(
        graph,
        source,
        sink,
        DistanceDirectedPreset::ExactTree,
        false,
    )
    .map(|run| run.result)
}

/// Traces DD2 exact-tree initialization, augmentation, and incremental repair.
///
/// # Errors
///
/// Returns the same failures as [`solve_distance_directed_dd2`] plus trace
/// construction failures.
pub fn trace_distance_directed_dd2(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DistanceDirectedTraceResult, DistanceDirectedError> {
    trace_internal(graph, source, sink, DistanceDirectedPreset::ExactTree)
}

/// Solves exact integral maximum flow with capacity-scaled DD2 phases.
///
/// # Errors
///
/// Returns the same bounded source-domain failures as
/// [`solve_distance_directed_dd2`].
pub fn solve_distance_directed_scaling(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DistanceDirectedResult, DistanceDirectedError> {
    solve_internal(
        graph,
        source,
        sink,
        DistanceDirectedPreset::CapacityScaling,
        false,
    )
    .map(|run| run.result)
}

/// Traces every capacity threshold and its incremental DD2 tree repairs.
///
/// # Errors
///
/// Returns the same failures as [`solve_distance_directed_scaling`] plus trace
/// construction failures.
pub fn trace_distance_directed_scaling(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DistanceDirectedTraceResult, DistanceDirectedError> {
    trace_internal(graph, source, sink, DistanceDirectedPreset::CapacityScaling)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DistanceDirectedPreset {
    ExactTree,
    CapacityScaling,
}

impl DistanceDirectedPreset {
    const fn is_scaling(self) -> bool {
        matches!(self, Self::CapacityScaling)
    }
}

struct DistanceDirectedInternalRun {
    result: DistanceDirectedResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct ExactTreeKernel {
    labels: Vec<usize>,
    parents: Vec<Option<ResidualArcId>>,
    current_arcs: Vec<usize>,
    outgoing_ids: Vec<Vec<ResidualArcId>>,
    unreachable: usize,
    threshold: u64,
}

struct TraceTransition {
    suffix: &'static str,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    detail: Option<(&'static str, i128)>,
}

fn trace_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DistanceDirectedPreset,
) -> Result<DistanceDirectedTraceResult, DistanceDirectedError> {
    let run = solve_internal(graph, source, sink, preset, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(DistanceDirectedError::Invariant)?;
    Ok(DistanceDirectedTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DistanceDirectedPreset,
    record_trace: bool,
) -> Result<DistanceDirectedInternalRun, DistanceDirectedError> {
    validate_graph(graph, source, sink)?;
    let zero_flows = vec![0; graph.edges().len()];
    let mut state = ResidualState::from_flows(graph, &zero_flows)?;
    let mut metrics = DistanceDirectedMetrics::default();
    let base_snapshot = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        trace_metrics(metrics),
    );
    let mut recorder = if record_trace {
        Some(FlowTraceRecorder::new(graph, base_snapshot)?)
    } else {
        None
    };

    run_all_thresholds(
        graph,
        &mut state,
        source,
        sink,
        preset,
        &mut metrics,
        recorder.as_mut(),
    )?;

    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    let terminal_kernel = ExactTreeKernel::initialize_without_count(&state, sink, 1)?;
    record_event(
        recorder.as_mut(),
        graph,
        &state,
        preset,
        &terminal_kernel,
        metrics,
        TraceTransition {
            suffix: "optimal",
            search_order: Vec::new(),
            active_path: Vec::new(),
            detail: None,
        },
    )?;
    let result = DistanceDirectedResult {
        flows,
        certificate,
        metrics,
    };
    Ok(DistanceDirectedInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_all_thresholds(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DistanceDirectedPreset,
    metrics: &mut DistanceDirectedMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), DistanceDirectedError> {
    for threshold in thresholds(graph, preset) {
        if preset.is_scaling() {
            metrics.scaling_phases = metrics
                .scaling_phases
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        }
        let (mut kernel, search_order) = ExactTreeKernel::initialize(
            state,
            sink,
            threshold,
            preset,
            metrics,
            recorder.as_deref_mut(),
        )?;
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            preset,
            &kernel,
            *metrics,
            TraceTransition {
                suffix: if preset.is_scaling() {
                    "start-scaling-phase"
                } else {
                    "reverse-bfs"
                },
                search_order,
                active_path: Vec::new(),
                detail: preset
                    .is_scaling()
                    .then_some(("threshold", i128::from(threshold))),
            },
        )?;

        run_threshold_phase(
            graph,
            state,
            source,
            sink,
            preset,
            &mut kernel,
            metrics,
            recorder.as_deref_mut(),
        )?;

        if preset.is_scaling() {
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                preset,
                &kernel,
                *metrics,
                TraceTransition {
                    suffix: "complete-scaling-phase",
                    // The phase is certified precisely because the source was
                    // deleted from the threshold residual tree. Keep that
                    // single certificate vertex visible; replaying the same
                    // empty search projection as `tree-repaired` made two
                    // adjacent source events graph-identical.
                    search_order: vec![source],
                    active_path: Vec::new(),
                    detail: Some(("threshold", i128::from(threshold))),
                },
            )?;
        }
    }
    Ok(())
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), DistanceDirectedError> {
    if graph.nodes().len() > DISTANCE_DIRECTED_MAX_NODES
        || graph.edges().len() > DISTANCE_DIRECTED_MAX_EDGES
    {
        return Err(DistanceDirectedError::AdmissionLimit);
    }
    if source == sink || graph.edges().iter().any(|edge| edge.lower() != 0) {
        return Err(DistanceDirectedError::GraphRequirement);
    }
    if graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(DistanceDirectedError::GraphRequirement);
    }
    Ok(())
}

fn thresholds(graph: &FlowNetwork, preset: DistanceDirectedPreset) -> Vec<u64> {
    if !preset.is_scaling() {
        return vec![1];
    }
    let maximum = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::capacity)
        .max()
        .unwrap_or(0);
    if maximum == 0 {
        return Vec::new();
    }
    let mut threshold = 1_u64 << (u64::BITS - 1 - maximum.leading_zeros());
    let mut result = Vec::new();
    loop {
        result.push(threshold);
        if threshold == 1 {
            break;
        }
        threshold /= 2;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_threshold_phase(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    preset: DistanceDirectedPreset,
    kernel: &mut ExactTreeKernel,
    metrics: &mut DistanceDirectedMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), DistanceDirectedError> {
    while kernel.labels[source.as_usize()] < kernel.unreachable {
        let path = kernel.tree_path(source, sink, state)?;
        count_transition(metrics)?;
        let bottleneck = path
            .iter()
            .map(|id| {
                state
                    .arc(id)
                    .map(|arc| arc.capacity)
                    .ok_or(DistanceDirectedError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(DistanceDirectedError::Invariant)?;
        if bottleneck < kernel.threshold {
            return Err(DistanceDirectedError::Invariant);
        }
        state.augment(&path, bottleneck)?;
        metrics.augmentations = metrics
            .augmentations
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;

        let mut invalid_nodes = BTreeSet::new();
        for id in &path {
            let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.capacity < kernel.threshold {
                invalid_nodes.insert(arc.from.as_usize());
                metrics.saturated_tree_arcs = metrics
                    .saturated_tree_arcs
                    .checked_add(1)
                    .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
            }
        }
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            preset,
            kernel,
            *metrics,
            TraceTransition {
                suffix: "augment",
                search_order: tree_path_nodes(state, source, &path)?,
                active_path: path,
                detail: Some(("bottleneck", i128::from(bottleneck))),
            },
        )?;

        repair_tree(
            graph,
            state,
            preset,
            kernel,
            metrics,
            &mut invalid_nodes,
            recorder.as_deref_mut(),
        )?;
        metrics.tree_repairs = metrics
            .tree_repairs
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        verify_exact_tree(state, sink, kernel)?;
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            preset,
            kernel,
            *metrics,
            TraceTransition {
                suffix: "tree-repaired",
                search_order: Vec::new(),
                active_path: Vec::new(),
                detail: None,
            },
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn repair_tree(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    preset: DistanceDirectedPreset,
    kernel: &mut ExactTreeKernel,
    metrics: &mut DistanceDirectedMetrics,
    invalid_nodes: &mut BTreeSet<usize>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), DistanceDirectedError> {
    while let Some(index) = invalid_nodes.pop_first() {
        count_transition(metrics)?;
        let node = NodeIndex::try_from_usize(index).ok_or(DistanceDirectedError::Invariant)?;
        let old_parent = kernel.parents[index]
            .take()
            .ok_or(DistanceDirectedError::Invariant)?;
        metrics.invalid_tree_arcs = metrics
            .invalid_tree_arcs
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;

        if let Some((position, arc)) =
            kernel.next_admissible(state, node, preset, metrics, recorder.as_deref_mut())?
        {
            kernel.current_arcs[index] = position;
            kernel.parents[index] = Some(arc.id.clone());
            metrics.tree_arc_replacements = metrics
                .tree_arc_replacements
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                preset,
                kernel,
                *metrics,
                TraceTransition {
                    suffix: "replace-tree-arc",
                    search_order: vec![node, arc.to],
                    active_path: vec![old_parent, arc.id],
                    detail: None,
                },
            )?;
            continue;
        }

        let old_label = kernel.labels[index];
        let new_label = kernel.relabel(state, node, preset, metrics, recorder.as_deref_mut())?;
        if new_label <= old_label {
            return Err(DistanceDirectedError::Invariant);
        }
        metrics.relabels = metrics
            .relabels
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;

        let children = kernel.children(state, node)?;
        for child in &children {
            if invalid_nodes.insert(child.as_usize()) {
                metrics.cascading_invalidations = metrics
                    .cascading_invalidations
                    .checked_add(1)
                    .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
            }
        }
        let deleted = new_label >= kernel.unreachable;
        if deleted {
            metrics.node_deletions = metrics
                .node_deletions
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        }
        let active_path = kernel.parents[index].iter().cloned().collect();
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            preset,
            kernel,
            *metrics,
            TraceTransition {
                suffix: if deleted {
                    "delete-node"
                } else {
                    "relabel-node"
                },
                search_order: std::iter::once(node).chain(children).collect(),
                active_path,
                detail: Some((
                    if deleted {
                        "unreachable-label"
                    } else {
                        "distance"
                    },
                    i128::try_from(new_label)
                        .map_err(|_| DistanceDirectedError::ArithmeticOverflow)?,
                )),
            },
        )?;
    }
    Ok(())
}

impl ExactTreeKernel {
    fn initialize(
        state: &ResidualState<'_>,
        sink: NodeIndex,
        threshold: u64,
        preset: DistanceDirectedPreset,
        metrics: &mut DistanceDirectedMetrics,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(Self, Vec<NodeIndex>), DistanceDirectedError> {
        metrics.reverse_bfs_runs = metrics
            .reverse_bfs_runs
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        Self::initialize_impl(state, sink, threshold, preset, Some(metrics), recorder)
    }

    fn initialize_without_count(
        state: &ResidualState<'_>,
        sink: NodeIndex,
        threshold: u64,
    ) -> Result<Self, DistanceDirectedError> {
        Self::initialize_impl(
            state,
            sink,
            threshold,
            DistanceDirectedPreset::ExactTree,
            None,
            None,
        )
        .map(|(kernel, _)| kernel)
    }

    fn initialize_impl(
        state: &ResidualState<'_>,
        sink: NodeIndex,
        threshold: u64,
        preset: DistanceDirectedPreset,
        mut metrics: Option<&mut DistanceDirectedMetrics>,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(Self, Vec<NodeIndex>), DistanceDirectedError> {
        if threshold == 0 {
            return Err(DistanceDirectedError::Invariant);
        }
        let graph = state.graph();
        let node_count = graph.nodes().len();
        let unreachable = node_count
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        let outgoing_ids = graph
            .node_indices()
            .map(|node| stable_outgoing_ids(graph, node))
            .collect::<Vec<_>>();
        let mut incoming = vec![Vec::<ResidualArcId>::new(); node_count];
        for ids in &outgoing_ids {
            for id in ids {
                let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
                if arc.capacity >= threshold {
                    incoming[arc.to.as_usize()].push(id.clone());
                }
            }
        }
        for ids in &mut incoming {
            ids.sort_unstable();
        }

        let mut kernel = Self {
            labels: vec![unreachable; node_count],
            parents: vec![None; node_count],
            current_arcs: vec![0; node_count],
            outgoing_ids,
            unreachable,
            threshold,
        };
        kernel.labels[sink.as_usize()] = 0;
        let mut queue = std::collections::VecDeque::from([sink]);
        let mut search_order = vec![sink];
        while let Some(node) = queue.pop_front() {
            let next = kernel.labels[node.as_usize()]
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
            for id in &incoming[node.as_usize()] {
                let mut metrics_snapshot = None;
                if let Some(metrics) = metrics.as_deref_mut() {
                    count_scan(metrics)?;
                    metrics_snapshot = Some(*metrics);
                }
                let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
                if kernel.labels[arc.from.as_usize()] == unreachable {
                    kernel.labels[arc.from.as_usize()] = next;
                    queue.push_back(arc.from);
                    search_order.push(arc.from);
                }
                if let Some(metrics_snapshot) = metrics_snapshot {
                    record_event(
                        recorder.as_deref_mut(),
                        graph,
                        state,
                        preset,
                        &kernel,
                        metrics_snapshot,
                        TraceTransition {
                            suffix: "inspect-residual-arc",
                            search_order: vec![arc.from, arc.to],
                            active_path: vec![id.clone()],
                            detail: Some(("threshold", i128::from(threshold))),
                        },
                    )?;
                }
            }
        }

        for node in graph.node_indices() {
            let index = node.as_usize();
            if node == sink || kernel.labels[index] >= unreachable {
                continue;
            }
            let expected = kernel.labels[index]
                .checked_sub(1)
                .ok_or(DistanceDirectedError::Invariant)?;
            let (position, id) = kernel.outgoing_ids[index]
                .iter()
                .enumerate()
                .find_map(|(position, id)| {
                    let arc = state.arc(id)?;
                    (arc.capacity >= threshold && kernel.labels[arc.to.as_usize()] == expected)
                        .then_some((position, id.clone()))
                })
                .ok_or(DistanceDirectedError::Invariant)?;
            kernel.parents[index] = Some(id);
            kernel.current_arcs[index] = position;
        }
        verify_exact_tree(state, sink, &kernel)?;
        Ok((kernel, search_order))
    }

    fn tree_path(
        &self,
        source: NodeIndex,
        sink: NodeIndex,
        state: &ResidualState<'_>,
    ) -> Result<Vec<ResidualArcId>, DistanceDirectedError> {
        let mut node = source;
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        while node != sink {
            if !visited.insert(node.as_usize()) || path.len() >= self.labels.len() {
                return Err(DistanceDirectedError::Invariant);
            }
            let id = self.parents[node.as_usize()]
                .clone()
                .ok_or(DistanceDirectedError::Invariant)?;
            let arc = state.arc(&id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.from != node || arc.capacity < self.threshold {
                return Err(DistanceDirectedError::Invariant);
            }
            path.push(id);
            node = arc.to;
        }
        Ok(path)
    }

    fn next_admissible(
        &self,
        state: &ResidualState<'_>,
        node: NodeIndex,
        preset: DistanceDirectedPreset,
        metrics: &mut DistanceDirectedMetrics,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<Option<(usize, ResidualArc)>, DistanceDirectedError> {
        let index = node.as_usize();
        let start = self.current_arcs[index];
        for (position, id) in self.outgoing_ids[index].iter().enumerate().skip(start) {
            let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.capacity >= self.threshold {
                count_scan(metrics)?;
                record_event(
                    recorder.as_deref_mut(),
                    state.graph(),
                    state,
                    preset,
                    self,
                    *metrics,
                    TraceTransition {
                        suffix: "inspect-residual-arc",
                        search_order: vec![arc.from, arc.to],
                        active_path: vec![id.clone()],
                        detail: Some(("threshold", i128::from(self.threshold))),
                    },
                )?;
                if self.labels[index] == self.labels[arc.to.as_usize()].saturating_add(1) {
                    return Ok(Some((position, arc)));
                }
            }
            metrics.current_arc_advances = metrics
                .current_arc_advances
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        }
        Ok(None)
    }

    fn relabel(
        &mut self,
        state: &ResidualState<'_>,
        node: NodeIndex,
        preset: DistanceDirectedPreset,
        metrics: &mut DistanceDirectedMetrics,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<usize, DistanceDirectedError> {
        let index = node.as_usize();
        let mut minimum = self.unreachable;
        for id in &self.outgoing_ids[index] {
            let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.capacity >= self.threshold && arc.to != node {
                count_scan(metrics)?;
                minimum = minimum.min(self.labels[arc.to.as_usize()]);
                record_event(
                    recorder.as_deref_mut(),
                    state.graph(),
                    state,
                    preset,
                    self,
                    *metrics,
                    TraceTransition {
                        suffix: "inspect-residual-arc",
                        search_order: vec![arc.from, arc.to],
                        active_path: vec![id.clone()],
                        detail: Some(("threshold", i128::from(self.threshold))),
                    },
                )?;
            }
        }
        let new_label = if minimum >= self.unreachable {
            self.unreachable
        } else {
            minimum
                .checked_add(1)
                .ok_or(DistanceDirectedError::ArithmeticOverflow)?
                .min(self.unreachable)
        };
        self.labels[index] = new_label;
        self.current_arcs[index] = 0;
        self.parents[index] = None;
        if new_label < self.unreachable {
            let expected = new_label
                .checked_sub(1)
                .ok_or(DistanceDirectedError::Invariant)?;
            let (position, id) = self.outgoing_ids[index]
                .iter()
                .enumerate()
                .find_map(|(position, id)| {
                    let arc = state.arc(id)?;
                    (arc.capacity >= self.threshold && self.labels[arc.to.as_usize()] == expected)
                        .then_some((position, id.clone()))
                })
                .ok_or(DistanceDirectedError::Invariant)?;
            self.current_arcs[index] = position;
            self.parents[index] = Some(id);
        }
        Ok(new_label)
    }

    fn children(
        &self,
        state: &ResidualState<'_>,
        parent: NodeIndex,
    ) -> Result<Vec<NodeIndex>, DistanceDirectedError> {
        let mut children = Vec::new();
        for (index, id) in self.parents.iter().enumerate() {
            let Some(id) = id else {
                continue;
            };
            let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.to == parent {
                children.push(
                    NodeIndex::try_from_usize(index).ok_or(DistanceDirectedError::Invariant)?,
                );
            }
        }
        Ok(children)
    }

    fn forest_arcs(&self) -> Vec<ResidualArcId> {
        self.parents.iter().flatten().cloned().collect()
    }
}

fn verify_exact_tree(
    state: &ResidualState<'_>,
    sink: NodeIndex,
    kernel: &ExactTreeKernel,
) -> Result<(), DistanceDirectedError> {
    let node_count = state.graph().nodes().len();
    if kernel.labels.len() != node_count
        || kernel.parents.len() != node_count
        || kernel.current_arcs.len() != node_count
        || kernel.labels[sink.as_usize()] != 0
        || kernel.parents[sink.as_usize()].is_some()
    {
        return Err(DistanceDirectedError::Invariant);
    }
    let (expected, _) = exact_labels(state, sink, kernel.threshold, None)?;
    if expected != kernel.labels {
        return Err(DistanceDirectedError::Invariant);
    }
    for node in state.graph().node_indices() {
        let index = node.as_usize();
        if node == sink || kernel.labels[index] >= kernel.unreachable {
            if kernel.parents[index].is_some() {
                return Err(DistanceDirectedError::Invariant);
            }
            continue;
        }
        let id = kernel.parents[index]
            .as_ref()
            .ok_or(DistanceDirectedError::Invariant)?;
        let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
        if arc.from != node
            || arc.capacity < kernel.threshold
            || kernel.labels[index] != kernel.labels[arc.to.as_usize()].saturating_add(1)
        {
            return Err(DistanceDirectedError::Invariant);
        }
    }
    Ok(())
}

fn exact_labels(
    state: &ResidualState<'_>,
    sink: NodeIndex,
    threshold: u64,
    mut metrics: Option<&mut DistanceDirectedMetrics>,
) -> Result<(Vec<usize>, Vec<NodeIndex>), DistanceDirectedError> {
    let node_count = state.graph().nodes().len();
    let unreachable = node_count
        .checked_add(1)
        .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
    let mut incoming = vec![Vec::<ResidualArcId>::new(); node_count];
    for node in state.graph().node_indices() {
        for id in stable_outgoing_ids(state.graph(), node) {
            let arc = state.arc(&id).ok_or(DistanceDirectedError::Invariant)?;
            if arc.capacity >= threshold {
                incoming[arc.to.as_usize()].push(id);
            }
        }
    }
    for ids in &mut incoming {
        ids.sort_unstable();
    }
    let mut labels = vec![unreachable; node_count];
    labels[sink.as_usize()] = 0;
    let mut queue = std::collections::VecDeque::from([sink]);
    let mut order = vec![sink];
    while let Some(node) = queue.pop_front() {
        let next = labels[node.as_usize()]
            .checked_add(1)
            .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
        for id in &incoming[node.as_usize()] {
            if let Some(metrics) = metrics.as_deref_mut() {
                count_scan(metrics)?;
            }
            let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
            if labels[arc.from.as_usize()] == unreachable {
                labels[arc.from.as_usize()] = next;
                queue.push_back(arc.from);
                order.push(arc.from);
            }
        }
    }
    Ok((labels, order))
}

fn stable_outgoing_ids(graph: &FlowNetwork, node: NodeIndex) -> Vec<ResidualArcId> {
    let mut ids =
        Vec::with_capacity(graph.outgoing_edges(node).len() + graph.incoming_edges(node).len());
    ids.extend(graph.outgoing_edges(node).iter().filter_map(|&index| {
        graph
            .edge(index)
            .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward))
    }));
    ids.extend(graph.incoming_edges(node).iter().filter_map(|&index| {
        graph
            .edge(index)
            .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse))
    }));
    ids.sort_unstable();
    ids
}

fn tree_path_nodes(
    state: &ResidualState<'_>,
    source: NodeIndex,
    path: &[ResidualArcId],
) -> Result<Vec<NodeIndex>, DistanceDirectedError> {
    let mut nodes = vec![source];
    let mut current = source;
    for id in path {
        let arc = state.arc(id).ok_or(DistanceDirectedError::Invariant)?;
        if arc.from != current {
            return Err(DistanceDirectedError::Invariant);
        }
        nodes.push(arc.to);
        current = arc.to;
    }
    Ok(nodes)
}

fn count_scan(metrics: &mut DistanceDirectedMetrics) -> Result<(), DistanceDirectedError> {
    if metrics.residual_arc_scans >= DISTANCE_DIRECTED_MAX_RESIDUAL_ARC_SCANS {
        return Err(DistanceDirectedError::WorkLimit);
    }
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
    Ok(())
}

fn count_transition(metrics: &mut DistanceDirectedMetrics) -> Result<(), DistanceDirectedError> {
    if metrics.state_transitions >= DISTANCE_DIRECTED_MAX_STATE_TRANSITIONS {
        return Err(DistanceDirectedError::WorkLimit);
    }
    metrics.state_transitions = metrics
        .state_transitions
        .checked_add(1)
        .ok_or(DistanceDirectedError::ArithmeticOverflow)?;
    Ok(())
}

fn record_event(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    preset: DistanceDirectedPreset,
    kernel: &ExactTreeKernel,
    metrics: DistanceDirectedMetrics,
    transition: TraceTransition,
) -> Result<(), DistanceDirectedError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    if recorder.event_count() >= DISTANCE_DIRECTED_MAX_TRACE_EVENTS {
        return Err(DistanceDirectedError::WorkLimit);
    }
    let TraceTransition {
        suffix,
        search_order,
        active_path,
        detail,
    } = transition;
    let local_focus = match suffix {
        "inspect-residual-arc" | "replace-tree-arc" => Some(
            active_path
                .iter()
                .cloned()
                .map(FlowTraceEntityRef::ResidualArc)
                .collect::<Vec<_>>(),
        ),
        "relabel-node" | "delete-node" | "complete-scaling-phase" => {
            let node = search_order
                .first()
                .copied()
                .ok_or(DistanceDirectedError::Invariant)?;
            Some(vec![FlowTraceEntityRef::Node(
                graph.nodes()[node.as_usize()].id().clone(),
            )])
        }
        _ => None,
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        kernel
            .labels
            .iter()
            .map(|&label| {
                i128::try_from(label)
                    .map(Some)
                    .map_err(|_| DistanceDirectedError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?,
        search_order,
        active_path,
        Vec::new(),
        trace_metrics(metrics),
    )
    .with_forest_overlay(graph, kernel.forest_arcs(), Vec::new());
    let metadata = event_metadata(preset, suffix)?;
    if let Some(local_focus) = local_focus {
        recorder.record_transition_with_detail_and_focus(
            metadata,
            &snapshot,
            detail,
            local_focus,
        )?;
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
    }
    Ok(())
}

fn event_metadata(
    preset: DistanceDirectedPreset,
    suffix: &str,
) -> Result<FlowTraceEventMetadata, DistanceDirectedError> {
    match preset {
        DistanceDirectedPreset::ExactTree => exact_tree_event_metadata(suffix),
        DistanceDirectedPreset::CapacityScaling => scaling_event_metadata(suffix),
    }
}

macro_rules! metadata {
    ($algorithm:literal, $suffix:literal, $granularity:ident, $line:literal) => {
        FlowTraceEventMetadata {
            catalog_id: concat!($algorithm, ".", $suffix),
            minimum_granularity: TraceGranularityV1::$granularity,
            pseudocode_line: concat!($algorithm, ":", $line),
        }
    };
}

fn exact_tree_event_metadata(
    suffix: &str,
) -> Result<FlowTraceEventMetadata, DistanceDirectedError> {
    let result = match suffix {
        "inspect-residual-arc" => metadata!(
            "distance-directed-augmenting-path",
            "inspect-residual-arc",
            Micro,
            "inspect-threshold-eligible-residual-arc"
        ),
        "reverse-bfs" => metadata!(
            "distance-directed-augmenting-path",
            "reverse-bfs",
            Phase,
            "initialize-exact-shortest-path-tree"
        ),
        "augment" => metadata!(
            "distance-directed-augmenting-path",
            "augment",
            Operation,
            "augment-unique-shortest-tree-path"
        ),
        "replace-tree-arc" => metadata!(
            "distance-directed-augmenting-path",
            "replace-tree-arc",
            Operation,
            "replace-invalid-parent-with-current-admissible-arc"
        ),
        "relabel-node" => metadata!(
            "distance-directed-augmenting-path",
            "relabel-node",
            Operation,
            "raise-exact-distance-and-invalidate-children"
        ),
        "delete-node" => metadata!(
            "distance-directed-augmenting-path",
            "delete-node",
            Operation,
            "delete-node-unreachable-at-current-threshold"
        ),
        "tree-repaired" => metadata!(
            "distance-directed-augmenting-path",
            "tree-repaired",
            Phase,
            "complete-incremental-shortest-path-tree-repair"
        ),
        "optimal" => metadata!(
            "distance-directed-augmenting-path",
            "optimal",
            Phase,
            "return-certified-minimum-cut"
        ),
        _ => return Err(DistanceDirectedError::Invariant),
    };
    Ok(result)
}

fn scaling_event_metadata(suffix: &str) -> Result<FlowTraceEventMetadata, DistanceDirectedError> {
    let result = match suffix {
        "inspect-residual-arc" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "inspect-residual-arc",
            Micro,
            "inspect-threshold-eligible-residual-arc"
        ),
        "start-scaling-phase" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "start-scaling-phase",
            Phase,
            "rebuild-exact-tree-on-threshold-residual"
        ),
        "complete-scaling-phase" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "complete-scaling-phase",
            Phase,
            "certify-no-threshold-eligible-augmenting-path"
        ),
        "augment" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "augment",
            Operation,
            "augment-unique-shortest-tree-path"
        ),
        "replace-tree-arc" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "replace-tree-arc",
            Operation,
            "replace-invalid-parent-with-current-admissible-arc"
        ),
        "relabel-node" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "relabel-node",
            Operation,
            "raise-exact-distance-and-invalidate-children"
        ),
        "delete-node" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "delete-node",
            Operation,
            "delete-node-unreachable-at-current-threshold"
        ),
        "tree-repaired" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "tree-repaired",
            Phase,
            "complete-incremental-shortest-path-tree-repair"
        ),
        "optimal" => metadata!(
            "distance-directed-scaling-augmenting-path",
            "optimal",
            Phase,
            "return-certified-minimum-cut"
        ),
        _ => return Err(DistanceDirectedError::Invariant),
    };
    Ok(result)
}

/// Projects DD2-specific counters into the stable 16-slot trace schema.
#[must_use]
pub const fn distance_directed_trace_metrics(metrics: DistanceDirectedMetrics) -> FlowTraceMetrics {
    trace_metrics(metrics)
}

const fn trace_metrics(metrics: DistanceDirectedMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: metrics.current_arc_advances,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.tree_repairs as u128,
        scaling_phases: metrics.scaling_phases as u128,
        blocking_flow_phases: 0,
        relabels: metrics.relabels as u128,
        retreats: metrics.invalid_tree_arcs as u128,
        reverse_bfs_runs: metrics.reverse_bfs_runs as u128,
        gap_terminations: metrics.node_deletions as u128,
        pushes: metrics.tree_arc_replacements as u128,
        saturating_pushes: metrics.saturated_tree_arcs as u128,
        nonsaturating_pushes: metrics.cascading_invalidations as u128,
        discharges: 0,
        active_vertex_selections: metrics.state_transitions as u128,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    fn network(edges: &[(&str, &str, &str, u64, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let mut node_ids = vec!["s", "t"];
        for &(_, from, to, _, _) in edges {
            node_ids.push(from);
            node_ids.push(to);
        }
        node_ids.sort_unstable();
        node_ids.dedup();
        let graph = FlowNetwork::new(
            node_ids
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
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
    fn dd2_repairs_saturated_and_cascading_tree_arcs() {
        let (graph, source, sink) = network(&[
            ("e0", "s", "a", 0, 2),
            ("e1", "a", "b", 0, 1),
            ("e2", "b", "t", 0, 2),
            ("e3", "a", "c", 0, 2),
            ("e4", "c", "d", 0, 2),
            ("e5", "d", "t", 0, 2),
        ]);
        let result = solve_distance_directed_dd2(&graph, source, sink).expect("DD2 maximum");
        assert_eq!(result.certificate.value, 2);
        assert!(result.metrics.tree_repairs >= 2);
        assert!(result.metrics.relabels > 0);
        assert!(result.metrics.invalid_tree_arcs >= result.metrics.saturated_tree_arcs);
    }

    #[test]
    fn scaling_uses_power_of_two_thresholds_and_matches_dd2() {
        let (graph, source, sink) = network(&[
            ("e0", "s", "a", 0, 9),
            ("e1", "a", "t", 0, 9),
            ("e2", "s", "b", 0, 3),
            ("e3", "b", "t", 0, 3),
        ]);
        let exact = solve_distance_directed_dd2(&graph, source, sink).expect("DD2 maximum");
        let scaled =
            solve_distance_directed_scaling(&graph, source, sink).expect("scaled DD2 maximum");
        assert_eq!(scaled.certificate, exact.certificate);
        assert_eq!(scaled.metrics.scaling_phases, 4);
        assert_eq!(scaled.metrics.reverse_bfs_runs, 4);
    }

    #[test]
    fn parallel_opposite_and_self_loop_arcs_are_exact() {
        let (graph, source, sink) = network(&[
            ("e0", "s", "a", 0, 3),
            ("e1", "s", "a", 0, 4),
            ("e2", "a", "s", 0, 2),
            ("e3", "a", "a", 0, 99),
            ("e4", "a", "t", 0, 7),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
        for result in [
            solve_distance_directed_dd2(&graph, source, sink).expect("DD2 maximum"),
            solve_distance_directed_scaling(&graph, source, sink).expect("scaled maximum"),
        ] {
            assert_eq!(result.certificate, expected.certificate);
        }
    }

    #[test]
    fn deterministic_tiny_multigraphs_match_edmonds_karp() {
        let nodes = ["s", "a", "b", "t"];
        let node_count = u64::try_from(nodes.len()).expect("small node count");
        let mut seed = 0x4d59_5df4_d0f3_3173_u64;
        for case in 0..96 {
            let mut edges = Vec::new();
            for edge_index in 0..10 {
                seed ^= seed << 7;
                seed ^= seed >> 9;
                seed ^= seed << 8;
                let from = nodes[usize::try_from(seed % node_count).expect("node index")];
                seed = seed.rotate_left(17).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let to = nodes[usize::try_from(seed % node_count).expect("node index")];
                seed = seed.rotate_left(13).wrapping_add(case);
                let capacity = seed % 8;
                edges.push((
                    format!("e{edge_index}"),
                    from.to_owned(),
                    to.to_owned(),
                    capacity,
                ));
            }
            let borrowed = edges
                .iter()
                .map(|(id, from, to, capacity)| {
                    (id.as_str(), from.as_str(), to.as_str(), 0, *capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&borrowed);
            let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
            let exact =
                solve_distance_directed_dd2(&graph, source, sink).expect("DD2 differential");
            let scaled = solve_distance_directed_scaling(&graph, source, sink)
                .expect("scaled DD2 differential");
            assert_eq!(exact.certificate.value, expected.certificate.value);
            assert_eq!(scaled.certificate.value, expected.certificate.value);
        }
    }

    #[test]
    fn trace_is_reversible_and_fast_metrics_match() {
        let (graph, source, sink) = network(&[
            ("e0", "s", "a", 0, 5),
            ("e1", "a", "t", 0, 3),
            ("e2", "s", "b", 0, 2),
            ("e3", "b", "t", 0, 4),
            ("e4", "a", "b", 0, 4),
        ]);
        let fast = solve_distance_directed_scaling(&graph, source, sink).expect("fast result");
        let traced = trace_distance_directed_scaling(&graph, source, sink).expect("trace result");
        assert_eq!(traced.result, fast);
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "distance-directed-scaling-augmenting-path.start-scaling-phase"
        }));
        assert!(traced.events.iter().all(|event| {
            event.catalog_id !=
                "distance-directed-scaling-augmenting-path.complete-scaling-phase"
                || matches!(event.entity_refs.as_slice(), [FlowTraceEntityRef::Node(node)] if node.as_str() == "s")
        }));
        assert!(traced.events.iter().all(|event| {
            match event.catalog_id.rsplit_once('.').map(|(_, suffix)| suffix) {
                Some("inspect-residual-arc") => {
                    matches!(
                        event.entity_refs.as_slice(),
                        [FlowTraceEntityRef::ResidualArc(_)]
                    )
                }
                Some("replace-tree-arc") => {
                    event.entity_refs.len() == 2
                        && event
                            .entity_refs
                            .iter()
                            .all(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                }
                Some("relabel-node" | "delete-node") => {
                    matches!(event.entity_refs.as_slice(), [FlowTraceEntityRef::Node(_)])
                }
                _ => true,
            }
        }));
        assert_eq!(
            traced
                .events
                .iter()
                .filter(|event| event.catalog_id.ends_with(".inspect-residual-arc"))
                .count() as u128,
            traced.result.metrics.residual_arc_scans,
            "every measured residual-arc inspection must own one visible Detail boundary",
        );
        let mut snapshot = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(snapshot, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Reverse)
                .expect("backward replay");
        }
        assert_eq!(snapshot, traced.base_snapshot);
    }

    #[test]
    fn nonzero_lower_bounds_are_rejected() {
        let (graph, source, sink) = network(&[("e0", "s", "t", 1, 2)]);
        assert_eq!(
            solve_distance_directed_dd2(&graph, source, sink),
            Err(DistanceDirectedError::GraphRequirement)
        );
    }
}
