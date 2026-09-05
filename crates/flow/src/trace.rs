//! Reversible, stable-identity trace transactions for flow algorithms.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{EdgeId, FlowNetwork, NodeId, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualState};
use crate::scenario::TraceGranularityV1;

/// Maximum state patches allowed in one atomic event.
pub const MAX_FLOW_TRACE_PATCHES_PER_EVENT: usize = 65_536;
/// Maximum stable entity references allowed in one event.
pub const MAX_FLOW_TRACE_ENTITY_REFS_PER_EVENT: usize = 65_536;

/// Absolute trace counters shared by the two Phase-2 algorithms.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlowTraceMetrics {
    /// Edmonds–Karp breadth-first searches, including the final failed search.
    pub bfs_runs: u128,
    /// Bellman–Ford outer relaxation passes.
    pub relaxation_passes: u128,
    /// Positive residual arcs inspected by the current algorithm.
    pub residual_arc_scans: u128,
    /// Successful residual augmentations.
    pub augmentations: u128,
    /// Complete augmenting-path searches that are not breadth-first searches.
    pub path_searches: u128,
    /// Capacity or cost scaling phases entered by the current algorithm.
    pub scaling_phases: u128,
    /// Completed blocking-flow phases.
    pub blocking_flow_phases: u128,
    /// Distance-label increases.
    pub relabels: u128,
    /// Backtracks after relabeling a non-source vertex.
    pub retreats: u128,
    /// Reverse breadth-first label initializations.
    pub reverse_bfs_runs: u128,
    /// Gap-heuristic terminations.
    pub gap_terminations: u128,
    /// Local residual-arc pushes, including source initialization.
    pub pushes: u128,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u128,
    /// Pushes that exhaust the active vertex's excess first.
    pub nonsaturating_pushes: u128,
    /// Completed active-vertex discharge phases.
    pub discharges: u128,
    /// Active vertices selected by the scheduling policy.
    pub active_vertex_selections: u128,
}

/// Complete replay state at one committed event boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowTraceSnapshot {
    /// Current edge capacities in canonical edge-ID order. For static runs
    /// these equal the immutable graph capacities.
    pub edge_capacities: Vec<u64>,
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Both residual directions, including zero-capacity arcs, in stable order.
    pub residual_capacities: Vec<(ResidualArcId, u64)>,
    /// Algorithm-defined exact labels in canonical node-ID order.
    pub node_labels: Vec<Option<i128>>,
    /// Stable node order produced by the current search.
    pub search_order: Vec<NodeId>,
    /// Currently selected residual path.
    pub active_path: Vec<ResidualArcId>,
    /// Original arcs temporarily removed from an algorithm's working graph.
    pub fixed_edges: Vec<EdgeId>,
    /// Algorithm-owned rooted-forest arcs. Most overlays use parent-to-child;
    /// dynamic-tree blocking flow stores its actual child-to-represented-root
    /// residual direction because that is the path-update direction.
    pub forest_arcs: Vec<ResidualArcId>,
    /// Nodes currently belonging to strong pseudoflow branches.
    pub strong_nodes: Vec<NodeId>,
    /// Remaining divergence per canonical node; empty for max flow.
    pub remaining_divergence: Vec<i128>,
    /// Dedicated Excesses-IBFS solver state, absent during certified-flow recovery
    /// and for every other algorithm family.
    pub eibfs_overlay: Option<EibfsTraceOverlay>,
    /// Dynamic EIBFS update/prefix state, absent from static EIBFS traces.
    pub dynamic_eibfs_overlay: Option<DynamicEibfsTraceOverlay>,
    /// Absolute deterministic operation counters.
    pub metrics: FlowTraceMetrics,
}

impl FlowTraceSnapshot {
    /// Captures all replay-visible state from one residual boundary.
    #[must_use]
    pub fn capture(
        graph: &FlowNetwork,
        state: &ResidualState<'_>,
        node_labels: Vec<Option<i128>>,
        search_order: Vec<NodeIndex>,
        active_path: Vec<ResidualArcId>,
        remaining_divergence: Vec<i128>,
        metrics: FlowTraceMetrics,
    ) -> Self {
        let mut residual_capacities = Vec::with_capacity(graph.edges().len() * 2);
        for edge in graph.edges() {
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let capacity = state.arc(&id).map_or(0, |arc| arc.capacity);
                residual_capacities.push((id, capacity));
            }
        }
        residual_capacities.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        Self {
            edge_capacities: state.capacities().to_vec(),
            flows: state.flows().to_vec(),
            residual_capacities,
            node_labels,
            search_order: search_order
                .into_iter()
                .filter_map(|index| graph.node(index).map(|node| node.id().clone()))
                .collect(),
            active_path,
            fixed_edges: Vec::new(),
            forest_arcs: Vec::new(),
            strong_nodes: Vec::new(),
            remaining_divergence,
            eibfs_overlay: None,
            dynamic_eibfs_overlay: None,
            metrics,
        }
    }

    /// Attaches the canonical set of original arcs temporarily fixed by a heuristic.
    #[must_use]
    pub fn with_fixed_edges(mut self, mut fixed_edges: Vec<EdgeId>) -> Self {
        fixed_edges.sort_unstable();
        fixed_edges.dedup();
        self.fixed_edges = fixed_edges;
        self
    }

    /// Attaches a canonical rooted-forest overlay to an otherwise complete snapshot.
    #[must_use]
    pub fn with_forest_overlay(
        mut self,
        graph: &FlowNetwork,
        mut forest_arcs: Vec<ResidualArcId>,
        strong_nodes: Vec<NodeIndex>,
    ) -> Self {
        forest_arcs.sort_unstable();
        forest_arcs.dedup();
        self.forest_arcs = forest_arcs;
        self.strong_nodes = strong_nodes
            .into_iter()
            .filter_map(|index| graph.node(index).map(|node| node.id().clone()))
            .collect();
        self.strong_nodes.sort_unstable();
        self.strong_nodes.dedup();
        self
    }

    /// Attaches the complete typed Excesses-IBFS pseudoflow-forest state.
    #[must_use]
    pub fn with_eibfs_overlay(mut self, overlay: EibfsTraceOverlay) -> Self {
        self.eibfs_overlay = Some(overlay);
        self
    }

    /// Attaches the current Dynamic EIBFS update/prefix state.
    #[must_use]
    pub fn with_dynamic_eibfs_overlay(mut self, overlay: DynamicEibfsTraceOverlay) -> Self {
        self.dynamic_eibfs_overlay = Some(overlay);
        self
    }
}

/// Current one-sided growth phase of Excesses IBFS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EibfsTracePhaseDirection {
    /// Grow the excess-rooted source forest.
    Forward,
    /// Grow the deficit-rooted sink forest.
    Reverse,
}

/// Explicit EIBFS forest membership; labels alone never encode this state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EibfsTraceMembership {
    /// Not currently in either forest.
    Free,
    /// Member of the excess-rooted source forest.
    Source,
    /// Member of the deficit-rooted sink forest.
    Sink,
}

/// Semantic kind of one EIBFS forest root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EibfsTraceRootKind {
    /// Not a root.
    None,
    /// Distinguished source, treated as infinite excess.
    Source,
    /// Distinguished sink, treated as infinite deficit.
    Sink,
    /// Nonterminal positive-excess root.
    Excess,
    /// Nonterminal negative-deficit root.
    Deficit,
}

/// Complete replay-visible EIBFS state for one canonical node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EibfsTraceNodeState {
    /// Stable node identity.
    pub node: NodeId,
    /// Retained source-distance label, even while outside the source forest.
    pub source_label: usize,
    /// Retained sink-distance label, even while outside the sink forest.
    pub sink_label: usize,
    /// Current explicit forest membership.
    pub membership: EibfsTraceMembership,
    /// Root semantics independent of the numeric label.
    pub root_kind: EibfsTraceRootKind,
    /// Whether the parent is currently being repaired.
    pub orphan: bool,
    /// Exact finite pseudoflow imbalance; terminals remain finite here even
    /// though their root semantics are infinite.
    pub imbalance: i128,
}

/// One EIBFS forest relation with both structural and residual directions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EibfsTraceForestArc {
    /// Parent node in the rooted forest.
    pub parent: NodeId,
    /// Child node in the rooted forest.
    pub child: NodeId,
    /// Forest side owning the relation.
    pub side: EibfsTraceMembership,
    /// Actual admissible residual direction. For the sink forest this points
    /// from child toward parent.
    pub admissible_residual: ResidualArcId,
}

/// Complete dedicated EIBFS overlay at one pseudoflow event boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EibfsTraceOverlay {
    /// Currently selected growth direction.
    pub phase_direction: EibfsTracePhaseDirection,
    /// Maximum source-forest boundary label.
    pub source_depth: usize,
    /// Maximum sink-forest boundary label.
    pub sink_depth: usize,
    /// Canonical node states.
    pub nodes: Vec<EibfsTraceNodeState>,
    /// Canonical parent-child relations.
    pub forest_arcs: Vec<EibfsTraceForestArc>,
}

/// Replay stage of one Dynamic EIBFS update prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicEibfsTraceStage {
    /// Initial static EIBFS solve on the first capacity vector.
    InitialSolve,
    /// Atomically install one changed current capacity. This is the only stage
    /// that may temporarily expose `flow > current capacity`.
    ApplyUpdate,
    /// Remove capacity-overflow flow and restore stable forest signs.
    RepairCapacity,
    /// Re-adopt parent arcs invalidated by the capacity change.
    RepairForest,
    /// Repair one newly residual structural violation.
    RepairViolation,
    /// Continue EIBFS from the retained pseudoflow and forests.
    ContinueSolve,
    /// Recover a feasible flow on a certification-only clone.
    PrefixRecovery,
    /// Independently certified maximum-flow/minimum-cut prefix result.
    PrefixCertified,
    /// Restore the untouched reusable pseudoflow before the next update.
    ResumeReusablePseudoflow,
}

/// Source-defined Dynamic EIBFS invariant violation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicEibfsTraceViolation {
    /// Current flow exceeded the newly decreased capacity.
    OverCapacity,
    /// Newly residual source-forest to sink-forest bridge.
    Bridge,
    /// Newly residual same-forest label inequality.
    Label,
    /// Newly admissible arc precedes the retained current arc.
    CurrentArc,
    /// Newly residual arc crosses a retained forest boundary.
    Boundary,
}

/// Complete replay-visible Dynamic EIBFS update context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicEibfsTraceOverlay {
    /// Current update/prefix stage.
    pub stage: DynamicEibfsTraceStage,
    /// Prefix zero is the initial graph; update `i` creates prefix `i`.
    pub update_index: usize,
    /// Fixed total number of sequential capacity updates.
    pub update_total: usize,
    /// Changed stable edge identity for nonzero prefixes.
    pub changed_edge: Option<EdgeId>,
    /// Capacity immediately before this prefix's update.
    pub old_capacity: Option<u64>,
    /// Capacity installed by this prefix's update.
    pub new_capacity: Option<u64>,
    /// Violation repaired at this event boundary, when applicable.
    pub violation: Option<DynamicEibfsTraceViolation>,
    /// Forest nodes retained exactly across the current update.
    pub reused_forest_nodes: u64,
    /// Updates applied cumulatively, including no-ops.
    pub updates_applied: u64,
    /// Strict capacity increases cumulatively.
    pub capacity_increases: u64,
    /// Strict capacity decreases cumulatively.
    pub capacity_decreases: u64,
    /// No-op updates cumulatively.
    pub no_op_updates: u64,
    /// Over-capacity reverse repairs cumulatively.
    pub over_capacity_repairs: u64,
    /// Parent arcs invalidated cumulatively through this prefix.
    pub invalidated_parent_arcs: u64,
    /// Correct-sign roots promoted cumulatively through this prefix.
    pub promoted_roots: u64,
    /// Dynamic repair residual-arc scans cumulatively through this prefix.
    pub repair_arc_scans: u128,
    /// Reusable warm solver state transitions cumulatively through this prefix.
    pub state_transitions: u64,
    /// New source-to-sink bridge repairs cumulatively.
    pub bridge_violations: u64,
    /// Same-forest label repairs cumulatively.
    pub label_violations: u64,
    /// Current-arc rewinds cumulatively.
    pub current_arc_violations: u64,
    /// Forest-boundary repairs cumulatively.
    pub boundary_violations: u64,
    /// Repair stabilization iterations cumulatively.
    pub repair_iterations: u64,
    /// Certification-only recovery paths cumulatively.
    pub certification_recoveries: u64,
    /// Certified prefix value, present only at `PrefixCertified`.
    pub prefix_value: Option<i128>,
}

/// Stable entity identity referenced by an event.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FlowTraceEntityRef {
    /// Original node.
    Node(NodeId),
    /// Original directed edge.
    Edge(EdgeId),
    /// Derived residual arc direction.
    ResidualArc(ResidualArcId),
}

/// Returns the exact graph entities owned by the supplied residual arcs.
///
/// Snapshot patches may contain global label or search-state changes. Trace
/// producers use this helper to keep the event focus on the arc primitive that
/// was actually inspected, selected, or changed.
pub(crate) fn residual_arc_entity_refs(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    arcs: &[ResidualArcId],
) -> Result<Vec<FlowTraceEntityRef>, FlowTraceError> {
    let mut refs = Vec::with_capacity(arcs.len().saturating_mul(4));
    for arc_id in arcs {
        let arc = state.arc(arc_id).ok_or(FlowTraceError::MissingEntity)?;
        refs.push(FlowTraceEntityRef::Node(
            graph.nodes()[arc.from.as_usize()].id().clone(),
        ));
        refs.push(FlowTraceEntityRef::Node(
            graph.nodes()[arc.to.as_usize()].id().clone(),
        ));
        refs.push(FlowTraceEntityRef::Edge(arc_id.original_edge().clone()));
        refs.push(FlowTraceEntityRef::ResidualArc(arc_id.clone()));
    }
    Ok(refs)
}

/// Returns the ordered nodes of one contiguous residual path.
pub(crate) fn residual_path_node_order(
    state: &ResidualState<'_>,
    arcs: &[ResidualArcId],
) -> Result<Vec<NodeIndex>, FlowTraceError> {
    let Some(first_id) = arcs.first() else {
        return Ok(Vec::new());
    };
    let first = state.arc(first_id).ok_or(FlowTraceError::MissingEntity)?;
    let mut nodes = vec![first.from, first.to];
    for id in &arcs[1..] {
        let arc = state.arc(id).ok_or(FlowTraceError::MissingEntity)?;
        if nodes.last().copied() != Some(arc.from) {
            return Err(FlowTraceError::Precondition);
        }
        nodes.push(arc.to);
    }
    Ok(nodes)
}

/// Absolute counter patched by an event.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowTraceMetricId {
    /// Breadth-first-search count.
    BfsRuns,
    /// Bellman–Ford relaxation-pass count.
    RelaxationPasses,
    /// Positive residual-arc scan count.
    ResidualArcScans,
    /// Successful augmentation count.
    Augmentations,
    /// Non-BFS augmenting-path search count.
    PathSearches,
    /// Scaling phase count.
    ScalingPhases,
    /// Blocking-flow phase count.
    BlockingFlowPhases,
    /// Distance-label increase count.
    Relabels,
    /// Distance-label retreat count.
    Retreats,
    /// Reverse-BFS initialization count.
    ReverseBfsRuns,
    /// Gap-heuristic termination count.
    GapTerminations,
    /// Local push count.
    Pushes,
    /// Saturating-push count.
    SaturatingPushes,
    /// Nonsaturating-push count.
    NonsaturatingPushes,
    /// Active-vertex discharge count.
    Discharges,
    /// Active-vertex selection count.
    ActiveVertexSelections,
}

/// One reversible before/after mutation keyed by stable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowTracePatch {
    /// Current original-edge capacity mutation within an immutable envelope.
    EdgeCapacity {
        /// Stable edge identity.
        edge: EdgeId,
        /// Required value before forward application.
        before: u64,
        /// Value after forward application.
        after: u64,
    },
    /// Original edge-flow mutation.
    EdgeFlow {
        /// Stable edge identity.
        edge: EdgeId,
        /// Required value before forward application.
        before: u64,
        /// Value after forward application.
        after: u64,
    },
    /// Residual-capacity mutation.
    ResidualCapacity {
        /// Stable residual identity.
        arc: ResidualArcId,
        /// Required value before forward application.
        before: u64,
        /// Value after forward application.
        after: u64,
    },
    /// Exact node-label mutation.
    NodeLabel {
        /// Stable node identity.
        node: NodeId,
        /// Required value before forward application.
        before: Option<i128>,
        /// Value after forward application.
        after: Option<i128>,
    },
    /// Search-order overlay replacement.
    SearchOrder {
        /// Required order before forward application.
        before: Vec<NodeId>,
        /// Order after forward application.
        after: Vec<NodeId>,
    },
    /// Selected residual-path overlay replacement.
    ActivePath {
        /// Required path before forward application.
        before: Vec<ResidualArcId>,
        /// Path after forward application.
        after: Vec<ResidualArcId>,
    },
    /// Temporarily fixed original-edge overlay replacement.
    FixedEdges {
        /// Required set before forward application.
        before: Vec<EdgeId>,
        /// Set after forward application.
        after: Vec<EdgeId>,
    },
    /// Rooted-forest overlay replacement.
    ForestArcs {
        /// Required forest before forward application.
        before: Vec<ResidualArcId>,
        /// Forest after forward application.
        after: Vec<ResidualArcId>,
    },
    /// Strong-branch membership replacement.
    StrongNodes {
        /// Required membership before forward application.
        before: Vec<NodeId>,
        /// Membership after forward application.
        after: Vec<NodeId>,
    },
    /// Remaining node imbalance mutation.
    RemainingDivergence {
        /// Stable node identity.
        node: NodeId,
        /// Required value before forward application.
        before: i128,
        /// Value after forward application.
        after: i128,
    },
    /// Complete typed EIBFS overlay replacement.
    EibfsOverlay {
        /// Required overlay before forward application.
        before: Option<EibfsTraceOverlay>,
        /// Overlay after forward application.
        after: Option<EibfsTraceOverlay>,
    },
    /// Complete typed Dynamic EIBFS update overlay replacement.
    DynamicEibfsOverlay {
        /// Required overlay before forward application.
        before: Option<Box<DynamicEibfsTraceOverlay>>,
        /// Overlay after forward application.
        after: Option<Box<DynamicEibfsTraceOverlay>>,
    },
    /// Absolute metric mutation.
    Metric {
        /// Stable metric identity.
        metric: FlowTraceMetricId,
        /// Required value before forward application.
        before: u128,
        /// Value after forward application.
        after: u128,
    },
}

/// One committed pedagogical event and its atomic state transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowTraceEvent {
    /// Canonical decimal-compatible event identity, starting at one.
    pub event_id: u64,
    /// Owning phase event, or `None` before the first phase.
    pub parent_phase_id: Option<u64>,
    /// Revision-owned event catalog identity.
    pub catalog_id: String,
    /// Coarsest UI granularity at which the event is directly shown.
    pub minimum_granularity: TraceGranularityV1,
    /// Sorted, deduplicated source-owned focus identities for this primitive.
    ///
    /// These identities need not contain every snapshot change. Renderers use
    /// patch-derived changed identities for side effects and this list only for
    /// the operation the algorithm is currently inspecting or selecting.
    pub entity_refs: Vec<FlowTraceEntityRef>,
    /// Atomic reversible state transaction.
    pub patches: Vec<FlowTracePatch>,
    /// Revision-owned pseudocode line identity.
    pub pseudocode_line: String,
    /// Optional exact scalar that explains the current selector or phase.
    pub detail: Option<FlowTraceEventDetail>,
}

/// Exact algorithm-specific scalar attached to one immutable event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowTraceEventDetail {
    /// Short revision-owned semantic label such as `delta` or `bottleneck`.
    pub label: String,
    /// Exact signed value serialized without JavaScript number conversion.
    pub value: i128,
}

/// Metadata supplied by an algorithm for one state transition.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FlowTraceEventMetadata {
    pub(crate) catalog_id: &'static str,
    pub(crate) minimum_granularity: TraceGranularityV1,
    pub(crate) pseudocode_line: &'static str,
}

/// Direction used to apply a reversible event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowTraceDirection {
    /// Applies `before -> after`.
    Forward,
    /// Applies `after -> before` in reverse patch order.
    Reverse,
}

/// Trace construction or replay invariant failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FlowTraceError {
    /// Snapshot shape does not match the immutable canonical graph.
    #[error("flow trace snapshot shape does not match the graph")]
    SnapshotShape,
    /// A patch refers to an entity absent from the canonical graph/snapshot.
    #[error("flow trace patch refers to a missing stable entity")]
    MissingEntity,
    /// A patch's required before/after value did not match current state.
    #[error("flow trace patch precondition failed")]
    Precondition,
    /// A transaction patches the same state cell more than once.
    #[error("flow trace transaction contains a duplicate patch target")]
    DuplicatePatchTarget,
    /// One atomic event exceeds its declared patch bound.
    #[error("flow trace event exceeds the atomic patch limit")]
    PatchLimit,
    /// One event exceeds its declared entity-reference bound.
    #[error("flow trace event exceeds the entity-reference limit")]
    EntityRefLimit,
    /// Canonical event identity overflowed.
    #[error("flow trace event identity overflow")]
    EventIdOverflow,
    /// An exact trace metric exceeded its integer domain.
    #[error("flow trace metric overflow")]
    MetricOverflow,
    /// An algorithm-specific eager trace projection exceeded its event budget.
    #[error("flow trace event budget exceeded")]
    EventLimit,
}

/// Snapshot-diff recorder shared by all flow kernels.
pub(crate) struct FlowTraceRecorder<'graph> {
    graph: &'graph FlowNetwork,
    base: FlowTraceSnapshot,
    current: FlowTraceSnapshot,
    events: Vec<FlowTraceEvent>,
    current_phase: Option<u64>,
}

impl<'graph> FlowTraceRecorder<'graph> {
    /// Returns the number of committed event transitions.
    pub(crate) fn event_count(&self) -> usize {
        self.events.len()
    }

    pub(crate) fn new(
        graph: &'graph FlowNetwork,
        base: FlowTraceSnapshot,
    ) -> Result<Self, FlowTraceError> {
        validate_snapshot_shape(graph, &base)?;
        Ok(Self {
            graph,
            base: base.clone(),
            current: base,
            events: Vec::new(),
            current_phase: None,
        })
    }

    pub(crate) fn record_transition(
        &mut self,
        metadata: FlowTraceEventMetadata,
        next: &FlowTraceSnapshot,
    ) -> Result<(), FlowTraceError> {
        self.record_transition_with_detail(metadata, next, None)
    }

    pub(crate) fn record_transition_with_detail(
        &mut self,
        metadata: FlowTraceEventMetadata,
        next: &FlowTraceSnapshot,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), FlowTraceError> {
        self.record_transition_with_detail_and_refs(metadata, next, detail, None)
    }

    /// Records a transition with the exact source-owned focus identities.
    ///
    /// Snapshot patches still carry every reversible state change. The focus
    /// list is intentionally independent: clearing stale search labels or
    /// replacing an aggregate overlay must not make every changed entity look
    /// like the primitive currently inspected by the algorithm.
    pub(crate) fn record_transition_with_detail_and_focus(
        &mut self,
        metadata: FlowTraceEventMetadata,
        next: &FlowTraceSnapshot,
        detail: Option<(&'static str, i128)>,
        entity_refs: Vec<FlowTraceEntityRef>,
    ) -> Result<(), FlowTraceError> {
        self.record_transition_with_detail_and_refs(metadata, next, detail, Some(entity_refs))
    }

    /// Records one exact metric primitive against the stable entity inspected
    /// by the kernel. This advances only the recorder's replay state; callers
    /// still own the matching algorithm counter and the next captured snapshot
    /// must agree with it.
    pub(crate) fn record_metric_observation(
        &mut self,
        metadata: FlowTraceEventMetadata,
        metric: FlowTraceMetricId,
        entity_ref: FlowTraceEntityRef,
    ) -> Result<(), FlowTraceError> {
        self.record_metric_observation_with_detail(metadata, metric, entity_ref, None)
    }

    /// Records one exact metric primitive and a source-owned scalar that the
    /// graph renderer can anchor to the inspected entity.
    pub(crate) fn record_metric_observation_with_detail(
        &mut self,
        metadata: FlowTraceEventMetadata,
        metric: FlowTraceMetricId,
        entity_ref: FlowTraceEntityRef,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), FlowTraceError> {
        let mut next = self.current.clone();
        let value = metric_value_mut(&mut next.metrics, metric);
        *value = value.checked_add(1).ok_or(FlowTraceError::MetricOverflow)?;
        self.record_transition_with_detail_and_refs(metadata, &next, detail, Some(vec![entity_ref]))
    }

    fn record_transition_with_detail_and_refs(
        &mut self,
        metadata: FlowTraceEventMetadata,
        next: &FlowTraceSnapshot,
        detail: Option<(&'static str, i128)>,
        explicit_entity_refs: Option<Vec<FlowTraceEntityRef>>,
    ) -> Result<(), FlowTraceError> {
        validate_snapshot_shape(self.graph, next)?;
        let patches = diff_snapshots(self.graph, &self.current, next)?;
        if patches.len() > MAX_FLOW_TRACE_PATCHES_PER_EVENT {
            return Err(FlowTraceError::PatchLimit);
        }
        let mut entity_refs = explicit_entity_refs.unwrap_or_else(|| entity_refs(&patches));
        entity_refs.sort_unstable();
        entity_refs.dedup();
        if entity_refs.len() > MAX_FLOW_TRACE_ENTITY_REFS_PER_EVENT {
            return Err(FlowTraceError::EntityRefLimit);
        }
        let event_id = u64::try_from(self.events.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(FlowTraceError::EventIdOverflow)?;
        let is_phase = metadata.minimum_granularity == TraceGranularityV1::Phase;
        let event = FlowTraceEvent {
            event_id,
            parent_phase_id: if is_phase { None } else { self.current_phase },
            catalog_id: metadata.catalog_id.to_owned(),
            minimum_granularity: metadata.minimum_granularity,
            entity_refs,
            patches,
            pseudocode_line: metadata.pseudocode_line.to_owned(),
            detail: detail.map(|(label, value)| FlowTraceEventDetail {
                label: label.to_owned(),
                value,
            }),
        };
        apply_trace_event(
            self.graph,
            &mut self.current,
            &event,
            FlowTraceDirection::Forward,
        )?;
        if self.current != *next {
            return Err(FlowTraceError::Precondition);
        }
        if is_phase {
            self.current_phase = Some(event_id);
        }
        self.events.push(event);
        Ok(())
    }

    pub(crate) fn finish(self) -> (FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot) {
        (self.base, self.events, self.current)
    }
}

/// Applies an event transaction only after every patch precondition succeeds.
///
/// # Errors
///
/// Rejects malformed snapshot shape, duplicate targets, missing entities, or
/// any before/after mismatch. The supplied snapshot is unchanged on error.
pub fn apply_trace_event(
    graph: &FlowNetwork,
    snapshot: &mut FlowTraceSnapshot,
    event: &FlowTraceEvent,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    validate_snapshot_shape(graph, snapshot)?;
    if event.patches.len() > MAX_FLOW_TRACE_PATCHES_PER_EVENT {
        return Err(FlowTraceError::PatchLimit);
    }
    let mut targets = BTreeSet::new();
    for patch in &event.patches {
        if !targets.insert(patch_target(patch)) {
            return Err(FlowTraceError::DuplicatePatchTarget);
        }
    }
    let mut candidate = snapshot.clone();
    let patches: Box<dyn Iterator<Item = &FlowTracePatch>> = match direction {
        FlowTraceDirection::Forward => Box::new(event.patches.iter()),
        FlowTraceDirection::Reverse => Box::new(event.patches.iter().rev()),
    };
    for patch in patches {
        apply_patch(graph, &mut candidate, patch, direction)?;
    }
    validate_snapshot_shape(graph, &candidate)?;
    *snapshot = candidate;
    Ok(())
}

fn validate_snapshot_shape(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), FlowTraceError> {
    if snapshot.edge_capacities.len() != graph.edges().len()
        || snapshot.flows.len() != graph.edges().len()
        || snapshot.residual_capacities.len() != graph.edges().len() * 2
        || snapshot.node_labels.len() != graph.nodes().len()
        || (!snapshot.remaining_divergence.is_empty()
            && snapshot.remaining_divergence.len() != graph.nodes().len())
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    validate_dynamic_eibfs_overlay(graph, snapshot)?;
    for ((edge, &capacity), &flow) in graph
        .edges()
        .iter()
        .zip(&snapshot.edge_capacities)
        .zip(&snapshot.flows)
    {
        let temporary_over_capacity = flow > capacity
            && dynamic_eibfs_allows_over_capacity(snapshot, edge.id(), capacity, flow);
        if capacity < edge.lower()
            || capacity > edge.capacity()
            || flow < edge.lower()
            || flow > edge.capacity()
            || flow > capacity && !temporary_over_capacity
        {
            return Err(FlowTraceError::SnapshotShape);
        }
    }
    validate_residual_capacities(graph, snapshot)?;
    if snapshot
        .search_order
        .iter()
        .any(|id| graph.node_index(id).is_none())
        || snapshot
            .active_path
            .iter()
            .any(|id| graph.edge_index(id.original_edge()).is_none())
        || snapshot
            .fixed_edges
            .iter()
            .any(|id| graph.edge_index(id).is_none())
        || snapshot
            .forest_arcs
            .iter()
            .any(|id| graph.edge_index(id.original_edge()).is_none())
        || snapshot
            .strong_nodes
            .iter()
            .any(|id| graph.node_index(id).is_none())
    {
        return Err(FlowTraceError::MissingEntity);
    }
    validate_eibfs_overlay(graph, snapshot)?;
    if snapshot
        .fixed_edges
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
        || snapshot
            .forest_arcs
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || snapshot
            .strong_nodes
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    Ok(())
}

fn validate_residual_capacities(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), FlowTraceError> {
    let expected = graph
        .edges()
        .iter()
        .flat_map(|edge| {
            [ResidualDirection::Forward, ResidualDirection::Reverse]
                .map(|direction| ResidualArcId::new(edge.id().clone(), direction))
        })
        .collect::<BTreeSet<_>>();
    let actual = snapshot
        .residual_capacities
        .iter()
        .map(|(id, capacity)| (id.clone(), *capacity))
        .collect::<BTreeMap<_, _>>();
    if actual.len() != snapshot.residual_capacities.len()
        || actual.len() != expected.len()
        || expected.iter().any(|id| !actual.contains_key(id))
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    for ((edge, &capacity), &flow) in graph
        .edges()
        .iter()
        .zip(&snapshot.edge_capacities)
        .zip(&snapshot.flows)
    {
        for (direction, expected_capacity) in [
            (ResidualDirection::Forward, capacity.saturating_sub(flow)),
            (ResidualDirection::Reverse, flow - edge.lower()),
        ] {
            let id = ResidualArcId::new(edge.id().clone(), direction);
            if actual.get(&id).copied() != Some(expected_capacity) {
                return Err(FlowTraceError::SnapshotShape);
            }
        }
    }
    Ok(())
}

fn diff_snapshots(
    graph: &FlowNetwork,
    before: &FlowTraceSnapshot,
    after: &FlowTraceSnapshot,
) -> Result<Vec<FlowTracePatch>, FlowTraceError> {
    validate_snapshot_shape(graph, before)?;
    validate_snapshot_shape(graph, after)?;
    let mut patches = Vec::new();
    diff_edge_state(graph, before, after, &mut patches);
    for ((before_id, before_capacity), (after_id, after_capacity)) in before
        .residual_capacities
        .iter()
        .zip(&after.residual_capacities)
    {
        if before_id != after_id {
            return Err(FlowTraceError::SnapshotShape);
        }
        if before_capacity != after_capacity {
            patches.push(FlowTracePatch::ResidualCapacity {
                arc: before_id.clone(),
                before: *before_capacity,
                after: *after_capacity,
            });
        }
    }
    for (((node, before), after), _) in graph
        .nodes()
        .iter()
        .zip(&before.node_labels)
        .zip(&after.node_labels)
        .zip(0..)
    {
        if before != after {
            patches.push(FlowTracePatch::NodeLabel {
                node: node.id().clone(),
                before: *before,
                after: *after,
            });
        }
    }
    if before.search_order != after.search_order {
        patches.push(FlowTracePatch::SearchOrder {
            before: before.search_order.clone(),
            after: after.search_order.clone(),
        });
    }
    if before.active_path != after.active_path {
        patches.push(FlowTracePatch::ActivePath {
            before: before.active_path.clone(),
            after: after.active_path.clone(),
        });
    }
    if before.fixed_edges != after.fixed_edges {
        patches.push(FlowTracePatch::FixedEdges {
            before: before.fixed_edges.clone(),
            after: after.fixed_edges.clone(),
        });
    }
    if before.forest_arcs != after.forest_arcs {
        patches.push(FlowTracePatch::ForestArcs {
            before: before.forest_arcs.clone(),
            after: after.forest_arcs.clone(),
        });
    }
    if before.strong_nodes != after.strong_nodes {
        patches.push(FlowTracePatch::StrongNodes {
            before: before.strong_nodes.clone(),
            after: after.strong_nodes.clone(),
        });
    }
    if before.eibfs_overlay != after.eibfs_overlay {
        patches.push(FlowTracePatch::EibfsOverlay {
            before: before.eibfs_overlay.clone(),
            after: after.eibfs_overlay.clone(),
        });
    }
    if before.dynamic_eibfs_overlay != after.dynamic_eibfs_overlay {
        patches.push(FlowTracePatch::DynamicEibfsOverlay {
            before: before.dynamic_eibfs_overlay.clone().map(Box::new),
            after: after.dynamic_eibfs_overlay.clone().map(Box::new),
        });
    }
    if before.remaining_divergence.len() != after.remaining_divergence.len() {
        return Err(FlowTraceError::SnapshotShape);
    }
    for ((node, &before), &after) in graph
        .nodes()
        .iter()
        .zip(&before.remaining_divergence)
        .zip(&after.remaining_divergence)
    {
        if before != after {
            patches.push(FlowTracePatch::RemainingDivergence {
                node: node.id().clone(),
                before,
                after,
            });
        }
    }
    diff_metrics(before.metrics, after.metrics, &mut patches);
    Ok(patches)
}

fn diff_edge_state(
    graph: &FlowNetwork,
    before: &FlowTraceSnapshot,
    after: &FlowTraceSnapshot,
    patches: &mut Vec<FlowTracePatch>,
) {
    for ((edge, &before), &after) in graph
        .edges()
        .iter()
        .zip(&before.edge_capacities)
        .zip(&after.edge_capacities)
    {
        if before != after {
            patches.push(FlowTracePatch::EdgeCapacity {
                edge: edge.id().clone(),
                before,
                after,
            });
        }
    }
    for ((edge, &before), &after) in graph.edges().iter().zip(&before.flows).zip(&after.flows) {
        if before != after {
            patches.push(FlowTracePatch::EdgeFlow {
                edge: edge.id().clone(),
                before,
                after,
            });
        }
    }
}

fn diff_metrics(
    before: FlowTraceMetrics,
    after: FlowTraceMetrics,
    patches: &mut Vec<FlowTracePatch>,
) {
    for metric in [
        FlowTraceMetricId::BfsRuns,
        FlowTraceMetricId::RelaxationPasses,
        FlowTraceMetricId::ResidualArcScans,
        FlowTraceMetricId::Augmentations,
        FlowTraceMetricId::PathSearches,
        FlowTraceMetricId::ScalingPhases,
        FlowTraceMetricId::BlockingFlowPhases,
        FlowTraceMetricId::Relabels,
        FlowTraceMetricId::Retreats,
        FlowTraceMetricId::ReverseBfsRuns,
        FlowTraceMetricId::GapTerminations,
        FlowTraceMetricId::Pushes,
        FlowTraceMetricId::SaturatingPushes,
        FlowTraceMetricId::NonsaturatingPushes,
        FlowTraceMetricId::Discharges,
        FlowTraceMetricId::ActiveVertexSelections,
    ] {
        let before_value = metric_value(before, metric);
        let after_value = metric_value(after, metric);
        if before_value != after_value {
            patches.push(FlowTracePatch::Metric {
                metric,
                before: before_value,
                after: after_value,
            });
        }
    }
}

fn entity_refs(patches: &[FlowTracePatch]) -> Vec<FlowTraceEntityRef> {
    let mut refs = BTreeSet::new();
    for patch in patches {
        match patch {
            FlowTracePatch::EdgeCapacity { edge, .. } | FlowTracePatch::EdgeFlow { edge, .. } => {
                refs.insert(FlowTraceEntityRef::Edge(edge.clone()));
            }
            FlowTracePatch::ResidualCapacity { arc, .. } => {
                refs.insert(FlowTraceEntityRef::Edge(arc.original_edge().clone()));
                refs.insert(FlowTraceEntityRef::ResidualArc(arc.clone()));
            }
            FlowTracePatch::NodeLabel { node, .. }
            | FlowTracePatch::RemainingDivergence { node, .. } => {
                refs.insert(FlowTraceEntityRef::Node(node.clone()));
            }
            FlowTracePatch::SearchOrder { after, .. }
            | FlowTracePatch::StrongNodes { after, .. } => {
                refs.extend(after.iter().cloned().map(FlowTraceEntityRef::Node));
            }
            FlowTracePatch::ActivePath { after, .. } | FlowTracePatch::ForestArcs { after, .. } => {
                for arc in after {
                    refs.insert(FlowTraceEntityRef::Edge(arc.original_edge().clone()));
                    refs.insert(FlowTraceEntityRef::ResidualArc(arc.clone()));
                }
            }
            FlowTracePatch::FixedEdges { before, after } => {
                refs.extend(
                    before
                        .iter()
                        .chain(after)
                        .cloned()
                        .map(FlowTraceEntityRef::Edge),
                );
            }
            FlowTracePatch::DynamicEibfsOverlay { before, after } => {
                for overlay in before.iter().chain(after) {
                    if let Some(edge) = &overlay.changed_edge {
                        refs.insert(FlowTraceEntityRef::Edge(edge.clone()));
                    }
                }
            }
            FlowTracePatch::Metric { .. } | FlowTracePatch::EibfsOverlay { .. } => {}
        }
    }
    refs.into_iter().collect()
}

fn patch_target(patch: &FlowTracePatch) -> String {
    match patch {
        FlowTracePatch::EdgeCapacity { edge, .. } => {
            format!("edge-capacity:{}", edge.as_str())
        }
        FlowTracePatch::EdgeFlow { edge, .. } => format!("edge-flow:{}", edge.as_str()),
        FlowTracePatch::ResidualCapacity { arc, .. } => format!(
            "residual:{}:{}",
            arc.original_edge().as_str(),
            match arc.direction() {
                ResidualDirection::Forward => "forward",
                ResidualDirection::Reverse => "reverse",
            }
        ),
        FlowTracePatch::NodeLabel { node, .. } => format!("label:{}", node.as_str()),
        FlowTracePatch::SearchOrder { .. } => "search-order".to_owned(),
        FlowTracePatch::ActivePath { .. } => "active-path".to_owned(),
        FlowTracePatch::FixedEdges { .. } => "fixed-edges".to_owned(),
        FlowTracePatch::ForestArcs { .. } => "forest-arcs".to_owned(),
        FlowTracePatch::StrongNodes { .. } => "strong-nodes".to_owned(),
        FlowTracePatch::RemainingDivergence { node, .. } => {
            format!("remaining:{}", node.as_str())
        }
        FlowTracePatch::Metric { metric, .. } => format!("metric:{metric:?}"),
        FlowTracePatch::EibfsOverlay { .. } => "eibfs-overlay".to_owned(),
        FlowTracePatch::DynamicEibfsOverlay { .. } => "dynamic-eibfs-overlay".to_owned(),
    }
}

fn apply_patch(
    graph: &FlowNetwork,
    snapshot: &mut FlowTraceSnapshot,
    patch: &FlowTracePatch,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    match patch {
        FlowTracePatch::EdgeCapacity {
            edge,
            before,
            after,
        } => apply_edge_capacity_patch(graph, snapshot, edge, *before, *after, direction),
        FlowTracePatch::EdgeFlow {
            edge,
            before,
            after,
        } => apply_edge_flow_patch(graph, snapshot, edge, *before, *after, direction),
        FlowTracePatch::ResidualCapacity { arc, before, after } => {
            let (_, value) = snapshot
                .residual_capacities
                .iter_mut()
                .find(|(id, _)| id == arc)
                .ok_or(FlowTraceError::MissingEntity)?;
            replace(value, *before, *after, direction)
        }
        FlowTracePatch::NodeLabel {
            node,
            before,
            after,
        } => {
            let index = graph
                .node_index(node)
                .ok_or(FlowTraceError::MissingEntity)?
                .as_usize();
            replace(
                snapshot
                    .node_labels
                    .get_mut(index)
                    .ok_or(FlowTraceError::MissingEntity)?,
                *before,
                *after,
                direction,
            )
        }
        FlowTracePatch::SearchOrder { before, after } => replace(
            &mut snapshot.search_order,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::ActivePath { before, after } => replace(
            &mut snapshot.active_path,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::FixedEdges { before, after } => replace(
            &mut snapshot.fixed_edges,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::ForestArcs { before, after } => replace(
            &mut snapshot.forest_arcs,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::StrongNodes { before, after } => replace(
            &mut snapshot.strong_nodes,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::RemainingDivergence {
            node,
            before,
            after,
        } => apply_remaining_divergence(graph, snapshot, node, *before, *after, direction),
        FlowTracePatch::EibfsOverlay { before, after } => replace(
            &mut snapshot.eibfs_overlay,
            before.clone(),
            after.clone(),
            direction,
        ),
        FlowTracePatch::DynamicEibfsOverlay { before, after } => replace(
            &mut snapshot.dynamic_eibfs_overlay,
            before.as_deref().cloned(),
            after.as_deref().cloned(),
            direction,
        ),
        FlowTracePatch::Metric {
            metric,
            before,
            after,
        } => replace(
            metric_value_mut(&mut snapshot.metrics, *metric),
            *before,
            *after,
            direction,
        ),
    }
}

fn apply_edge_capacity_patch(
    graph: &FlowNetwork,
    snapshot: &mut FlowTraceSnapshot,
    edge: &EdgeId,
    before: u64,
    after: u64,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    let index = graph
        .edge_index(edge)
        .ok_or(FlowTraceError::MissingEntity)?
        .as_usize();
    replace(
        snapshot
            .edge_capacities
            .get_mut(index)
            .ok_or(FlowTraceError::MissingEntity)?,
        before,
        after,
        direction,
    )
}

fn apply_edge_flow_patch(
    graph: &FlowNetwork,
    snapshot: &mut FlowTraceSnapshot,
    edge: &EdgeId,
    before: u64,
    after: u64,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    let index = graph
        .edge_index(edge)
        .ok_or(FlowTraceError::MissingEntity)?
        .as_usize();
    replace(
        snapshot
            .flows
            .get_mut(index)
            .ok_or(FlowTraceError::MissingEntity)?,
        before,
        after,
        direction,
    )
}

fn validate_dynamic_eibfs_overlay(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), FlowTraceError> {
    let Some(overlay) = snapshot.dynamic_eibfs_overlay.as_ref() else {
        return Ok(());
    };
    if overlay.update_total == 0
        || overlay.update_total > 256
        || overlay.update_index > overlay.update_total
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    if overlay.update_index == 0 {
        if overlay.changed_edge.is_some()
            || overlay.old_capacity.is_some()
            || overlay.new_capacity.is_some()
        {
            return Err(FlowTraceError::SnapshotShape);
        }
    } else {
        let (Some(edge), Some(old_capacity), Some(new_capacity)) = (
            overlay.changed_edge.as_ref(),
            overlay.old_capacity,
            overlay.new_capacity,
        ) else {
            return Err(FlowTraceError::SnapshotShape);
        };
        let edge_index = graph
            .edge_index(edge)
            .ok_or(FlowTraceError::MissingEntity)?;
        if old_capacity
            > graph
                .edge(edge_index)
                .ok_or(FlowTraceError::SnapshotShape)?
                .capacity()
            || snapshot.edge_capacities.get(edge_index.as_usize()) != Some(&new_capacity)
        {
            return Err(FlowTraceError::SnapshotShape);
        }
    }
    let valid_stage = match overlay.stage {
        DynamicEibfsTraceStage::InitialSolve => {
            overlay.update_index == 0
                && overlay.violation.is_none()
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_some()
        }
        DynamicEibfsTraceStage::ApplyUpdate | DynamicEibfsTraceStage::RepairForest => {
            overlay.update_index > 0
                && overlay.violation.is_none()
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_some()
        }
        DynamicEibfsTraceStage::RepairCapacity => {
            overlay.update_index > 0
                && overlay.violation == Some(DynamicEibfsTraceViolation::OverCapacity)
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_some()
        }
        DynamicEibfsTraceStage::RepairViolation => {
            overlay.update_index > 0
                && matches!(
                    overlay.violation,
                    Some(
                        DynamicEibfsTraceViolation::Bridge
                            | DynamicEibfsTraceViolation::Label
                            | DynamicEibfsTraceViolation::CurrentArc
                            | DynamicEibfsTraceViolation::Boundary
                    )
                )
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_some()
        }
        DynamicEibfsTraceStage::ContinueSolve
        | DynamicEibfsTraceStage::ResumeReusablePseudoflow => {
            overlay.violation.is_none()
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_some()
        }
        DynamicEibfsTraceStage::PrefixRecovery => {
            overlay.violation.is_none()
                && overlay.prefix_value.is_none()
                && snapshot.eibfs_overlay.is_none()
        }
        DynamicEibfsTraceStage::PrefixCertified => {
            overlay.violation.is_none()
                && overlay.prefix_value.is_some()
                && snapshot.eibfs_overlay.is_none()
        }
    };
    valid_stage
        .then_some(())
        .ok_or(FlowTraceError::SnapshotShape)
}

fn dynamic_eibfs_allows_over_capacity(
    snapshot: &FlowTraceSnapshot,
    edge: &EdgeId,
    capacity: u64,
    flow: u64,
) -> bool {
    snapshot
        .dynamic_eibfs_overlay
        .as_ref()
        .is_some_and(|overlay| {
            overlay.stage == DynamicEibfsTraceStage::ApplyUpdate
                && overlay.changed_edge.as_ref() == Some(edge)
                && overlay.old_capacity.is_some_and(|old| flow <= old)
                && overlay.new_capacity == Some(capacity)
                && overlay.old_capacity.is_some_and(|old| old > capacity)
        })
}

fn validate_eibfs_overlay(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), FlowTraceError> {
    let Some(overlay) = snapshot.eibfs_overlay.as_ref() else {
        return Ok(());
    };
    if !snapshot.forest_arcs.is_empty() || !snapshot.strong_nodes.is_empty() {
        return Err(FlowTraceError::SnapshotShape);
    }
    validate_eibfs_node_states(graph, snapshot, overlay)?;
    let node_indices = overlay
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.node.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let child_parent = validate_eibfs_forest_arcs(graph, snapshot, overlay, &node_indices)?;
    validate_eibfs_roots(overlay, &child_parent)
}

fn validate_eibfs_node_states(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
    overlay: &EibfsTraceOverlay,
) -> Result<(), FlowTraceError> {
    if overlay.nodes.len() != graph.nodes().len()
        || snapshot.remaining_divergence.len() != graph.nodes().len()
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    for (index, (expected, actual)) in graph.nodes().iter().zip(&overlay.nodes).enumerate() {
        let expected_label = match actual.membership {
            EibfsTraceMembership::Free => None,
            EibfsTraceMembership::Source => Some(
                i128::try_from(actual.source_label).map_err(|_| FlowTraceError::SnapshotShape)?,
            ),
            EibfsTraceMembership::Sink => Some(
                i128::try_from(actual.sink_label)
                    .map_err(|_| FlowTraceError::SnapshotShape)?
                    .checked_add(1)
                    .and_then(i128::checked_neg)
                    .ok_or(FlowTraceError::SnapshotShape)?,
            ),
        };
        if expected.id() != &actual.node
            || snapshot.node_labels.get(index) != Some(&expected_label)
            || snapshot.remaining_divergence.get(index) != Some(&actual.imbalance)
            || matches!(actual.membership, EibfsTraceMembership::Free)
                && (actual.orphan || actual.root_kind != EibfsTraceRootKind::None)
        {
            return Err(FlowTraceError::SnapshotShape);
        }
    }
    Ok(())
}

fn validate_eibfs_forest_arcs(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
    overlay: &EibfsTraceOverlay,
    node_indices: &BTreeMap<NodeId, usize>,
) -> Result<BTreeMap<NodeId, NodeId>, FlowTraceError> {
    let mut child_parent = BTreeMap::new();
    for relation in &overlay.forest_arcs {
        if relation.side == EibfsTraceMembership::Free
            || relation.parent == relation.child
            || graph.node_index(&relation.parent).is_none()
            || graph.node_index(&relation.child).is_none()
            || graph
                .edge_index(relation.admissible_residual.original_edge())
                .is_none()
            || child_parent
                .insert(relation.child.clone(), relation.parent.clone())
                .is_some()
        {
            return Err(FlowTraceError::SnapshotShape);
        }
        validate_eibfs_forest_arc(graph, snapshot, overlay, node_indices, relation)?;
    }
    Ok(child_parent)
}

fn validate_eibfs_forest_arc(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
    overlay: &EibfsTraceOverlay,
    node_indices: &BTreeMap<NodeId, usize>,
    relation: &EibfsTraceForestArc,
) -> Result<(), FlowTraceError> {
    let parent = overlay
        .nodes
        .get(
            *node_indices
                .get(&relation.parent)
                .ok_or(FlowTraceError::SnapshotShape)?,
        )
        .ok_or(FlowTraceError::SnapshotShape)?;
    let child = overlay
        .nodes
        .get(
            *node_indices
                .get(&relation.child)
                .ok_or(FlowTraceError::SnapshotShape)?,
        )
        .ok_or(FlowTraceError::SnapshotShape)?;
    let edge_index = graph
        .edge_index(relation.admissible_residual.original_edge())
        .ok_or(FlowTraceError::SnapshotShape)?;
    let edge = graph
        .edge(edge_index)
        .ok_or(FlowTraceError::SnapshotShape)?;
    let (residual_from, residual_to) = match relation.admissible_residual.direction() {
        ResidualDirection::Forward => (edge.from(), edge.to()),
        ResidualDirection::Reverse => (edge.to(), edge.from()),
    };
    let residual_from = graph
        .node(residual_from)
        .ok_or(FlowTraceError::SnapshotShape)?
        .id();
    let residual_to = graph
        .node(residual_to)
        .ok_or(FlowTraceError::SnapshotShape)?
        .id();
    let residual_capacity = snapshot
        .residual_capacities
        .iter()
        .find_map(|(id, capacity)| (id == &relation.admissible_residual).then_some(*capacity))
        .ok_or(FlowTraceError::SnapshotShape)?;
    let transiently_invalidated_parent = residual_capacity == 0
        && snapshot
            .dynamic_eibfs_overlay
            .as_ref()
            .is_some_and(|dynamic| {
                dynamic.stage == DynamicEibfsTraceStage::ApplyUpdate
                    && dynamic.changed_edge.as_ref()
                        == Some(relation.admissible_residual.original_edge())
            });
    let admissible = match relation.side {
        EibfsTraceMembership::Source => {
            residual_from == &relation.parent
                && residual_to == &relation.child
                && parent.source_label.checked_add(1) == Some(child.source_label)
        }
        EibfsTraceMembership::Sink => {
            residual_from == &relation.child
                && residual_to == &relation.parent
                && parent.sink_label.checked_add(1) == Some(child.sink_label)
        }
        EibfsTraceMembership::Free => false,
    };
    if parent.membership != relation.side
        || child.membership != relation.side
        || child.orphan
        || child.root_kind != EibfsTraceRootKind::None
        || residual_capacity == 0 && !transiently_invalidated_parent
        || !admissible
    {
        return Err(FlowTraceError::SnapshotShape);
    }
    Ok(())
}

fn validate_eibfs_roots(
    overlay: &EibfsTraceOverlay,
    child_parent: &BTreeMap<NodeId, NodeId>,
) -> Result<(), FlowTraceError> {
    let mut source_roots = 0;
    let mut sink_roots = 0;
    for node in &overlay.nodes {
        let has_parent = child_parent.contains_key(&node.node);
        let valid = match node.root_kind {
            EibfsTraceRootKind::None => {
                if node.membership == EibfsTraceMembership::Free {
                    !node.orphan && !has_parent
                } else if node.orphan {
                    !has_parent
                } else {
                    has_parent
                }
            }
            EibfsTraceRootKind::Source => {
                source_roots += 1;
                node.membership == EibfsTraceMembership::Source
                    && node.source_label == 0
                    && !node.orphan
                    && !has_parent
            }
            EibfsTraceRootKind::Sink => {
                sink_roots += 1;
                node.membership == EibfsTraceMembership::Sink
                    && node.sink_label == 0
                    && !node.orphan
                    && !has_parent
            }
            EibfsTraceRootKind::Excess => {
                node.membership == EibfsTraceMembership::Source
                    && node.imbalance > 0
                    && !node.orphan
                    && !has_parent
            }
            EibfsTraceRootKind::Deficit => {
                node.membership == EibfsTraceMembership::Sink
                    && node.imbalance < 0
                    && !node.orphan
                    && !has_parent
            }
        };
        if !valid {
            return Err(FlowTraceError::SnapshotShape);
        }
        let mut ancestry = BTreeSet::new();
        let mut ancestor = Some(&node.node);
        while let Some(current) = ancestor {
            if !ancestry.insert(current.clone()) {
                return Err(FlowTraceError::SnapshotShape);
            }
            ancestor = child_parent.get(current);
        }
    }
    if source_roots != 1 || sink_roots != 1 {
        return Err(FlowTraceError::SnapshotShape);
    }
    Ok(())
}

fn apply_remaining_divergence(
    graph: &FlowNetwork,
    snapshot: &mut FlowTraceSnapshot,
    node: &NodeId,
    before: i128,
    after: i128,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    let index = graph
        .node_index(node)
        .ok_or(FlowTraceError::MissingEntity)?
        .as_usize();
    replace(
        snapshot
            .remaining_divergence
            .get_mut(index)
            .ok_or(FlowTraceError::MissingEntity)?,
        before,
        after,
        direction,
    )
}

fn replace<T: Eq>(
    target: &mut T,
    before: T,
    after: T,
    direction: FlowTraceDirection,
) -> Result<(), FlowTraceError> {
    let (expected, replacement) = match direction {
        FlowTraceDirection::Forward => (before, after),
        FlowTraceDirection::Reverse => (after, before),
    };
    if *target != expected {
        return Err(FlowTraceError::Precondition);
    }
    *target = replacement;
    Ok(())
}

const fn metric_value(metrics: FlowTraceMetrics, metric: FlowTraceMetricId) -> u128 {
    match metric {
        FlowTraceMetricId::BfsRuns => metrics.bfs_runs,
        FlowTraceMetricId::RelaxationPasses => metrics.relaxation_passes,
        FlowTraceMetricId::ResidualArcScans => metrics.residual_arc_scans,
        FlowTraceMetricId::Augmentations => metrics.augmentations,
        FlowTraceMetricId::PathSearches => metrics.path_searches,
        FlowTraceMetricId::ScalingPhases => metrics.scaling_phases,
        FlowTraceMetricId::BlockingFlowPhases => metrics.blocking_flow_phases,
        FlowTraceMetricId::Relabels => metrics.relabels,
        FlowTraceMetricId::Retreats => metrics.retreats,
        FlowTraceMetricId::ReverseBfsRuns => metrics.reverse_bfs_runs,
        FlowTraceMetricId::GapTerminations => metrics.gap_terminations,
        FlowTraceMetricId::Pushes => metrics.pushes,
        FlowTraceMetricId::SaturatingPushes => metrics.saturating_pushes,
        FlowTraceMetricId::NonsaturatingPushes => metrics.nonsaturating_pushes,
        FlowTraceMetricId::Discharges => metrics.discharges,
        FlowTraceMetricId::ActiveVertexSelections => metrics.active_vertex_selections,
    }
}

const fn metric_value_mut(metrics: &mut FlowTraceMetrics, metric: FlowTraceMetricId) -> &mut u128 {
    match metric {
        FlowTraceMetricId::BfsRuns => &mut metrics.bfs_runs,
        FlowTraceMetricId::RelaxationPasses => &mut metrics.relaxation_passes,
        FlowTraceMetricId::ResidualArcScans => &mut metrics.residual_arc_scans,
        FlowTraceMetricId::Augmentations => &mut metrics.augmentations,
        FlowTraceMetricId::PathSearches => &mut metrics.path_searches,
        FlowTraceMetricId::ScalingPhases => &mut metrics.scaling_phases,
        FlowTraceMetricId::BlockingFlowPhases => &mut metrics.blocking_flow_phases,
        FlowTraceMetricId::Relabels => &mut metrics.relabels,
        FlowTraceMetricId::Retreats => &mut metrics.retreats,
        FlowTraceMetricId::ReverseBfsRuns => &mut metrics.reverse_bfs_runs,
        FlowTraceMetricId::GapTerminations => &mut metrics.gap_terminations,
        FlowTraceMetricId::Pushes => &mut metrics.pushes,
        FlowTraceMetricId::SaturatingPushes => &mut metrics.saturating_pushes,
        FlowTraceMetricId::NonsaturatingPushes => &mut metrics.nonsaturating_pushes,
        FlowTraceMetricId::Discharges => &mut metrics.discharges,
        FlowTraceMetricId::ActiveVertexSelections => &mut metrics.active_vertex_selections,
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{FlowNode, UnresolvedFlowEdge};

    use super::*;

    fn graph() -> FlowNetwork {
        FlowNetwork::new(
            ["s", "t"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
                .collect(),
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("e").expect("edge"),
                from: NodeId::parse("s").expect("tail"),
                to: NodeId::parse("t").expect("head"),
                lower: 0,
                capacity: 3,
                cost: 1,
            }],
        )
        .expect("graph")
    }

    fn base(graph: &FlowNetwork) -> FlowTraceSnapshot {
        let state = ResidualState::at_lower_bounds(graph);
        FlowTraceSnapshot::capture(
            graph,
            &state,
            vec![None; 2],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            FlowTraceMetrics::default(),
        )
    }

    fn eibfs_snapshot(graph: &FlowNetwork) -> FlowTraceSnapshot {
        let state = ResidualState::at_lower_bounds(graph);
        FlowTraceSnapshot::capture(
            graph,
            &state,
            vec![Some(0), Some(-1)],
            Vec::new(),
            Vec::new(),
            vec![0, 0],
            FlowTraceMetrics::default(),
        )
        .with_eibfs_overlay(EibfsTraceOverlay {
            phase_direction: EibfsTracePhaseDirection::Forward,
            source_depth: 0,
            sink_depth: 0,
            nodes: vec![
                EibfsTraceNodeState {
                    node: NodeId::parse("s").expect("source"),
                    source_label: 0,
                    sink_label: 0,
                    membership: EibfsTraceMembership::Source,
                    root_kind: EibfsTraceRootKind::Source,
                    orphan: false,
                    imbalance: 0,
                },
                EibfsTraceNodeState {
                    node: NodeId::parse("t").expect("sink"),
                    source_label: 0,
                    sink_label: 0,
                    membership: EibfsTraceMembership::Sink,
                    root_kind: EibfsTraceRootKind::Sink,
                    orphan: false,
                    imbalance: 0,
                },
            ],
            forest_arcs: Vec::new(),
        })
    }

    #[test]
    fn recorder_replays_forward_and_reverse_exactly() {
        let graph = graph();
        let base = base(&graph);
        let mut next_state = ResidualState::at_lower_bounds(&graph);
        let arc = ResidualArcId::new(
            EdgeId::parse("e").expect("edge"),
            ResidualDirection::Forward,
        );
        next_state
            .augment(std::slice::from_ref(&arc), 2)
            .expect("augment");
        let next = FlowTraceSnapshot::capture(
            &graph,
            &next_state,
            vec![Some(0), Some(1)],
            graph.node_indices().collect(),
            vec![arc],
            Vec::new(),
            FlowTraceMetrics {
                bfs_runs: 1,
                residual_arc_scans: 1,
                augmentations: 1,
                path_searches: 0,
                ..FlowTraceMetrics::default()
            },
        );
        let mut recorder = FlowTraceRecorder::new(&graph, base.clone()).expect("recorder");
        recorder
            .record_transition(
                FlowTraceEventMetadata {
                    catalog_id: "test.augment",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "test:1",
                },
                &next,
            )
            .expect("record");
        let (_, events, final_snapshot) = recorder.finish();
        assert_eq!(final_snapshot, next);

        let mut replay = base.clone();
        apply_trace_event(&graph, &mut replay, &events[0], FlowTraceDirection::Forward)
            .expect("forward");
        assert_eq!(replay, next);
        apply_trace_event(&graph, &mut replay, &events[0], FlowTraceDirection::Reverse)
            .expect("reverse");
        assert_eq!(replay, base);
    }

    #[test]
    fn recorder_replays_current_capacity_and_residual_patches_exactly() {
        let graph = graph();
        let base = base(&graph);
        let mut next_state = ResidualState::at_lower_bounds(&graph);
        let edge = EdgeId::parse("e").expect("edge");
        next_state
            .set_current_capacity(&edge, 2)
            .expect("capacity update");
        let next = FlowTraceSnapshot::capture(
            &graph,
            &next_state,
            vec![None; 2],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            FlowTraceMetrics::default(),
        );
        let mut recorder = FlowTraceRecorder::new(&graph, base.clone()).expect("recorder");
        recorder
            .record_transition(
                FlowTraceEventMetadata {
                    catalog_id: "test.capacity-update",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "test:capacity-update",
                },
                &next,
            )
            .expect("record");
        let (_, events, final_snapshot) = recorder.finish();
        assert_eq!(final_snapshot, next);
        assert!(events[0].patches.iter().any(|patch| {
            matches!(
                patch,
                FlowTracePatch::EdgeCapacity {
                    edge: actual,
                    before: 3,
                    after: 2,
                } if actual == &edge
            )
        }));
        assert!(events[0].patches.iter().any(|patch| {
            matches!(
                patch,
                FlowTracePatch::ResidualCapacity {
                    before: 3,
                    after: 2,
                    ..
                }
            )
        }));

        let mut replay = base.clone();
        apply_trace_event(&graph, &mut replay, &events[0], FlowTraceDirection::Forward)
            .expect("forward");
        assert_eq!(replay, next);
        apply_trace_event(&graph, &mut replay, &events[0], FlowTraceDirection::Reverse)
            .expect("reverse");
        assert_eq!(replay, base);
    }

    #[test]
    fn failed_precondition_is_atomic_and_duplicate_targets_are_rejected() {
        let graph = graph();
        let mut snapshot = base(&graph);
        let original = snapshot.clone();
        let edge = EdgeId::parse("e").expect("edge");
        let event = FlowTraceEvent {
            event_id: 1,
            parent_phase_id: None,
            catalog_id: "test.invalid".to_owned(),
            minimum_granularity: TraceGranularityV1::Operation,
            entity_refs: Vec::new(),
            patches: vec![
                FlowTracePatch::EdgeFlow {
                    edge: edge.clone(),
                    before: 1,
                    after: 2,
                },
                FlowTracePatch::Metric {
                    metric: FlowTraceMetricId::BfsRuns,
                    before: 0,
                    after: 1,
                },
            ],
            pseudocode_line: "test:2".to_owned(),
            detail: None,
        };
        assert_eq!(
            apply_trace_event(&graph, &mut snapshot, &event, FlowTraceDirection::Forward),
            Err(FlowTraceError::Precondition)
        );
        assert_eq!(snapshot, original);

        let duplicate = FlowTraceEvent {
            patches: vec![
                FlowTracePatch::EdgeFlow {
                    edge: edge.clone(),
                    before: 0,
                    after: 1,
                },
                FlowTracePatch::EdgeFlow {
                    edge,
                    before: 1,
                    after: 2,
                },
            ],
            ..event
        };
        assert_eq!(
            apply_trace_event(
                &graph,
                &mut snapshot,
                &duplicate,
                FlowTraceDirection::Forward
            ),
            Err(FlowTraceError::DuplicatePatchTarget)
        );
        assert_eq!(snapshot, original);
    }

    #[test]
    fn eibfs_overlay_must_match_generic_labels_and_divergence() {
        let graph = graph();
        let valid = eibfs_snapshot(&graph);
        validate_snapshot_shape(&graph, &valid).expect("valid overlay");

        let mut label_drift = valid.clone();
        label_drift.node_labels[1] = Some(0);
        assert_eq!(
            validate_snapshot_shape(&graph, &label_drift),
            Err(FlowTraceError::SnapshotShape)
        );

        let mut divergence_drift = valid.clone();
        divergence_drift.remaining_divergence[0] = 1;
        assert_eq!(
            validate_snapshot_shape(&graph, &divergence_drift),
            Err(FlowTraceError::SnapshotShape)
        );

        let mut overlay_drift = valid;
        overlay_drift.eibfs_overlay.as_mut().expect("overlay").nodes[0].imbalance = 1;
        assert_eq!(
            validate_snapshot_shape(&graph, &overlay_drift),
            Err(FlowTraceError::SnapshotShape)
        );
    }

    #[test]
    fn eibfs_overlay_is_exclusive_with_the_generic_forest_overlay() {
        let graph = graph();
        let generic_arc = ResidualArcId::new(
            EdgeId::parse("e").expect("edge"),
            ResidualDirection::Forward,
        );

        let with_generic_arc =
            eibfs_snapshot(&graph).with_forest_overlay(&graph, vec![generic_arc], Vec::new());
        assert_eq!(
            validate_snapshot_shape(&graph, &with_generic_arc),
            Err(FlowTraceError::SnapshotShape)
        );

        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let with_strong_node =
            eibfs_snapshot(&graph).with_forest_overlay(&graph, Vec::new(), vec![source]);
        assert_eq!(
            validate_snapshot_shape(&graph, &with_strong_node),
            Err(FlowTraceError::SnapshotShape)
        );
    }
}
