//! Goldberg--Tarjan FIFO push--relabel with bounded dynamic trees.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use thiserror::Error;

use super::data_structures::link_cut::{
    DynamicTreeEdge, DynamicTreeVertex, LinkCutError, LinkCutForest,
};
use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for the paper-complete kernel.
pub const DYNAMIC_TREE_PUSH_RELABEL_MAX_NODES: usize = 256;
/// Conservative interactive original-edge limit.
pub const DYNAMIC_TREE_PUSH_RELABEL_MAX_EDGES: usize = 2_048;
/// Deterministic current-edge and relabel scan ceiling.
pub const DYNAMIC_TREE_PUSH_RELABEL_MAX_ARC_SCANS: u128 = 10_000_000;
/// Deterministic source-push, link, send, cut, push, and relabel ceiling.
pub const DYNAMIC_TREE_PUSH_RELABEL_MAX_STATE_TRANSITIONS: u64 = 100_000;
/// Eager semantic trace ceiling.
pub const DYNAMIC_TREE_PUSH_RELABEL_MAX_TRACE_EVENTS: usize = 25_000;

/// Exact counters for the dynamic-tree push--relabel kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreePushRelabelMetrics {
    /// Paper parameter `k`, the maximum represented-tree vertex count.
    pub tree_size_limit: u64,
    /// Current-edge and relabel residual-arc inspections.
    pub residual_arc_scans: u128,
    /// Saturating pushes used to initialize the source preflow.
    pub source_pushes: u64,
    /// Ordinary pushes taken when two represented trees exceed `k`.
    pub ordinary_pushes: u64,
    /// Lazy root-path sends.
    pub tree_path_sends: u64,
    /// Admissible current edges linked into represented trees.
    pub tree_links: u64,
    /// Saturated or relabel-invalidated represented edges cut during the run.
    pub tree_cuts: u64,
    /// Represented component-size queries used by the `k` gate.
    pub component_size_queries: u64,
    /// Eligible edges rejected by the `k` gate.
    pub size_gate_rejections: u64,
    /// Lazy root-path residual updates; equal to `tree_path_sends`.
    pub path_updates: u64,
    /// Valid distance-label increases.
    pub relabels: u64,
    /// FIFO discharge operations.
    pub discharges: u64,
    /// Active roots removed from the FIFO queue.
    pub active_vertex_selections: u64,
    /// Zero-to-positive roots added to the FIFO queue.
    pub queue_additions: u64,
    /// Source, ordinary, and root-path push operations.
    pub pushes: u64,
    /// Push/send operations that exhaust at least one selected residual edge.
    pub saturating_pushes: u64,
    /// Push/send operations that first exhaust the selected excess.
    pub nonsaturating_pushes: u64,
    /// Tree edges whose implicit flow is materialized only at termination.
    pub final_tree_materializations: u64,
    /// Source-push, link, send, cut, ordinary-push, and relabel transitions.
    pub state_transitions: u64,
}

/// Certified maximum flow from dynamic-tree FIFO push--relabel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreePushRelabelResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact source-conformance counters.
    pub metrics: DynamicTreePushRelabelMetrics,
}

/// Certified result and complete reversible semantic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreePushRelabelTraceResult {
    /// Same canonical result as the fast profile.
    pub result: DynamicTreePushRelabelResult,
    /// Boundary before source-preflow initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete semantic transitions.
    pub events: Vec<FlowTraceEvent>,
    /// Independently certified optimum boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Dynamic-tree push--relabel construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreePushRelabelError {
    /// Graph exceeds the bounded interactive implementation band.
    #[error("graph exceeds dynamic-tree push-relabel admission limits")]
    AdmissionLimit,
    /// Deterministic work budget was exhausted.
    #[error("dynamic-tree push-relabel deterministic work limit exceeded")]
    WorkLimit,
    /// Lower-bound feasible-flow construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Explicit residual mutation or reconstruction failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent certificate rejected the candidate.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Exact arithmetic or a counter overflowed.
    #[error("dynamic-tree push-relabel arithmetic overflow")]
    ArithmeticOverflow,
    /// A preflow, height, queue, current-edge, or represented-tree invariant failed.
    #[error("dynamic-tree push-relabel invariant failed")]
    Invariant,
}

/// Solves maximum flow using Goldberg--Tarjan's FIFO dynamic-tree algorithm.
///
/// # Errors
///
/// Rejects out-of-band input, infeasible lower bounds, deterministic work
/// exhaustion, invariant/arithmetic failures, or a failed certificate.
pub fn solve_dynamic_tree_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DynamicTreePushRelabelResult, DynamicTreePushRelabelError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves maximum flow while publishing the paper's dynamic-tree operations.
///
/// # Errors
///
/// Returns the same failures as [`solve_dynamic_tree_push_relabel`], plus
/// bounded reversible-trace failures.
pub fn trace_dynamic_tree_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DynamicTreePushRelabelTraceResult, DynamicTreePushRelabelError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(DynamicTreePushRelabelError::Invariant)?;
    Ok(DynamicTreePushRelabelTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Solves dynamic-tree push--relabel while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_dynamic_tree_push_relabel_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreePushRelabelResult, DynamicTreePushRelabelError> {
    solve_internal_with_feasibility(graph, source, sink, false, feasibility).map(|run| run.result)
}

/// Traces dynamic-tree push--relabel while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_dynamic_tree_push_relabel_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreePushRelabelTraceResult, DynamicTreePushRelabelError> {
    let run = solve_internal_with_feasibility(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(DynamicTreePushRelabelError::Invariant)?;
    Ok(DynamicTreePushRelabelTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: DynamicTreePushRelabelResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone, Copy)]
enum EventKind {
    Initialize,
    SelectActive,
    InspectCurrent,
    InspectRelabel,
    Link,
    Send,
    CutSaturated,
    PushLargeTree,
    CutForRelabel,
    Relabel,
    DischargeComplete,
    MaterializeFinal,
    Optimal,
}

impl EventKind {
    const fn metadata(self) -> FlowTraceEventMetadata {
        let (catalog_id, minimum_granularity, pseudocode_line) = match self {
            Self::Initialize => (
                "dynamic-tree-push-relabel.initialize-source-preflow",
                TraceGranularityV1::Phase,
                "dynamic-tree-push-relabel:initialize-source-preflow",
            ),
            Self::SelectActive => (
                "dynamic-tree-push-relabel.select-fifo-root",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:dequeue-active-root",
            ),
            Self::InspectCurrent => (
                "dynamic-tree-push-relabel.inspect-current-edge",
                TraceGranularityV1::Micro,
                "dynamic-tree-push-relabel:inspect-current-edge",
            ),
            Self::InspectRelabel => (
                "dynamic-tree-push-relabel.inspect-relabel-edge",
                TraceGranularityV1::Micro,
                "dynamic-tree-push-relabel:inspect-relabel-edge",
            ),
            Self::Link => (
                "dynamic-tree-push-relabel.link-small-trees",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:link-when-size-at-most-k",
            ),
            Self::Send => (
                "dynamic-tree-push-relabel.send-root-path",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:send-lazy-root-path",
            ),
            Self::CutSaturated => (
                "dynamic-tree-push-relabel.cut-saturated-edge",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:cut-rootward-zero-edge",
            ),
            Self::PushLargeTree => (
                "dynamic-tree-push-relabel.push-large-tree-edge",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:ordinary-push-when-size-exceeds-k",
            ),
            Self::CutForRelabel => (
                "dynamic-tree-push-relabel.cut-child-before-relabel",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:cut-all-children-before-relabel",
            ),
            Self::Relabel => (
                "dynamic-tree-push-relabel.relabel-root",
                TraceGranularityV1::Operation,
                "dynamic-tree-push-relabel:relabel-active-root",
            ),
            Self::DischargeComplete => (
                "dynamic-tree-push-relabel.complete-discharge",
                TraceGranularityV1::Phase,
                "dynamic-tree-push-relabel:complete-fifo-discharge",
            ),
            Self::MaterializeFinal => (
                "dynamic-tree-push-relabel.materialize-final-tree-flows",
                TraceGranularityV1::Phase,
                "dynamic-tree-push-relabel:materialize-remaining-tree-flows",
            ),
            Self::Optimal => (
                "dynamic-tree-push-relabel.optimal",
                TraceGranularityV1::Phase,
                "dynamic-tree-push-relabel:return-certified-flow",
            ),
        };
        FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity,
            pseudocode_line,
        }
    }
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<InternalRun, DynamicTreePushRelabelError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, source, sink, with_trace, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, DynamicTreePushRelabelError> {
    if graph.nodes().len() > DYNAMIC_TREE_PUSH_RELABEL_MAX_NODES
        || graph.edges().len() > DYNAMIC_TREE_PUSH_RELABEL_MAX_EDGES
    {
        return Err(DynamicTreePushRelabelError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let explicit = ResidualState::from_flows(graph, &initial.flows)?;
    let initial_excess = excess_from_flows(graph, explicit.flows())?;
    let base_snapshot = FlowTraceSnapshot::capture(
        graph,
        &explicit,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        initial_excess.clone(),
        FlowTraceMetrics::default(),
    );
    let mut recorder = if with_trace {
        Some(FlowTraceRecorder::new(graph, base_snapshot)?)
    } else {
        None
    };
    let mut work = Work::new(graph, source, sink, explicit, initial_excess)?;
    work.initialize_source_preflow()?;
    let initialization_queue = work.active_queue();
    record(
        recorder.as_mut(),
        &mut work,
        EventKind::Initialize,
        initialization_queue,
        Vec::new(),
        None,
    )?;

    while let Some(vertex) = work.pop_active()? {
        let old_height = work.heights[vertex.as_usize()];
        let selected_excess = work.excess[vertex.as_usize()];
        record(
            recorder.as_mut(),
            &mut work,
            EventKind::SelectActive,
            vec![vertex],
            Vec::new(),
            Some(("excess", selected_excess)),
        )?;
        while work.is_active(vertex) && work.heights[vertex.as_usize()] == old_height {
            work.tree_push_relabel(vertex, recorder.as_mut())?;
        }
        work.metrics.discharges = checked_increment(work.metrics.discharges)?;
        if work.is_active(vertex) {
            work.activate(vertex)?;
        }
        let remaining_active = work.active_queue().len();
        record(
            recorder.as_mut(),
            &mut work,
            EventKind::DischargeComplete,
            Vec::new(),
            Vec::new(),
            Some((
                "remaining-active",
                i128::try_from(remaining_active)
                    .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?,
            )),
        )?;
    }

    if graph
        .node_indices()
        .any(|node| node != source && node != sink && work.excess[node.as_usize()] != 0)
    {
        return Err(DynamicTreePushRelabelError::Invariant);
    }
    let materialized = work.materialize_final_tree_flows()?;
    if materialized > 0 {
        record(
            recorder.as_mut(),
            &mut work,
            EventKind::MaterializeFinal,
            Vec::new(),
            Vec::new(),
            Some(("tree-edges", i128::from(materialized))),
        )?;
    }
    work.validate_materialized_preflow()?;
    let flows = work.explicit.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record(
        recorder.as_mut(),
        &mut work,
        EventKind::Optimal,
        Vec::new(),
        Vec::new(),
        None,
    )?;
    Ok(InternalRun {
        result: DynamicTreePushRelabelResult {
            flows,
            certificate,
            metrics: work.metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

struct Work<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    explicit: ResidualState<'graph>,
    heights: Vec<usize>,
    excess: Vec<i128>,
    outgoing: Vec<Vec<ResidualArcId>>,
    current: Vec<usize>,
    queue: VecDeque<NodeIndex>,
    queued: Vec<bool>,
    forest: LinkCutForest,
    nodes: Vec<NodeIndex>,
    vertices: Vec<DynamicTreeVertex>,
    edge_handles: Vec<DynamicTreeEdge>,
    ids: Vec<ResidualArcId>,
    slot_by_id: BTreeMap<ResidualArcId, usize>,
    active_tree_edge: Vec<bool>,
    linked_capacity: Vec<u64>,
    tree_child: Vec<Option<NodeIndex>>,
    tree_parent: Vec<Option<NodeIndex>>,
    tree_parent_edge: Vec<Option<usize>>,
    children: Vec<BTreeSet<NodeIndex>>,
    tree_size_limit: usize,
    metrics: DynamicTreePushRelabelMetrics,
}

impl<'graph> Work<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        explicit: ResidualState<'graph>,
        excess: Vec<i128>,
    ) -> Result<Self, DynamicTreePushRelabelError> {
        let node_count = graph.nodes().len();
        let mut ids = graph
            .edges()
            .iter()
            .flat_map(|edge| {
                [
                    ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward),
                    ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse),
                ]
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        let slot_by_id = ids
            .iter()
            .cloned()
            .enumerate()
            .map(|(slot, id)| (id, slot))
            .collect();
        let forest = LinkCutForest::new(node_count, ids.len());
        let vertices = (0..node_count)
            .map(|index| tree(forest.vertex(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let edge_handles = (0..ids.len())
            .map(|index| tree(forest.edge(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let denominator = graph.edges().len().max(1);
        let tree_size_limit = node_count
            .checked_mul(node_count)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?
            .checked_div(denominator)
            .unwrap_or(1)
            .clamp(1, node_count.max(1));
        let mut heights = vec![0; node_count];
        heights[source.as_usize()] = node_count;
        Ok(Self {
            graph,
            source,
            sink,
            explicit,
            heights,
            excess,
            outgoing: graph
                .node_indices()
                .map(|node| stable_outgoing_ids(graph, node))
                .collect(),
            current: vec![0; node_count],
            queue: VecDeque::new(),
            queued: vec![false; node_count],
            forest,
            nodes: graph.node_indices().collect(),
            vertices,
            edge_handles,
            ids,
            slot_by_id,
            active_tree_edge: vec![false; graph.edges().len().saturating_mul(2)],
            linked_capacity: vec![0; graph.edges().len().saturating_mul(2)],
            tree_child: vec![None; graph.edges().len().saturating_mul(2)],
            tree_parent: vec![None; graph.edges().len().saturating_mul(2)],
            tree_parent_edge: vec![None; node_count],
            children: vec![BTreeSet::new(); node_count],
            tree_size_limit,
            metrics: DynamicTreePushRelabelMetrics {
                tree_size_limit: u64::try_from(tree_size_limit)
                    .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?,
                ..DynamicTreePushRelabelMetrics::default()
            },
        })
    }

    fn initialize_source_preflow(&mut self) -> Result<(), DynamicTreePushRelabelError> {
        for id in self.outgoing[self.source.as_usize()].clone() {
            let arc = self
                .explicit
                .arc(&id)
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            if arc.capacity == 0 || arc.to == self.source {
                continue;
            }
            let before = self.excess[arc.to.as_usize()];
            self.count_transition()?;
            self.explicit
                .augment(std::slice::from_ref(&id), arc.capacity)?;
            self.move_excess(self.source, arc.to, arc.capacity)?;
            self.metrics.source_pushes = checked_increment(self.metrics.source_pushes)?;
            self.count_push(true)?;
            if before <= 0 && self.excess[arc.to.as_usize()] > 0 {
                self.activate(arc.to)?;
            }
        }
        self.validate_materialized_preflow()
    }

    fn is_active(&self, node: NodeIndex) -> bool {
        node != self.source && node != self.sink && self.excess[node.as_usize()] > 0
    }

    fn activate(&mut self, node: NodeIndex) -> Result<(), DynamicTreePushRelabelError> {
        if !self.is_active(node) || self.queued[node.as_usize()] {
            return Ok(());
        }
        self.queued[node.as_usize()] = true;
        self.queue.push_back(node);
        self.metrics.queue_additions = checked_increment(self.metrics.queue_additions)?;
        Ok(())
    }

    fn active_queue(&self) -> Vec<NodeIndex> {
        self.queue.iter().copied().collect()
    }

    fn pop_active(&mut self) -> Result<Option<NodeIndex>, DynamicTreePushRelabelError> {
        while let Some(node) = self.queue.pop_front() {
            self.queued[node.as_usize()] = false;
            if self.is_active(node) {
                if tree(self.forest.represented_root(self.vertices[node.as_usize()]))?
                    != self.vertices[node.as_usize()]
                {
                    return Err(DynamicTreePushRelabelError::Invariant);
                }
                self.metrics.active_vertex_selections =
                    checked_increment(self.metrics.active_vertex_selections)?;
                return Ok(Some(node));
            }
        }
        Ok(None)
    }

    fn tree_push_relabel(
        &mut self,
        vertex: NodeIndex,
        mut recorder: Option<&mut FlowTraceRecorder<'graph>>,
    ) -> Result<(), DynamicTreePushRelabelError> {
        if !self.is_active(vertex)
            || tree(
                self.forest
                    .represented_root(self.vertices[vertex.as_usize()]),
            )? != self.vertices[vertex.as_usize()]
        {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        let outgoing = &self.outgoing[vertex.as_usize()];
        if self.current[vertex.as_usize()] >= outgoing.len() {
            return self.relabel_root(vertex, recorder);
        }
        let id = outgoing[self.current[vertex.as_usize()]].clone();
        let arc = self
            .explicit
            .arc(&id)
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        self.count_scan()?;
        record(
            recorder.as_deref_mut(),
            self,
            EventKind::InspectCurrent,
            vec![vertex, arc.to],
            vec![id.clone()],
            Some(("residual", i128::from(arc.capacity))),
        )?;
        if arc.to == vertex
            || arc.capacity == 0
            || self.heights[vertex.as_usize()] != self.heights[arc.to.as_usize()].saturating_add(1)
        {
            self.current[vertex.as_usize()] = self.current[vertex.as_usize()]
                .checked_add(1)
                .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
            return Ok(());
        }
        let slot = *self
            .slot_by_id
            .get(&id)
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        if self.active_tree_edge[slot] {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        self.push_over_eligible_current(vertex, id, arc.to, arc.capacity, slot, recorder)
    }

    fn push_over_eligible_current(
        &mut self,
        vertex: NodeIndex,
        id: ResidualArcId,
        target: NodeIndex,
        capacity: u64,
        slot: usize,
        mut recorder: Option<&mut FlowTraceRecorder<'graph>>,
    ) -> Result<(), DynamicTreePushRelabelError> {
        let left_size = tree(
            self.forest
                .represented_vertex_count(self.vertices[vertex.as_usize()]),
        )?;
        let right_size = tree(
            self.forest
                .represented_vertex_count(self.vertices[target.as_usize()]),
        )?;
        self.metrics.component_size_queries = self
            .metrics
            .component_size_queries
            .checked_add(2)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
        if left_size
            .checked_add(right_size)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?
            <= self.tree_size_limit
        {
            self.link_current(slot, vertex, target, capacity)?;
            record(
                recorder.as_deref_mut(),
                self,
                EventKind::Link,
                vec![vertex, target],
                vec![id],
                Some((
                    "combined-size",
                    i128::try_from(left_size + right_size)
                        .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?,
                )),
            )?;
            self.send(vertex, vertex, recorder)?;
        } else {
            self.metrics.size_gate_rejections =
                checked_increment(self.metrics.size_gate_rejections)?;
            let amount = u64::try_from(self.excess[vertex.as_usize()])
                .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?
                .min(capacity);
            let before = self.excess[target.as_usize()];
            self.count_transition()?;
            self.explicit.augment(std::slice::from_ref(&id), amount)?;
            self.move_excess(vertex, target, amount)?;
            self.metrics.ordinary_pushes = checked_increment(self.metrics.ordinary_pushes)?;
            self.count_push(amount == capacity)?;
            record(
                recorder.as_deref_mut(),
                self,
                EventKind::PushLargeTree,
                vec![vertex, target],
                vec![id],
                Some(("delta", i128::from(amount))),
            )?;
            self.send(target, vertex, recorder)?;
            if before <= 0 && self.excess[target.as_usize()] > 0 {
                self.activate(target)?;
            }
        }
        Ok(())
    }

    fn link_current(
        &mut self,
        slot: usize,
        child: NodeIndex,
        parent: NodeIndex,
        capacity: u64,
    ) -> Result<(), DynamicTreePushRelabelError> {
        tree(self.forest.link_rooted(
            self.edge_handles[slot],
            self.vertices[child.as_usize()],
            self.vertices[parent.as_usize()],
            BigInt::from(capacity),
        ))?;
        if self.tree_parent_edge[child.as_usize()]
            .replace(slot)
            .is_some()
            || !self.children[parent.as_usize()].insert(child)
        {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        self.active_tree_edge[slot] = true;
        self.linked_capacity[slot] = capacity;
        self.tree_child[slot] = Some(child);
        self.tree_parent[slot] = Some(parent);
        self.metrics.tree_links = checked_increment(self.metrics.tree_links)?;
        self.count_transition()
    }

    fn send(
        &mut self,
        start: NodeIndex,
        deferred: NodeIndex,
        mut recorder: Option<&mut FlowTraceRecorder<'graph>>,
    ) -> Result<(), DynamicTreePushRelabelError> {
        loop {
            let root = tree(
                self.forest
                    .represented_root(self.vertices[start.as_usize()]),
            )?;
            if root == self.vertices[start.as_usize()] || self.excess[start.as_usize()] <= 0 {
                break;
            }
            let minimum = tree(
                self.forest
                    .root_path_minimum_closest_to_root(self.vertices[start.as_usize()]),
            )?
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let residual = minimum
                .value
                .to_u64()
                .filter(|value| *value > 0)
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let amount = u64::try_from(self.excess[start.as_usize()])
                .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?
                .min(residual);
            let root_node = *self
                .nodes
                .get(root.index())
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let before_root = self.excess[root_node.as_usize()];
            let path = if recorder.is_some() {
                self.tree_path(start, root_node)?
            } else {
                Vec::new()
            };
            tree(
                self.forest
                    .root_path_add(self.vertices[start.as_usize()], &-BigInt::from(amount)),
            )?;
            self.move_excess(start, root_node, amount)?;
            self.metrics.tree_path_sends = checked_increment(self.metrics.tree_path_sends)?;
            self.metrics.path_updates = checked_increment(self.metrics.path_updates)?;
            self.count_push(amount == residual)?;
            self.count_transition()?;
            record(
                recorder.as_deref_mut(),
                self,
                EventKind::Send,
                vec![start, root_node],
                path,
                Some(("delta", i128::from(amount))),
            )?;
            if root_node != deferred && before_root <= 0 && self.excess[root_node.as_usize()] > 0 {
                self.activate(root_node)?;
            }

            loop {
                let Some(candidate) = tree(
                    self.forest
                        .root_path_minimum_closest_to_root(self.vertices[start.as_usize()]),
                )?
                else {
                    break;
                };
                if !candidate.value.is_zero() {
                    break;
                }
                let slot = candidate.edge.index();
                let id = self.ids[slot].clone();
                let child = self.finalize_and_cut(slot)?;
                self.metrics.tree_cuts = checked_increment(self.metrics.tree_cuts)?;
                self.count_transition()?;
                record(
                    recorder.as_deref_mut(),
                    self,
                    EventKind::CutSaturated,
                    vec![child],
                    vec![id],
                    Some(("remaining", 0)),
                )?;
            }
        }
        if start != deferred && self.is_active(start) {
            self.activate(start)?;
        }
        Ok(())
    }

    fn relabel_root(
        &mut self,
        vertex: NodeIndex,
        mut recorder: Option<&mut FlowTraceRecorder<'graph>>,
    ) -> Result<(), DynamicTreePushRelabelError> {
        let children = self.children[vertex.as_usize()]
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for child in children {
            let slot = self.tree_parent_edge[child.as_usize()]
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let id = self.ids[slot].clone();
            self.finalize_and_cut(slot)?;
            self.metrics.tree_cuts = checked_increment(self.metrics.tree_cuts)?;
            self.count_transition()?;
            record(
                recorder.as_deref_mut(),
                self,
                EventKind::CutForRelabel,
                vec![child, vertex],
                vec![id],
                None,
            )?;
        }
        let old_height = self.heights[vertex.as_usize()];
        let mut minimum = None;
        for id in &self.outgoing[vertex.as_usize()].clone() {
            let arc = self
                .explicit
                .arc(id)
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            if arc.capacity == 0 || arc.to == vertex {
                continue;
            }
            self.count_scan()?;
            record(
                recorder.as_deref_mut(),
                self,
                EventKind::InspectRelabel,
                vec![vertex, arc.to],
                vec![id.clone()],
                Some((
                    "neighbor-height",
                    i128::try_from(self.heights[arc.to.as_usize()])
                        .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?,
                )),
            )?;
            minimum = Some(
                minimum.map_or(self.heights[arc.to.as_usize()], |current: usize| {
                    current.min(self.heights[arc.to.as_usize()])
                }),
            );
        }
        let new_height = minimum
            .and_then(|height| height.checked_add(1))
            .filter(|height| *height > old_height && *height < self.graph.nodes().len() * 2)
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        self.heights[vertex.as_usize()] = new_height;
        self.current[vertex.as_usize()] = 0;
        self.metrics.relabels = checked_increment(self.metrics.relabels)?;
        self.count_transition()?;
        record(
            recorder,
            self,
            EventKind::Relabel,
            vec![vertex],
            Vec::new(),
            Some((
                "height",
                i128::try_from(new_height)
                    .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?,
            )),
        )
    }

    fn finalize_and_cut(&mut self, slot: usize) -> Result<NodeIndex, DynamicTreePushRelabelError> {
        if !self.active_tree_edge.get(slot).copied().unwrap_or(false) {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        let remaining = tree(self.forest.edge_value(self.edge_handles[slot]))?;
        let remaining = remaining
            .to_u64()
            .filter(|value| *value <= self.linked_capacity[slot])
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        let used = self.linked_capacity[slot] - remaining;
        if used > 0 {
            self.explicit
                .augment(std::slice::from_ref(&self.ids[slot]), used)?;
        }
        let cut_value = tree(self.forest.cut_rooted(self.edge_handles[slot]))?;
        if cut_value != BigInt::from(remaining) {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        let child = self.tree_child[slot]
            .take()
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        let parent = self.tree_parent[slot]
            .take()
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        if self.tree_parent_edge[child.as_usize()].take() != Some(slot)
            || !self.children[parent.as_usize()].remove(&child)
        {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        self.active_tree_edge[slot] = false;
        self.linked_capacity[slot] = 0;
        Ok(child)
    }

    fn materialize_final_tree_flows(&mut self) -> Result<u64, DynamicTreePushRelabelError> {
        let slots = self
            .active_tree_edge
            .iter()
            .enumerate()
            .filter_map(|(slot, active)| active.then_some(slot))
            .collect::<Vec<_>>();
        for slot in &slots {
            let remaining = tree(self.forest.edge_value(self.edge_handles[*slot]))?
                .to_u64()
                .filter(|value| *value <= self.linked_capacity[*slot])
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let used = self.linked_capacity[*slot] - remaining;
            if used > 0 {
                self.explicit
                    .augment(std::slice::from_ref(&self.ids[*slot]), used)?;
            }
            self.active_tree_edge[*slot] = false;
            self.tree_child[*slot] = None;
            self.tree_parent[*slot] = None;
            self.linked_capacity[*slot] = 0;
        }
        self.tree_parent_edge.fill(None);
        for children in &mut self.children {
            children.clear();
        }
        let count = u64::try_from(slots.len())
            .map_err(|_| DynamicTreePushRelabelError::ArithmeticOverflow)?;
        self.metrics.final_tree_materializations = self
            .metrics
            .final_tree_materializations
            .checked_add(count)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
        Ok(count)
    }

    fn materialized_state(&mut self) -> Result<ResidualState<'graph>, DynamicTreePushRelabelError> {
        let mut flows = self.explicit.flows().to_vec();
        for slot in 0..self.active_tree_edge.len() {
            if !self.active_tree_edge[slot] {
                continue;
            }
            let remaining = tree(self.forest.edge_value(self.edge_handles[slot]))?
                .to_u64()
                .filter(|value| *value <= self.linked_capacity[slot])
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let used = self.linked_capacity[slot] - remaining;
            if used == 0 {
                continue;
            }
            let edge_index = self
                .graph
                .edge_index(self.ids[slot].original_edge())
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            let flow = flows
                .get_mut(edge_index.as_usize())
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            *flow = match self.ids[slot].direction() {
                ResidualDirection::Forward => flow.checked_add(used),
                ResidualDirection::Reverse => flow.checked_sub(used),
            }
            .ok_or(DynamicTreePushRelabelError::Invariant)?;
        }
        ResidualState::from_flows(self.graph, &flows).map_err(Into::into)
    }

    fn validate_materialized_preflow(&mut self) -> Result<(), DynamicTreePushRelabelError> {
        let state = self.materialized_state()?;
        if excess_from_flows(self.graph, state.flows())? != self.excess
            || self
                .excess
                .iter()
                .try_fold(0_i128, |sum, value| sum.checked_add(*value))
                .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?
                != 0
            || self.heights[self.source.as_usize()] != self.graph.nodes().len()
            || self.heights[self.sink.as_usize()] != 0
            || self.graph.node_indices().any(|node| {
                node != self.source && node != self.sink && self.excess[node.as_usize()] < 0
            })
        {
            return Err(DynamicTreePushRelabelError::Invariant);
        }
        for node in self.graph.node_indices() {
            for arc in state.outgoing_arcs(node) {
                if arc.to != node
                    && self.heights[node.as_usize()]
                        > self.heights[arc.to.as_usize()].saturating_add(1)
                {
                    return Err(DynamicTreePushRelabelError::Invariant);
                }
            }
        }
        Ok(())
    }

    fn tree_path(
        &self,
        start: NodeIndex,
        root: NodeIndex,
    ) -> Result<Vec<ResidualArcId>, DynamicTreePushRelabelError> {
        let mut result = Vec::new();
        let mut node = start;
        while node != root {
            if result.len() >= self.graph.nodes().len() {
                return Err(DynamicTreePushRelabelError::Invariant);
            }
            let slot = self.tree_parent_edge[node.as_usize()]
                .ok_or(DynamicTreePushRelabelError::Invariant)?;
            result.push(self.ids[slot].clone());
            node = self.tree_parent[slot].ok_or(DynamicTreePushRelabelError::Invariant)?;
        }
        Ok(result)
    }

    fn forest_arcs(&self) -> Vec<ResidualArcId> {
        self.active_tree_edge
            .iter()
            .enumerate()
            .filter(|(_, active)| **active)
            .map(|(slot, _)| self.ids[slot].clone())
            .collect()
    }

    fn move_excess(
        &mut self,
        from: NodeIndex,
        to: NodeIndex,
        amount: u64,
    ) -> Result<(), DynamicTreePushRelabelError> {
        let amount = i128::from(amount);
        self.excess[from.as_usize()] = self.excess[from.as_usize()]
            .checked_sub(amount)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
        self.excess[to.as_usize()] = self.excess[to.as_usize()]
            .checked_add(amount)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
        Ok(())
    }

    fn count_scan(&mut self) -> Result<(), DynamicTreePushRelabelError> {
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)?;
        if self.metrics.residual_arc_scans > DYNAMIC_TREE_PUSH_RELABEL_MAX_ARC_SCANS {
            return Err(DynamicTreePushRelabelError::WorkLimit);
        }
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), DynamicTreePushRelabelError> {
        self.metrics.state_transitions = checked_increment(self.metrics.state_transitions)?;
        if self.metrics.state_transitions > DYNAMIC_TREE_PUSH_RELABEL_MAX_STATE_TRANSITIONS {
            return Err(DynamicTreePushRelabelError::WorkLimit);
        }
        Ok(())
    }

    fn count_push(&mut self, saturating: bool) -> Result<(), DynamicTreePushRelabelError> {
        self.metrics.pushes = checked_increment(self.metrics.pushes)?;
        if saturating {
            self.metrics.saturating_pushes = checked_increment(self.metrics.saturating_pushes)?;
        } else {
            self.metrics.nonsaturating_pushes =
                checked_increment(self.metrics.nonsaturating_pushes)?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    work: &mut Work<'_>,
    kind: EventKind,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    detail: Option<(&'static str, i128)>,
) -> Result<(), DynamicTreePushRelabelError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    if recorder.event_count() >= DYNAMIC_TREE_PUSH_RELABEL_MAX_TRACE_EVENTS {
        return Err(FlowTraceError::EventLimit.into());
    }
    let local_edge_focus = matches!(kind, EventKind::InspectCurrent | EventKind::InspectRelabel)
        .then(|| {
            active_path
                .last()
                .cloned()
                .map(FlowTraceEntityRef::ResidualArc)
                .into_iter()
                .collect::<Vec<_>>()
        });
    let state = work.materialized_state()?;
    let labels = work
        .heights
        .iter()
        .map(|height| i128::try_from(*height).ok())
        .collect();
    let snapshot = FlowTraceSnapshot::capture(
        work.graph,
        &state,
        labels,
        search_order,
        active_path,
        work.excess.clone(),
        trace_metrics(work.metrics),
    )
    .with_forest_overlay(work.graph, work.forest_arcs(), Vec::new());
    if let Some(focus) = local_edge_focus {
        recorder
            .record_transition_with_detail_and_focus(kind.metadata(), &snapshot, detail, focus)
            .map_err(Into::into)
    } else {
        recorder
            .record_transition_with_detail(kind.metadata(), &snapshot, detail)
            .map_err(Into::into)
    }
}

const fn trace_metrics(metrics: DynamicTreePushRelabelMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.source_pushes as u128,
        relaxation_passes: metrics.tree_size_limit as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.tree_path_sends as u128,
        path_searches: metrics.component_size_queries as u128,
        scaling_phases: metrics.tree_links as u128,
        blocking_flow_phases: metrics.tree_cuts as u128,
        relabels: metrics.relabels as u128,
        retreats: metrics.final_tree_materializations as u128,
        reverse_bfs_runs: metrics.queue_additions as u128,
        gap_terminations: metrics.size_gate_rejections as u128,
        pushes: metrics.pushes as u128,
        saturating_pushes: metrics.saturating_pushes as u128,
        nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
        discharges: metrics.discharges as u128,
        active_vertex_selections: metrics.active_vertex_selections as u128,
    }
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

fn excess_from_flows(
    graph: &FlowNetwork,
    flows: &[u64],
) -> Result<Vec<i128>, DynamicTreePushRelabelError> {
    divergences(graph, flows)?
        .into_iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)
        })
        .collect()
}

fn tree<T>(result: Result<T, LinkCutError>) -> Result<T, DynamicTreePushRelabelError> {
    result.map_err(|_| DynamicTreePushRelabelError::Invariant)
}

fn checked_increment(value: u64) -> Result<u64, DynamicTreePushRelabelError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreePushRelabelError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use crate::algorithms::{solve_edmonds_karp, solve_fifo_push_relabel};
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
    fn links_sends_cuts_and_relabels_with_fifo_selection() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 7),
            ("sb", "s", "b", 0, 5),
            ("ab", "a", "b", 0, 3),
            ("at", "a", "t", 0, 4),
            ("bt", "b", "t", 0, 8),
        ]);
        let expected = solve_fifo_push_relabel(&graph, source, sink).expect("FIFO reference");
        let traced = trace_dynamic_tree_push_relabel(&graph, source, sink)
            .expect("dynamic-tree push-relabel");

        assert_eq!(traced.result.certificate.value, expected.certificate.value);
        assert!(traced.result.metrics.tree_links > 0);
        assert!(traced.result.metrics.tree_path_sends > 0);
        assert!(traced.result.metrics.tree_cuts > 0);
        assert!(traced.result.metrics.relabels > 0);
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.catalog_id == "dynamic-tree-push-relabel.send-root-path" })
        );
        let scans = traced
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.catalog_id.as_str(),
                    "dynamic-tree-push-relabel.inspect-current-edge"
                        | "dynamic-tree-push-relabel.inspect-relabel-edge"
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scans.len()).expect("scan event count"),
            traced.result.metrics.residual_arc_scans
        );
        assert!(scans.iter().all(|event| {
            !event.entity_refs.is_empty()
                && event.patches.iter().any(|patch| {
                    matches!(
                        patch,
                        crate::trace::FlowTracePatch::Metric {
                            metric: crate::trace::FlowTraceMetricId::ResidualArcScans,
                            before,
                            after,
                        } if *after == before.saturating_add(1)
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
    fn lower_bounds_parallel_opposite_and_self_loop_are_certified() {
        let (graph, source, sink) = network(&[
            ("sa0", "s", "a", 1, 5),
            ("sa1", "s", "a", 0, 2),
            ("at", "a", "t", 1, 7),
            ("as", "a", "s", 0, 3),
            ("aa", "a", "a", 0, 9),
            ("sb", "s", "b", 0, 3),
            ("bt", "b", "t", 0, 3),
        ]);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference");
        let fast = solve_dynamic_tree_push_relabel(&graph, source, sink).expect("dynamic-tree");
        let traced = trace_dynamic_tree_push_relabel(&graph, source, sink).expect("trace");

        assert_eq!(fast.certificate.value, expected.certificate.value);
        assert_eq!(fast.certificate.value, 10);
        assert_eq!(traced.result, fast);
        assert_eq!(traced.final_snapshot.flows, fast.flows);
    }

    #[test]
    fn bounded_multigraphs_match_independent_edmonds_karp() {
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
                    let capacity = seed
                        .wrapping_mul(31)
                        .wrapping_add(u64::try_from(index).expect("small index") * 17)
                        % 9;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&edges);
            let expected = solve_edmonds_karp(&graph, source, sink).expect("reference");
            let actual = solve_dynamic_tree_push_relabel(&graph, source, sink)
                .expect("dynamic-tree push-relabel");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "fixture {seed}"
            );
        }
    }
}
