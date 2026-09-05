//! Deterministic shortest-augmenting-path and ISAP kernels.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for SAP/ISAP.
pub const SAP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge limit for SAP/ISAP.
pub const SAP_MAX_EDGES: usize = 20_000;
/// Hard ceiling for positive residual-arc inspections.
pub const SAP_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for advance, relabel, augmentation, and gap transitions.
pub const SAP_MAX_STATE_TRANSITIONS: u64 = 100_000;

/// Exact operation counts from the distance-label kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SapMetrics {
    /// Positive residual arcs inspected by reverse BFS or the path search.
    pub residual_arc_scans: u128,
    /// Successful source-to-sink path augmentations.
    pub augmentations: u64,
    /// Distance-label increases.
    pub relabels: u64,
    /// Backtracks from a relabeled non-source vertex.
    pub retreats: u64,
    /// Reverse breadth-first initializations; zero for plain SAP.
    pub reverse_bfs_runs: u64,
    /// Gap-heuristic terminations; zero for plain SAP.
    pub gap_terminations: u64,
}

/// Certified canonical SAP/ISAP result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SapResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: SapMetrics,
}

/// Certified SAP/ISAP result with a complete reversible event sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SapTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: SapResult,
    /// Replay boundary before label initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the independently certified optimum.
    pub final_snapshot: FlowTraceSnapshot,
}

/// SAP/ISAP construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SapError {
    /// Input exceeds the practical admission band for this algorithm.
    #[error("graph exceeds SAP/ISAP admission limits")]
    AdmissionLimit,
    /// A deterministic execution work ceiling was reached.
    #[error("SAP/ISAP work limit reached")]
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
    #[error("SAP/ISAP metric overflow")]
    MetricOverflow,
    /// The maintained admissible path or label counts became inconsistent.
    #[error("SAP/ISAP distance-label invariant failed")]
    LabelInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves maximum flow with a valid distance labeling initialized to zero.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible lower bounds, work-limit exhaustion,
/// residual/label invariant failure, metric overflow, or a rejected certificate.
pub fn solve_shortest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SapResult, SapError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_sap_preset_with_feasibility(
        graph,
        source,
        sink,
        SapExecutionPreset::Plain,
        &mut feasibility,
    )
}

/// Solves maximum flow with reverse-BFS labels, current arcs, and gap termination.
///
/// # Errors
///
/// Returns the same bounded execution failures as
/// [`solve_shortest_augmenting_path`].
pub fn solve_isap(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SapResult, SapError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_sap_preset_with_feasibility(
        graph,
        source,
        sink,
        SapExecutionPreset::Improved,
        &mut feasibility,
    )
}

/// Solves one SAP preset while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_sap_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: SapExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<SapResult, SapError> {
    solve_internal(graph, source, sink, false, preset, feasibility).map(|run| run.result)
}

/// Traces zero-label initialization, admissible advances, relabels, and paths.
///
/// # Errors
///
/// Returns the same failures as [`solve_shortest_augmenting_path`], plus trace
/// invariant failures.
pub fn trace_shortest_augmenting_path(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SapTraceResult, SapError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_sap_preset_with_feasibility(
        graph,
        source,
        sink,
        SapExecutionPreset::Plain,
        &mut feasibility,
    )
}

/// Traces reverse BFS, current-arc advances, relabels, and gap termination.
///
/// # Errors
///
/// Returns the same failures as [`solve_isap`], plus trace invariant failures.
pub fn trace_isap(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SapTraceResult, SapError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_sap_preset_with_feasibility(
        graph,
        source,
        sink,
        SapExecutionPreset::Improved,
        &mut feasibility,
    )
}

/// Traces one SAP preset while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_sap_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: SapExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<SapTraceResult, SapError> {
    trace_internal(graph, source, sink, preset, feasibility)
}

/// Closed set of shortest-augmenting-path execution presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SapExecutionPreset {
    /// Plain SAP with zero-initialized distance labels.
    Plain,
    /// ISAP with reverse-BFS initialization and gap termination.
    Improved,
}

impl SapExecutionPreset {
    const fn uses_reverse_bfs(self) -> bool {
        matches!(self, Self::Improved)
    }

    const fn uses_gap(self) -> bool {
        matches!(self, Self::Improved)
    }

    const fn inspect_catalog_id(self) -> &'static str {
        match self {
            Self::Plain => "shortest-augmenting-path.inspect-residual-arc",
            Self::Improved => "isap.inspect-residual-arc",
        }
    }
}

struct SapInternalRun {
    result: SapResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct SapKernelState {
    labels: Vec<usize>,
    label_counts: Vec<usize>,
    current_arcs: Vec<usize>,
    outgoing_ids: Vec<Vec<ResidualArcId>>,
    path_nodes: Vec<NodeIndex>,
    path_arcs: Vec<ResidualArcId>,
    logical_transitions: u64,
}

impl SapKernelState {
    fn new(graph: &FlowNetwork, labels: Vec<usize>, source: NodeIndex) -> Result<Self, SapError> {
        let node_count = graph.nodes().len();
        let mut label_counts = vec![0_usize; node_count + 1];
        for &label in &labels {
            let count = label_counts
                .get_mut(label)
                .ok_or(SapError::LabelInvariant)?;
            *count = count.checked_add(1).ok_or(SapError::MetricOverflow)?;
        }
        Ok(Self {
            labels,
            label_counts,
            current_arcs: vec![0; node_count],
            outgoing_ids: graph
                .node_indices()
                .map(|node| stable_outgoing_ids(graph, node))
                .collect(),
            path_nodes: vec![source],
            path_arcs: Vec::new(),
            logical_transitions: 0,
        })
    }

    fn current_node(&self) -> Result<NodeIndex, SapError> {
        self.path_nodes
            .last()
            .copied()
            .ok_or(SapError::LabelInvariant)
    }

    fn count_transition(&mut self) -> Result<(), SapError> {
        if self.logical_transitions >= SAP_MAX_STATE_TRANSITIONS {
            return Err(SapError::WorkLimit);
        }
        self.logical_transitions = self
            .logical_transitions
            .checked_add(1)
            .ok_or(SapError::MetricOverflow)?;
        Ok(())
    }
}

fn trace_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: SapExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<SapTraceResult, SapError> {
    let run = solve_internal(graph, source, sink, true, preset, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(SapError::LabelInvariant)?;
    Ok(SapTraceResult {
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
    record_trace: bool,
    preset: SapExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<SapInternalRun, SapError> {
    if graph.nodes().len() > SAP_MAX_NODES || graph.edges().len() > SAP_MAX_EDGES {
        return Err(SapError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let node_count = graph.nodes().len();
    let mut metrics = SapMetrics::default();
    let base_snapshot = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; node_count],
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

    let (labels, initialization_order) = if preset.uses_reverse_bfs() {
        metrics.reverse_bfs_runs = 1;
        record_event(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            TraceTransition {
                preset,
                suffix: "reverse-bfs",
                view: TraceEventView::reverse_bfs_start(node_count, sink),
                detail: None,
            },
        )?;
        reverse_bfs_labels(&state, sink, preset, &mut metrics, recorder.as_mut())?
    } else {
        (vec![0; node_count], Vec::new())
    };
    let mut kernel = SapKernelState::new(graph, labels, source)?;
    record_event(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        TraceTransition {
            preset,
            suffix: if preset.uses_reverse_bfs() {
                "publish-reverse-bfs"
            } else {
                "initialize"
            },
            view: TraceEventView::initial(&kernel, initialization_order),
            detail: None,
        },
    )?;

    run_distance_label_kernel(
        graph,
        &mut state,
        source,
        sink,
        preset,
        &mut metrics,
        &mut kernel,
        recorder.as_mut(),
    )?;

    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_event(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        TraceTransition {
            preset,
            suffix: "optimal",
            view: TraceEventView::empty(&kernel),
            detail: None,
        },
    )?;
    let result = SapResult {
        flows,
        certificate,
        metrics,
    };
    Ok(SapInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_distance_label_kernel(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    preset: SapExecutionPreset,
    metrics: &mut SapMetrics,
    kernel: &mut SapKernelState,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), SapError> {
    let node_count = graph.nodes().len();
    while kernel.labels[source.as_usize()] < node_count {
        let node = kernel.current_node()?;
        if node == sink {
            augment_current_path(
                graph,
                state,
                preset,
                metrics,
                kernel,
                recorder.as_deref_mut(),
            )?;
            continue;
        }

        if let Some((arc_index, arc)) = next_admissible_arc(
            state,
            node,
            preset,
            kernel,
            metrics,
            recorder.as_deref_mut(),
        )? {
            kernel.count_transition()?;
            kernel.current_arcs[node.as_usize()] = arc_index;
            kernel.path_arcs.push(arc.id);
            kernel.path_nodes.push(arc.to);
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                TraceTransition {
                    preset,
                    suffix: "advance",
                    view: TraceEventView::kernel(kernel),
                    detail: None,
                },
            )?;
            continue;
        }

        if relabel_and_retreat(
            graph,
            state,
            source,
            preset,
            metrics,
            kernel,
            recorder.as_deref_mut(),
        )? {
            break;
        }
    }
    Ok(())
}

fn next_admissible_arc(
    state: &ResidualState<'_>,
    node: NodeIndex,
    preset: SapExecutionPreset,
    kernel: &SapKernelState,
    metrics: &mut SapMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Option<(usize, ResidualArc)>, SapError> {
    let start = *kernel
        .current_arcs
        .get(node.as_usize())
        .ok_or(SapError::LabelInvariant)?;
    let outgoing = kernel
        .outgoing_ids
        .get(node.as_usize())
        .ok_or(SapError::LabelInvariant)?;
    for (index, id) in outgoing.iter().enumerate().skip(start) {
        let arc = state.arc(id).ok_or(SapError::LabelInvariant)?;
        if arc.capacity == 0 {
            continue;
        }
        count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
        let from_label = *kernel
            .labels
            .get(node.as_usize())
            .ok_or(SapError::LabelInvariant)?;
        let to_label = *kernel
            .labels
            .get(arc.to.as_usize())
            .ok_or(SapError::LabelInvariant)?;
        if from_label == to_label.saturating_add(1) {
            return Ok(Some((index, arc)));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn relabel_and_retreat(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    preset: SapExecutionPreset,
    metrics: &mut SapMetrics,
    kernel: &mut SapKernelState,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<bool, SapError> {
    kernel.count_transition()?;
    let node_count = graph.nodes().len();
    let node = kernel.current_node()?;
    let old_label = kernel.labels[node.as_usize()];
    let mut minimum = node_count;
    let outgoing = kernel
        .outgoing_ids
        .get(node.as_usize())
        .ok_or(SapError::LabelInvariant)?;
    for id in outgoing {
        let arc = state.arc(id).ok_or(SapError::LabelInvariant)?;
        if arc.capacity == 0 {
            continue;
        }
        count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
        minimum = minimum.min(kernel.labels[arc.to.as_usize()]);
    }
    let new_label = if minimum >= node_count {
        node_count
    } else {
        minimum.checked_add(1).ok_or(SapError::MetricOverflow)?
    };
    if new_label <= old_label {
        return Err(SapError::LabelInvariant);
    }
    decrement_label_count(kernel, old_label)?;
    increment_label_count(kernel, new_label)?;
    kernel.labels[node.as_usize()] = new_label;
    kernel.current_arcs[node.as_usize()] = 0;
    metrics.relabels = metrics
        .relabels
        .checked_add(1)
        .ok_or(SapError::MetricOverflow)?;

    let gap_level =
        if preset.uses_gap() && old_label < node_count && kernel.label_counts[old_label] == 0 {
            if node != source {
                let source_label = kernel.labels[source.as_usize()];
                if source_label != node_count {
                    decrement_label_count(kernel, source_label)?;
                    increment_label_count(kernel, node_count)?;
                    kernel.labels[source.as_usize()] = node_count;
                }
            }
            metrics.gap_terminations = metrics
                .gap_terminations
                .checked_add(1)
                .ok_or(SapError::MetricOverflow)?;
            Some(old_label)
        } else {
            None
        };

    if node != source && gap_level.is_none() {
        kernel.path_nodes.pop().ok_or(SapError::LabelInvariant)?;
        kernel.path_arcs.pop().ok_or(SapError::LabelInvariant)?;
        let predecessor = kernel.current_node()?;
        kernel.current_arcs[predecessor.as_usize()] = kernel.current_arcs[predecessor.as_usize()]
            .checked_add(1)
            .ok_or(SapError::MetricOverflow)?;
        metrics.retreats = metrics
            .retreats
            .checked_add(1)
            .ok_or(SapError::MetricOverflow)?;
    }

    record_event(
        recorder,
        graph,
        state,
        *metrics,
        TraceTransition {
            preset,
            suffix: if gap_level.is_some() {
                "gap"
            } else {
                "relabel"
            },
            view: TraceEventView::kernel(kernel),
            detail: gap_level.map(|level| ("gap-level", level as i128)),
        },
    )?;
    Ok(gap_level.is_some())
}

fn augment_current_path(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    preset: SapExecutionPreset,
    metrics: &mut SapMetrics,
    kernel: &mut SapKernelState,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), SapError> {
    kernel.count_transition()?;
    let capacities = kernel
        .path_arcs
        .iter()
        .map(|id| {
            state
                .arc(id)
                .map(|arc| arc.capacity)
                .ok_or(SapError::LabelInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bottleneck = capacities
        .iter()
        .copied()
        .min()
        .ok_or(SapError::LabelInvariant)?;
    let first_saturated = capacities
        .iter()
        .position(|&capacity| capacity == bottleneck)
        .ok_or(SapError::LabelInvariant)?;
    state.augment(&kernel.path_arcs, bottleneck)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(SapError::MetricOverflow)?;
    record_event(
        recorder,
        graph,
        state,
        *metrics,
        TraceTransition {
            preset,
            suffix: "augment",
            view: TraceEventView::kernel(kernel),
            detail: Some(("bottleneck", i128::from(bottleneck))),
        },
    )?;

    let saturated_tail = *kernel
        .path_nodes
        .get(first_saturated)
        .ok_or(SapError::LabelInvariant)?;
    kernel.current_arcs[saturated_tail.as_usize()] = kernel.current_arcs[saturated_tail.as_usize()]
        .checked_add(1)
        .ok_or(SapError::MetricOverflow)?;
    kernel.path_arcs.truncate(first_saturated);
    kernel.path_nodes.truncate(first_saturated + 1);
    Ok(())
}

fn reverse_bfs_labels(
    state: &ResidualState<'_>,
    sink: NodeIndex,
    preset: SapExecutionPreset,
    metrics: &mut SapMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(Vec<usize>, Vec<NodeIndex>), SapError> {
    let node_count = state.graph().nodes().len();
    let mut incoming = vec![Vec::<ResidualArcId>::new(); node_count];
    for node in state.graph().node_indices() {
        for id in stable_outgoing_ids(state.graph(), node) {
            let arc = state.arc(&id).ok_or(SapError::LabelInvariant)?;
            if arc.capacity > 0 {
                incoming[arc.to.as_usize()].push(id);
            }
        }
    }
    for arcs in &mut incoming {
        arcs.sort_unstable();
    }

    let mut labels = vec![node_count; node_count];
    labels[sink.as_usize()] = 0;
    let mut queue = VecDeque::from([sink]);
    let mut search_order = vec![sink];
    while let Some(node) = queue.pop_front() {
        let next_label = labels[node.as_usize()]
            .checked_add(1)
            .ok_or(SapError::MetricOverflow)?;
        for id in &incoming[node.as_usize()] {
            count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
            let arc = state.arc(id).ok_or(SapError::LabelInvariant)?;
            if labels[arc.from.as_usize()] == node_count {
                labels[arc.from.as_usize()] = next_label;
                queue.push_back(arc.from);
                search_order.push(arc.from);
            }
        }
    }
    Ok((labels, search_order))
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

fn decrement_label_count(kernel: &mut SapKernelState, label: usize) -> Result<(), SapError> {
    let count = kernel
        .label_counts
        .get_mut(label)
        .ok_or(SapError::LabelInvariant)?;
    *count = count.checked_sub(1).ok_or(SapError::LabelInvariant)?;
    Ok(())
}

fn increment_label_count(kernel: &mut SapKernelState, label: usize) -> Result<(), SapError> {
    let count = kernel
        .label_counts
        .get_mut(label)
        .ok_or(SapError::LabelInvariant)?;
    *count = count.checked_add(1).ok_or(SapError::MetricOverflow)?;
    Ok(())
}

fn count_arc_scan(
    metrics: &mut SapMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: SapExecutionPreset,
    arc: &ResidualArcId,
) -> Result<(), SapError> {
    if metrics.residual_arc_scans >= SAP_MAX_RESIDUAL_ARC_SCANS {
        return Err(SapError::WorkLimit);
    }
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(SapError::MetricOverflow)?;
    if let Some(recorder) = recorder {
        recorder.record_metric_observation(
            FlowTraceEventMetadata {
                catalog_id: preset.inspect_catalog_id(),
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "sap:inspect-residual-arc",
            },
            FlowTraceMetricId::ResidualArcScans,
            FlowTraceEntityRef::ResidualArc(arc.clone()),
        )?;
    }
    Ok(())
}

struct TraceEventView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path_arcs: Vec<ResidualArcId>,
}

struct TraceTransition {
    preset: SapExecutionPreset,
    suffix: &'static str,
    view: TraceEventView,
    detail: Option<(&'static str, i128)>,
}

impl TraceEventView {
    fn reverse_bfs_start(node_count: usize, sink: NodeIndex) -> Self {
        Self {
            labels: vec![None; node_count],
            search_order: vec![sink],
            path_arcs: Vec::new(),
        }
    }

    fn initial(kernel: &SapKernelState, search_order: Vec<NodeIndex>) -> Self {
        Self {
            labels: kernel
                .labels
                .iter()
                .map(|&label| Some(label as i128))
                .collect(),
            search_order,
            path_arcs: Vec::new(),
        }
    }

    fn kernel(kernel: &SapKernelState) -> Self {
        Self {
            labels: kernel
                .labels
                .iter()
                .map(|&label| Some(label as i128))
                .collect(),
            search_order: kernel.path_nodes.clone(),
            path_arcs: kernel.path_arcs.clone(),
        }
    }

    fn empty(kernel: &SapKernelState) -> Self {
        Self {
            labels: kernel
                .labels
                .iter()
                .map(|&label| Some(label as i128))
                .collect(),
            search_order: Vec::new(),
            path_arcs: Vec::new(),
        }
    }
}

fn record_event(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: SapMetrics,
    transition: TraceTransition,
) -> Result<(), SapError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let metadata = event_metadata(transition.preset, transition.suffix)?;
    let local_focus = if transition.suffix == "advance" {
        Some(vec![FlowTraceEntityRef::ResidualArc(
            transition
                .view
                .path_arcs
                .last()
                .cloned()
                .ok_or(SapError::LabelInvariant)?,
        )])
    } else {
        None
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        transition.view.labels,
        transition.view.search_order,
        transition.view.path_arcs,
        Vec::new(),
        trace_metrics(metrics),
    );
    if let Some(local_focus) = local_focus {
        recorder.record_transition_with_detail_and_focus(
            metadata,
            &snapshot,
            transition.detail,
            local_focus,
        )?;
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, transition.detail)?;
    }
    Ok(())
}

fn event_metadata(
    preset: SapExecutionPreset,
    suffix: &str,
) -> Result<FlowTraceEventMetadata, SapError> {
    let metadata = match (preset, suffix) {
        (SapExecutionPreset::Plain, "initialize") => FlowTraceEventMetadata {
            catalog_id: "shortest-augmenting-path.initialize",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "shortest-augmenting-path:initialize-valid-zero-labels",
        },
        (SapExecutionPreset::Plain, "advance") => FlowTraceEventMetadata {
            catalog_id: "shortest-augmenting-path.advance",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "shortest-augmenting-path:advance-admissible-current-arc",
        },
        (SapExecutionPreset::Plain, "relabel") => FlowTraceEventMetadata {
            catalog_id: "shortest-augmenting-path.relabel",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "shortest-augmenting-path:raise-distance-and-retreat",
        },
        (SapExecutionPreset::Plain, "augment") => FlowTraceEventMetadata {
            catalog_id: "shortest-augmenting-path.augment",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "shortest-augmenting-path:augment-by-bottleneck",
        },
        (SapExecutionPreset::Plain, "optimal") => FlowTraceEventMetadata {
            catalog_id: "shortest-augmenting-path.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "shortest-augmenting-path:return-certified-cut",
        },
        (SapExecutionPreset::Improved, "reverse-bfs") => FlowTraceEventMetadata {
            catalog_id: "isap.reverse-bfs",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "isap:initialize-exact-reverse-bfs-labels",
        },
        (SapExecutionPreset::Improved, "publish-reverse-bfs") => FlowTraceEventMetadata {
            catalog_id: "isap.publish-reverse-bfs-labels",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "isap:publish-exact-reverse-bfs-labels",
        },
        (SapExecutionPreset::Improved, "advance") => FlowTraceEventMetadata {
            catalog_id: "isap.advance",
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: "isap:advance-admissible-current-arc",
        },
        (SapExecutionPreset::Improved, "relabel") => FlowTraceEventMetadata {
            catalog_id: "isap.relabel",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "isap:raise-distance-and-retreat",
        },
        (SapExecutionPreset::Improved, "gap") => FlowTraceEventMetadata {
            catalog_id: "isap.gap",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "isap:terminate-empty-distance-level",
        },
        (SapExecutionPreset::Improved, "augment") => FlowTraceEventMetadata {
            catalog_id: "isap.augment",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "isap:augment-by-bottleneck",
        },
        (SapExecutionPreset::Improved, "optimal") => FlowTraceEventMetadata {
            catalog_id: "isap.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "isap:return-certified-cut",
        },
        _ => return Err(SapError::LabelInvariant),
    };
    Ok(metadata)
}

const fn trace_metrics(metrics: SapMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: metrics.relabels as u128,
        retreats: metrics.retreats as u128,
        reverse_bfs_runs: metrics.reverse_bfs_runs as u128,
        gap_terminations: metrics.gap_terminations as u128,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
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
    fn both_presets_match_edmonds_karp_and_expose_distinct_initialization() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 5),
            ("sb", "s", "b", 0, 4),
            ("ab", "a", "b", 0, 2),
            ("at", "a", "t", 0, 3),
            ("bt", "b", "t", 0, 6),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum");
        let sap = solve_shortest_augmenting_path(&graph, source, sink).expect("SAP maximum");
        let isap = solve_isap(&graph, source, sink).expect("ISAP maximum");

        assert_eq!(sap.certificate, expected.certificate);
        assert_eq!(isap.certificate, expected.certificate);
        assert_eq!(sap.metrics.reverse_bfs_runs, 0);
        assert_eq!(sap.metrics.gap_terminations, 0);
        assert_eq!(isap.metrics.reverse_bfs_runs, 1);
    }

    #[test]
    fn isap_gap_terminates_a_reachable_dead_branch_after_the_last_flow_path() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 1),
            ("at", "a", "t", 0, 1),
            ("sd", "s", "dead", 0, 1),
            ("dx", "dead", "x", 0, 1),
        ]);
        let traced = trace_isap(&graph, source, sink).expect("ISAP maximum");

        assert_eq!(traced.result.certificate.value, 1);
        assert_eq!(traced.result.metrics.reverse_bfs_runs, 1);
        assert_eq!(traced.result.metrics.gap_terminations, 1);
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "isap.gap"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.label == "gap-level")
        }));
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
        for result in [
            solve_shortest_augmenting_path(&graph, source, sink).expect("SAP maximum"),
            solve_isap(&graph, source, sink).expect("ISAP maximum"),
        ] {
            assert_eq!(result.certificate.value, 7);
            assert_eq!(result.certificate.cut_bound, 7);
        }
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
        let fast = solve_isap(&graph, source, sink).expect("fast result");
        let traced = trace_isap(&graph, source, sink).expect("trace result");
        let traced_sap =
            trace_shortest_augmenting_path(&graph, source, sink).expect("SAP trace result");

        assert_eq!(traced.result, fast);
        assert_eq!(
            traced.events.first().map(|event| event.catalog_id.as_str()),
            Some("isap.reverse-bfs")
        );
        for (run, catalog_id) in [
            (&traced_sap, "shortest-augmenting-path.inspect-residual-arc"),
            (&traced, "isap.inspect-residual-arc"),
        ] {
            let scans = run
                .events
                .iter()
                .filter(|event| event.catalog_id == catalog_id)
                .collect::<Vec<_>>();
            assert_eq!(
                u128::try_from(scans.len()).expect("scan event count"),
                run.result.metrics.residual_arc_scans
            );
            assert!(scans.iter().all(|event| {
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
            assert!(
                run.events
                    .iter()
                    .filter(|event| event.catalog_id.ends_with(".advance"))
                    .all(|event| {
                        matches!(
                            event.entity_refs.as_slice(),
                            [FlowTraceEntityRef::ResidualArc(_)]
                        )
                    })
            );
        }
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_edmonds_karp() {
        const ARCS: [(&str, &str, &str); 8] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("ab", "a", "b"),
            ("ba", "b", "a"),
            ("at", "a", "t"),
            ("bt", "b", "t"),
            ("as", "a", "s"),
            ("tb", "t", "b"),
        ];
        for fixture in 0_u64..32 {
            let edges = ARCS
                .iter()
                .enumerate()
                .map(|(index, &(id, from, to))| {
                    let rotation = u32::try_from(index).expect("fixture arc index fits u32");
                    let capacity = (fixture.rotate_left(rotation) & 3) + 1;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&edges);
            let expected = solve_edmonds_karp(&graph, source, sink)
                .expect("reference maximum")
                .certificate
                .value;
            assert_eq!(
                solve_shortest_augmenting_path(&graph, source, sink)
                    .expect("SAP maximum")
                    .certificate
                    .value,
                expected
            );
            assert_eq!(
                solve_isap(&graph, source, sink)
                    .expect("ISAP maximum")
                    .certificate
                    .value,
                expected
            );
        }
    }
}
