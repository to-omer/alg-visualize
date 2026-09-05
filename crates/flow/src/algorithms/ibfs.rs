//! Standard one-direction Incremental Breadth-First Search maximum flow.
//!
//! This is the polynomial standard IBFS algorithm from Goldberg, Hed, Kaplan,
//! Tarjan, and Werneck (ESA 2011), not the simultaneous-growth experimental
//! variant and not the later excesses/pseudoflow EIBFS algorithm.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative complete-trace node limit for the first public IBFS slice.
pub const IBFS_MAX_NODES: usize = 256;
/// Conservative complete-trace edge limit for the first public IBFS slice.
pub const IBFS_MAX_EDGES: usize = 2_048;
/// Hard ceiling for positive residual-arc inspections.
pub const IBFS_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for logical tree, augmentation, and adoption transitions.
pub const IBFS_MAX_STATE_TRANSITIONS: u64 = 100_000;
/// Hard ceiling for successful augmentations.
pub const IBFS_MAX_AUGMENTATIONS: u64 = 10_000;

/// Exact deterministic counters exposed by the standard IBFS kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IbfsMetrics {
    /// Growth passes entered.
    pub passes: u64,
    /// Source-tree growth passes entered.
    pub forward_passes: u64,
    /// Sink-tree growth passes entered.
    pub reverse_passes: u64,
    /// All positive residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Positive residual arcs inspected while growing a tree.
    pub growth_arc_scans: u128,
    /// Positive residual arcs inspected while adopting an orphan.
    pub adoption_arc_scans: u128,
    /// Boundary vertices whose growth adjacency was exhausted.
    pub active_vertex_scans: u64,
    /// Vertices attached to either search tree during growth.
    pub tree_attachments: u64,
    /// Shortest-path augmentations.
    pub augmentations: u64,
    /// Sum of residual arcs across augmented paths.
    pub augmented_path_arcs: u128,
    /// Tree arcs saturated by augmentation.
    pub saturated_tree_arcs: u64,
    /// Orphans created, including cascaded children.
    pub orphan_creations: u64,
    /// FIFO orphan records processed.
    pub orphan_visits: u64,
    /// Orphans adopted without changing their distance.
    pub same_level_adoptions: u64,
    /// Orphans adopted after increasing their distance.
    pub orphan_relabels: u64,
    /// Orphans removed from a tree.
    pub tree_removals: u64,
    /// Logical mutations charged against the work ceiling.
    pub state_transitions: u64,
}

/// Certified standard IBFS result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IbfsResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut witness.
    pub certificate: MaxFlowCertificate,
    /// Deterministic operation counts.
    pub metrics: IbfsMetrics,
}

/// Certified result plus a complete reversible trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IbfsTraceResult {
    /// Same result produced by the non-tracing profile.
    pub result: IbfsResult,
    /// Replay boundary before tree initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible semantic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Verified optimal replay boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// IBFS admission, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum IbfsError {
    /// Input exceeds the deliberately small complete-trace band.
    #[error("graph exceeds IBFS admission limits")]
    AdmissionLimit,
    /// The graph is outside the zero-feasible-flow static IBFS contract.
    #[error("IBFS graph requirement is not satisfied: {0}")]
    GraphRequirement(&'static str),
    /// A deterministic work ceiling was reached.
    #[error("IBFS work limit reached")]
    WorkLimit,
    /// Checked counter or distance arithmetic overflowed.
    #[error("IBFS arithmetic overflow")]
    ArithmeticOverflow,
    /// A search-tree, shortest-path, or adoption invariant failed.
    #[error("IBFS tree invariant failed")]
    TreeInvariant,
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The solver-independent maximum-flow checker rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Validates the static standard-IBFS problem contract.
///
/// # Errors
///
/// Requires distinct in-range terminals, zero supplies, zero lower bounds, and
/// the conservative complete-trace admission band. Parallel, opposite,
/// zero-capacity, and self-loop edges are accepted.
pub fn validate_ibfs_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), IbfsError> {
    if graph.nodes().len() > IBFS_MAX_NODES || graph.edges().len() > IBFS_MAX_EDGES {
        return Err(IbfsError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(IbfsError::GraphRequirement("distinct source and sink"));
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(IbfsError::GraphRequirement("zero supplies"));
    }
    if graph.edges().iter().any(|edge| edge.lower() != 0) {
        return Err(IbfsError::GraphRequirement("zero lower bounds"));
    }
    Ok(())
}

/// Solves a zero-feasible-flow maximum-flow problem with standard IBFS.
///
/// # Errors
///
/// Rejects input outside [`validate_ibfs_graph`], bounded-work exhaustion,
/// residual or tree invariant failures, and an independently rejected result.
pub fn solve_ibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<IbfsResult, IbfsError> {
    solve_ibfs_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves standard IBFS while recording semantic, reversible state changes.
///
/// # Errors
///
/// Returns the same failures as [`solve_ibfs`], plus trace-diff failures.
pub fn trace_ibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<IbfsTraceResult, IbfsError> {
    let run = solve_ibfs_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(IbfsError::TreeInvariant)?;
    Ok(IbfsTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct IbfsInternalRun {
    result: IbfsResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TreeSide {
    Free,
    Source,
    Sink,
    SourceOrphan,
    SinkOrphan,
}

impl TreeSide {
    const fn normal(self) -> Option<NormalSide> {
        match self {
            Self::Source => Some(NormalSide::Source),
            Self::Sink => Some(NormalSide::Sink),
            Self::Free | Self::SourceOrphan | Self::SinkOrphan => None,
        }
    }

    const fn orphan(self) -> Option<NormalSide> {
        match self {
            Self::SourceOrphan => Some(NormalSide::Source),
            Self::SinkOrphan => Some(NormalSide::Sink),
            Self::Free | Self::Source | Self::Sink => None,
        }
    }

    const fn belongs_to(self, side: NormalSide) -> bool {
        matches!(
            (self, side),
            (Self::Source | Self::SourceOrphan, NormalSide::Source)
                | (Self::Sink | Self::SinkOrphan, NormalSide::Sink)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NormalSide {
    Source,
    Sink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PassDirection {
    Forward,
    Reverse,
}

impl PassDirection {
    const fn growing_side(self) -> NormalSide {
        match self {
            Self::Forward => NormalSide::Source,
            Self::Reverse => NormalSide::Sink,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Forward => Self::Reverse,
            Self::Reverse => Self::Forward,
        }
    }
}

#[derive(Clone, Debug)]
struct IbfsNode {
    side: TreeSide,
    distance: usize,
    parent: Option<ResidualArcId>,
    growth_cursor: usize,
    adoption_cursor: usize,
}

impl Default for IbfsNode {
    fn default() -> Self {
        Self {
            side: TreeSide::Free,
            distance: 0,
            parent: None,
            growth_cursor: 0,
            adoption_cursor: 0,
        }
    }
}

struct AugmentingPath {
    arcs: Vec<ResidualArcId>,
    bridge_index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActiveScanResult {
    Exhausted,
    InterruptedByAugmentation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrphanReattachment {
    SameLevel { pseudocode_line: &'static str },
    Relabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OrphanParent {
    arc: ResidualArcId,
    position: usize,
    distance: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OrphanContext {
    node: NodeIndex,
    side: NormalSide,
    old_distance: usize,
}

struct IbfsEngine<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    residual: ResidualState<'graph>,
    nodes: Vec<IbfsNode>,
    children: Vec<BTreeSet<NodeIndex>>,
    outgoing: Vec<Vec<ResidualArcId>>,
    incoming: Vec<Vec<ResidualArcId>>,
    source_depth: usize,
    sink_depth: usize,
    metrics: IbfsMetrics,
}

fn solve_ibfs_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<IbfsInternalRun, IbfsError> {
    validate_ibfs_graph(graph, source, sink)?;
    let residual = ResidualState::at_lower_bounds(graph);
    let mut recorder = if with_trace {
        let base = FlowTraceSnapshot::capture(
            graph,
            &residual,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            FlowTraceMetrics::default(),
        );
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };
    let mut engine = IbfsEngine::new(graph, source, sink, residual)?;
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "ibfs.initialize-two-trees",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "ibfs:initialize-s-and-t-roots",
        },
        Vec::new(),
        Vec::new(),
        Some(("shortest-path-length", 1)),
        Some((vec![source, sink], Vec::new())),
    )?;

    engine.run_passes(&mut recorder)?;

    engine.validate_forest()?;
    let flows = engine.residual.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    let cut_order = certificate
        .source_side
        .iter()
        .filter_map(|id| graph.node_index(id))
        .collect::<Vec<_>>();
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "ibfs.optimal-cut",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "ibfs:return-maximum-flow-minimum-cut",
        },
        cut_order,
        Vec::new(),
        Some(("cut", certificate.cut_bound)),
        None,
    )?;
    let result = IbfsResult {
        flows,
        certificate,
        metrics: engine.metrics,
    };
    Ok(IbfsInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

impl<'graph> IbfsEngine<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        residual: ResidualState<'graph>,
    ) -> Result<Self, IbfsError> {
        let node_count = graph.nodes().len();
        let mut outgoing = vec![Vec::new(); node_count];
        let mut incoming = vec![Vec::new(); node_count];
        for edge in graph.edges() {
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let arc = residual.arc(&id).ok_or(IbfsError::TreeInvariant)?;
                outgoing[arc.from.as_usize()].push(id.clone());
                incoming[arc.to.as_usize()].push(id);
            }
        }
        for ids in outgoing.iter_mut().chain(incoming.iter_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }
        let mut nodes = vec![IbfsNode::default(); node_count];
        nodes[source.as_usize()].side = TreeSide::Source;
        nodes[sink.as_usize()].side = TreeSide::Sink;
        Ok(Self {
            graph,
            source,
            sink,
            residual,
            nodes,
            children: vec![BTreeSet::new(); node_count],
            outgoing,
            incoming,
            source_depth: 0,
            sink_depth: 0,
            metrics: IbfsMetrics::default(),
        })
    }

    fn enter_pass(&mut self, direction: PassDirection) -> Result<(), IbfsError> {
        self.metrics.passes = checked_add(self.metrics.passes, 1)?;
        match direction {
            PassDirection::Forward => {
                self.metrics.forward_passes = checked_add(self.metrics.forward_passes, 1)?;
            }
            PassDirection::Reverse => {
                self.metrics.reverse_passes = checked_add(self.metrics.reverse_passes, 1)?;
            }
        }
        self.validate_forest()
    }

    fn run_passes(
        &mut self,
        recorder: &mut Option<FlowTraceRecorder<'graph>>,
    ) -> Result<(), IbfsError> {
        let mut direction = PassDirection::Forward;
        loop {
            self.enter_pass(direction)?;
            let depth = self.current_depth(direction);
            self.record(
                recorder.as_mut(),
                FlowTraceEventMetadata {
                    catalog_id: match direction {
                        PassDirection::Forward => "ibfs.start-forward-pass",
                        PassDirection::Reverse => "ibfs.start-reverse-pass",
                    },
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: match direction {
                        PassDirection::Forward => "ibfs:grow-source-tree-one-level",
                        PassDirection::Reverse => "ibfs:grow-sink-tree-one-level",
                    },
                },
                self.frontier(direction.growing_side(), depth),
                Vec::new(),
                Some(("shortest-path-length", self.shortest_path_length()?)),
                None,
            )?;
            if !self.grow_pass(direction, recorder.as_mut())? {
                self.record(
                    recorder.as_mut(),
                    FlowTraceEventMetadata {
                        catalog_id: "ibfs.no-next-level",
                        minimum_granularity: TraceGranularityV1::Phase,
                        pseudocode_line: "ibfs:stop-when-next-level-is-empty",
                    },
                    Vec::new(),
                    Vec::new(),
                    Some((
                        "depth",
                        i128::try_from(depth).map_err(|_| IbfsError::ArithmeticOverflow)?,
                    )),
                    Some((Vec::new(), Vec::new())),
                )?;
                return Ok(());
            }
            self.advance_depth(direction)?;
            self.record(
                recorder.as_mut(),
                FlowTraceEventMetadata {
                    catalog_id: "ibfs.complete-pass",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "ibfs:advance-grown-tree-boundary",
                },
                self.frontier(direction.growing_side(), self.current_depth(direction)),
                Vec::new(),
                Some(("shortest-path-length", self.shortest_path_length()?)),
                None,
            )?;
            direction = direction.next();
        }
    }

    const fn current_depth(&self, direction: PassDirection) -> usize {
        match direction {
            PassDirection::Forward => self.source_depth,
            PassDirection::Reverse => self.sink_depth,
        }
    }

    fn advance_depth(&mut self, direction: PassDirection) -> Result<(), IbfsError> {
        match direction {
            PassDirection::Forward => {
                self.source_depth = self
                    .source_depth
                    .checked_add(1)
                    .ok_or(IbfsError::ArithmeticOverflow)?;
            }
            PassDirection::Reverse => {
                self.sink_depth = self
                    .sink_depth
                    .checked_add(1)
                    .ok_or(IbfsError::ArithmeticOverflow)?;
            }
        }
        Ok(())
    }

    fn shortest_path_length(&self) -> Result<i128, IbfsError> {
        let length = self
            .source_depth
            .checked_add(self.sink_depth)
            .and_then(|value| value.checked_add(1))
            .ok_or(IbfsError::ArithmeticOverflow)?;
        i128::try_from(length).map_err(|_| IbfsError::ArithmeticOverflow)
    }

    fn grow_pass(
        &mut self,
        direction: PassDirection,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<bool, IbfsError> {
        let side = direction.growing_side();
        let depth = self.current_depth(direction);
        let mut exhausted = vec![false; self.nodes.len()];
        loop {
            let active = self.graph.node_indices().find(|node| {
                self.nodes[node.as_usize()].side.normal() == Some(side)
                    && self.nodes[node.as_usize()].distance == depth
                    && !exhausted[node.as_usize()]
            });
            let Some(active) = active else {
                break;
            };
            if self.scan_active_vertex(direction, side, depth, active, recorder.as_deref_mut())?
                == ActiveScanResult::Exhausted
            {
                exhausted[active.as_usize()] = true;
            }
        }
        self.validate_forest()?;
        let next_depth = depth.checked_add(1).ok_or(IbfsError::ArithmeticOverflow)?;
        Ok(self.graph.node_indices().any(|node| {
            self.nodes[node.as_usize()].side.normal() == Some(side)
                && self.nodes[node.as_usize()].distance == next_depth
        }))
    }

    fn scan_active_vertex(
        &mut self,
        direction: PassDirection,
        side: NormalSide,
        depth: usize,
        active: NodeIndex,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<ActiveScanResult, IbfsError> {
        loop {
            let position = self.nodes[active.as_usize()].growth_cursor;
            let adjacency = match direction {
                PassDirection::Forward => &self.outgoing[active.as_usize()],
                PassDirection::Reverse => &self.incoming[active.as_usize()],
            };
            let Some(id) = adjacency.get(position).cloned() else {
                self.metrics.active_vertex_scans =
                    checked_add(self.metrics.active_vertex_scans, 1)?;
                return Ok(ActiveScanResult::Exhausted);
            };
            let arc = self.residual.arc(&id).ok_or(IbfsError::TreeInvariant)?;
            if arc.capacity == 0 {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                continue;
            }
            self.count_scan(false)?;
            let candidate = match direction {
                PassDirection::Forward => arc.to,
                PassDirection::Reverse => arc.from,
            };
            let candidate_side = self.nodes[candidate.as_usize()].side;
            if candidate_side == TreeSide::Free {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                self.attach_new_vertex(direction, active, candidate, id.clone())?;
                self.record(
                    recorder.as_deref_mut(),
                    FlowTraceEventMetadata {
                        catalog_id: match direction {
                            PassDirection::Forward => "ibfs.attach-source-tree",
                            PassDirection::Reverse => "ibfs.attach-sink-tree",
                        },
                        minimum_granularity: TraceGranularityV1::Operation,
                        pseudocode_line: "ibfs:attach-free-vertex-to-next-level",
                    },
                    vec![active, candidate],
                    vec![id.clone()],
                    Some((
                        "distance",
                        i128::try_from(depth + 1).map_err(|_| IbfsError::ArithmeticOverflow)?,
                    )),
                    Some((vec![candidate], vec![id])),
                )?;
                continue;
            }
            let is_bridge = match direction {
                PassDirection::Forward => candidate_side == TreeSide::Sink,
                PassDirection::Reverse => candidate_side == TreeSide::Source,
            };
            if !is_bridge {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                continue;
            }
            let path = match direction {
                PassDirection::Forward => self.reconstruct_path(active, candidate, id)?,
                PassDirection::Reverse => self.reconstruct_path(candidate, active, id)?,
            };
            if i128::try_from(path.arcs.len()).map_err(|_| IbfsError::ArithmeticOverflow)?
                != self.shortest_path_length()?
            {
                return Err(IbfsError::TreeInvariant);
            }
            self.record(
                recorder.as_deref_mut(),
                FlowTraceEventMetadata {
                    catalog_id: "ibfs.connect-trees",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "ibfs:join-s-and-t-trees",
                },
                vec![active, candidate],
                path.arcs.clone(),
                Some(("shortest-path-length", self.shortest_path_length()?)),
                Some((vec![active, candidate], path.arcs.clone())),
            )?;
            self.augment_path(&path, recorder.as_deref_mut())?;
            // Resume at the bridge only when adoption left the scanned vertex
            // active on this boundary. A relabel owns its reset cursor.
            if self.nodes[active.as_usize()].side.normal() == Some(side)
                && self.nodes[active.as_usize()].distance == depth
            {
                self.nodes[active.as_usize()].growth_cursor = position;
            }
            return Ok(ActiveScanResult::InterruptedByAugmentation);
        }
    }

    fn attach_new_vertex(
        &mut self,
        direction: PassDirection,
        parent: NodeIndex,
        child: NodeIndex,
        parent_arc: ResidualArcId,
    ) -> Result<(), IbfsError> {
        let distance = self.nodes[parent.as_usize()]
            .distance
            .checked_add(1)
            .ok_or(IbfsError::ArithmeticOverflow)?;
        let child_state = &mut self.nodes[child.as_usize()];
        if child_state.side != TreeSide::Free {
            return Err(IbfsError::TreeInvariant);
        }
        child_state.side = match direction {
            PassDirection::Forward => TreeSide::Source,
            PassDirection::Reverse => TreeSide::Sink,
        };
        child_state.distance = distance;
        child_state.parent = Some(parent_arc);
        child_state.growth_cursor = 0;
        child_state.adoption_cursor = 0;
        self.children[parent.as_usize()].insert(child);
        self.metrics.tree_attachments = checked_add(self.metrics.tree_attachments, 1)?;
        self.count_transition()
    }

    fn reconstruct_path(
        &self,
        source_endpoint: NodeIndex,
        sink_endpoint: NodeIndex,
        bridge: ResidualArcId,
    ) -> Result<AugmentingPath, IbfsError> {
        let bridge_arc = self.residual.arc(&bridge).ok_or(IbfsError::TreeInvariant)?;
        if bridge_arc.capacity == 0
            || bridge_arc.from != source_endpoint
            || bridge_arc.to != sink_endpoint
        {
            return Err(IbfsError::TreeInvariant);
        }
        let mut source_reversed = Vec::new();
        let mut cursor = source_endpoint;
        while cursor != self.source {
            let id = self.nodes[cursor.as_usize()]
                .parent
                .clone()
                .ok_or(IbfsError::TreeInvariant)?;
            let arc = self.residual.arc(&id).ok_or(IbfsError::TreeInvariant)?;
            if arc.capacity == 0 || arc.to != cursor {
                return Err(IbfsError::TreeInvariant);
            }
            source_reversed.push(id);
            cursor = arc.from;
            if source_reversed.len() > self.nodes.len() {
                return Err(IbfsError::TreeInvariant);
            }
        }
        source_reversed.reverse();
        let bridge_index = source_reversed.len();
        let mut arcs = source_reversed;
        arcs.push(bridge);
        cursor = sink_endpoint;
        while cursor != self.sink {
            let id = self.nodes[cursor.as_usize()]
                .parent
                .clone()
                .ok_or(IbfsError::TreeInvariant)?;
            let arc = self.residual.arc(&id).ok_or(IbfsError::TreeInvariant)?;
            if arc.capacity == 0 || arc.from != cursor {
                return Err(IbfsError::TreeInvariant);
            }
            arcs.push(id);
            cursor = arc.to;
            if arcs.len() > self.nodes.len() + 1 {
                return Err(IbfsError::TreeInvariant);
            }
        }
        Ok(AugmentingPath { arcs, bridge_index })
    }

    fn augment_path(
        &mut self,
        path: &AugmentingPath,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), IbfsError> {
        if self.metrics.augmentations >= IBFS_MAX_AUGMENTATIONS {
            return Err(IbfsError::WorkLimit);
        }
        let bottleneck = path
            .arcs
            .iter()
            .map(|id| {
                self.residual
                    .arc(id)
                    .map(|arc| arc.capacity)
                    .ok_or(IbfsError::TreeInvariant)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(IbfsError::TreeInvariant)?;
        let mut saturated_children = Vec::new();
        for (position, id) in path.arcs.iter().enumerate() {
            if position == path.bridge_index {
                continue;
            }
            let arc = self.residual.arc(id).ok_or(IbfsError::TreeInvariant)?;
            if arc.capacity == bottleneck {
                saturated_children.push(if position < path.bridge_index {
                    arc.to
                } else {
                    arc.from
                });
            }
        }
        self.residual.augment(&path.arcs, bottleneck)?;
        self.metrics.augmentations = checked_add(self.metrics.augmentations, 1)?;
        self.metrics.augmented_path_arcs = self
            .metrics
            .augmented_path_arcs
            .checked_add(
                u128::try_from(path.arcs.len()).map_err(|_| IbfsError::ArithmeticOverflow)?,
            )
            .ok_or(IbfsError::ArithmeticOverflow)?;
        self.metrics.saturated_tree_arcs = self
            .metrics
            .saturated_tree_arcs
            .checked_add(
                u64::try_from(saturated_children.len())
                    .map_err(|_| IbfsError::ArithmeticOverflow)?,
            )
            .ok_or(IbfsError::ArithmeticOverflow)?;
        self.count_transition()?;

        let mut queue = VecDeque::new();
        let mut queued = BTreeSet::new();
        for child in saturated_children {
            self.make_orphan(child, &mut queue, &mut queued)?;
        }
        self.record(
            recorder.as_deref_mut(),
            FlowTraceEventMetadata {
                catalog_id: "ibfs.augment-shortest-path",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "ibfs:augment-and-create-orphans",
            },
            queue.iter().copied().collect(),
            path.arcs.clone(),
            Some(("bottleneck", i128::from(bottleneck))),
            Some((queue.iter().copied().collect(), path.arcs.clone())),
        )?;
        self.adopt_orphans(&mut queue, &mut queued, recorder)?;
        if self.nodes.iter().any(|node| node.side.orphan().is_some()) {
            return Err(IbfsError::TreeInvariant);
        }
        self.validate_forest()
    }

    fn make_orphan(
        &mut self,
        node: NodeIndex,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
    ) -> Result<(), IbfsError> {
        if node == self.source || node == self.sink {
            return Err(IbfsError::TreeInvariant);
        }
        let normal = match self.nodes[node.as_usize()].side {
            TreeSide::Source => NormalSide::Source,
            TreeSide::Sink => NormalSide::Sink,
            TreeSide::SourceOrphan | TreeSide::SinkOrphan => {
                if queued.insert(node) {
                    queue.push_back(node);
                }
                return Ok(());
            }
            TreeSide::Free => return Ok(()),
        };
        self.detach_parent(node, normal)?;
        self.nodes[node.as_usize()].side = match normal {
            NormalSide::Source => TreeSide::SourceOrphan,
            NormalSide::Sink => TreeSide::SinkOrphan,
        };
        self.nodes[node.as_usize()].growth_cursor = 0;
        self.metrics.orphan_creations = checked_add(self.metrics.orphan_creations, 1)?;
        self.count_transition()?;
        if queued.insert(node) {
            queue.push_back(node);
        }
        Ok(())
    }

    fn adopt_orphans(
        &mut self,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), IbfsError> {
        while let Some(orphan) = queue.pop_front() {
            queued.remove(&orphan);
            self.adopt_one_orphan(orphan, queue, queued, recorder.as_deref_mut())?;
        }
        Ok(())
    }

    fn adopt_one_orphan(
        &mut self,
        orphan: NodeIndex,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), IbfsError> {
        let side = self.nodes[orphan.as_usize()]
            .side
            .orphan()
            .ok_or(IbfsError::TreeInvariant)?;
        self.metrics.orphan_visits = checked_add(self.metrics.orphan_visits, 1)?;
        let old_distance = self.nodes[orphan.as_usize()].distance;
        let context = OrphanContext {
            node: orphan,
            side,
            old_distance,
        };
        if let Some((position, parent_arc)) = self.same_level_parent(orphan, side)? {
            return self.reattach_orphan(
                context,
                OrphanParent {
                    arc: parent_arc,
                    position,
                    distance: old_distance,
                },
                OrphanReattachment::SameLevel {
                    pseudocode_line: "ibfs:adopt-at-current-distance",
                },
                queue,
                recorder,
            );
        }
        let candidate = self.minimum_parent(orphan, side)?;
        if let Some((position, parent_arc, parent_distance)) = candidate.clone()
            && parent_distance.checked_add(1) == Some(old_distance)
        {
            return self.reattach_orphan(
                context,
                OrphanParent {
                    arc: parent_arc,
                    position,
                    distance: old_distance,
                },
                OrphanReattachment::SameLevel {
                    pseudocode_line: "ibfs:adopt-newly-available-current-level-parent",
                },
                queue,
                recorder,
            );
        }
        if let Some((position, parent_arc, parent_distance)) = candidate
            && self.relabel_allowed(side, parent_distance)
        {
            let new_distance = parent_distance
                .checked_add(1)
                .ok_or(IbfsError::ArithmeticOverflow)?;
            if new_distance <= old_distance {
                return Err(IbfsError::TreeInvariant);
            }
            self.orphan_children(orphan, queue, queued)?;
            return self.reattach_orphan(
                context,
                OrphanParent {
                    arc: parent_arc,
                    position,
                    distance: new_distance,
                },
                OrphanReattachment::Relabel,
                queue,
                recorder,
            );
        }
        self.remove_orphan(context, queue, queued, recorder)
    }

    fn reattach_orphan(
        &mut self,
        context: OrphanContext,
        parent: OrphanParent,
        repair: OrphanReattachment,
        queue: &VecDeque<NodeIndex>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), IbfsError> {
        let parent_arc = parent.arc.clone();
        self.attach_orphan(
            context.node,
            context.side,
            parent.arc,
            parent.position,
            parent.distance,
        )?;
        let (catalog_id, pseudocode_line) = match repair {
            OrphanReattachment::SameLevel { pseudocode_line } => {
                self.metrics.same_level_adoptions =
                    checked_add(self.metrics.same_level_adoptions, 1)?;
                (
                    match context.side {
                        NormalSide::Source => "ibfs.adopt-source-orphan",
                        NormalSide::Sink => "ibfs.adopt-sink-orphan",
                    },
                    pseudocode_line,
                )
            }
            OrphanReattachment::Relabel => {
                self.metrics.orphan_relabels = checked_add(self.metrics.orphan_relabels, 1)?;
                (
                    match context.side {
                        NormalSide::Source => "ibfs.relabel-source-orphan",
                        NormalSide::Sink => "ibfs.relabel-sink-orphan",
                    },
                    "ibfs:raise-distance-and-orphan-children",
                )
            }
        };
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id,
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line,
            },
            orphan_queue_view(context.node, queue),
            Vec::new(),
            Some((
                "distance",
                i128::try_from(parent.distance).map_err(|_| IbfsError::ArithmeticOverflow)?,
            )),
            Some((vec![context.node], vec![parent_arc])),
        )
    }

    fn orphan_children(
        &mut self,
        orphan: NodeIndex,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
    ) -> Result<(), IbfsError> {
        let children = self.children[orphan.as_usize()]
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for child in children {
            self.make_orphan(child, queue, queued)?;
        }
        Ok(())
    }

    fn remove_orphan(
        &mut self,
        context: OrphanContext,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), IbfsError> {
        self.orphan_children(context.node, queue, queued)?;
        let state = &mut self.nodes[context.node.as_usize()];
        state.side = TreeSide::Free;
        state.parent = None;
        state.growth_cursor = 0;
        state.adoption_cursor = 0;
        self.metrics.tree_removals = checked_add(self.metrics.tree_removals, 1)?;
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: match context.side {
                    NormalSide::Source => "ibfs.remove-source-orphan",
                    NormalSide::Sink => "ibfs.remove-sink-orphan",
                },
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "ibfs:remove-orphan-beyond-current-boundary",
            },
            orphan_queue_view(context.node, queue),
            Vec::new(),
            Some((
                "distance",
                i128::try_from(context.old_distance).map_err(|_| IbfsError::ArithmeticOverflow)?,
            )),
            Some((vec![context.node], Vec::new())),
        )
    }

    fn same_level_parent(
        &mut self,
        orphan: NodeIndex,
        side: NormalSide,
    ) -> Result<Option<(usize, ResidualArcId)>, IbfsError> {
        let old_distance = self.nodes[orphan.as_usize()].distance;
        let start = self.nodes[orphan.as_usize()].adoption_cursor;
        let ids = self.parent_adjacency(orphan, side).to_vec();
        for (position, id) in ids.iter().enumerate().skip(start) {
            let Some((parent, parent_distance)) = self.valid_parent(id, orphan, side)? else {
                continue;
            };
            let _ = parent;
            if parent_distance.checked_add(1) == Some(old_distance) {
                return Ok(Some((position, id.clone())));
            }
        }
        Ok(None)
    }

    fn minimum_parent(
        &mut self,
        orphan: NodeIndex,
        side: NormalSide,
    ) -> Result<Option<(usize, ResidualArcId, usize)>, IbfsError> {
        let ids = self.parent_adjacency(orphan, side).to_vec();
        let mut best: Option<(usize, ResidualArcId, usize)> = None;
        for (position, id) in ids.iter().enumerate() {
            let Some((_parent, distance)) = self.valid_parent(id, orphan, side)? else {
                continue;
            };
            let replace = best.as_ref().is_none_or(|(_, best_id, best_distance)| {
                (distance, id) < (*best_distance, best_id)
            });
            if replace {
                best = Some((position, id.clone(), distance));
            }
        }
        Ok(best)
    }

    fn valid_parent(
        &mut self,
        id: &ResidualArcId,
        orphan: NodeIndex,
        side: NormalSide,
    ) -> Result<Option<(NodeIndex, usize)>, IbfsError> {
        let arc = self.residual.arc(id).ok_or(IbfsError::TreeInvariant)?;
        if arc.capacity == 0 {
            return Ok(None);
        }
        self.count_scan(true)?;
        let parent = match side {
            NormalSide::Source if arc.to == orphan => arc.from,
            NormalSide::Sink if arc.from == orphan => arc.to,
            NormalSide::Source | NormalSide::Sink => return Err(IbfsError::TreeInvariant),
        };
        if parent == orphan {
            return Ok(None);
        }
        if !self.nodes[parent.as_usize()].side.belongs_to(side) {
            return Ok(None);
        }
        Ok(Some((parent, self.nodes[parent.as_usize()].distance)))
    }

    fn parent_adjacency(&self, node: NodeIndex, side: NormalSide) -> &[ResidualArcId] {
        match side {
            NormalSide::Source => &self.incoming[node.as_usize()],
            NormalSide::Sink => &self.outgoing[node.as_usize()],
        }
    }

    fn relabel_allowed(&self, side: NormalSide, parent_distance: usize) -> bool {
        match (side, self.metrics.passes % 2 == 1) {
            // The first and every odd pass are forward passes.
            (NormalSide::Source, true) => parent_distance <= self.source_depth,
            (NormalSide::Sink, true) => parent_distance < self.sink_depth,
            (NormalSide::Sink, false) => parent_distance <= self.sink_depth,
            (NormalSide::Source, false) => parent_distance < self.source_depth,
        }
    }

    fn attach_orphan(
        &mut self,
        orphan: NodeIndex,
        side: NormalSide,
        parent_arc: ResidualArcId,
        cursor: usize,
        distance: usize,
    ) -> Result<(), IbfsError> {
        let arc = self
            .residual
            .arc(&parent_arc)
            .ok_or(IbfsError::TreeInvariant)?;
        if arc.capacity == 0 {
            return Err(IbfsError::TreeInvariant);
        }
        let parent = match side {
            NormalSide::Source if arc.to == orphan => arc.from,
            NormalSide::Sink if arc.from == orphan => arc.to,
            NormalSide::Source | NormalSide::Sink => return Err(IbfsError::TreeInvariant),
        };
        if !self.nodes[parent.as_usize()].side.belongs_to(side)
            || self.nodes[parent.as_usize()].distance.checked_add(1) != Some(distance)
        {
            return Err(IbfsError::TreeInvariant);
        }
        let state = &mut self.nodes[orphan.as_usize()];
        state.side = match side {
            NormalSide::Source => TreeSide::Source,
            NormalSide::Sink => TreeSide::Sink,
        };
        state.distance = distance;
        state.parent = Some(parent_arc);
        state.adoption_cursor = cursor;
        state.growth_cursor = 0;
        self.children[parent.as_usize()].insert(orphan);
        Ok(())
    }

    fn detach_parent(&mut self, child: NodeIndex, side: NormalSide) -> Result<(), IbfsError> {
        let id = self.nodes[child.as_usize()]
            .parent
            .take()
            .ok_or(IbfsError::TreeInvariant)?;
        let arc = self.residual.arc(&id).ok_or(IbfsError::TreeInvariant)?;
        let parent = match side {
            NormalSide::Source if arc.to == child => arc.from,
            NormalSide::Sink if arc.from == child => arc.to,
            NormalSide::Source | NormalSide::Sink => return Err(IbfsError::TreeInvariant),
        };
        if !self.children[parent.as_usize()].remove(&child) {
            return Err(IbfsError::TreeInvariant);
        }
        Ok(())
    }

    fn frontier(&self, side: NormalSide, distance: usize) -> Vec<NodeIndex> {
        self.graph
            .node_indices()
            .filter(|node| {
                self.nodes[node.as_usize()].side.normal() == Some(side)
                    && self.nodes[node.as_usize()].distance == distance
            })
            .collect()
    }

    fn count_scan(&mut self, adoption: bool) -> Result<(), IbfsError> {
        if self.metrics.residual_arc_scans >= IBFS_MAX_RESIDUAL_ARC_SCANS {
            return Err(IbfsError::WorkLimit);
        }
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(IbfsError::ArithmeticOverflow)?;
        if adoption {
            self.metrics.adoption_arc_scans = self
                .metrics
                .adoption_arc_scans
                .checked_add(1)
                .ok_or(IbfsError::ArithmeticOverflow)?;
        } else {
            self.metrics.growth_arc_scans = self
                .metrics
                .growth_arc_scans
                .checked_add(1)
                .ok_or(IbfsError::ArithmeticOverflow)?;
        }
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), IbfsError> {
        if self.metrics.state_transitions >= IBFS_MAX_STATE_TRANSITIONS {
            return Err(IbfsError::WorkLimit);
        }
        self.metrics.state_transitions = checked_add(self.metrics.state_transitions, 1)?;
        Ok(())
    }

    fn validate_forest(&self) -> Result<(), IbfsError> {
        if self.nodes[self.source.as_usize()].side != TreeSide::Source
            || self.nodes[self.source.as_usize()].distance != 0
            || self.nodes[self.source.as_usize()].parent.is_some()
            || self.nodes[self.sink.as_usize()].side != TreeSide::Sink
            || self.nodes[self.sink.as_usize()].distance != 0
            || self.nodes[self.sink.as_usize()].parent.is_some()
        {
            return Err(IbfsError::TreeInvariant);
        }
        let mut expected_children = vec![BTreeSet::new(); self.nodes.len()];
        for node in self.graph.node_indices() {
            let state = &self.nodes[node.as_usize()];
            let Some(side) = state.side.normal() else {
                if state.side == TreeSide::Free && state.parent.is_some() {
                    return Err(IbfsError::TreeInvariant);
                }
                continue;
            };
            if node == self.source || node == self.sink {
                continue;
            }
            let id = state.parent.as_ref().ok_or(IbfsError::TreeInvariant)?;
            let arc = self.residual.arc(id).ok_or(IbfsError::TreeInvariant)?;
            if arc.capacity == 0 {
                return Err(IbfsError::TreeInvariant);
            }
            let parent = match side {
                NormalSide::Source if arc.to == node => arc.from,
                NormalSide::Sink if arc.from == node => arc.to,
                NormalSide::Source | NormalSide::Sink => return Err(IbfsError::TreeInvariant),
            };
            let parent_state = &self.nodes[parent.as_usize()];
            if parent_state.side.normal() != Some(side)
                || parent_state.distance.checked_add(1) != Some(state.distance)
            {
                return Err(IbfsError::TreeInvariant);
            }
            expected_children[parent.as_usize()].insert(node);
        }
        if expected_children != self.children {
            return Err(IbfsError::TreeInvariant);
        }
        Ok(())
    }

    fn labels(&self) -> Result<Vec<Option<i128>>, IbfsError> {
        self.nodes
            .iter()
            .map(|node| match node.side {
                TreeSide::Source | TreeSide::SourceOrphan => i128::try_from(node.distance)
                    .map(Some)
                    .map_err(|_| IbfsError::ArithmeticOverflow),
                TreeSide::Sink | TreeSide::SinkOrphan => i128::try_from(node.distance)
                    .ok()
                    .and_then(|distance| distance.checked_add(1))
                    .and_then(i128::checked_neg)
                    .map(Some)
                    .ok_or(IbfsError::ArithmeticOverflow),
                TreeSide::Free => Ok(None),
            })
            .collect()
    }

    fn forest_arcs(&self) -> Result<Vec<ResidualArcId>, IbfsError> {
        self.graph
            .node_indices()
            .filter_map(|node| {
                let state = &self.nodes[node.as_usize()];
                let side = state.side.normal()?;
                let id = state.parent.as_ref()?;
                Some(match side {
                    NormalSide::Source => Ok(id.clone()),
                    NormalSide::Sink => Ok(reverse_residual_id(id)),
                })
            })
            .collect()
    }

    fn record(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        metadata: FlowTraceEventMetadata,
        search_order: Vec<NodeIndex>,
        active_path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
        exact_focus: Option<(Vec<NodeIndex>, Vec<ResidualArcId>)>,
    ) -> Result<(), IbfsError> {
        let Some(recorder) = recorder else {
            return Ok(());
        };
        let snapshot = FlowTraceSnapshot::capture(
            self.graph,
            &self.residual,
            self.labels()?,
            search_order,
            active_path,
            Vec::new(),
            trace_metrics(self.metrics),
        )
        .with_forest_overlay(self.graph, self.forest_arcs()?, Vec::new());
        if let Some((nodes, arcs)) = exact_focus {
            let mut focus = nodes
                .into_iter()
                .map(|node| {
                    self.graph
                        .node(node)
                        .map(|node| FlowTraceEntityRef::Node(node.id().clone()))
                        .ok_or(IbfsError::TreeInvariant)
                })
                .collect::<Result<Vec<_>, _>>()?;
            focus.extend(arcs.into_iter().map(FlowTraceEntityRef::ResidualArc));
            recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
        } else {
            recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
        }
        Ok(())
    }
}

fn orphan_queue_view(orphan: NodeIndex, queue: &VecDeque<NodeIndex>) -> Vec<NodeIndex> {
    std::iter::once(orphan)
        .chain(queue.iter().copied())
        .collect()
}

fn reverse_residual_id(id: &ResidualArcId) -> ResidualArcId {
    ResidualArcId::new(
        id.original_edge().clone(),
        match id.direction() {
            ResidualDirection::Forward => ResidualDirection::Reverse,
            ResidualDirection::Reverse => ResidualDirection::Forward,
        },
    )
}

fn checked_add(value: u64, amount: u64) -> Result<u64, IbfsError> {
    value
        .checked_add(amount)
        .ok_or(IbfsError::ArithmeticOverflow)
}

const fn trace_metrics(metrics: IbfsMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.passes as u128,
        relaxation_passes: metrics.forward_passes as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.augmented_path_arcs,
        scaling_phases: metrics.reverse_passes as u128,
        blocking_flow_phases: metrics.tree_attachments as u128,
        relabels: metrics.orphan_relabels as u128,
        retreats: metrics.tree_removals as u128,
        reverse_bfs_runs: metrics.adoption_arc_scans,
        gap_terminations: metrics.orphan_creations as u128,
        pushes: metrics.orphan_visits as u128,
        saturating_pushes: metrics.saturated_tree_arcs as u128,
        nonsaturating_pushes: metrics.same_level_adoptions as u128,
        discharges: metrics.active_vertex_scans as u128,
        active_vertex_selections: metrics.state_transitions as u128,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn graph(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect(),
            edges
                .iter()
                .map(|(id, from, to, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower: 0,
                    capacity: *capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("network");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn solves_parallel_opposite_and_zero_capacity_edges() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("e0", "s", "a", 3),
                ("e1", "s", "a", 2),
                ("e2", "a", "b", 4),
                ("e3", "b", "a", 1),
                ("e4", "b", "t", 4),
                ("e5", "s", "t", 0),
                ("e6", "a", "a", 7),
            ],
        );
        let ibfs = solve_ibfs(&graph, source, sink).expect("IBFS result");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(ibfs.certificate.value, 4);
        assert_eq!(ibfs.certificate.value, oracle.certificate.value);
        assert!(ibfs.metrics.forward_passes > 0);
        assert!(ibfs.metrics.reverse_passes > 0);
    }

    #[test]
    fn repairs_a_source_orphan_by_same_level_adoption() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "d", "t"],
            &[
                ("sa", "s", "a", 2),
                ("sb", "s", "b", 2),
                ("ac", "a", "c", 1),
                ("bc", "b", "c", 2),
                ("cd", "c", "d", 2),
                ("dt", "d", "t", 2),
            ],
        );
        let traced = trace_ibfs(&graph, source, sink).expect("trace");
        assert_eq!(traced.result.certificate.value, 2);
        assert!(traced.result.metrics.orphan_creations > 0);
        assert!(
            traced.result.metrics.same_level_adoptions > 0,
            "events={:?}",
            traced
                .events
                .iter()
                .map(|event| event.catalog_id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "ibfs.adopt-source-orphan")
        );
    }

    #[test]
    fn repairs_a_sink_orphan_by_same_level_adoption() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "d", "e", "t"],
            &[
                ("at", "a", "t", 2),
                ("bt", "b", "t", 2),
                ("ca", "c", "a", 1),
                ("cb", "c", "b", 2),
                ("dc", "d", "c", 2),
                ("ed", "e", "d", 2),
                ("se", "s", "e", 2),
            ],
        );
        let traced = trace_ibfs(&graph, source, sink).expect("trace");
        assert_eq!(traced.result.certificate.value, 2);
        assert!(traced.result.metrics.orphan_creations > 0);
        assert!(
            traced.result.metrics.same_level_adoptions > 0,
            "events={:?}",
            traced
                .events
                .iter()
                .map(|event| event.catalog_id.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "ibfs.adopt-sink-orphan")
        );
    }

    #[test]
    fn repairs_interdependent_orphans_before_certifying_the_cut() {
        let (graph, source, sink) = graph(
            &["s", "n1", "n2", "n3", "n4", "n5", "n6", "t"],
            &[
                ("e000", "s", "n2", 3),
                ("e001", "s", "n4", 4),
                ("e002", "s", "n5", 1),
                ("e003", "s", "n6", 1),
                ("e006", "n1", "n3", 2),
                ("e009", "n1", "t", 2),
                ("e011", "n2", "n1", 1),
                ("e013", "n2", "n5", 1),
                ("e015", "n2", "t", 2),
                ("e021", "n3", "t", 2),
                ("e023", "n4", "n2", 1),
                ("e024", "n4", "n5", 1),
                ("e026", "n4", "t", 3),
                ("e028", "n5", "n1", 3),
                ("e033", "n6", "n2", 1),
            ],
        );
        let ibfs = trace_ibfs(&graph, source, sink).expect("IBFS repairs the orphan wave");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(oracle.certificate.value, 9);
        assert_eq!(ibfs.result.certificate.value, oracle.certificate.value);
        assert!(ibfs.result.metrics.orphan_visits > 1);
    }

    #[test]
    fn preserves_the_boundary_invariant_across_dense_orphan_waves() {
        let (graph, source, sink) = graph(
            &["s", "n01", "n02", "n03", "n04", "n05", "n06", "t"],
            &[
                ("e0000", "s", "n01", 6),
                ("e0001", "s", "n02", 5),
                ("e0002", "s", "n03", 4),
                ("e0003", "s", "n05", 3),
                ("e0004", "s", "n06", 1),
                ("e0006", "n01", "n02", 3),
                ("e0007", "n01", "n03", 3),
                ("e0009", "n01", "t", 2),
                ("e0012", "n02", "n05", 1),
                ("e0014", "n02", "t", 6),
                ("e0016", "n03", "n04", 2),
                ("e0017", "n03", "n05", 3),
                ("e0019", "n03", "t", 2),
                ("e0023", "n04", "t", 4),
                ("e0027", "n05", "n04", 2),
                ("e0029", "n05", "t", 5),
                ("e0031", "n06", "n02", 1),
            ],
        );
        let ibfs = trace_ibfs(&graph, source, sink).expect("IBFS preserves the BFS boundary");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(oracle.certificate.value, 19);
        assert_eq!(ibfs.result.certificate.value, oracle.certificate.value);
        assert!(ibfs.result.metrics.orphan_visits > 1);
    }

    #[test]
    fn ignores_self_loops_as_orphan_parent_candidates() {
        let (graph, source, sink) = graph(
            &["s", "n01", "n06", "n08", "t"],
            &[
                ("e0008", "n01", "t", 1),
                ("e0009", "n06", "n06", 1),
                ("e0011", "n06", "n08", 1),
                ("e0031", "n08", "n01", 1),
                ("e0037", "n06", "t", 6),
                ("e0041", "s", "n06", 7),
            ],
        );
        let ibfs = trace_ibfs(&graph, source, sink).expect("IBFS ignores self-parent arcs");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(oracle.certificate.value, 7);
        assert_eq!(ibfs.result.certificate.value, oracle.certificate.value);
        assert!(ibfs.result.metrics.orphan_visits > 0);
    }

    #[test]
    fn fast_and_trace_profiles_match_and_replay_both_directions() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "c", "t"],
            &[
                ("sa", "s", "a", 4),
                ("sb", "s", "b", 3),
                ("ab", "a", "b", 2),
                ("ac", "a", "c", 2),
                ("bc", "b", "c", 4),
                ("ct", "c", "t", 5),
            ],
        );
        let fast = solve_ibfs(&graph, source, sink).expect("fast");
        let traced = trace_ibfs(&graph, source, sink).expect("trace");
        assert_eq!(traced.result, fast);
        let mut snapshot = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(snapshot, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(snapshot, traced.base_snapshot);
    }

    #[test]
    fn agrees_with_edmonds_karp_on_bounded_deterministic_graphs() {
        for seed in 0_u64..48 {
            let node_ids = ["s", "a", "b", "c", "d", "t"];
            let mut edges = Vec::new();
            let mut ordinal = 0_u64;
            for (from_index, from) in node_ids.iter().enumerate() {
                for (to_index, to) in node_ids.iter().enumerate() {
                    if from_index == to_index || (from_index == 5 && to_index == 0) {
                        continue;
                    }
                    let mixed = seed
                        .wrapping_mul(0x9E37_79B9)
                        .wrapping_add((from_index as u64) * 17)
                        .wrapping_add((to_index as u64) * 31);
                    if mixed % 5 == 0 {
                        let id = format!("e{ordinal:02}");
                        ordinal += 1;
                        edges.push((id, (*from).to_owned(), (*to).to_owned(), mixed % 7));
                    }
                }
            }
            let edge_refs = edges
                .iter()
                .map(|(id, from, to, capacity)| {
                    (id.as_str(), from.as_str(), to.as_str(), *capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&node_ids, &edge_refs);
            let ibfs = solve_ibfs(&graph, source, sink).expect("IBFS");
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                ibfs.certificate.value, oracle.certificate.value,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn exhaustively_agrees_on_all_four_node_unit_digraphs() {
        let node_ids = ["s", "a", "b", "t"];
        let pairs = node_ids
            .iter()
            .enumerate()
            .flat_map(|(from_index, from)| {
                node_ids
                    .iter()
                    .enumerate()
                    .filter(move |(to_index, _)| *to_index != from_index)
                    .map(move |(_, to)| (*from, *to))
            })
            .collect::<Vec<_>>();
        for mask in 0_u64..(1_u64 << pairs.len()) {
            let edges = pairs
                .iter()
                .enumerate()
                .filter(|(ordinal, _)| mask & (1_u64 << ordinal) != 0)
                .map(|(ordinal, (from, to))| (format!("e{ordinal:02}"), *from, *to, 1_u64))
                .collect::<Vec<_>>();
            let edge_refs = edges
                .iter()
                .map(|(id, from, to, capacity)| (id.as_str(), *from, *to, *capacity))
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&node_ids, &edge_refs);
            let ibfs = solve_ibfs(&graph, source, sink)
                .unwrap_or_else(|error| panic!("IBFS failed for mask {mask:#x}: {error}"));
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                ibfs.certificate.value, oracle.certificate.value,
                "mask {mask:#x}"
            );
        }
    }

    #[test]
    fn exact_admission_boundaries_are_stable() {
        let make_nodes = |count: usize| {
            (0..count)
                .map(|index| {
                    let id = match index {
                        0 => "s".to_owned(),
                        1 => "t".to_owned(),
                        _ => format!("n{index:03}"),
                    };
                    FlowNode::new(NodeId::parse(&id).expect("node id"), 0)
                })
                .collect::<Vec<_>>()
        };
        let admitted_nodes = FlowNetwork::new(make_nodes(IBFS_MAX_NODES), Vec::new())
            .expect("graph at node boundary");
        let source = admitted_nodes
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = admitted_nodes
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        assert_eq!(
            solve_ibfs(&admitted_nodes, source, sink)
                .expect("node boundary is admitted")
                .certificate
                .value,
            0
        );

        let rejected_nodes =
            FlowNetwork::new(make_nodes(IBFS_MAX_NODES + 1), Vec::new()).expect("oversized graph");
        assert_eq!(
            solve_ibfs(&rejected_nodes, source, sink),
            Err(IbfsError::AdmissionLimit)
        );

        let make_edges = |count: usize| {
            (0..count)
                .map(|index| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("e{index:04}")).expect("edge id"),
                    from: NodeId::parse("s").expect("tail"),
                    to: NodeId::parse("t").expect("head"),
                    lower: 0,
                    capacity: 0,
                    cost: 0,
                })
                .collect::<Vec<_>>()
        };
        let admitted_edges = FlowNetwork::new(make_nodes(2), make_edges(IBFS_MAX_EDGES))
            .expect("graph at edge boundary");
        let edge_source = admitted_edges
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let edge_sink = admitted_edges
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        assert_eq!(
            solve_ibfs(&admitted_edges, edge_source, edge_sink)
                .expect("edge boundary is admitted")
                .certificate
                .value,
            0
        );
        let rejected_edges = FlowNetwork::new(make_nodes(2), make_edges(IBFS_MAX_EDGES + 1))
            .expect("oversized graph");
        assert_eq!(
            solve_ibfs(&rejected_edges, edge_source, edge_sink),
            Err(IbfsError::AdmissionLimit)
        );
    }

    #[test]
    fn mixed_capacity_suite_covers_both_orphan_trees_and_matches_the_oracle() {
        let node_ids = ["s", "a", "b", "c", "d", "t"];
        let mut covered = BTreeSet::new();
        for seed in 0_u64..512 {
            let mut state = seed ^ 0xD1B5_4A32_D192_ED03;
            let mut edges = Vec::new();
            let mut ordinal = 0_u64;
            for (from_index, from) in node_ids.iter().enumerate() {
                for (to_index, to) in node_ids.iter().enumerate() {
                    if from_index == to_index {
                        continue;
                    }
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let capacity = (state >> 32) % 5;
                    if capacity == 0 {
                        continue;
                    }
                    edges.push((format!("e{ordinal:02}"), *from, *to, capacity));
                    ordinal += 1;
                }
            }
            let edge_refs = edges
                .iter()
                .map(|(id, from, to, capacity)| (id.as_str(), *from, *to, *capacity))
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&node_ids, &edge_refs);
            let traced = trace_ibfs(&graph, source, sink)
                .unwrap_or_else(|error| panic!("IBFS failed for seed {seed}: {error}; {edges:?}"));
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                traced.result.certificate.value, oracle.certificate.value,
                "seed {seed}"
            );
            for event in &traced.events {
                if matches!(
                    event.catalog_id.as_str(),
                    "ibfs.adopt-source-orphan"
                        | "ibfs.adopt-sink-orphan"
                        | "ibfs.relabel-source-orphan"
                        | "ibfs.relabel-sink-orphan"
                        | "ibfs.remove-source-orphan"
                        | "ibfs.remove-sink-orphan"
                ) {
                    covered.insert(event.catalog_id.clone());
                }
            }
        }
        assert_eq!(
            covered,
            BTreeSet::from([
                "ibfs.adopt-source-orphan".to_owned(),
                "ibfs.relabel-sink-orphan".to_owned(),
                "ibfs.relabel-source-orphan".to_owned(),
                "ibfs.remove-sink-orphan".to_owned(),
                "ibfs.remove-source-orphan".to_owned(),
            ])
        );
    }

    #[test]
    fn rejects_nonzero_supply_and_lower_bound_contracts() {
        let (base, source, sink) = graph(&["s", "t"], &[("st", "s", "t", 1)]);
        let supplied = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("s"), 1),
                FlowNode::new(NodeId::parse("t").expect("t"), -1),
            ],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("edge"),
                from: NodeId::parse("s").expect("s"),
                to: NodeId::parse("t").expect("t"),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("supplied graph");
        assert_eq!(
            solve_ibfs(&supplied, source, sink),
            Err(IbfsError::GraphRequirement("zero supplies"))
        );
        let lowered = FlowNetwork::new(
            base.nodes().to_vec(),
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("edge"),
                from: NodeId::parse("s").expect("s"),
                to: NodeId::parse("t").expect("t"),
                lower: 1,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("lowered graph");
        assert_eq!(
            solve_ibfs(&lowered, source, sink),
            Err(IbfsError::GraphRequirement("zero lower bounds"))
        );
    }
}
