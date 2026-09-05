//! Deterministic preflow push–relabel kernels with replaceable active policies.

use std::cmp::Reverse;
use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
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

/// Conservative interactive node limit for push–relabel presets.
pub const PUSH_RELABEL_MAX_NODES: usize = 2_000;
/// Conservative interactive edge limit for push–relabel presets.
pub const PUSH_RELABEL_MAX_EDGES: usize = 20_000;
/// Hard ceiling for positive residual-arc inspections.
pub const PUSH_RELABEL_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for initialization pushes, selections, pushes, and relabels.
pub const PUSH_RELABEL_MAX_STATE_TRANSITIONS: u64 = 100_000;
/// Fixed admissible-path bound used by the interactive PAR preset.
pub const PARTIAL_AUGMENT_RELABEL_PATH_LENGTH: usize = 4;
/// Global relabel runs after this many edge-count units of residual scanning.
pub const PUSH_RELABEL_GLOBAL_RELABEL_SCAN_MULTIPLIER: u128 = 1;

/// Exact counters shared by deterministic active-vertex policies.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PushRelabelMetrics {
    /// Positive residual arcs inspected by current-arc scans or relabels.
    pub residual_arc_scans: u128,
    /// Local pushes, including source-preflow initialization.
    pub pushes: u64,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that exhaust the selected vertex's excess first.
    pub nonsaturating_pushes: u64,
    /// Active vertices completely discharged.
    pub discharges: u64,
    /// Active vertices selected by the preset policy.
    pub active_vertex_selections: u64,
    /// Valid distance-label increases.
    pub relabels: u64,
    /// Successful bounded-path augmentations; zero for local-discharge presets.
    pub augmentations: u64,
    /// Bounded admissible-path searches; zero for local-discharge presets.
    pub path_searches: u64,
    /// Recursive-search backtracks after relabeling a path endpoint.
    pub retreats: u64,
    /// Sink-rooted reverse breadth-first global relabels.
    pub global_relabels: u64,
    /// Gap relabel batches that changed at least one vertex.
    pub gap_relabels: u64,
    /// Excess-dominator scales processed from the initial power of two to one.
    pub scaling_phases: u64,
}

/// Certified canonical push–relabel result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushRelabelResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: PushRelabelMetrics,
}

/// Certified push–relabel result with a complete reversible event sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushRelabelTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: PushRelabelResult,
    /// Replay boundary before source-preflow initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the independently certified optimum.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Push–relabel construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PushRelabelError {
    /// Input exceeds the practical admission band for this algorithm family.
    #[error("graph exceeds push-relabel admission limits")]
    AdmissionLimit,
    /// A deterministic execution work ceiling was reached.
    #[error("push-relabel work limit reached")]
    WorkLimit,
    /// Lower-bound circulation construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Flow arithmetic or final solver-independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact counters or excess arithmetic exceeded their declared domain.
    #[error("push-relabel arithmetic overflow")]
    ArithmeticOverflow,
    /// Height, excess, current-arc, or active-policy state became inconsistent.
    #[error("push-relabel invariant failed")]
    Invariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves maximum flow with the deterministic generic active-vertex policy.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible lower bounds, work-limit exhaustion,
/// arithmetic/invariant failures, or a rejected independent certificate.
pub fn solve_generic_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::Generic).map(|run| run.result)
}

/// Solves maximum flow with stable active selection and the current-arc
/// optimization.
///
/// Unlike [`solve_generic_push_relabel`], a discharge resumes each residual
/// scan at the first arc not already proved inadmissible at the current
/// height. This makes the heuristic independently runnable and measurable.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_current_arc_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::CurrentArc).map(|run| run.result)
}

/// Solves maximum flow with a FIFO active queue.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_fifo_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::Fifo).map(|run| run.result)
}

/// Solves maximum flow by repeatedly selecting a highest-label active vertex.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_highest_label_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::HighestLabel)
        .map(|run| run.result)
}

/// Solves maximum flow with the relabel-to-front discharge-list policy.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_relabel_to_front(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(
        graph,
        source,
        sink,
        false,
        PushRelabelPreset::RelabelToFront,
    )
    .map(|run| run.result)
}

/// Solves maximum flow with Goldberg's bounded partial augment–relabel method.
///
/// The interactive preset fixes the admissible-path bound at
/// [`PARTIAL_AUGMENT_RELABEL_PATH_LENGTH`] and deterministically selects a
/// highest-label active root. The source is an absorbing terminal during flow
/// recovery so the published result is a feasible flow, not only a maximum
/// preflow.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_partial_augment_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(
        graph,
        source,
        sink,
        false,
        PushRelabelPreset::PartialAugmentRelabel,
    )
    .map(|run| run.result)
}

/// Solves maximum flow with highest-label selection and global relabeling.
///
/// A sink-rooted reverse BFS runs after initialization and after each stable
/// edge-count interval of positive residual-arc inspections.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_global_relabel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::GlobalRelabel)
        .map(|run| run.result)
}

/// Solves maximum flow with highest-label selection and gap relabeling.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_gap_relabel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::GapRelabel).map(|run| run.result)
}

/// Solves maximum flow with Ahuja–Orlin excess scaling.
///
/// At scale `delta`, the deterministic implementation selects the minimum-label
/// vertex whose excess is greater than `delta / 2`, then caps every push so a
/// nonterminal head never exceeds `delta` excess.
///
/// # Errors
///
/// Returns the same bounded failures as [`solve_generic_push_relabel`].
pub fn solve_excess_scaling_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal(graph, source, sink, false, PushRelabelPreset::ExcessScaling)
        .map(|run| run.result)
}

/// Traces generic active selection, pushes, relabels, and discharges.
///
/// # Errors
///
/// Returns the same failures as [`solve_generic_push_relabel`], plus trace
/// invariant failures.
pub fn trace_generic_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::Generic)
}

/// Traces the independently selectable current-arc heuristic.
///
/// # Errors
///
/// Returns the same failures as [`solve_current_arc_push_relabel`], plus trace
/// invariant failures.
pub fn trace_current_arc_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::CurrentArc)
}

/// Traces the FIFO active-queue preset.
///
/// # Errors
///
/// Returns the same failures as [`solve_fifo_push_relabel`], plus trace
/// invariant failures.
pub fn trace_fifo_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::Fifo)
}

/// Traces the highest-label active-bucket policy.
///
/// # Errors
///
/// Returns the same failures as [`solve_highest_label_push_relabel`], plus
/// trace invariant failures.
pub fn trace_highest_label_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::HighestLabel)
}

/// Traces relabel-to-front list scans, discharges, and list moves.
///
/// # Errors
///
/// Returns the same failures as [`solve_relabel_to_front`], plus trace
/// invariant failures.
pub fn trace_relabel_to_front(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::RelabelToFront)
}

/// Traces bounded admissible-path search, recursive relabel/retreat work, and
/// each multi-edge partial augmentation.
///
/// # Errors
///
/// Returns the same failures as [`solve_partial_augment_relabel`], plus trace
/// invariant failures.
pub fn trace_partial_augment_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(
        graph,
        source,
        sink,
        PushRelabelPreset::PartialAugmentRelabel,
    )
}

/// Traces every sink-rooted reverse-BFS global relabel boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_global_relabel_push_relabel`], plus
/// trace invariant failures.
pub fn trace_global_relabel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::GlobalRelabel)
}

/// Traces local relabels and every non-empty gap batch independently.
///
/// # Errors
///
/// Returns the same failures as [`solve_gap_relabel_push_relabel`], plus trace
/// invariant failures.
pub fn trace_gap_relabel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::GapRelabel)
}

/// Traces every excess-dominator phase, minimum-label scaled selection, push,
/// and relabel in the Ahuja–Orlin excess-scaling algorithm.
///
/// # Errors
///
/// Returns the same failures as [`solve_excess_scaling_push_relabel`], plus
/// trace invariant failures.
pub fn trace_excess_scaling_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    trace_internal(graph, source, sink, PushRelabelPreset::ExcessScaling)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PushRelabelPreset {
    Generic,
    CurrentArc,
    Fifo,
    RelabelToFront,
    HighestLabel,
    PartialAugmentRelabel,
    GlobalRelabel,
    GapRelabel,
    ExcessScaling,
}

/// Closed set of published push--relabel execution presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PushRelabelExecutionPreset {
    /// Generic stable active selection.
    Generic,
    /// Current-arc optimization.
    CurrentArc,
    /// FIFO active queue.
    Fifo,
    /// Relabel-to-front discharge list.
    RelabelToFront,
    /// Highest-label active selection.
    HighestLabel,
    /// Bounded partial augment--relabel.
    PartialAugmentRelabel,
    /// Highest-label with periodic global relabeling.
    GlobalRelabel,
    /// Highest-label with gap relabeling.
    GapRelabel,
    /// Ahuja--Orlin excess scaling.
    ExcessScaling,
}

impl From<PushRelabelExecutionPreset> for PushRelabelPreset {
    fn from(value: PushRelabelExecutionPreset) -> Self {
        match value {
            PushRelabelExecutionPreset::Generic => Self::Generic,
            PushRelabelExecutionPreset::CurrentArc => Self::CurrentArc,
            PushRelabelExecutionPreset::Fifo => Self::Fifo,
            PushRelabelExecutionPreset::RelabelToFront => Self::RelabelToFront,
            PushRelabelExecutionPreset::HighestLabel => Self::HighestLabel,
            PushRelabelExecutionPreset::PartialAugmentRelabel => Self::PartialAugmentRelabel,
            PushRelabelExecutionPreset::GlobalRelabel => Self::GlobalRelabel,
            PushRelabelExecutionPreset::GapRelabel => Self::GapRelabel,
            PushRelabelExecutionPreset::ExcessScaling => Self::ExcessScaling,
        }
    }
}

/// Solves one push--relabel preset while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_push_relabel_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: PushRelabelExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<PushRelabelResult, PushRelabelError> {
    solve_internal_with_feasibility(graph, source, sink, false, preset.into(), feasibility)
        .map(|run| run.result)
}

/// Traces one push--relabel preset while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_push_relabel_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: PushRelabelExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    let run =
        solve_internal_with_feasibility(graph, source, sink, true, preset.into(), feasibility)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PushRelabelError::Invariant)?;
    Ok(PushRelabelTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

impl PushRelabelPreset {
    #[cfg(test)]
    const fn prefix(self) -> &'static str {
        match self {
            Self::Generic => "generic-push-relabel",
            Self::CurrentArc => "current-arc-heuristic",
            Self::Fifo => "fifo-push-relabel",
            Self::RelabelToFront => "relabel-to-front",
            Self::HighestLabel => "highest-label-push-relabel",
            Self::PartialAugmentRelabel => "partial-augment-relabel-max-flow",
            Self::GlobalRelabel => "global-relabel-heuristic",
            Self::GapRelabel => "gap-relabel-heuristic",
            Self::ExcessScaling => "excess-scaling-push-relabel",
        }
    }

    const fn inspect_catalog_id(self) -> &'static str {
        match self {
            Self::Generic => "generic-push-relabel.inspect-residual-arc",
            Self::CurrentArc => "current-arc-heuristic.inspect-residual-arc",
            Self::Fifo => "fifo-push-relabel.inspect-residual-arc",
            Self::RelabelToFront => "relabel-to-front.inspect-residual-arc",
            Self::HighestLabel => "highest-label-push-relabel.inspect-residual-arc",
            Self::PartialAugmentRelabel => "partial-augment-relabel-max-flow.inspect-residual-arc",
            Self::GlobalRelabel => "global-relabel-heuristic.inspect-residual-arc",
            Self::GapRelabel => "gap-relabel-heuristic.inspect-residual-arc",
            Self::ExcessScaling => "excess-scaling-push-relabel.inspect-residual-arc",
        }
    }
}

struct PushRelabelInternalRun {
    result: PushRelabelResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct PushRelabelKernel {
    heights: Vec<usize>,
    height_counts: Vec<usize>,
    excess: Vec<i128>,
    current_arcs: Vec<usize>,
    outgoing_ids: Vec<Vec<ResidualArcId>>,
    incoming_ids: Vec<Vec<ResidualArcId>>,
    logical_transitions: u64,
}

impl PushRelabelKernel {
    fn count_transition(&mut self) -> Result<(), PushRelabelError> {
        if self.logical_transitions >= PUSH_RELABEL_MAX_STATE_TRANSITIONS {
            return Err(PushRelabelError::WorkLimit);
        }
        self.logical_transitions = self
            .logical_transitions
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
        Ok(())
    }

    fn is_active(&self, node: NodeIndex, source: NodeIndex, sink: NodeIndex) -> bool {
        node != source && node != sink && self.excess[node.as_usize()] > 0
    }

    fn set_height(&mut self, node: NodeIndex, new_height: usize) -> Result<(), PushRelabelError> {
        let old_height = *self
            .heights
            .get(node.as_usize())
            .ok_or(PushRelabelError::Invariant)?;
        if old_height == new_height {
            return Ok(());
        }
        let old_count = self
            .height_counts
            .get_mut(old_height)
            .ok_or(PushRelabelError::Invariant)?;
        *old_count = old_count
            .checked_sub(1)
            .ok_or(PushRelabelError::Invariant)?;
        let new_count = self
            .height_counts
            .get_mut(new_height)
            .ok_or(PushRelabelError::Invariant)?;
        *new_count = new_count
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
        self.heights[node.as_usize()] = new_height;
        Ok(())
    }

    fn rebuild_height_counts(&mut self) -> Result<(), PushRelabelError> {
        self.height_counts.fill(0);
        for &height in &self.heights {
            let count = self
                .height_counts
                .get_mut(height)
                .ok_or(PushRelabelError::Invariant)?;
            *count = count
                .checked_add(1)
                .ok_or(PushRelabelError::ArithmeticOverflow)?;
        }
        Ok(())
    }
}

struct ActiveScheduler {
    preset: PushRelabelPreset,
    fifo: VecDeque<NodeIndex>,
    queued: Vec<bool>,
    discharge_list: Vec<NodeIndex>,
    list_cursor: usize,
}

impl ActiveScheduler {
    fn new(
        graph: &FlowNetwork,
        kernel: &PushRelabelKernel,
        source: NodeIndex,
        sink: NodeIndex,
        preset: PushRelabelPreset,
    ) -> Self {
        let mut scheduler = Self {
            preset,
            fifo: VecDeque::new(),
            queued: vec![false; graph.nodes().len()],
            discharge_list: graph
                .node_indices()
                .filter(|&node| node != source && node != sink)
                .collect(),
            list_cursor: 0,
        };
        if preset == PushRelabelPreset::Fifo {
            for node in graph.node_indices() {
                if kernel.is_active(node, source, sink) {
                    scheduler.activate(node);
                }
            }
        }
        scheduler
    }

    fn activate(&mut self, node: NodeIndex) {
        if self.preset != PushRelabelPreset::Fifo || self.queued[node.as_usize()] {
            return;
        }
        self.queued[node.as_usize()] = true;
        self.fifo.push_back(node);
    }

    fn next(
        &mut self,
        graph: &FlowNetwork,
        kernel: &PushRelabelKernel,
        source: NodeIndex,
        sink: NodeIndex,
    ) -> Option<NodeIndex> {
        match self.preset {
            PushRelabelPreset::Generic | PushRelabelPreset::CurrentArc => graph
                .node_indices()
                .find(|&node| kernel.is_active(node, source, sink)),
            PushRelabelPreset::HighestLabel
            | PushRelabelPreset::GlobalRelabel
            | PushRelabelPreset::GapRelabel => graph
                .node_indices()
                .filter(|&node| kernel.is_active(node, source, sink))
                .min_by_key(|&node| (Reverse(kernel.heights[node.as_usize()]), node)),
            PushRelabelPreset::ExcessScaling => graph
                .node_indices()
                .filter(|&node| kernel.is_active(node, source, sink))
                .min_by_key(|&node| (kernel.heights[node.as_usize()], node)),
            PushRelabelPreset::PartialAugmentRelabel => graph
                .node_indices()
                .filter(|&node| kernel.is_active(node, source, sink))
                .min_by_key(|&node| (Reverse(kernel.heights[node.as_usize()]), node)),
            PushRelabelPreset::RelabelToFront => {
                while self.list_cursor < self.discharge_list.len() {
                    let node = self.discharge_list[self.list_cursor];
                    self.list_cursor += 1;
                    if kernel.is_active(node, source, sink) {
                        return Some(node);
                    }
                }
                None
            }
            PushRelabelPreset::Fifo => {
                while let Some(node) = self.fifo.pop_front() {
                    self.queued[node.as_usize()] = false;
                    if kernel.is_active(node, source, sink) {
                        return Some(node);
                    }
                }
                None
            }
        }
    }

    fn after_discharge(
        &mut self,
        node: NodeIndex,
        old_height: usize,
        new_height: usize,
    ) -> Result<Option<Vec<NodeIndex>>, PushRelabelError> {
        if self.preset != PushRelabelPreset::RelabelToFront || new_height == old_height {
            return Ok(None);
        }
        let position = self
            .list_cursor
            .checked_sub(1)
            .ok_or(PushRelabelError::Invariant)?;
        if self.discharge_list.get(position).copied() != Some(node) || new_height < old_height {
            return Err(PushRelabelError::Invariant);
        }
        self.discharge_list.remove(position);
        self.discharge_list.insert(0, node);
        self.list_cursor = usize::from(!self.discharge_list.is_empty());
        Ok(Some(self.discharge_list.clone()))
    }
}

fn trace_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: PushRelabelPreset,
) -> Result<PushRelabelTraceResult, PushRelabelError> {
    let run = solve_internal(graph, source, sink, true, preset)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PushRelabelError::Invariant)?;
    Ok(PushRelabelTraceResult {
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
    preset: PushRelabelPreset,
) -> Result<PushRelabelInternalRun, PushRelabelError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, source, sink, record_trace, preset, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
    preset: PushRelabelPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<PushRelabelInternalRun, PushRelabelError> {
    if graph.nodes().len() > PUSH_RELABEL_MAX_NODES || graph.edges().len() > PUSH_RELABEL_MAX_EDGES
    {
        return Err(PushRelabelError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let initial_excess = excess_from_flows(graph, state.flows())?;
    let base_snapshot = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        initial_excess.clone(),
        FlowTraceMetrics::default(),
    );
    let mut recorder = if record_trace {
        Some(FlowTraceRecorder::new(graph, base_snapshot)?)
    } else {
        None
    };
    let mut kernel = initial_kernel(graph, initial_excess);
    let mut metrics = PushRelabelMetrics::default();
    initialize_preflow(graph, &mut state, source, sink, &mut kernel, &mut metrics)?;
    validate_preflow(graph, &state, source, sink, &kernel)?;
    let mut scheduler = ActiveScheduler::new(graph, &kernel, source, sink, preset);
    let initialization_order = if preset == PushRelabelPreset::RelabelToFront {
        scheduler.discharge_list.clone()
    } else {
        active_nodes(graph, &kernel, source, sink)
    };
    record_event(
        recorder.as_mut(),
        graph,
        &state,
        &kernel,
        metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::Initialize,
            search_order: initialization_order,
            active_path: Vec::new(),
            detail: None,
        },
    )?;

    run_selected_preset(
        graph,
        &mut state,
        source,
        sink,
        &mut kernel,
        &mut scheduler,
        preset,
        &mut metrics,
        &mut recorder,
    )?;

    validate_preflow(graph, &state, source, sink, &kernel)?;
    if !active_nodes(graph, &kernel, source, sink).is_empty()
        || graph
            .node_indices()
            .any(|node| node != source && node != sink && kernel.excess[node.as_usize()] != 0)
    {
        return Err(PushRelabelError::Invariant);
    }
    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_event(
        recorder.as_mut(),
        graph,
        &state,
        &kernel,
        metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::Optimal,
            search_order: Vec::new(),
            active_path: Vec::new(),
            detail: None,
        },
    )?;
    Ok(PushRelabelInternalRun {
        result: PushRelabelResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn initial_kernel(graph: &FlowNetwork, excess: Vec<i128>) -> PushRelabelKernel {
    let node_count = graph.nodes().len();
    let mut height_counts = vec![0; node_count.saturating_mul(2)];
    if let Some(count) = height_counts.first_mut() {
        *count = node_count;
    }
    PushRelabelKernel {
        heights: vec![0; node_count],
        height_counts,
        excess,
        current_arcs: vec![0; node_count],
        outgoing_ids: graph
            .node_indices()
            .map(|node| stable_outgoing_ids(graph, node))
            .collect(),
        incoming_ids: graph
            .node_indices()
            .map(|node| stable_incoming_ids(graph, node))
            .collect(),
        logical_transitions: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_selected_preset(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    scheduler: &mut ActiveScheduler,
    preset: PushRelabelPreset,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    match preset {
        PushRelabelPreset::PartialAugmentRelabel => run_partial_augment_relabel(
            graph, state, source, sink, kernel, scheduler, metrics, recorder,
        ),
        PushRelabelPreset::ExcessScaling => {
            run_excess_scaling(graph, state, source, sink, kernel, metrics, recorder)
        }
        PushRelabelPreset::Generic
        | PushRelabelPreset::CurrentArc
        | PushRelabelPreset::Fifo
        | PushRelabelPreset::RelabelToFront
        | PushRelabelPreset::HighestLabel
        | PushRelabelPreset::GlobalRelabel
        | PushRelabelPreset::GapRelabel => run_active_discharges(
            graph, state, source, sink, kernel, scheduler, preset, metrics, recorder,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_excess_scaling(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let preset = PushRelabelPreset::ExcessScaling;
    let Some(mut delta) = initial_excess_scale(graph, kernel, source, sink)? else {
        return Ok(());
    };

    while delta >= 1 {
        kernel.count_transition()?;
        metrics.scaling_phases = metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
        record_event(
            recorder.as_mut(),
            graph,
            state,
            kernel,
            *metrics,
            TraceTransition {
                preset,
                kind: PushRelabelEventKind::ScalePhase,
                search_order: scaled_active_nodes(graph, kernel, source, sink, delta),
                active_path: Vec::new(),
                detail: Some(("delta", delta)),
            },
        )?;

        while let Some(node) = select_scaled_active(graph, kernel, source, sink, delta) {
            process_scaled_active(
                graph, state, source, sink, node, delta, kernel, metrics, recorder,
            )?;
        }

        if graph.node_indices().any(|node| {
            node != source && node != sink && kernel.excess[node.as_usize()] > delta / 2
        }) {
            return Err(PushRelabelError::Invariant);
        }
        delta /= 2;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_scaled_active(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    node: NodeIndex,
    delta: i128,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let preset = PushRelabelPreset::ExcessScaling;
    kernel.count_transition()?;
    metrics.active_vertex_selections = metrics
        .active_vertex_selections
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    record_event(
        recorder.as_mut(),
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::SelectScaledActive,
            // The scale-phase event already exposes the complete eligible
            // band. Selection is one local operation, so emphasize only the
            // vertex that will actually be discharged. Repeating the whole
            // band here made adjacent Detail events visually identical and
            // over-emphasized unrelated vertices.
            search_order: vec![node],
            active_path: Vec::new(),
            detail: Some(("excess", kernel.excess[node.as_usize()])),
        },
    )?;

    let Some(arc) = next_admissible_arc(state, node, kernel, metrics, recorder.as_mut(), preset)?
    else {
        return relabel(
            graph,
            state,
            source,
            sink,
            node,
            kernel,
            preset,
            metrics,
            recorder.as_mut(),
        );
    };
    let source_excess = kernel.excess[node.as_usize()];
    let head_room = scaled_head_room(kernel, arc.to, source, sink, delta)?;
    let amount_i128 = source_excess.min(i128::from(arc.capacity)).min(head_room);
    let amount = u64::try_from(amount_i128).map_err(|_| PushRelabelError::ArithmeticOverflow)?;
    let saturating = amount == arc.capacity;
    if amount == 0 || (!saturating && i128::from(amount).saturating_mul(2) < delta) {
        return Err(PushRelabelError::Invariant);
    }
    kernel.count_transition()?;
    state.augment(std::slice::from_ref(&arc.id), amount)?;
    move_excess(kernel, node, arc.to, amount)?;
    count_push(metrics, saturating)?;
    if saturating {
        kernel.current_arcs[node.as_usize()] = kernel.current_arcs[node.as_usize()]
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
    }
    validate_excess_dominator(graph, kernel, source, sink, delta)?;
    record_event(
        recorder.as_mut(),
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::Push,
            search_order: vec![node, arc.to],
            active_path: vec![arc.id],
            detail: Some(("delta", i128::from(amount))),
        },
    )
}

fn scaled_head_room(
    kernel: &PushRelabelKernel,
    head: NodeIndex,
    source: NodeIndex,
    sink: NodeIndex,
    delta: i128,
) -> Result<i128, PushRelabelError> {
    if head == source || head == sink {
        return Ok(i128::MAX);
    }
    let head_excess = kernel.excess[head.as_usize()];
    if head_excess < 0 || head_excess > delta / 2 {
        return Err(PushRelabelError::Invariant);
    }
    delta
        .checked_sub(head_excess)
        .ok_or(PushRelabelError::ArithmeticOverflow)
}

fn initial_excess_scale(
    graph: &FlowNetwork,
    kernel: &PushRelabelKernel,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<Option<i128>, PushRelabelError> {
    let maximum = graph
        .node_indices()
        .filter(|&node| node != source && node != sink)
        .map(|node| kernel.excess[node.as_usize()])
        .max()
        .unwrap_or(0);
    if maximum <= 0 {
        return Ok(None);
    }
    let maximum = u128::try_from(maximum).map_err(|_| PushRelabelError::ArithmeticOverflow)?;
    let delta = maximum
        .checked_next_power_of_two()
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    i128::try_from(delta)
        .map(Some)
        .map_err(|_| PushRelabelError::ArithmeticOverflow)
}

fn scaled_active_nodes(
    graph: &FlowNetwork,
    kernel: &PushRelabelKernel,
    source: NodeIndex,
    sink: NodeIndex,
    delta: i128,
) -> Vec<NodeIndex> {
    let mut nodes = graph
        .node_indices()
        .filter(|&node| {
            node != source && node != sink && kernel.excess[node.as_usize()] > delta / 2
        })
        .collect::<Vec<_>>();
    nodes.sort_unstable_by_key(|&node| (kernel.heights[node.as_usize()], node));
    nodes
}

fn select_scaled_active(
    graph: &FlowNetwork,
    kernel: &PushRelabelKernel,
    source: NodeIndex,
    sink: NodeIndex,
    delta: i128,
) -> Option<NodeIndex> {
    scaled_active_nodes(graph, kernel, source, sink, delta)
        .into_iter()
        .next()
}

fn validate_excess_dominator(
    graph: &FlowNetwork,
    kernel: &PushRelabelKernel,
    source: NodeIndex,
    sink: NodeIndex,
    delta: i128,
) -> Result<(), PushRelabelError> {
    if graph
        .node_indices()
        .any(|node| node != source && node != sink && kernel.excess[node.as_usize()] > delta)
    {
        return Err(PushRelabelError::Invariant);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_active_discharges(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    scheduler: &mut ActiveScheduler,
    preset: PushRelabelPreset,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let global_interval = (graph.edges().len() as u128)
        .saturating_mul(PUSH_RELABEL_GLOBAL_RELABEL_SCAN_MULTIPLIER)
        .max(1);
    let mut next_global_scan = u128::MAX;
    if preset == PushRelabelPreset::GlobalRelabel {
        global_relabel(
            graph,
            state,
            source,
            sink,
            kernel,
            metrics,
            recorder.as_mut(),
        )?;
        next_global_scan = metrics.residual_arc_scans.saturating_add(global_interval);
    }
    while let Some(node) = scheduler.next(graph, kernel, source, sink) {
        let old_height = kernel.heights[node.as_usize()];
        discharge(
            graph,
            state,
            source,
            sink,
            node,
            kernel,
            scheduler,
            preset,
            metrics,
            recorder.as_mut(),
        )?;
        let new_height = kernel.heights[node.as_usize()];
        let Some(list_order) = scheduler.after_discharge(node, old_height, new_height)? else {
            if preset == PushRelabelPreset::GlobalRelabel
                && metrics.residual_arc_scans >= next_global_scan
            {
                global_relabel(
                    graph,
                    state,
                    source,
                    sink,
                    kernel,
                    metrics,
                    recorder.as_mut(),
                )?;
                next_global_scan = metrics.residual_arc_scans.saturating_add(global_interval);
            }
            continue;
        };
        kernel.count_transition()?;
        record_event(
            recorder.as_mut(),
            graph,
            state,
            kernel,
            *metrics,
            TraceTransition {
                preset,
                kind: PushRelabelEventKind::MoveToFront,
                search_order: list_order,
                active_path: Vec::new(),
                detail: Some(("height", new_height as i128)),
            },
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn global_relabel(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let mut recorder = recorder;
    kernel.count_transition()?;
    let node_count = graph.nodes().len();
    let unreachable_height = node_count
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    let mut distances = vec![None; node_count];
    let mut queue = VecDeque::from([sink]);
    let mut search_order = vec![sink];
    distances[sink.as_usize()] = Some(0_usize);

    while let Some(node) = queue.pop_front() {
        let distance = distances[node.as_usize()].ok_or(PushRelabelError::Invariant)?;
        for id in &kernel.incoming_ids[node.as_usize()] {
            let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
            if arc.capacity == 0 {
                continue;
            }
            count_arc_scan(
                metrics,
                recorder.as_deref_mut(),
                PushRelabelPreset::GlobalRelabel,
                id,
            )?;
            if arc.from == source || distances[arc.from.as_usize()].is_some() {
                continue;
            }
            let next_distance = distance
                .checked_add(1)
                .ok_or(PushRelabelError::ArithmeticOverflow)?;
            distances[arc.from.as_usize()] = Some(next_distance);
            search_order.push(arc.from);
            queue.push_back(arc.from);
        }
    }

    for node in graph.node_indices() {
        kernel.heights[node.as_usize()] = if node == source {
            node_count
        } else {
            distances[node.as_usize()].unwrap_or(unreachable_height)
        };
        kernel.current_arcs[node.as_usize()] = 0;
    }
    kernel.rebuild_height_counts()?;
    metrics.global_relabels = metrics
        .global_relabels
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    record_event(
        recorder,
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset: PushRelabelPreset::GlobalRelabel,
            kind: PushRelabelEventKind::GlobalRelabel,
            search_order,
            active_path: Vec::new(),
            detail: Some((
                "reachable",
                i128::try_from(
                    distances
                        .iter()
                        .enumerate()
                        .filter(|(index, distance)| {
                            *index != source.as_usize() && distance.is_some()
                        })
                        .count(),
                )
                .map_err(|_| PushRelabelError::ArithmeticOverflow)?,
            )),
        },
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_partial_augment_relabel(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    scheduler: &mut ActiveScheduler,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    while let Some(root) = scheduler.next(graph, kernel, source, sink) {
        let (path_nodes, path) =
            begin_partial_path_search(graph, state, root, kernel, metrics, recorder.as_mut())?;
        explore_partial_path(
            graph, state, source, sink, root, kernel, metrics, recorder, path_nodes, path,
        )?;
    }
    Ok(())
}

fn begin_partial_path_search(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    root: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(Vec<NodeIndex>, Vec<ResidualArcId>), PushRelabelError> {
    kernel.count_transition()?;
    metrics.active_vertex_selections = metrics
        .active_vertex_selections
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    metrics.path_searches = metrics
        .path_searches
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    let path_nodes = vec![root];
    let path = Vec::with_capacity(PARTIAL_AUGMENT_RELABEL_PATH_LENGTH);
    record_event(
        recorder,
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset: PushRelabelPreset::PartialAugmentRelabel,
            kind: PushRelabelEventKind::SelectPath,
            search_order: path_nodes.clone(),
            active_path: path.clone(),
            detail: Some((
                "path-limit",
                i128::try_from(PARTIAL_AUGMENT_RELABEL_PATH_LENGTH)
                    .map_err(|_| PushRelabelError::ArithmeticOverflow)?,
            )),
        },
    )?;
    Ok((path_nodes, path))
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn explore_partial_path(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    root: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    mut path_nodes: Vec<NodeIndex>,
    mut path: Vec<ResidualArcId>,
) -> Result<(), PushRelabelError> {
    let preset = PushRelabelPreset::PartialAugmentRelabel;
    loop {
        let endpoint = path_nodes
            .last()
            .copied()
            .ok_or(PushRelabelError::Invariant)?;
        if let Some(kind) = partial_path_terminal_kind(endpoint, source, sink, path.len()) {
            augment_partial_path(
                graph,
                state,
                root,
                endpoint,
                kernel,
                metrics,
                recorder.as_mut(),
                path_nodes,
                path,
                kind,
            )?;
            return Ok(());
        }

        if let Some(arc) =
            next_admissible_arc(state, endpoint, kernel, metrics, recorder.as_mut(), preset)?
        {
            kernel.count_transition()?;
            path.push(arc.id);
            path_nodes.push(arc.to);
            record_event(
                recorder.as_mut(),
                graph,
                state,
                kernel,
                *metrics,
                TraceTransition {
                    preset,
                    kind: PushRelabelEventKind::AdvancePath,
                    search_order: path_nodes.clone(),
                    active_path: path.clone(),
                    detail: Some((
                        "path-length",
                        i128::try_from(path.len())
                            .map_err(|_| PushRelabelError::ArithmeticOverflow)?,
                    )),
                },
            )?;
            continue;
        }

        let new_height = relabel_height(
            graph,
            state,
            endpoint,
            kernel,
            metrics,
            recorder.as_mut(),
            preset,
        )?;
        record_event(
            recorder.as_mut(),
            graph,
            state,
            kernel,
            *metrics,
            TraceTransition {
                preset,
                kind: PushRelabelEventKind::Relabel,
                search_order: path_nodes.clone(),
                active_path: path.clone(),
                detail: Some(("height", new_height as i128)),
            },
        )?;
        if path.is_empty() {
            return Ok(());
        }

        path.pop().ok_or(PushRelabelError::Invariant)?;
        path_nodes.pop().ok_or(PushRelabelError::Invariant)?;
        let predecessor = path_nodes
            .last()
            .copied()
            .ok_or(PushRelabelError::Invariant)?;
        kernel.current_arcs[predecessor.as_usize()] = kernel.current_arcs[predecessor.as_usize()]
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
        kernel.count_transition()?;
        metrics.retreats = metrics
            .retreats
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
        record_event(
            recorder.as_mut(),
            graph,
            state,
            kernel,
            *metrics,
            TraceTransition {
                preset,
                kind: PushRelabelEventKind::RetreatPath,
                search_order: path_nodes.clone(),
                active_path: path.clone(),
                detail: Some((
                    "path-length",
                    i128::try_from(path.len()).map_err(|_| PushRelabelError::ArithmeticOverflow)?,
                )),
            },
        )?;
    }
}

fn partial_path_terminal_kind(
    endpoint: NodeIndex,
    source: NodeIndex,
    sink: NodeIndex,
    path_length: usize,
) -> Option<PushRelabelEventKind> {
    if endpoint == sink {
        Some(PushRelabelEventKind::AugmentToSink)
    } else if endpoint == source {
        Some(PushRelabelEventKind::ReturnToSource)
    } else if path_length == PARTIAL_AUGMENT_RELABEL_PATH_LENGTH {
        Some(PushRelabelEventKind::AugmentAtLimit)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn augment_partial_path(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    root: NodeIndex,
    endpoint: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    path_nodes: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
    kind: PushRelabelEventKind,
) -> Result<(), PushRelabelError> {
    if path.is_empty()
        || !matches!(
            kind,
            PushRelabelEventKind::AugmentAtLimit
                | PushRelabelEventKind::AugmentToSink
                | PushRelabelEventKind::ReturnToSource
        )
    {
        return Err(PushRelabelError::Invariant);
    }
    let root_excess = u64::try_from(kernel.excess[root.as_usize()])
        .map_err(|_| PushRelabelError::ArithmeticOverflow)?;
    let mut amount = root_excess;
    let mut saturating = Vec::with_capacity(path.len());
    for id in &path {
        let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
        amount = amount.min(arc.capacity);
    }
    if amount == 0 {
        return Err(PushRelabelError::Invariant);
    }
    for id in &path {
        let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
        saturating.push((arc.from, amount == arc.capacity));
    }

    kernel.count_transition()?;
    state.augment(&path, amount)?;
    move_excess(kernel, root, endpoint, amount)?;
    for (from, exhausts_arc) in saturating {
        count_push(metrics, exhausts_arc)?;
        if exhausts_arc {
            kernel.current_arcs[from.as_usize()] = kernel.current_arcs[from.as_usize()]
                .checked_add(1)
                .ok_or(PushRelabelError::ArithmeticOverflow)?;
        }
    }
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    record_event(
        recorder,
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset: PushRelabelPreset::PartialAugmentRelabel,
            kind,
            search_order: path_nodes,
            active_path: path,
            detail: Some(("delta", i128::from(amount))),
        },
    )?;
    Ok(())
}

fn initialize_preflow(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
) -> Result<(), PushRelabelError> {
    kernel.set_height(source, graph.nodes().len())?;
    for id in kernel.outgoing_ids[source.as_usize()].clone() {
        let arc = state.arc(&id).ok_or(PushRelabelError::Invariant)?;
        if arc.capacity == 0 || arc.to == source {
            continue;
        }
        kernel.count_transition()?;
        state.augment(std::slice::from_ref(&id), arc.capacity)?;
        move_excess(kernel, source, arc.to, arc.capacity)?;
        count_push(metrics, true)?;
        if arc.to != sink && kernel.excess[arc.to.as_usize()] <= 0 {
            return Err(PushRelabelError::Invariant);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn discharge(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    node: NodeIndex,
    kernel: &mut PushRelabelKernel,
    scheduler: &mut ActiveScheduler,
    preset: PushRelabelPreset,
    metrics: &mut PushRelabelMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    if !kernel.is_active(node, source, sink) {
        return Err(PushRelabelError::Invariant);
    }
    kernel.count_transition()?;
    metrics.active_vertex_selections = metrics
        .active_vertex_selections
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    metrics.discharges = metrics
        .discharges
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    record_event(
        recorder.as_deref_mut(),
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::Discharge,
            search_order: vec![node],
            active_path: Vec::new(),
            detail: Some(("excess", kernel.excess[node.as_usize()])),
        },
    )?;

    while kernel.excess[node.as_usize()] > 0 {
        let next_arc = if preset == PushRelabelPreset::Generic {
            next_admissible_arc_from_start(
                state,
                node,
                kernel,
                metrics,
                recorder.as_deref_mut(),
                preset,
            )?
        } else {
            next_admissible_arc(
                state,
                node,
                kernel,
                metrics,
                recorder.as_deref_mut(),
                preset,
            )?
        };
        if let Some(arc) = next_arc {
            let before_target_excess = kernel.excess[arc.to.as_usize()];
            let amount =
                u64::try_from(kernel.excess[node.as_usize()].min(i128::from(arc.capacity)))
                    .map_err(|_| PushRelabelError::ArithmeticOverflow)?;
            let saturating = amount == arc.capacity;
            kernel.count_transition()?;
            state.augment(std::slice::from_ref(&arc.id), amount)?;
            move_excess(kernel, node, arc.to, amount)?;
            count_push(metrics, saturating)?;
            if saturating && preset != PushRelabelPreset::Generic {
                kernel.current_arcs[node.as_usize()] = kernel.current_arcs[node.as_usize()]
                    .checked_add(1)
                    .ok_or(PushRelabelError::ArithmeticOverflow)?;
            }
            if arc.to != source
                && arc.to != sink
                && before_target_excess == 0
                && kernel.excess[arc.to.as_usize()] > 0
            {
                scheduler.activate(arc.to);
            }
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                kernel,
                *metrics,
                TraceTransition {
                    preset,
                    kind: PushRelabelEventKind::Push,
                    search_order: vec![node, arc.to],
                    active_path: vec![arc.id],
                    detail: Some(("delta", i128::from(amount))),
                },
            )?;
            continue;
        }
        relabel(
            graph,
            state,
            source,
            sink,
            node,
            kernel,
            preset,
            metrics,
            recorder.as_deref_mut(),
        )?;
    }
    Ok(())
}

fn next_admissible_arc_from_start(
    state: &ResidualState<'_>,
    node: NodeIndex,
    kernel: &PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: PushRelabelPreset,
) -> Result<Option<ResidualArc>, PushRelabelError> {
    let mut recorder = recorder;
    let outgoing = kernel
        .outgoing_ids
        .get(node.as_usize())
        .ok_or(PushRelabelError::Invariant)?;
    for id in outgoing {
        let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
        if arc.capacity == 0 {
            continue;
        }
        count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
        if kernel.heights[node.as_usize()] == kernel.heights[arc.to.as_usize()].saturating_add(1) {
            return Ok(Some(arc));
        }
    }
    Ok(None)
}

fn next_admissible_arc(
    state: &ResidualState<'_>,
    node: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: PushRelabelPreset,
) -> Result<Option<ResidualArc>, PushRelabelError> {
    let mut recorder = recorder;
    let outgoing = kernel
        .outgoing_ids
        .get(node.as_usize())
        .ok_or(PushRelabelError::Invariant)?;
    while kernel.current_arcs[node.as_usize()] < outgoing.len() {
        let id = &outgoing[kernel.current_arcs[node.as_usize()]];
        let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
        if arc.capacity > 0 {
            count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
            if kernel.heights[node.as_usize()]
                == kernel.heights[arc.to.as_usize()].saturating_add(1)
            {
                return Ok(Some(arc));
            }
        }
        kernel.current_arcs[node.as_usize()] = kernel.current_arcs[node.as_usize()]
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn relabel(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    node: NodeIndex,
    kernel: &mut PushRelabelKernel,
    preset: PushRelabelPreset,
    metrics: &mut PushRelabelMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let old_height = kernel.heights[node.as_usize()];
    let new_height = relabel_height(
        graph,
        state,
        node,
        kernel,
        metrics,
        recorder.as_deref_mut(),
        preset,
    )?;
    record_event(
        recorder.as_deref_mut(),
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::Relabel,
            search_order: vec![node],
            active_path: Vec::new(),
            detail: Some(("height", new_height as i128)),
        },
    )?;
    maybe_gap_relabel(
        graph, state, source, sink, old_height, kernel, preset, metrics, recorder,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_gap_relabel(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    gap_level: usize,
    kernel: &mut PushRelabelKernel,
    preset: PushRelabelPreset,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), PushRelabelError> {
    let node_count = graph.nodes().len();
    if preset != PushRelabelPreset::GapRelabel
        || gap_level >= node_count
        || kernel
            .height_counts
            .get(gap_level)
            .copied()
            .ok_or(PushRelabelError::Invariant)?
            != 0
    {
        return Ok(());
    }
    let replacement = node_count
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    let changed = graph
        .node_indices()
        .filter(|&candidate| {
            candidate != source
                && candidate != sink
                && kernel.heights[candidate.as_usize()] > gap_level
                && kernel.heights[candidate.as_usize()] < node_count
        })
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(());
    }
    kernel.count_transition()?;
    for &candidate in &changed {
        kernel.set_height(candidate, replacement)?;
        kernel.current_arcs[candidate.as_usize()] = 0;
    }
    metrics.gap_relabels = metrics
        .gap_relabels
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    record_event(
        recorder,
        graph,
        state,
        kernel,
        *metrics,
        TraceTransition {
            preset,
            kind: PushRelabelEventKind::GapRelabel,
            search_order: changed,
            active_path: Vec::new(),
            detail: Some(("gap-level", gap_level as i128)),
        },
    )?;
    Ok(())
}

fn relabel_height(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    node: NodeIndex,
    kernel: &mut PushRelabelKernel,
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: PushRelabelPreset,
) -> Result<usize, PushRelabelError> {
    let mut recorder = recorder;
    kernel.count_transition()?;
    let old_height = kernel.heights[node.as_usize()];
    let mut minimum = None;
    for id in &kernel.outgoing_ids[node.as_usize()] {
        let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
        if arc.capacity == 0 {
            continue;
        }
        count_arc_scan(metrics, recorder.as_deref_mut(), preset, id)?;
        minimum = Some(
            minimum.map_or(kernel.heights[arc.to.as_usize()], |height: usize| {
                height.min(kernel.heights[arc.to.as_usize()])
            }),
        );
    }
    let new_height = minimum
        .and_then(|height| height.checked_add(1))
        .ok_or(PushRelabelError::Invariant)?;
    let height_ceiling = graph
        .nodes()
        .len()
        .checked_mul(2)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    if new_height <= old_height || new_height >= height_ceiling {
        return Err(PushRelabelError::Invariant);
    }
    kernel.set_height(node, new_height)?;
    kernel.current_arcs[node.as_usize()] = 0;
    metrics.relabels = metrics
        .relabels
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    Ok(new_height)
}

fn move_excess(
    kernel: &mut PushRelabelKernel,
    from: NodeIndex,
    to: NodeIndex,
    amount: u64,
) -> Result<(), PushRelabelError> {
    let amount = i128::from(amount);
    kernel.excess[from.as_usize()] = kernel.excess[from.as_usize()]
        .checked_sub(amount)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    kernel.excess[to.as_usize()] = kernel.excess[to.as_usize()]
        .checked_add(amount)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    Ok(())
}

fn count_arc_scan(
    metrics: &mut PushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    preset: PushRelabelPreset,
    arc: &ResidualArcId,
) -> Result<(), PushRelabelError> {
    if metrics.residual_arc_scans >= PUSH_RELABEL_MAX_RESIDUAL_ARC_SCANS {
        return Err(PushRelabelError::WorkLimit);
    }
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    if let Some(recorder) = recorder {
        recorder.record_metric_observation(
            FlowTraceEventMetadata {
                catalog_id: preset.inspect_catalog_id(),
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "push-relabel:inspect-residual-arc",
            },
            FlowTraceMetricId::ResidualArcScans,
            FlowTraceEntityRef::ResidualArc(arc.clone()),
        )?;
    }
    Ok(())
}

fn count_push(metrics: &mut PushRelabelMetrics, saturating: bool) -> Result<(), PushRelabelError> {
    metrics.pushes = metrics
        .pushes
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    let counter = if saturating {
        &mut metrics.saturating_pushes
    } else {
        &mut metrics.nonsaturating_pushes
    };
    *counter = counter
        .checked_add(1)
        .ok_or(PushRelabelError::ArithmeticOverflow)?;
    Ok(())
}

fn excess_from_flows(graph: &FlowNetwork, flows: &[u64]) -> Result<Vec<i128>, PushRelabelError> {
    divergences(graph, flows)?
        .into_iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(PushRelabelError::ArithmeticOverflow)
        })
        .collect()
}

fn active_nodes(
    graph: &FlowNetwork,
    kernel: &PushRelabelKernel,
    source: NodeIndex,
    sink: NodeIndex,
) -> Vec<NodeIndex> {
    graph
        .node_indices()
        .filter(|&node| kernel.is_active(node, source, sink))
        .collect()
}

fn validate_preflow(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &PushRelabelKernel,
) -> Result<(), PushRelabelError> {
    if kernel.heights.len() != graph.nodes().len()
        || kernel.height_counts.len() != graph.nodes().len().saturating_mul(2)
        || kernel.excess.len() != graph.nodes().len()
        || kernel.current_arcs.len() != graph.nodes().len()
        || kernel.outgoing_ids.len() != graph.nodes().len()
        || kernel.incoming_ids.len() != graph.nodes().len()
        || kernel.heights[source.as_usize()] != graph.nodes().len()
        || kernel.heights[sink.as_usize()] != 0
        || graph
            .node_indices()
            .any(|node| node != source && node != sink && kernel.excess[node.as_usize()] < 0)
    {
        return Err(PushRelabelError::Invariant);
    }
    let reconstructed = excess_from_flows(graph, state.flows())?;
    if reconstructed != kernel.excess
        || kernel
            .excess
            .iter()
            .try_fold(0_i128, |sum, value| sum.checked_add(*value))
            .ok_or(PushRelabelError::ArithmeticOverflow)?
            != 0
    {
        return Err(PushRelabelError::Invariant);
    }
    let mut expected_height_counts = vec![0_usize; kernel.height_counts.len()];
    for &height in &kernel.heights {
        let count = expected_height_counts
            .get_mut(height)
            .ok_or(PushRelabelError::Invariant)?;
        *count = count
            .checked_add(1)
            .ok_or(PushRelabelError::ArithmeticOverflow)?;
    }
    if expected_height_counts != kernel.height_counts {
        return Err(PushRelabelError::Invariant);
    }
    for node in graph.node_indices() {
        for id in &kernel.outgoing_ids[node.as_usize()] {
            let arc = state.arc(id).ok_or(PushRelabelError::Invariant)?;
            if arc.capacity > 0
                && kernel.heights[arc.from.as_usize()]
                    > kernel.heights[arc.to.as_usize()].saturating_add(1)
            {
                return Err(PushRelabelError::Invariant);
            }
        }
    }
    Ok(())
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

fn stable_incoming_ids(graph: &FlowNetwork, node: NodeIndex) -> Vec<ResidualArcId> {
    let mut ids =
        Vec::with_capacity(graph.incoming_edges(node).len() + graph.outgoing_edges(node).len());
    ids.extend(graph.incoming_edges(node).iter().filter_map(|&index| {
        graph
            .edge(index)
            .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward))
    }));
    ids.extend(graph.outgoing_edges(node).iter().filter_map(|&index| {
        graph
            .edge(index)
            .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse))
    }));
    ids.sort_unstable();
    ids
}

#[derive(Clone, Copy)]
enum PushRelabelEventKind {
    Initialize,
    ScalePhase,
    SelectScaledActive,
    Discharge,
    Push,
    Relabel,
    MoveToFront,
    SelectPath,
    AdvancePath,
    RetreatPath,
    AugmentAtLimit,
    AugmentToSink,
    ReturnToSource,
    GlobalRelabel,
    GapRelabel,
    Optimal,
}

struct TraceTransition {
    preset: PushRelabelPreset,
    kind: PushRelabelEventKind,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    detail: Option<(&'static str, i128)>,
}

fn record_event(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    kernel: &PushRelabelKernel,
    metrics: PushRelabelMetrics,
    transition: TraceTransition,
) -> Result<(), PushRelabelError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let TraceTransition {
        preset,
        kind,
        search_order,
        mut active_path,
        detail,
    } = transition;
    let local_focus = match (preset, kind) {
        (PushRelabelPreset::PartialAugmentRelabel, PushRelabelEventKind::AdvancePath) => {
            Some(vec![FlowTraceEntityRef::ResidualArc(
                active_path
                    .last()
                    .cloned()
                    .ok_or(PushRelabelError::Invariant)?,
            )])
        }
        (
            PushRelabelPreset::PartialAugmentRelabel,
            PushRelabelEventKind::Relabel | PushRelabelEventKind::RetreatPath,
        ) => {
            let node = search_order
                .last()
                .copied()
                .ok_or(PushRelabelError::Invariant)?;
            Some(vec![FlowTraceEntityRef::Node(
                graph.nodes()[node.as_usize()].id().clone(),
            )])
        }
        (PushRelabelPreset::ExcessScaling, PushRelabelEventKind::SelectScaledActive) => {
            let node = search_order
                .first()
                .copied()
                .ok_or(PushRelabelError::Invariant)?;
            Some(vec![FlowTraceEntityRef::Node(
                graph.nodes()[node.as_usize()].id().clone(),
            )])
        }
        _ => None,
    };
    if preset == PushRelabelPreset::CurrentArc
        && active_path.is_empty()
        && matches!(
            kind,
            PushRelabelEventKind::Discharge | PushRelabelEventKind::Relabel
        )
        && let Some(node) = search_order.first()
        && let Some(id) =
            kernel.outgoing_ids[node.as_usize()].get(kernel.current_arcs[node.as_usize()])
    {
        // A one-arc active projection makes the retained current-arc cursor
        // visible without changing the generic replay-state schema.
        active_path.push(id.clone());
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        kernel
            .heights
            .iter()
            .map(|&height| Some(height as i128))
            .collect(),
        search_order,
        active_path,
        kernel.excess.clone(),
        trace_metrics(metrics),
    );
    let metadata = event_metadata(preset, kind)?;
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
    preset: PushRelabelPreset,
    kind: PushRelabelEventKind,
) -> Result<FlowTraceEventMetadata, PushRelabelError> {
    let minimum_granularity = match kind {
        PushRelabelEventKind::Initialize
        | PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::Optimal => TraceGranularityV1::Phase,
        PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::Discharge
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GapRelabel => TraceGranularityV1::Operation,
        PushRelabelEventKind::Push
        | PushRelabelEventKind::Relabel
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath => TraceGranularityV1::Micro,
    };
    let (catalog_id, pseudocode_line) = event_identity(preset, kind)?;
    Ok(FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    })
}

fn event_identity(
    preset: PushRelabelPreset,
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    match preset {
        PushRelabelPreset::Generic => generic_event_identity(kind),
        PushRelabelPreset::CurrentArc => current_arc_event_identity(kind),
        PushRelabelPreset::Fifo => fifo_event_identity(kind),
        PushRelabelPreset::RelabelToFront => relabel_to_front_event_identity(kind),
        PushRelabelPreset::HighestLabel => highest_label_event_identity(kind),
        PushRelabelPreset::PartialAugmentRelabel => partial_augment_relabel_event_identity(kind),
        PushRelabelPreset::GlobalRelabel => global_relabel_event_identity(kind),
        PushRelabelPreset::GapRelabel => gap_relabel_event_identity(kind),
        PushRelabelPreset::ExcessScaling => excess_scaling_event_identity(kind),
    }
}

fn current_arc_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "current-arc-heuristic.initialize",
            "current-arc-heuristic:initialize-source-preflow-and-cursors",
        ),
        PushRelabelEventKind::Discharge => (
            "current-arc-heuristic.discharge",
            "current-arc-heuristic:select-lowest-stable-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "current-arc-heuristic.push",
            "current-arc-heuristic:push-and-retain-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "current-arc-heuristic.relabel",
            "current-arc-heuristic:raise-height-and-reset-current-arc",
        ),
        PushRelabelEventKind::Optimal => (
            "current-arc-heuristic.optimal",
            "current-arc-heuristic:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn excess_scaling_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "excess-scaling-push-relabel.initialize",
            "excess-scaling-push-relabel:initialize-source-preflow",
        ),
        PushRelabelEventKind::ScalePhase => (
            "excess-scaling-push-relabel.scale-phase",
            "excess-scaling-push-relabel:start-delta-phase",
        ),
        PushRelabelEventKind::SelectScaledActive => (
            "excess-scaling-push-relabel.select-scaled-active",
            "excess-scaling-push-relabel:select-minimum-label-excess-above-half-delta",
        ),
        PushRelabelEventKind::Push => (
            "excess-scaling-push-relabel.push",
            "excess-scaling-push-relabel:push-with-delta-head-room",
        ),
        PushRelabelEventKind::Relabel => (
            "excess-scaling-push-relabel.relabel",
            "excess-scaling-push-relabel:raise-height-and-reset-current-arc",
        ),
        PushRelabelEventKind::Optimal => (
            "excess-scaling-push-relabel.optimal",
            "excess-scaling-push-relabel:return-certified-cut",
        ),
        PushRelabelEventKind::Discharge
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn generic_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "generic-push-relabel.initialize",
            "generic-push-relabel:initialize-source-preflow",
        ),
        PushRelabelEventKind::Discharge => (
            "generic-push-relabel.discharge",
            "generic-push-relabel:select-lowest-stable-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "generic-push-relabel.push",
            "generic-push-relabel:rescan-and-push-on-admissible-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "generic-push-relabel.relabel",
            "generic-push-relabel:raise-height-to-residual-neighbor",
        ),
        PushRelabelEventKind::Optimal => (
            "generic-push-relabel.optimal",
            "generic-push-relabel:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn fifo_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "fifo-push-relabel.initialize",
            "fifo-push-relabel:initialize-source-preflow-and-queue",
        ),
        PushRelabelEventKind::Discharge => (
            "fifo-push-relabel.discharge",
            "fifo-push-relabel:dequeue-and-discharge-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "fifo-push-relabel.push",
            "fifo-push-relabel:push-and-enqueue-new-active-head",
        ),
        PushRelabelEventKind::Relabel => (
            "fifo-push-relabel.relabel",
            "fifo-push-relabel:raise-height-and-reset-current-arc",
        ),
        PushRelabelEventKind::Optimal => (
            "fifo-push-relabel.optimal",
            "fifo-push-relabel:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn relabel_to_front_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "relabel-to-front.initialize",
            "relabel-to-front:initialize-source-preflow-and-discharge-list",
        ),
        PushRelabelEventKind::Discharge => (
            "relabel-to-front.discharge",
            "relabel-to-front:scan-list-and-discharge-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "relabel-to-front.push",
            "relabel-to-front:push-on-admissible-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "relabel-to-front.relabel",
            "relabel-to-front:raise-height-and-reset-current-arc",
        ),
        PushRelabelEventKind::MoveToFront => (
            "relabel-to-front.move-to-front",
            "relabel-to-front:move-relabeled-vertex-to-list-front",
        ),
        PushRelabelEventKind::Optimal => (
            "relabel-to-front.optimal",
            "relabel-to-front:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn highest_label_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "highest-label-push-relabel.initialize",
            "highest-label-push-relabel:initialize-source-preflow-and-buckets",
        ),
        PushRelabelEventKind::Discharge => (
            "highest-label-push-relabel.discharge",
            "highest-label-push-relabel:select-highest-stable-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "highest-label-push-relabel.push",
            "highest-label-push-relabel:push-on-admissible-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "highest-label-push-relabel.relabel",
            "highest-label-push-relabel:raise-height-and-update-bucket",
        ),
        PushRelabelEventKind::Optimal => (
            "highest-label-push-relabel.optimal",
            "highest-label-push-relabel:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn partial_augment_relabel_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "partial-augment-relabel-max-flow.initialize",
            "partial-augment-relabel-max-flow:initialize-source-preflow",
        ),
        PushRelabelEventKind::SelectPath => (
            "partial-augment-relabel-max-flow.select",
            "partial-augment-relabel-max-flow:select-highest-active-root-with-k-4",
        ),
        PushRelabelEventKind::AdvancePath => (
            "partial-augment-relabel-max-flow.advance",
            "partial-augment-relabel-max-flow:extend-along-admissible-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "partial-augment-relabel-max-flow.relabel",
            "partial-augment-relabel-max-flow:relabel-search-endpoint",
        ),
        PushRelabelEventKind::RetreatPath => (
            "partial-augment-relabel-max-flow.retreat",
            "partial-augment-relabel-max-flow:return-false-and-try-next-current-arc",
        ),
        PushRelabelEventKind::AugmentAtLimit => (
            "partial-augment-relabel-max-flow.augment-at-limit",
            "partial-augment-relabel-max-flow:push-sequence-on-length-4-path",
        ),
        PushRelabelEventKind::AugmentToSink => (
            "partial-augment-relabel-max-flow.augment-to-sink",
            "partial-augment-relabel-max-flow:push-sequence-to-sink",
        ),
        PushRelabelEventKind::ReturnToSource => (
            "partial-augment-relabel-max-flow.return-to-source",
            "partial-augment-relabel-max-flow:recover-feasible-flow-at-source",
        ),
        PushRelabelEventKind::Optimal => (
            "partial-augment-relabel-max-flow.optimal",
            "partial-augment-relabel-max-flow:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::Discharge
        | PushRelabelEventKind::Push
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::GlobalRelabel
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn global_relabel_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "global-relabel-heuristic.initialize",
            "global-relabel-heuristic:initialize-source-preflow",
        ),
        PushRelabelEventKind::GlobalRelabel => (
            "global-relabel-heuristic.global-relabel",
            "global-relabel-heuristic:reverse-bfs-exact-sink-distances",
        ),
        PushRelabelEventKind::Discharge => (
            "global-relabel-heuristic.discharge",
            "global-relabel-heuristic:select-highest-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "global-relabel-heuristic.push",
            "global-relabel-heuristic:push-on-admissible-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "global-relabel-heuristic.relabel",
            "global-relabel-heuristic:raise-height-and-reset-current-arc",
        ),
        PushRelabelEventKind::Optimal => (
            "global-relabel-heuristic.optimal",
            "global-relabel-heuristic:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GapRelabel => return Err(PushRelabelError::Invariant),
    })
}

fn gap_relabel_event_identity(
    kind: PushRelabelEventKind,
) -> Result<(&'static str, &'static str), PushRelabelError> {
    Ok(match kind {
        PushRelabelEventKind::Initialize => (
            "gap-relabel-heuristic.initialize",
            "gap-relabel-heuristic:initialize-source-preflow-and-height-counts",
        ),
        PushRelabelEventKind::Discharge => (
            "gap-relabel-heuristic.discharge",
            "gap-relabel-heuristic:select-highest-active-vertex",
        ),
        PushRelabelEventKind::Push => (
            "gap-relabel-heuristic.push",
            "gap-relabel-heuristic:push-on-admissible-current-arc",
        ),
        PushRelabelEventKind::Relabel => (
            "gap-relabel-heuristic.relabel",
            "gap-relabel-heuristic:raise-height-and-update-level-count",
        ),
        PushRelabelEventKind::GapRelabel => (
            "gap-relabel-heuristic.gap-relabel",
            "gap-relabel-heuristic:raise-above-empty-level-to-n-plus-one",
        ),
        PushRelabelEventKind::Optimal => (
            "gap-relabel-heuristic.optimal",
            "gap-relabel-heuristic:return-certified-cut",
        ),
        PushRelabelEventKind::ScalePhase
        | PushRelabelEventKind::SelectScaledActive
        | PushRelabelEventKind::MoveToFront
        | PushRelabelEventKind::SelectPath
        | PushRelabelEventKind::AdvancePath
        | PushRelabelEventKind::RetreatPath
        | PushRelabelEventKind::AugmentAtLimit
        | PushRelabelEventKind::AugmentToSink
        | PushRelabelEventKind::ReturnToSource
        | PushRelabelEventKind::GlobalRelabel => return Err(PushRelabelError::Invariant),
    })
}

const fn trace_metrics(metrics: PushRelabelMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.path_searches as u128,
        scaling_phases: metrics.scaling_phases as u128,
        blocking_flow_phases: 0,
        relabels: metrics.relabels as u128,
        retreats: metrics.retreats as u128,
        reverse_bfs_runs: metrics.global_relabels as u128,
        gap_terminations: metrics.gap_relabels as u128,
        pushes: metrics.pushes as u128,
        saturating_pushes: metrics.saturating_pushes as u128,
        nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
        discharges: metrics.discharges as u128,
        active_vertex_selections: metrics.active_vertex_selections as u128,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTracePatch, apply_trace_event};

    fn graph(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let unresolved = edges
            .iter()
            .map(|&(id, from, to, lower, capacity)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge id"),
                from: NodeId::parse(from).expect("tail"),
                to: NodeId::parse(to).expect("head"),
                lower,
                capacity,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, unresolved).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    fn assert_push_partition(metrics: PushRelabelMetrics) {
        assert_eq!(
            metrics.pushes,
            metrics.saturating_pushes + metrics.nonsaturating_pushes
        );
    }

    #[test]
    fn all_active_policies_match_edmonds_karp_and_expose_local_work() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "d", "t"],
            &[
                ("e1", "s", "a", 0, 10),
                ("e2", "s", "c", 0, 10),
                ("e3", "a", "b", 0, 4),
                ("e4", "a", "c", 0, 2),
                ("e5", "a", "d", 0, 8),
                ("e6", "b", "t", 0, 10),
                ("e7", "c", "d", 0, 9),
                ("e8", "d", "b", 0, 6),
                ("e9", "d", "t", 0, 10),
            ],
        );
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        let results = [
            solve_generic_push_relabel(&graph, source, sink).expect("generic"),
            solve_current_arc_push_relabel(&graph, source, sink).expect("current arc"),
            solve_fifo_push_relabel(&graph, source, sink).expect("fifo"),
            solve_relabel_to_front(&graph, source, sink).expect("relabel-to-front"),
            solve_highest_label_push_relabel(&graph, source, sink).expect("highest"),
            solve_partial_augment_relabel(&graph, source, sink).expect("partial augment-relabel"),
            solve_global_relabel_push_relabel(&graph, source, sink).expect("global relabel"),
            solve_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel"),
            solve_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling"),
        ];
        for result in results {
            assert_eq!(result.certificate, expected.certificate);
            assert!(result.metrics.pushes > 0);
            assert_push_partition(result.metrics);
        }
    }

    #[test]
    fn current_arc_preserves_result_and_avoids_generic_rescans() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "d", "t"],
            &[
                ("sa", "s", "a", 0, 10),
                ("sb", "s", "b", 0, 8),
                ("ac", "a", "c", 0, 4),
                ("ad", "a", "d", 0, 6),
                ("bc", "b", "c", 0, 5),
                ("bd", "b", "d", 0, 3),
                ("cd", "c", "d", 0, 2),
                ("ct", "c", "t", 0, 7),
                ("dt", "d", "t", 0, 11),
            ],
        );
        let generic = solve_generic_push_relabel(&graph, source, sink).expect("generic");
        let current = solve_current_arc_push_relabel(&graph, source, sink).expect("current arc");
        assert_eq!(current.flows, generic.flows);
        assert_eq!(current.certificate, generic.certificate);
        assert!(
            current.metrics.residual_arc_scans < generic.metrics.residual_arc_scans,
            "fixture must demonstrate retained-cursor work: current={} generic={}",
            current.metrics.residual_arc_scans,
            generic.metrics.residual_arc_scans
        );

        let traced =
            trace_current_arc_push_relabel(&graph, source, sink).expect("current-arc trace");
        assert_eq!(traced.result, current);
        assert!(traced.events.iter().all(|event| {
            event
                .catalog_id
                .starts_with(PushRelabelPreset::CurrentArc.prefix())
        }));
        let mut replay = traced.base_snapshot.clone();
        let mut cursor_events = 0_usize;
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("current-arc replay");
            if !matches!(
                event.catalog_id.as_str(),
                "current-arc-heuristic.discharge" | "current-arc-heuristic.relabel"
            ) {
                continue;
            }
            cursor_events += 1;
            let selected = replay.search_order.first().expect("selected node");
            let cursor = replay.active_path.first().expect("visible current arc");
            assert_eq!(replay.active_path.len(), 1);
            let edge = graph
                .edge(
                    graph
                        .edge_index(cursor.original_edge())
                        .expect("cursor original edge"),
                )
                .expect("cursor edge");
            let tail = match cursor.direction() {
                ResidualDirection::Forward => edge.from(),
                ResidualDirection::Reverse => edge.to(),
            };
            assert_eq!(graph.node(tail).expect("cursor tail").id(), selected);
        }
        assert!(cursor_events > 0);
    }

    #[test]
    fn preserves_lower_bounds_parallel_opposite_and_self_loop_edges() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("lower", "s", "a", 2, 6),
                ("parallel", "s", "a", 0, 4),
                ("forward", "a", "b", 0, 7),
                ("opposite", "b", "a", 0, 3),
                ("to-t", "b", "t", 2, 8),
                ("direct", "a", "t", 0, 2),
                ("loop", "a", "a", 1, 5),
            ],
        );
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        for result in [
            solve_generic_push_relabel(&graph, source, sink).expect("generic"),
            solve_current_arc_push_relabel(&graph, source, sink).expect("current arc"),
            solve_fifo_push_relabel(&graph, source, sink).expect("fifo"),
            solve_relabel_to_front(&graph, source, sink).expect("relabel-to-front"),
            solve_highest_label_push_relabel(&graph, source, sink).expect("highest"),
            solve_partial_augment_relabel(&graph, source, sink).expect("partial augment-relabel"),
            solve_global_relabel_push_relabel(&graph, source, sink).expect("global relabel"),
            solve_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel"),
            solve_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling"),
        ] {
            assert_eq!(result.certificate, expected.certificate);
            for (edge, flow) in graph.edges().iter().zip(result.flows) {
                assert!((edge.lower()..=edge.capacity()).contains(&flow));
            }
        }
    }

    #[test]
    fn traces_replay_forward_and_reverse_for_every_policy() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 0, 5),
                ("sb", "s", "b", 0, 4),
                ("ab", "a", "b", 0, 2),
                ("at", "a", "t", 0, 3),
                ("bt", "b", "t", 0, 6),
            ],
        );
        let traces = vec![
            trace_generic_push_relabel(&graph, source, sink).expect("generic"),
            trace_current_arc_push_relabel(&graph, source, sink).expect("current arc"),
            trace_fifo_push_relabel(&graph, source, sink).expect("fifo"),
            trace_relabel_to_front(&graph, source, sink).expect("relabel-to-front"),
            trace_highest_label_push_relabel(&graph, source, sink).expect("highest"),
            trace_partial_augment_relabel(&graph, source, sink).expect("partial augment-relabel"),
            trace_global_relabel_push_relabel(&graph, source, sink).expect("global relabel"),
            trace_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel"),
            trace_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling"),
        ];
        for traced in traces {
            let scan_events = traced
                .events
                .iter()
                .filter(|event| event.catalog_id.ends_with(".inspect-residual-arc"))
                .collect::<Vec<_>>();
            assert_eq!(
                u128::try_from(scan_events.len()).expect("scan event count"),
                traced.result.metrics.residual_arc_scans
            );
            assert!(scan_events.iter().all(|event| {
                event.entity_refs.len() == 1
                    && matches!(event.entity_refs[0], FlowTraceEntityRef::ResidualArc(_))
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
            assert!(traced.events.iter().any(|event| {
                event.catalog_id.strip_suffix(".push").is_some()
                    || event.catalog_id.contains(".augment-")
            }));
        }
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_edmonds_karp() {
        for mask in 0_u64..32 {
            let capacities = |shift: u32| 1 + ((mask >> shift) & 3);
            let (graph, source, sink) = graph(
                &["s", "a", "b", "c", "t"],
                &[
                    ("sa", "s", "a", 0, capacities(0)),
                    ("sb", "s", "b", 0, capacities(1)),
                    ("ac", "a", "c", 0, capacities(2)),
                    ("bc", "b", "c", 0, capacities(3)),
                    ("ab", "a", "b", 0, capacities(4)),
                    ("ba", "b", "a", 0, capacities(0)),
                    ("ct", "c", "t", 0, capacities(1) + capacities(2)),
                    ("at", "a", "t", 0, capacities(3)),
                ],
            );
            let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            for result in [
                solve_generic_push_relabel(&graph, source, sink).expect("generic"),
                solve_current_arc_push_relabel(&graph, source, sink).expect("current arc"),
                solve_fifo_push_relabel(&graph, source, sink).expect("fifo"),
                solve_relabel_to_front(&graph, source, sink).expect("relabel-to-front"),
                solve_highest_label_push_relabel(&graph, source, sink).expect("highest"),
                solve_partial_augment_relabel(&graph, source, sink)
                    .expect("partial augment-relabel"),
                solve_global_relabel_push_relabel(&graph, source, sink).expect("global relabel"),
                solve_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel"),
                solve_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling"),
            ] {
                assert_eq!(result.certificate, expected.certificate, "mask={mask}");
                assert_push_partition(result.metrics);
            }
        }
    }

    #[test]
    fn event_prefixes_distinguish_active_selection_policies() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 0, 3),
                ("sb", "s", "b", 0, 2),
                ("at", "a", "t", 0, 3),
                ("bt", "b", "t", 0, 2),
            ],
        );
        for (prefix, traced) in vec![
            (
                PushRelabelPreset::Generic.prefix(),
                trace_generic_push_relabel(&graph, source, sink).expect("generic"),
            ),
            (
                PushRelabelPreset::CurrentArc.prefix(),
                trace_current_arc_push_relabel(&graph, source, sink).expect("current arc"),
            ),
            (
                PushRelabelPreset::Fifo.prefix(),
                trace_fifo_push_relabel(&graph, source, sink).expect("fifo"),
            ),
            (
                PushRelabelPreset::RelabelToFront.prefix(),
                trace_relabel_to_front(&graph, source, sink).expect("relabel-to-front"),
            ),
            (
                PushRelabelPreset::HighestLabel.prefix(),
                trace_highest_label_push_relabel(&graph, source, sink).expect("highest"),
            ),
            (
                PushRelabelPreset::PartialAugmentRelabel.prefix(),
                trace_partial_augment_relabel(&graph, source, sink)
                    .expect("partial augment-relabel"),
            ),
            (
                PushRelabelPreset::GlobalRelabel.prefix(),
                trace_global_relabel_push_relabel(&graph, source, sink).expect("global relabel"),
            ),
            (
                PushRelabelPreset::GapRelabel.prefix(),
                trace_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel"),
            ),
            (
                PushRelabelPreset::ExcessScaling.prefix(),
                trace_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling"),
            ),
        ] {
            assert!(
                traced
                    .events
                    .iter()
                    .all(|event| event.catalog_id.starts_with(prefix))
            );
            if prefix == "relabel-to-front" {
                assert!(
                    traced
                        .events
                        .iter()
                        .any(|event| event.catalog_id == "relabel-to-front.move-to-front")
                );
            }
        }
    }

    #[test]
    fn excess_scaling_trace_exposes_delta_band_and_push_invariants() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "t"],
            &[
                ("sa", "s", "a", 0, 13),
                ("sb", "s", "b", 0, 6),
                ("ac", "a", "c", 0, 9),
                ("bc", "b", "c", 0, 6),
                ("ab", "a", "b", 0, 5),
                ("ct", "c", "t", 0, 11),
                ("at", "a", "t", 0, 4),
            ],
        );
        let traced =
            trace_excess_scaling_push_relabel(&graph, source, sink).expect("excess scaling");
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(traced.result.certificate, expected.certificate);

        let mut replay = traced.base_snapshot.clone();
        let mut current_delta = None;
        let mut phase_count = 0_u64;
        for event in &traced.events {
            let before = replay.clone();
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            match event.catalog_id.as_str() {
                "excess-scaling-push-relabel.scale-phase" => {
                    let detail = event.detail.as_ref().expect("scale detail");
                    assert_eq!(detail.label, "delta");
                    let delta = detail.value;
                    assert!(delta > 0);
                    assert!(
                        u128::try_from(delta)
                            .expect("positive delta")
                            .is_power_of_two()
                    );
                    if let Some(previous) = current_delta {
                        assert_eq!(delta, previous / 2);
                    }
                    current_delta = Some(delta);
                    phase_count += 1;
                }
                "excess-scaling-push-relabel.select-scaled-active" => {
                    let delta = current_delta.expect("active scale");
                    assert_eq!(replay.search_order.len(), 1);
                    let ordered = replay
                        .search_order
                        .iter()
                        .map(|id| graph.node_index(id).expect("selected node"))
                        .collect::<Vec<_>>();
                    assert!(ordered.windows(2).all(|pair| {
                        (replay.node_labels[pair[0].as_usize()], pair[0])
                            <= (replay.node_labels[pair[1].as_usize()], pair[1])
                    }));
                    assert!(
                        ordered.iter().all(|node| {
                            replay.remaining_divergence[node.as_usize()] > delta / 2
                        })
                    );
                    assert_eq!(
                        event.detail.as_ref().expect("selection detail").value,
                        replay.remaining_divergence[ordered[0].as_usize()]
                    );
                }
                "excess-scaling-push-relabel.push" => {
                    let delta = current_delta.expect("push scale");
                    let amount = event.detail.as_ref().expect("push detail").value;
                    let arc_id = replay.active_path.first().expect("pushed arc");
                    let capacity_before = before
                        .residual_capacities
                        .iter()
                        .find_map(|(id, capacity)| (id == arc_id).then_some(*capacity))
                        .expect("residual capacity");
                    if u64::try_from(amount).expect("positive push") < capacity_before {
                        assert!(amount * 2 >= delta);
                    }
                    assert!(graph.node_indices().all(|node| {
                        node == source
                            || node == sink
                            || replay.remaining_divergence[node.as_usize()] <= delta
                    }));
                }
                _ => {}
            }
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(phase_count, traced.result.metrics.scaling_phases);
        assert!(phase_count >= 4);
        assert_eq!(current_delta, Some(1));
        assert!(traced.result.metrics.active_vertex_selections > 0);
        assert_push_partition(traced.result.metrics);
    }

    #[test]
    fn partial_augment_relabel_commits_a_bounded_multi_edge_path_atomically() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "d", "e", "f", "t"],
            &[
                ("sa", "s", "a", 0, 9),
                ("ab", "a", "b", 0, 9),
                ("bc", "b", "c", 0, 9),
                ("cd", "c", "d", 0, 9),
                ("de", "d", "e", 0, 9),
                ("ef", "e", "f", 0, 9),
                ("ft", "f", "t", 0, 9),
            ],
        );
        let traced =
            trace_partial_augment_relabel(&graph, source, sink).expect("partial augment-relabel");
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(traced.result.certificate, expected.certificate);
        assert!(traced.result.metrics.augmentations >= 2);
        assert!(traced.result.metrics.path_searches >= traced.result.metrics.augmentations);
        assert!(traced.result.metrics.retreats > 0);
        assert_eq!(traced.result.metrics.discharges, 0);
        assert_push_partition(traced.result.metrics);
        assert!(
            traced
                .events
                .iter()
                .all(|event| match event.catalog_id.as_str() {
                    "partial-augment-relabel-max-flow.advance" => matches!(
                        event.entity_refs.as_slice(),
                        [FlowTraceEntityRef::ResidualArc(_)]
                    ),
                    "partial-augment-relabel-max-flow.relabel"
                    | "partial-augment-relabel-max-flow.retreat" =>
                        matches!(event.entity_refs.as_slice(), [FlowTraceEntityRef::Node(_)]),
                    _ => true,
                })
        );

        let mut replay = traced.base_snapshot.clone();
        let mut found_limit = false;
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id != "partial-augment-relabel-max-flow.augment-at-limit" {
                continue;
            }
            found_limit = true;
            assert_eq!(
                replay.active_path.len(),
                PARTIAL_AUGMENT_RELABEL_PATH_LENGTH
            );
            assert_eq!(
                event
                    .detail
                    .as_ref()
                    .map(|detail| (detail.label.as_str(), detail.value)),
                Some(("delta", 9))
            );
            assert_eq!(
                event
                    .patches
                    .iter()
                    .filter(|patch| matches!(patch, FlowTracePatch::EdgeFlow { .. }))
                    .count(),
                PARTIAL_AUGMENT_RELABEL_PATH_LENGTH
            );
            break;
        }
        assert!(found_limit);
    }

    #[test]
    fn global_relabel_publishes_exact_sink_distances_and_unreachable_height() {
        let (graph, source, sink) = graph(
            &["s", "a", "x", "y", "t"],
            &[
                ("sa", "s", "a", 0, 1),
                ("at", "a", "t", 0, 1),
                ("sx", "s", "x", 0, 1),
                ("xy", "x", "y", 0, 1),
            ],
        );
        let traced =
            trace_global_relabel_push_relabel(&graph, source, sink).expect("global relabel trace");
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(traced.result.certificate, expected.certificate);
        assert!(traced.result.metrics.global_relabels > 0);
        assert_eq!(traced.result.metrics.gap_relabels, 0);

        let mut replay = traced.base_snapshot.clone();
        let event = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "global-relabel-heuristic.global-relabel")
            .expect("global relabel event");
        let event_count = usize::try_from(event.event_id).expect("bounded event count");
        for candidate in traced.events.iter().take(event_count) {
            apply_trace_event(&graph, &mut replay, candidate, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        let label = |id: &str| {
            let index = graph
                .node_index(&NodeId::parse(id).expect("node id"))
                .expect("node index");
            replay.node_labels[index.as_usize()]
        };
        assert_eq!(label("t"), Some(0));
        assert_eq!(label("a"), Some(1));
        assert_eq!(label("s"), Some(5));
        assert_eq!(label("x"), Some(6));
        assert_eq!(label("y"), Some(6));
        assert_eq!(
            event
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("reachable", 2))
        );
    }

    #[test]
    fn gap_relabel_raises_every_vertex_above_the_empty_level_in_one_event() {
        let (graph, source, sink) = graph(
            &["s", "a", "x", "y", "t"],
            &[
                ("sa", "s", "a", 0, 1),
                ("at", "a", "t", 0, 1),
                ("sx", "s", "x", 0, 1),
                ("xy", "x", "y", 0, 1),
            ],
        );
        let traced =
            trace_gap_relabel_push_relabel(&graph, source, sink).expect("gap relabel trace");
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(traced.result.certificate, expected.certificate);
        assert_eq!(traced.result.metrics.global_relabels, 0);
        assert!(traced.result.metrics.gap_relabels > 0);

        let mut replay = traced.base_snapshot.clone();
        let mut found_gap = false;
        for event in &traced.events {
            let before = replay.node_labels.clone();
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id != "gap-relabel-heuristic.gap-relabel" {
                continue;
            }
            found_gap = true;
            let gap_level = event
                .detail
                .as_ref()
                .filter(|detail| detail.label == "gap-level")
                .map(|detail| detail.value)
                .expect("gap detail");
            let node_count = i128::try_from(graph.nodes().len()).expect("node count");
            let expected = graph
                .node_indices()
                .filter(|&node| node != source && node != sink)
                .filter(|&node| {
                    before[node.as_usize()]
                        .is_some_and(|height| height > gap_level && height < node_count)
                })
                .collect::<Vec<_>>();
            assert!(!expected.is_empty());
            for node in expected {
                assert_eq!(replay.node_labels[node.as_usize()], Some(node_count + 1));
            }
            break;
        }
        assert!(found_gap);
    }
}
