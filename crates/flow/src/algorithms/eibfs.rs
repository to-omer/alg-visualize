//! Excesses Incremental Breadth-First Search maximum flow.
//!
//! This is the explicit-tree, round-robin-adoption algorithm from Goldberg,
//! Hed, Kaplan, Kohli, Tarjan, and Werneck (ESA 2015). It deliberately does
//! not share the source/sink-rooted feasible-flow state of standard IBFS:
//! intermediate states are pseudoflows and every positive excess or negative
//! deficit is a forest root on its corresponding side.

mod dynamic;

pub use dynamic::{
    DynamicEibfsMetrics, DynamicEibfsPrefixResult, DynamicEibfsResult, DynamicEibfsSolveError,
    DynamicEibfsTraceResult, solve_dynamic_eibfs, trace_dynamic_eibfs,
};

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    DynamicEibfsTraceOverlay, EibfsTraceForestArc, EibfsTraceMembership, EibfsTraceNodeState,
    EibfsTraceOverlay, EibfsTracePhaseDirection, EibfsTraceRootKind, FlowTraceEntityRef,
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot,
};

/// Conservative complete-trace node limit for the explicit-tree kernel.
pub const EIBFS_MAX_NODES: usize = 256;
/// Conservative complete-trace edge limit for the explicit-tree kernel.
pub const EIBFS_MAX_EDGES: usize = 2_048;
/// Hard ceiling for positive residual and recovery-arc inspections.
pub const EIBFS_MAX_RESIDUAL_ARC_SCANS: u128 = 12_000_000;
/// Hard ceiling for logical forest and pseudoflow mutations.
pub const EIBFS_MAX_STATE_TRANSITIONS: u64 = 150_000;
/// Hard ceiling for connecting-arc pushes.
pub const EIBFS_MAX_AUGMENTATIONS: u64 = 20_000;
/// Conservative before/after node-and-forest cells retained by eager trace.
pub const EIBFS_MAX_TRACE_PROJECTION_UNITS: usize = 250_000;

/// Exact deterministic counters exposed by the EIBFS kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EibfsMetrics {
    /// Growth phases entered.
    pub phases: u64,
    /// Source-forest growth phases.
    pub forward_phases: u64,
    /// Sink-forest growth phases.
    pub reverse_phases: u64,
    /// All charged residual or positive-flow arc inspections.
    pub residual_arc_scans: u128,
    /// Residual arcs inspected while growing a forest.
    pub growth_arc_scans: u128,
    /// Residual arcs inspected by round-robin adoption.
    pub adoption_arc_scans: u128,
    /// Positive-flow arcs inspected by final feasible-flow recovery.
    pub recovery_arc_scans: u128,
    /// Residual arcs inspected while restoring dynamic-update invariants.
    pub dynamic_repair_arc_scans: u128,
    /// Free vertices attached during growth.
    pub tree_attachments: u64,
    /// Connecting-arc pseudoflow pushes.
    pub bridge_pushes: u64,
    /// Bridge pushes whose two roots are source and sink.
    pub terminal_terminal_bridges: u64,
    /// Bridge pushes whose source-side root alone is the source.
    pub source_root_bridges: u64,
    /// Bridge pushes whose sink-side root alone is the sink.
    pub sink_root_bridges: u64,
    /// Bridge pushes whose two roots are nonterminals.
    pub nonterminal_root_bridges: u64,
    /// Units pushed on connecting arcs.
    pub bridge_units: u128,
    /// Local pushes along a forest path to drain a bad-sign vertex.
    pub tree_path_pushes: u64,
    /// Tree arcs saturated by a drain push.
    pub saturated_tree_arcs: u64,
    /// Orphans created, including cascaded children and invalid roots.
    pub orphan_creations: u64,
    /// FIFO orphan records processed.
    pub orphan_visits: u64,
    /// Orphans adopted at their current label.
    pub same_level_adoptions: u64,
    /// Orphan relabel operations.
    pub orphan_relabels: u64,
    /// Orphans removed from their old forest.
    pub tree_removals: u64,
    /// Removed bad-sign roots migrated to the opposite forest.
    pub side_migrations: u64,
    /// Same-cut positive-flow cancellations used for final recovery.
    pub recovery_cancellations: u64,
    /// Units cancelled during final recovery.
    pub recovered_units: u128,
    /// Logical mutations charged against the work ceiling.
    pub state_transitions: u64,
}

/// Certified static Excesses-IBFS result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EibfsResult {
    /// Feasible original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut witness.
    pub certificate: MaxFlowCertificate,
    /// Deterministic operation counts.
    pub metrics: EibfsMetrics,
}

/// Certified result plus a complete reversible trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EibfsTraceResult {
    /// Same result produced by the non-tracing profile.
    pub result: EibfsResult,
    /// Replay boundary before forest initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible semantic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Verified feasible optimal replay boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// EIBFS admission, execution, recovery, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EibfsError {
    /// Input exceeds the deliberately small explicit-tree band.
    #[error("graph exceeds EIBFS admission limits")]
    AdmissionLimit,
    /// The graph is outside the zero-feasible-flow contract.
    #[error("EIBFS graph requirement is not satisfied: {0}")]
    GraphRequirement(&'static str),
    /// A deterministic work ceiling was reached.
    #[error("EIBFS work limit reached")]
    WorkLimit,
    /// Checked counter, divergence, or label arithmetic overflowed.
    #[error("EIBFS arithmetic overflow")]
    ArithmeticOverflow,
    /// A pseudoflow-forest, sign, current-arc, or label invariant failed.
    #[error("EIBFS forest invariant failed")]
    ForestInvariant,
    /// Same-cut feasible-flow recovery could not find its required flow path.
    #[error("EIBFS feasible-flow recovery invariant failed")]
    RecoveryInvariant,
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

/// Validates the static zero-pseudoflow Excesses-IBFS contract.
///
/// # Errors
///
/// Requires distinct in-range terminals, zero supplies, zero lower bounds, and
/// the conservative explicit-tree admission band. Parallel, opposite,
/// zero-capacity, and self-loop edges are accepted.
pub fn validate_eibfs_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), EibfsError> {
    if graph.nodes().len() > EIBFS_MAX_NODES || graph.edges().len() > EIBFS_MAX_EDGES {
        return Err(EibfsError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(EibfsError::GraphRequirement("distinct source and sink"));
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(EibfsError::GraphRequirement("zero supplies"));
    }
    if graph.edges().iter().any(|edge| edge.lower() != 0) {
        return Err(EibfsError::GraphRequirement("zero lower bounds"));
    }
    Ok(())
}

/// Solves a zero-feasible-flow maximum-flow problem with Excesses IBFS.
///
/// # Errors
///
/// Rejects input outside [`validate_eibfs_graph`], bounded-work exhaustion,
/// pseudoflow-forest or recovery failures, and an independently rejected result.
pub fn solve_eibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<EibfsResult, EibfsError> {
    solve_eibfs_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves Excesses IBFS while recording semantic, reversible state changes.
///
/// # Errors
///
/// Returns the same failures as [`solve_eibfs`], plus trace-diff failures.
pub fn trace_eibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<EibfsTraceResult, EibfsError> {
    let run = solve_eibfs_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(EibfsError::ForestInvariant)?;
    Ok(EibfsTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct EibfsInternalRun {
    result: EibfsResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForestState {
    Free,
    Source,
    Sink,
    SourceOrphan,
    SinkOrphan,
}

impl ForestState {
    const fn normal(self) -> Option<ForestSide> {
        match self {
            Self::Source => Some(ForestSide::Source),
            Self::Sink => Some(ForestSide::Sink),
            Self::Free | Self::SourceOrphan | Self::SinkOrphan => None,
        }
    }

    const fn orphan(self) -> Option<ForestSide> {
        match self {
            Self::SourceOrphan => Some(ForestSide::Source),
            Self::SinkOrphan => Some(ForestSide::Sink),
            Self::Free | Self::Source | Self::Sink => None,
        }
    }

    const fn belongs_to(self, side: ForestSide) -> bool {
        matches!(
            (self, side),
            (Self::Source | Self::SourceOrphan, ForestSide::Source)
                | (Self::Sink | Self::SinkOrphan, ForestSide::Sink)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForestSide {
    Source,
    Sink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhaseDirection {
    Forward,
    Reverse,
}

impl PhaseDirection {
    const fn growing_side(self) -> ForestSide {
        match self {
            Self::Forward => ForestSide::Source,
            Self::Reverse => ForestSide::Sink,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Forward => Self::Reverse,
            Self::Reverse => Self::Forward,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EibfsNode {
    state: ForestState,
    source_label: usize,
    sink_label: usize,
    parent: Option<ResidualArcId>,
    growth_cursor: usize,
    current_arc: usize,
}

impl Default for EibfsNode {
    fn default() -> Self {
        Self {
            state: ForestState::Free,
            source_label: 0,
            sink_label: 0,
            parent: None,
            growth_cursor: 0,
            current_arc: 0,
        }
    }
}

#[derive(Clone, Debug)]
struct ParentCandidate {
    arc: ResidualArcId,
    position: usize,
    label: usize,
}

#[derive(Clone)]
struct EibfsEngine<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    residual: ResidualState<'graph>,
    excess: Vec<i128>,
    nodes: Vec<EibfsNode>,
    children: Vec<BTreeSet<NodeIndex>>,
    outgoing: Vec<Vec<ResidualArcId>>,
    incoming: Vec<Vec<ResidualArcId>>,
    source_depth: usize,
    sink_depth: usize,
    direction: PhaseDirection,
    recovery_mode: bool,
    dynamic_overlay: Option<DynamicEibfsTraceOverlay>,
    metrics: EibfsMetrics,
    work_scan_start: u128,
    work_transition_start: u64,
    work_bridge_start: u64,
}

fn solve_eibfs_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<EibfsInternalRun, EibfsError> {
    validate_eibfs_graph(graph, source, sink)?;
    let residual = ResidualState::at_lower_bounds(graph);
    let mut recorder = if with_trace {
        let base = FlowTraceSnapshot::capture(
            graph,
            &residual,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            vec![0; graph.nodes().len()],
            FlowTraceMetrics::default(),
        );
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };
    let mut engine = EibfsEngine::new(graph, source, sink, residual)?;
    engine.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "eibfs.initialize-pseudoflow-forests",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "eibfs:initialize-s-t-root-forests",
        },
        vec![source, sink],
        Vec::new(),
        Some(("forest-roots", 2)),
    )?;

    engine.run(&mut recorder)?;
    let cut = engine
        .nodes
        .iter()
        .map(|node| match engine.direction {
            PhaseDirection::Forward => node.state.belongs_to(ForestSide::Source),
            PhaseDirection::Reverse => !node.state.belongs_to(ForestSide::Sink),
        })
        .collect::<Vec<_>>();
    engine.begin_recovery(&cut, recorder.as_mut())?;
    engine.recover_feasible_flow(&cut, recorder.as_mut())?;
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
            catalog_id: "eibfs.optimal-feasible-flow",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "eibfs:return-recovered-max-flow-min-cut",
        },
        cut_order,
        Vec::new(),
        Some(("cut", certificate.cut_bound)),
    )?;
    let result = EibfsResult {
        flows,
        certificate,
        metrics: engine.metrics,
    };
    Ok(EibfsInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

impl<'graph> EibfsEngine<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        residual: ResidualState<'graph>,
    ) -> Result<Self, EibfsError> {
        let count = graph.nodes().len();
        let mut outgoing = vec![Vec::new(); count];
        let mut incoming = vec![Vec::new(); count];
        for edge in graph.edges() {
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let arc = residual.arc(&id).ok_or(EibfsError::ForestInvariant)?;
                outgoing[arc.from.as_usize()].push(id.clone());
                incoming[arc.to.as_usize()].push(id);
            }
        }
        for ids in outgoing.iter_mut().chain(incoming.iter_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }
        let mut nodes = vec![EibfsNode::default(); count];
        nodes[source.as_usize()].state = ForestState::Source;
        nodes[sink.as_usize()].state = ForestState::Sink;
        Ok(Self {
            graph,
            source,
            sink,
            residual,
            excess: vec![0; count],
            nodes,
            children: vec![BTreeSet::new(); count],
            outgoing,
            incoming,
            source_depth: 0,
            sink_depth: 0,
            direction: PhaseDirection::Forward,
            recovery_mode: false,
            dynamic_overlay: None,
            metrics: EibfsMetrics::default(),
            work_scan_start: 0,
            work_transition_start: 0,
            work_bridge_start: 0,
        })
    }

    fn run(&mut self, recorder: &mut Option<FlowTraceRecorder<'graph>>) -> Result<(), EibfsError> {
        loop {
            self.enter_phase()?;
            let depth = self.current_depth();
            self.record(
                recorder.as_mut(),
                FlowTraceEventMetadata {
                    catalog_id: match self.direction {
                        PhaseDirection::Forward => "eibfs.start-forward-phase",
                        PhaseDirection::Reverse => "eibfs.start-reverse-phase",
                    },
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: match self.direction {
                        PhaseDirection::Forward => "eibfs:grow-source-forest-one-level",
                        PhaseDirection::Reverse => "eibfs:grow-sink-forest-one-level",
                    },
                },
                self.frontier(self.direction.growing_side(), depth),
                Vec::new(),
                Some(("depth", to_i128(depth)?)),
            )?;
            if !self.grow_phase(recorder)? {
                self.record(
                    recorder.as_mut(),
                    FlowTraceEventMetadata {
                        catalog_id: "eibfs.no-next-level",
                        minimum_granularity: TraceGranularityV1::Phase,
                        pseudocode_line: "eibfs:terminate-when-next-level-is-empty",
                    },
                    Vec::new(),
                    Vec::new(),
                    Some(("depth", to_i128(depth)?)),
                )?;
                return Ok(());
            }
            self.advance_depth()?;
            self.validate_stable_forest()?;
            self.record(
                recorder.as_mut(),
                FlowTraceEventMetadata {
                    catalog_id: "eibfs.complete-phase",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "eibfs:advance-grown-forest-boundary",
                },
                self.frontier(self.direction.growing_side(), self.current_depth()),
                Vec::new(),
                Some(("depth", to_i128(self.current_depth())?)),
            )?;
            self.direction = self.direction.next();
        }
    }

    fn enter_phase(&mut self) -> Result<(), EibfsError> {
        self.metrics.phases = add_u64(self.metrics.phases, 1)?;
        match self.direction {
            PhaseDirection::Forward => {
                self.metrics.forward_phases = add_u64(self.metrics.forward_phases, 1)?;
            }
            PhaseDirection::Reverse => {
                self.metrics.reverse_phases = add_u64(self.metrics.reverse_phases, 1)?;
            }
        }
        self.validate_stable_forest()
    }

    const fn current_depth(&self) -> usize {
        match self.direction {
            PhaseDirection::Forward => self.source_depth,
            PhaseDirection::Reverse => self.sink_depth,
        }
    }

    fn advance_depth(&mut self) -> Result<(), EibfsError> {
        let depth = match self.direction {
            PhaseDirection::Forward => &mut self.source_depth,
            PhaseDirection::Reverse => &mut self.sink_depth,
        };
        *depth = depth.checked_add(1).ok_or(EibfsError::ArithmeticOverflow)?;
        Ok(())
    }

    fn grow_phase(
        &mut self,
        recorder: &mut Option<FlowTraceRecorder<'graph>>,
    ) -> Result<bool, EibfsError> {
        let side = self.direction.growing_side();
        let depth = self.current_depth();
        let mut exhausted = vec![false; self.nodes.len()];
        loop {
            let active = self.graph.node_indices().find(|node| {
                self.nodes[node.as_usize()].state.normal() == Some(side)
                    && self.label(*node, side) == depth
                    && !exhausted[node.as_usize()]
            });
            let Some(active) = active else {
                break;
            };
            if self.scan_active(active, side, depth, recorder.as_mut())? {
                exhausted[active.as_usize()] = true;
            }
        }
        self.validate_stable_forest()?;
        let next = depth.checked_add(1).ok_or(EibfsError::ArithmeticOverflow)?;
        Ok(self.graph.node_indices().any(|node| {
            self.nodes[node.as_usize()].state.normal() == Some(side)
                && self.label(node, side) == next
        }))
    }

    /// Returns true when the active vertex exhausted its growth adjacency.
    fn scan_active(
        &mut self,
        active: NodeIndex,
        side: ForestSide,
        depth: usize,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<bool, EibfsError> {
        loop {
            let position = self.nodes[active.as_usize()].growth_cursor;
            let adjacency = match self.direction {
                PhaseDirection::Forward => &self.outgoing[active.as_usize()],
                PhaseDirection::Reverse => &self.incoming[active.as_usize()],
            };
            let Some(id) = adjacency.get(position).cloned() else {
                return Ok(true);
            };
            let arc = self.residual.arc(&id).ok_or(EibfsError::ForestInvariant)?;
            if arc.capacity == 0 {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                continue;
            }
            self.count_scan(ScanKind::Growth)?;
            let candidate = match self.direction {
                PhaseDirection::Forward => arc.to,
                PhaseDirection::Reverse => arc.from,
            };
            let candidate_state = self.nodes[candidate.as_usize()].state;
            if candidate_state == ForestState::Free {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                self.attach_free(active, candidate, id.clone())?;
                self.record(
                    recorder.as_deref_mut(),
                    FlowTraceEventMetadata {
                        catalog_id: match self.direction {
                            PhaseDirection::Forward => "eibfs.attach-source-forest",
                            PhaseDirection::Reverse => "eibfs.attach-sink-forest",
                        },
                        minimum_granularity: TraceGranularityV1::Operation,
                        pseudocode_line: "eibfs:attach-free-vertex-at-next-level",
                    },
                    vec![active, candidate],
                    vec![id],
                    Some(("distance", to_i128(depth + 1)?)),
                )?;
                continue;
            }
            if candidate_state.belongs_to(side) {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                continue;
            }
            let is_bridge = match self.direction {
                PhaseDirection::Forward => candidate_state.belongs_to(ForestSide::Sink),
                PhaseDirection::Reverse => candidate_state.belongs_to(ForestSide::Source),
            };
            if !is_bridge {
                self.nodes[active.as_usize()].growth_cursor = position + 1;
                continue;
            }
            let (source_endpoint, sink_endpoint) = match self.direction {
                PhaseDirection::Forward => (active, candidate),
                PhaseDirection::Reverse => (candidate, active),
            };
            self.push_connecting_arc(
                source_endpoint,
                sink_endpoint,
                id.clone(),
                recorder.as_deref_mut(),
            )?;
            if self.nodes[active.as_usize()].state.normal() == Some(side)
                && self.label(active, side) == depth
            {
                self.nodes[active.as_usize()].growth_cursor = position;
            }
            return Ok(false);
        }
    }

    fn attach_free(
        &mut self,
        parent: NodeIndex,
        child: NodeIndex,
        parent_arc: ResidualArcId,
    ) -> Result<(), EibfsError> {
        if self.nodes[child.as_usize()].state != ForestState::Free {
            return Err(EibfsError::ForestInvariant);
        }
        let side = self.direction.growing_side();
        let label = self
            .label(parent, side)
            .checked_add(1)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        let state = &mut self.nodes[child.as_usize()];
        state.state = match side {
            ForestSide::Source => ForestState::Source,
            ForestSide::Sink => ForestState::Sink,
        };
        match side {
            ForestSide::Source => state.source_label = label,
            ForestSide::Sink => state.sink_label = label,
        }
        state.parent = Some(parent_arc);
        state.growth_cursor = 0;
        state.current_arc = 0;
        self.children[parent.as_usize()].insert(child);
        self.metrics.tree_attachments = add_u64(self.metrics.tree_attachments, 1)?;
        self.count_transition()
    }

    fn push_connecting_arc(
        &mut self,
        source_endpoint: NodeIndex,
        sink_endpoint: NodeIndex,
        bridge: ResidualArcId,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        if self
            .metrics
            .bridge_pushes
            .checked_sub(self.work_bridge_start)
            .ok_or(EibfsError::ArithmeticOverflow)?
            >= EIBFS_MAX_AUGMENTATIONS
        {
            return Err(EibfsError::WorkLimit);
        }
        let bridge_arc = self
            .residual
            .arc(&bridge)
            .ok_or(EibfsError::ForestInvariant)?;
        if bridge_arc.capacity == 0
            || bridge_arc.from != source_endpoint
            || bridge_arc.to != sink_endpoint
            || !self.nodes[source_endpoint.as_usize()]
                .state
                .belongs_to(ForestSide::Source)
            || !self.nodes[sink_endpoint.as_usize()]
                .state
                .belongs_to(ForestSide::Sink)
        {
            return Err(EibfsError::ForestInvariant);
        }
        let (source_root, source_bottleneck) =
            self.root_and_bottleneck(source_endpoint, ForestSide::Source)?;
        let (sink_root, sink_bottleneck) =
            self.root_and_bottleneck(sink_endpoint, ForestSide::Sink)?;
        let bridge_case = match (source_root == self.source, sink_root == self.sink) {
            (true, true) => BridgeCase::TerminalTerminal,
            (true, false) => BridgeCase::SourceRoot,
            (false, true) => BridgeCase::SinkRoot,
            (false, false) => BridgeCase::NonterminalRoots,
        };
        let amount = match bridge_case {
            BridgeCase::TerminalTerminal => bridge_arc.capacity,
            BridgeCase::SourceRoot => bridge_arc.capacity.min(source_bottleneck),
            BridgeCase::SinkRoot => bridge_arc.capacity.min(sink_bottleneck),
            BridgeCase::NonterminalRoots => {
                let source_excess =
                    positive_cap(self.excess[source_root.as_usize()], bridge_arc.capacity);
                let sink_deficit =
                    negative_cap(self.excess[sink_root.as_usize()], bridge_arc.capacity)?;
                bridge_arc
                    .capacity
                    .min(source_bottleneck)
                    .min(sink_bottleneck)
                    .min(source_excess)
                    .min(sink_deficit)
            }
        };
        if amount == 0 {
            return Err(EibfsError::ForestInvariant);
        }
        self.push_residual(&bridge, amount)?;
        self.metrics.bridge_pushes = add_u64(self.metrics.bridge_pushes, 1)?;
        self.count_bridge_case(bridge_case)?;
        self.metrics.bridge_units = self
            .metrics
            .bridge_units
            .checked_add(u128::from(amount))
            .ok_or(EibfsError::ArithmeticOverflow)?;
        self.count_transition()?;
        let mut root_orphans = VecDeque::new();
        let mut queued_root_orphans = BTreeSet::new();
        while let Some((root, side)) = self.invalid_nonterminal_root() {
            self.make_orphan(root, side, &mut root_orphans, &mut queued_root_orphans)?;
        }
        self.record(
            recorder.as_deref_mut(),
            FlowTraceEventMetadata {
                catalog_id: bridge_case.catalog_id(),
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "eibfs:push-source-defined-bridge-amount",
            },
            vec![source_root, source_endpoint, sink_endpoint, sink_root],
            vec![bridge],
            Some(("amount", i128::from(amount))),
        )?;
        self.adopt_orphans(
            &mut root_orphans,
            &mut queued_root_orphans,
            recorder.as_deref_mut(),
        )?;
        self.repair_bad_signs(recorder)?;
        self.validate_stable_forest()
    }

    fn count_bridge_case(&mut self, bridge_case: BridgeCase) -> Result<(), EibfsError> {
        let counter = match bridge_case {
            BridgeCase::TerminalTerminal => &mut self.metrics.terminal_terminal_bridges,
            BridgeCase::SourceRoot => &mut self.metrics.source_root_bridges,
            BridgeCase::SinkRoot => &mut self.metrics.sink_root_bridges,
            BridgeCase::NonterminalRoots => &mut self.metrics.nonterminal_root_bridges,
        };
        *counter = add_u64(*counter, 1)?;
        Ok(())
    }

    fn root_and_bottleneck(
        &self,
        node: NodeIndex,
        side: ForestSide,
    ) -> Result<(NodeIndex, u64), EibfsError> {
        let mut cursor = node;
        let mut bottleneck = u64::MAX;
        let mut length = 0;
        while let Some(id) = self.nodes[cursor.as_usize()].parent.as_ref() {
            let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
            if arc.capacity == 0 {
                return Err(EibfsError::ForestInvariant);
            }
            bottleneck = bottleneck.min(arc.capacity);
            cursor = match side {
                ForestSide::Source if arc.to == cursor => arc.from,
                ForestSide::Sink if arc.from == cursor => arc.to,
                ForestSide::Source | ForestSide::Sink => {
                    return Err(EibfsError::ForestInvariant);
                }
            };
            length += 1;
            if length > self.nodes.len() {
                return Err(EibfsError::ForestInvariant);
            }
        }
        Ok((cursor, bottleneck))
    }

    fn repair_bad_signs(
        &mut self,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        loop {
            if let Some((root, side)) = self.invalid_nonterminal_root() {
                let mut queue = VecDeque::new();
                let mut queued = BTreeSet::new();
                self.make_orphan(root, side, &mut queue, &mut queued)?;
                self.adopt_orphans(&mut queue, &mut queued, recorder.as_deref_mut())?;
                continue;
            }
            if let Some(node) = self.highest_bad_sign(ForestSide::Sink) {
                let mut queue = VecDeque::new();
                let mut queued = BTreeSet::new();
                self.drain_bad_sign(
                    node,
                    ForestSide::Sink,
                    &mut queue,
                    &mut queued,
                    recorder.as_deref_mut(),
                )?;
                self.adopt_orphans(&mut queue, &mut queued, recorder.as_deref_mut())?;
                continue;
            }
            if let Some(node) = self.highest_bad_sign(ForestSide::Source) {
                let mut queue = VecDeque::new();
                let mut queued = BTreeSet::new();
                self.drain_bad_sign(
                    node,
                    ForestSide::Source,
                    &mut queue,
                    &mut queued,
                    recorder.as_deref_mut(),
                )?;
                self.adopt_orphans(&mut queue, &mut queued, recorder.as_deref_mut())?;
                continue;
            }
            return Ok(());
        }
    }

    fn invalid_nonterminal_root(&self) -> Option<(NodeIndex, ForestSide)> {
        self.graph.node_indices().find_map(|node| {
            let side = self.nodes[node.as_usize()].state.normal()?;
            if self.nodes[node.as_usize()].parent.is_some() || node == self.terminal(side) {
                return None;
            }
            self.root_has_wrong_sign(node, side).then_some((node, side))
        })
    }

    fn highest_bad_sign(&self, side: ForestSide) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .filter(|node| {
                self.nodes[node.as_usize()].state.normal() == Some(side)
                    && match side {
                        ForestSide::Source => {
                            *node != self.source && self.excess[node.as_usize()] < 0
                        }
                        ForestSide::Sink => *node != self.sink && self.excess[node.as_usize()] > 0,
                    }
            })
            .max_by(|left, right| {
                self.label(*left, side)
                    .cmp(&self.label(*right, side))
                    .then_with(|| right.cmp(left))
            })
    }

    fn drain_bad_sign(
        &mut self,
        start: NodeIndex,
        side: ForestSide,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        let mut cursor = start;
        let mut traversed = 0;
        loop {
            if cursor == self.terminal(side) {
                return Ok(());
            }
            let wrong_amount = match side {
                ForestSide::Source => negative_cap(self.excess[cursor.as_usize()], u64::MAX)?,
                ForestSide::Sink => positive_cap(self.excess[cursor.as_usize()], u64::MAX),
            };
            if wrong_amount == 0 {
                return Ok(());
            }
            let Some(id) = self.nodes[cursor.as_usize()].parent.clone() else {
                self.make_orphan(cursor, side, queue, queued)?;
                return Ok(());
            };
            let arc = self.residual.arc(&id).ok_or(EibfsError::ForestInvariant)?;
            if arc.capacity == 0 {
                self.make_orphan(cursor, side, queue, queued)?;
                return Ok(());
            }
            let parent = match side {
                ForestSide::Source if arc.to == cursor => arc.from,
                ForestSide::Sink if arc.from == cursor => arc.to,
                ForestSide::Source | ForestSide::Sink => {
                    return Err(EibfsError::ForestInvariant);
                }
            };
            let amount = wrong_amount.min(arc.capacity);
            self.push_residual(&id, amount)?;
            self.metrics.tree_path_pushes = add_u64(self.metrics.tree_path_pushes, 1)?;
            if amount == arc.capacity {
                self.metrics.saturated_tree_arcs = add_u64(self.metrics.saturated_tree_arcs, 1)?;
                self.make_orphan(cursor, side, queue, queued)?;
            }
            while let Some((root, root_side)) = self.invalid_nonterminal_root() {
                self.make_orphan(root, root_side, queue, queued)?;
            }
            self.count_transition()?;
            self.record(
                recorder.as_deref_mut(),
                FlowTraceEventMetadata {
                    catalog_id: match side {
                        ForestSide::Source => "eibfs.drain-source-deficit",
                        ForestSide::Sink => "eibfs.drain-sink-excess",
                    },
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "eibfs:push-bad-sign-toward-forest-root",
                },
                orphan_queue_view(cursor, queue),
                vec![id],
                Some(("amount", i128::from(amount))),
            )?;
            if self.has_bad_sign(cursor, side) {
                return Ok(());
            }
            cursor = parent;
            traversed += 1;
            if traversed > self.nodes.len() {
                return Err(EibfsError::ForestInvariant);
            }
            if self.nodes[cursor.as_usize()].parent.is_none()
                && cursor != self.terminal(side)
                && self.root_has_wrong_sign(cursor, side)
            {
                self.make_orphan(cursor, side, queue, queued)?;
                return Ok(());
            }
        }
    }

    fn has_bad_sign(&self, node: NodeIndex, side: ForestSide) -> bool {
        match side {
            ForestSide::Source => node != self.source && self.excess[node.as_usize()] < 0,
            ForestSide::Sink => node != self.sink && self.excess[node.as_usize()] > 0,
        }
    }

    fn root_has_wrong_sign(&self, node: NodeIndex, side: ForestSide) -> bool {
        match side {
            ForestSide::Source => self.excess[node.as_usize()] <= 0,
            ForestSide::Sink => self.excess[node.as_usize()] >= 0,
        }
    }

    const fn terminal(&self, side: ForestSide) -> NodeIndex {
        match side {
            ForestSide::Source => self.source,
            ForestSide::Sink => self.sink,
        }
    }

    fn make_orphan(
        &mut self,
        node: NodeIndex,
        side: ForestSide,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
    ) -> Result<(), EibfsError> {
        if node == self.source || node == self.sink {
            return Err(EibfsError::ForestInvariant);
        }
        if self.nodes[node.as_usize()].state.orphan() == Some(side) {
            if queued.insert(node) {
                queue.push_back(node);
            }
            return Ok(());
        }
        if self.nodes[node.as_usize()].state.normal() != Some(side) {
            return Err(EibfsError::ForestInvariant);
        }
        if self.nodes[node.as_usize()].parent.is_some() {
            self.detach_parent(node, side)?;
        }
        self.nodes[node.as_usize()].state = match side {
            ForestSide::Source => ForestState::SourceOrphan,
            ForestSide::Sink => ForestState::SinkOrphan,
        };
        self.nodes[node.as_usize()].growth_cursor = 0;
        self.metrics.orphan_creations = add_u64(self.metrics.orphan_creations, 1)?;
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
    ) -> Result<(), EibfsError> {
        while let Some(orphan) = queue.pop_front() {
            queued.remove(&orphan);
            self.adopt_one(orphan, queue, queued, recorder.as_deref_mut())?;
        }
        Ok(())
    }

    fn adopt_one(
        &mut self,
        orphan: NodeIndex,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        let side = self.nodes[orphan.as_usize()]
            .state
            .orphan()
            .ok_or(EibfsError::ForestInvariant)?;
        self.metrics.orphan_visits = add_u64(self.metrics.orphan_visits, 1)?;
        let old_label = self.label(orphan, side);
        if let Some(parent) = self.same_level_parent(orphan, side)? {
            self.attach_orphan(orphan, side, &parent, old_label)?;
            self.metrics.same_level_adoptions = add_u64(self.metrics.same_level_adoptions, 1)?;
            self.count_transition()?;
            return self.record_adoption(
                recorder,
                orphan,
                queue,
                side,
                "eibfs:adopt-at-current-distance",
            );
        }
        let candidate = self.minimum_parent(orphan, side)?;
        if let Some(parent) = candidate.as_ref()
            && parent.label.checked_add(1) == Some(old_label)
        {
            self.attach_orphan(orphan, side, parent, old_label)?;
            self.metrics.same_level_adoptions = add_u64(self.metrics.same_level_adoptions, 1)?;
            self.count_transition()?;
            return self.record_adoption(
                recorder,
                orphan,
                queue,
                side,
                "eibfs:adopt-newly-available-current-level-parent",
            );
        }
        if let Some(parent) = candidate
            && self.relabel_allowed(side, parent.label)
        {
            let new_label = parent
                .label
                .checked_add(1)
                .ok_or(EibfsError::ArithmeticOverflow)?;
            if new_label <= old_label {
                return Err(EibfsError::ForestInvariant);
            }
            self.orphan_children(orphan, side, queue, queued)?;
            self.attach_orphan(orphan, side, &parent, new_label)?;
            self.metrics.orphan_relabels = add_u64(self.metrics.orphan_relabels, 1)?;
            self.count_transition()?;
            self.record(
                recorder,
                FlowTraceEventMetadata {
                    catalog_id: match side {
                        ForestSide::Source => "eibfs.relabel-source-orphan",
                        ForestSide::Sink => "eibfs.relabel-sink-orphan",
                    },
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "eibfs:raise-label-and-orphan-children",
                },
                orphan_queue_view(orphan, queue),
                Vec::new(),
                Some(("distance", to_i128(new_label)?)),
            )?;
            return Ok(());
        }
        self.remove_orphan(orphan, side, old_label, queue, queued, recorder)
    }

    fn record_adoption(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        orphan: NodeIndex,
        queue: &VecDeque<NodeIndex>,
        side: ForestSide,
        pseudocode_line: &'static str,
    ) -> Result<(), EibfsError> {
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: match side {
                    ForestSide::Source => "eibfs.adopt-source-orphan",
                    ForestSide::Sink => "eibfs.adopt-sink-orphan",
                },
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line,
            },
            orphan_queue_view(orphan, queue),
            Vec::new(),
            Some(("distance", to_i128(self.label(orphan, side))?)),
        )
    }

    fn same_level_parent(
        &mut self,
        orphan: NodeIndex,
        side: ForestSide,
    ) -> Result<Option<ParentCandidate>, EibfsError> {
        let label = self.label(orphan, side);
        let start = self.nodes[orphan.as_usize()].current_arc;
        let ids = self.parent_adjacency(orphan, side).to_vec();
        for (position, id) in ids.iter().enumerate().skip(start) {
            let Some((_, parent_label)) = self.valid_parent(id, orphan, side)? else {
                continue;
            };
            if parent_label.checked_add(1) == Some(label) {
                return Ok(Some(ParentCandidate {
                    arc: id.clone(),
                    position,
                    label: parent_label,
                }));
            }
        }
        Ok(None)
    }

    fn minimum_parent(
        &mut self,
        orphan: NodeIndex,
        side: ForestSide,
    ) -> Result<Option<ParentCandidate>, EibfsError> {
        let ids = self.parent_adjacency(orphan, side).to_vec();
        let mut best: Option<ParentCandidate> = None;
        for (position, id) in ids.iter().enumerate() {
            let Some((_, label)) = self.valid_parent(id, orphan, side)? else {
                continue;
            };
            if best
                .as_ref()
                .is_none_or(|current| (label, id) < (current.label, &current.arc))
            {
                best = Some(ParentCandidate {
                    arc: id.clone(),
                    position,
                    label,
                });
            }
        }
        Ok(best)
    }

    fn valid_parent(
        &mut self,
        id: &ResidualArcId,
        orphan: NodeIndex,
        side: ForestSide,
    ) -> Result<Option<(NodeIndex, usize)>, EibfsError> {
        let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
        if arc.capacity == 0 {
            return Ok(None);
        }
        self.count_scan(ScanKind::Adoption)?;
        let parent = match side {
            ForestSide::Source if arc.to == orphan => arc.from,
            ForestSide::Sink if arc.from == orphan => arc.to,
            ForestSide::Source | ForestSide::Sink => {
                return Err(EibfsError::ForestInvariant);
            }
        };
        if parent == orphan || !self.nodes[parent.as_usize()].state.belongs_to(side) {
            return Ok(None);
        }
        Ok(Some((parent, self.label(parent, side))))
    }

    fn relabel_allowed(&self, side: ForestSide, parent_label: usize) -> bool {
        match (side, self.direction) {
            (ForestSide::Source, PhaseDirection::Forward) => parent_label <= self.source_depth,
            (ForestSide::Sink, PhaseDirection::Forward) => parent_label < self.sink_depth,
            (ForestSide::Sink, PhaseDirection::Reverse) => parent_label <= self.sink_depth,
            (ForestSide::Source, PhaseDirection::Reverse) => parent_label < self.source_depth,
        }
    }

    fn attach_orphan(
        &mut self,
        orphan: NodeIndex,
        side: ForestSide,
        parent: &ParentCandidate,
        label: usize,
    ) -> Result<(), EibfsError> {
        let arc = self
            .residual
            .arc(&parent.arc)
            .ok_or(EibfsError::ForestInvariant)?;
        if arc.capacity == 0 || parent.label.checked_add(1) != Some(label) {
            return Err(EibfsError::ForestInvariant);
        }
        let parent_node = match side {
            ForestSide::Source if arc.to == orphan => arc.from,
            ForestSide::Sink if arc.from == orphan => arc.to,
            ForestSide::Source | ForestSide::Sink => {
                return Err(EibfsError::ForestInvariant);
            }
        };
        if parent_node == orphan || !self.nodes[parent_node.as_usize()].state.belongs_to(side) {
            return Err(EibfsError::ForestInvariant);
        }
        let state = &mut self.nodes[orphan.as_usize()];
        state.state = match side {
            ForestSide::Source => ForestState::Source,
            ForestSide::Sink => ForestState::Sink,
        };
        match side {
            ForestSide::Source => state.source_label = label,
            ForestSide::Sink => state.sink_label = label,
        }
        state.parent = Some(parent.arc.clone());
        state.current_arc = parent.position;
        state.growth_cursor = 0;
        self.children[parent_node.as_usize()].insert(orphan);
        Ok(())
    }

    fn remove_orphan(
        &mut self,
        orphan: NodeIndex,
        side: ForestSide,
        old_label: usize,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        self.orphan_children(orphan, side, queue, queued)?;
        let amount = self.excess[orphan.as_usize()];
        let (new_state, new_label, migrated) = match (side, amount.cmp(&0)) {
            (_, std::cmp::Ordering::Equal) => (ForestState::Free, None, false),
            (ForestSide::Sink, std::cmp::Ordering::Greater) => {
                let label = match self.direction {
                    PhaseDirection::Forward => self
                        .source_depth
                        .checked_add(1)
                        .ok_or(EibfsError::ArithmeticOverflow)?,
                    PhaseDirection::Reverse => self.source_depth,
                };
                (ForestState::Source, Some((ForestSide::Source, label)), true)
            }
            (ForestSide::Source, std::cmp::Ordering::Less) => {
                let label = match self.direction {
                    PhaseDirection::Reverse => self
                        .sink_depth
                        .checked_add(1)
                        .ok_or(EibfsError::ArithmeticOverflow)?,
                    PhaseDirection::Forward => self.sink_depth,
                };
                (ForestState::Sink, Some((ForestSide::Sink, label)), true)
            }
            _ => return Err(EibfsError::ForestInvariant),
        };
        let state = &mut self.nodes[orphan.as_usize()];
        state.state = new_state;
        state.parent = None;
        state.growth_cursor = 0;
        state.current_arc = 0;
        if let Some((new_side, label)) = new_label {
            match new_side {
                ForestSide::Source => state.source_label = label,
                ForestSide::Sink => state.sink_label = label,
            }
        }
        self.metrics.tree_removals = add_u64(self.metrics.tree_removals, 1)?;
        if migrated {
            self.metrics.side_migrations = add_u64(self.metrics.side_migrations, 1)?;
        }
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: if migrated {
                    match side {
                        ForestSide::Source => "eibfs.migrate-deficit-to-sink-root",
                        ForestSide::Sink => "eibfs.migrate-excess-to-source-root",
                    }
                } else {
                    match side {
                        ForestSide::Source => "eibfs.remove-source-orphan",
                        ForestSide::Sink => "eibfs.remove-sink-orphan",
                    }
                },
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: if migrated {
                    "eibfs:remove-and-add-bad-sign-opposite-root"
                } else {
                    "eibfs:remove-balanced-orphan-as-free"
                },
            },
            orphan_queue_view(orphan, queue),
            Vec::new(),
            Some(("old-distance", to_i128(old_label)?)),
        )
    }

    fn orphan_children(
        &mut self,
        node: NodeIndex,
        side: ForestSide,
        queue: &mut VecDeque<NodeIndex>,
        queued: &mut BTreeSet<NodeIndex>,
    ) -> Result<(), EibfsError> {
        let children = self.children[node.as_usize()]
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for child in children {
            self.make_orphan(child, side, queue, queued)?;
        }
        Ok(())
    }

    fn detach_parent(&mut self, child: NodeIndex, side: ForestSide) -> Result<(), EibfsError> {
        let id = self.nodes[child.as_usize()]
            .parent
            .take()
            .ok_or(EibfsError::ForestInvariant)?;
        let arc = self.residual.arc(&id).ok_or(EibfsError::ForestInvariant)?;
        let parent = match side {
            ForestSide::Source if arc.to == child => arc.from,
            ForestSide::Sink if arc.from == child => arc.to,
            ForestSide::Source | ForestSide::Sink => {
                return Err(EibfsError::ForestInvariant);
            }
        };
        if !self.children[parent.as_usize()].remove(&child) {
            return Err(EibfsError::ForestInvariant);
        }
        Ok(())
    }

    fn push_residual(&mut self, id: &ResidualArcId, amount: u64) -> Result<(), EibfsError> {
        let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
        self.residual.augment(std::slice::from_ref(id), amount)?;
        let amount = i128::from(amount);
        let from = &mut self.excess[arc.from.as_usize()];
        *from = from
            .checked_sub(amount)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        let to = &mut self.excess[arc.to.as_usize()];
        *to = to
            .checked_add(amount)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        Ok(())
    }

    fn begin_recovery(
        &mut self,
        source_side: &[bool],
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        self.validate_stable_forest()?;
        if source_side.len() != self.nodes.len() {
            return Err(EibfsError::RecoveryInvariant);
        }
        self.recovery_mode = true;
        for (index, node) in self.nodes.iter_mut().enumerate() {
            node.parent = None;
            node.growth_cursor = 0;
            node.current_arc = 0;
            node.state = if source_side[index] {
                ForestState::Source
            } else {
                ForestState::Sink
            };
        }
        self.children = vec![BTreeSet::new(); self.nodes.len()];
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: "eibfs.begin-feasible-flow-recovery",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "eibfs:freeze-cut-and-drop-search-forests",
            },
            Vec::new(),
            Vec::new(),
            None,
        )
    }

    fn recover_feasible_flow(
        &mut self,
        source_side: &[bool],
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        if source_side.len() != self.nodes.len()
            || !source_side[self.source.as_usize()]
            || source_side[self.sink.as_usize()]
        {
            return Err(EibfsError::RecoveryInvariant);
        }
        loop {
            let excess_node = self.graph.node_indices().find(|node| {
                *node != self.source
                    && source_side[node.as_usize()]
                    && self.excess[node.as_usize()] > 0
            });
            let Some(node) = excess_node else {
                break;
            };
            let path = self.positive_flow_path(self.source, node, source_side, true)?;
            let amount = positive_cap(self.excess[node.as_usize()], path.bottleneck);
            self.cancel_positive_flow_path(&path.forward_arcs, amount)?;
            self.record_recovery(node, amount, path.reverse_arcs, recorder.as_deref_mut())?;
        }
        loop {
            let deficit_node = self.graph.node_indices().find(|node| {
                *node != self.sink
                    && !source_side[node.as_usize()]
                    && self.excess[node.as_usize()] < 0
            });
            let Some(node) = deficit_node else {
                break;
            };
            let path = self.positive_flow_path(node, self.sink, source_side, false)?;
            let amount = negative_cap(self.excess[node.as_usize()], path.bottleneck)?;
            self.cancel_positive_flow_path(&path.forward_arcs, amount)?;
            self.record_recovery(node, amount, path.reverse_arcs, recorder.as_deref_mut())?;
        }
        if self.graph.node_indices().any(|node| {
            node != self.source && node != self.sink && self.excess[node.as_usize()] != 0
        }) {
            return Err(EibfsError::RecoveryInvariant);
        }
        Ok(())
    }

    fn positive_flow_path(
        &mut self,
        start: NodeIndex,
        goal: NodeIndex,
        source_side: &[bool],
        inside_source_side: bool,
    ) -> Result<PositiveFlowPath, EibfsError> {
        let mut queue = VecDeque::from([start]);
        let mut seen = vec![false; self.nodes.len()];
        let mut parent = vec![None; self.nodes.len()];
        seen[start.as_usize()] = true;
        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            for &edge_index in self.graph.outgoing_edges(node) {
                self.count_scan(ScanKind::Recovery)?;
                let edge = self
                    .graph
                    .edge(edge_index)
                    .ok_or(EibfsError::RecoveryInvariant)?;
                if edge.from() == edge.to()
                    || self.residual.flows()[edge_index.as_usize()] == 0
                    || source_side[edge.to().as_usize()] != inside_source_side
                    || seen[edge.to().as_usize()]
                {
                    continue;
                }
                seen[edge.to().as_usize()] = true;
                parent[edge.to().as_usize()] = Some(edge_index);
                queue.push_back(edge.to());
            }
        }
        if !seen[goal.as_usize()] {
            return Err(EibfsError::RecoveryInvariant);
        }
        let mut forward_arcs = Vec::new();
        let mut cursor = goal;
        let mut bottleneck = u64::MAX;
        while cursor != start {
            let edge_index = parent[cursor.as_usize()].ok_or(EibfsError::RecoveryInvariant)?;
            let edge = self
                .graph
                .edge(edge_index)
                .ok_or(EibfsError::RecoveryInvariant)?;
            bottleneck = bottleneck.min(self.residual.flows()[edge_index.as_usize()]);
            forward_arcs.push(ResidualArcId::new(
                edge.id().clone(),
                ResidualDirection::Forward,
            ));
            cursor = edge.from();
        }
        forward_arcs.reverse();
        let reverse_arcs = forward_arcs.iter().rev().map(reverse_residual_id).collect();
        Ok(PositiveFlowPath {
            forward_arcs,
            reverse_arcs,
            bottleneck,
        })
    }

    fn cancel_positive_flow_path(
        &mut self,
        forward: &[ResidualArcId],
        amount: u64,
    ) -> Result<(), EibfsError> {
        for id in forward.iter().rev() {
            let reverse = reverse_residual_id(id);
            self.push_residual(&reverse, amount)?;
        }
        Ok(())
    }

    fn record_recovery(
        &mut self,
        node: NodeIndex,
        amount: u64,
        path: Vec<ResidualArcId>,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), EibfsError> {
        self.metrics.recovery_cancellations = add_u64(self.metrics.recovery_cancellations, 1)?;
        self.metrics.recovered_units = self
            .metrics
            .recovered_units
            .checked_add(u128::from(amount))
            .ok_or(EibfsError::ArithmeticOverflow)?;
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: "eibfs.cancel-same-cut-positive-flow",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "eibfs:recover-conservation-without-changing-cut",
            },
            vec![node],
            path,
            Some(("amount", i128::from(amount))),
        )
    }

    fn validate_stable_forest(&self) -> Result<(), EibfsError> {
        if self.recovery_mode {
            return Ok(());
        }
        self.validate_terminal_roots()?;
        let mut expected_children = vec![BTreeSet::new(); self.nodes.len()];
        for node in self.graph.node_indices() {
            self.validate_stable_node(node, &mut expected_children)?;
        }
        if expected_children != self.children {
            return Err(EibfsError::ForestInvariant);
        }
        self.validate_label_inequalities()
    }

    fn validate_terminal_roots(&self) -> Result<(), EibfsError> {
        if self.nodes.iter().any(|node| node.state.orphan().is_some())
            || self.nodes[self.source.as_usize()].state != ForestState::Source
            || self.nodes[self.source.as_usize()].parent.is_some()
            || self.nodes[self.source.as_usize()].source_label != 0
            || self.nodes[self.sink.as_usize()].state != ForestState::Sink
            || self.nodes[self.sink.as_usize()].parent.is_some()
            || self.nodes[self.sink.as_usize()].sink_label != 0
        {
            return Err(EibfsError::ForestInvariant);
        }
        Ok(())
    }

    fn validate_stable_node(
        &self,
        node: NodeIndex,
        expected_children: &mut [BTreeSet<NodeIndex>],
    ) -> Result<(), EibfsError> {
        let state = &self.nodes[node.as_usize()];
        let Some(side) = state.state.normal() else {
            return self.validate_free_node(node);
        };
        self.validate_stable_cursors(node, side)?;
        self.validate_current_arc_prefix(node, side)?;
        let wrong_sign = match side {
            ForestSide::Source => node != self.source && self.excess[node.as_usize()] < 0,
            ForestSide::Sink => node != self.sink && self.excess[node.as_usize()] > 0,
        };
        if wrong_sign {
            return Err(EibfsError::ForestInvariant);
        }
        let Some(id) = state.parent.as_ref() else {
            let valid_root = match side {
                ForestSide::Source => node == self.source || self.excess[node.as_usize()] > 0,
                ForestSide::Sink => node == self.sink || self.excess[node.as_usize()] < 0,
            };
            return valid_root.then_some(()).ok_or(EibfsError::ForestInvariant);
        };
        if node != self.source && node != self.sink && self.excess[node.as_usize()] != 0 {
            return Err(EibfsError::ForestInvariant);
        }
        let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
        if arc.capacity == 0 {
            return Err(EibfsError::ForestInvariant);
        }
        let parent = match side {
            ForestSide::Source if arc.to == node => arc.from,
            ForestSide::Sink if arc.from == node => arc.to,
            ForestSide::Source | ForestSide::Sink => return Err(EibfsError::ForestInvariant),
        };
        if parent == node
            || self.nodes[parent.as_usize()].state.normal() != Some(side)
            || self.label(parent, side).checked_add(1) != Some(self.label(node, side))
        {
            return Err(EibfsError::ForestInvariant);
        }
        expected_children[parent.as_usize()].insert(node);
        Ok(())
    }

    fn validate_free_node(&self, node: NodeIndex) -> Result<(), EibfsError> {
        let state = &self.nodes[node.as_usize()];
        if state.parent.is_some()
            || state.current_arc != 0
            || state.growth_cursor != 0
            || self.excess[node.as_usize()] != 0
        {
            return Err(EibfsError::ForestInvariant);
        }
        Ok(())
    }

    fn validate_stable_cursors(&self, node: NodeIndex, side: ForestSide) -> Result<(), EibfsError> {
        let state = &self.nodes[node.as_usize()];
        let growth_limit = match side {
            ForestSide::Source => self.outgoing[node.as_usize()].len(),
            ForestSide::Sink => self.incoming[node.as_usize()].len(),
        };
        let label_limit = self
            .nodes
            .len()
            .checked_mul(2)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        if state.current_arc > self.parent_adjacency(node, side).len()
            || state.growth_cursor > growth_limit
            || self.label(node, side) >= label_limit
        {
            return Err(EibfsError::ForestInvariant);
        }
        Ok(())
    }

    fn validate_current_arc_prefix(
        &self,
        node: NodeIndex,
        side: ForestSide,
    ) -> Result<(), EibfsError> {
        let state = &self.nodes[node.as_usize()];
        for id in self
            .parent_adjacency(node, side)
            .iter()
            .take(state.current_arc)
        {
            let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
            if arc.capacity == 0 {
                continue;
            }
            let parent = match side {
                ForestSide::Source if arc.to == node => arc.from,
                ForestSide::Sink if arc.from == node => arc.to,
                ForestSide::Source | ForestSide::Sink => {
                    return Err(EibfsError::ForestInvariant);
                }
            };
            if parent != node
                && self.nodes[parent.as_usize()].state.normal() == Some(side)
                && self.label(parent, side).checked_add(1) == Some(self.label(node, side))
            {
                return Err(EibfsError::ForestInvariant);
            }
        }
        Ok(())
    }

    fn validate_label_inequalities(&self) -> Result<(), EibfsError> {
        for node in self.graph.node_indices() {
            for id in &self.outgoing[node.as_usize()] {
                let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
                if arc.capacity == 0 || arc.from == arc.to {
                    continue;
                }
                let from_side = self.nodes[arc.from.as_usize()].state.normal();
                let to_side = self.nodes[arc.to.as_usize()].state.normal();
                if from_side == Some(ForestSide::Source)
                    && to_side == Some(ForestSide::Source)
                    && self.label(arc.to, ForestSide::Source)
                        > self.label(arc.from, ForestSide::Source) + 1
                {
                    return Err(EibfsError::ForestInvariant);
                }
                if from_side == Some(ForestSide::Sink)
                    && to_side == Some(ForestSide::Sink)
                    && self.label(arc.from, ForestSide::Sink)
                        > self.label(arc.to, ForestSide::Sink) + 1
                {
                    return Err(EibfsError::ForestInvariant);
                }
            }
        }
        Ok(())
    }

    fn label(&self, node: NodeIndex, side: ForestSide) -> usize {
        match side {
            ForestSide::Source => self.nodes[node.as_usize()].source_label,
            ForestSide::Sink => self.nodes[node.as_usize()].sink_label,
        }
    }

    fn parent_adjacency(&self, node: NodeIndex, side: ForestSide) -> &[ResidualArcId] {
        match side {
            ForestSide::Source => &self.incoming[node.as_usize()],
            ForestSide::Sink => &self.outgoing[node.as_usize()],
        }
    }

    fn frontier(&self, side: ForestSide, label: usize) -> Vec<NodeIndex> {
        self.graph
            .node_indices()
            .filter(|node| {
                self.nodes[node.as_usize()].state.normal() == Some(side)
                    && self.label(*node, side) == label
            })
            .collect()
    }

    fn labels(&self) -> Result<Vec<Option<i128>>, EibfsError> {
        self.nodes
            .iter()
            .map(|node| match node.state {
                ForestState::Source | ForestState::SourceOrphan => {
                    to_i128(node.source_label).map(Some)
                }
                ForestState::Sink | ForestState::SinkOrphan => to_i128(node.sink_label)
                    .and_then(|label| label.checked_add(1).ok_or(EibfsError::ArithmeticOverflow))
                    .and_then(|label| label.checked_neg().ok_or(EibfsError::ArithmeticOverflow))
                    .map(Some),
                ForestState::Free => Ok(None),
            })
            .collect()
    }

    fn eibfs_overlay(&self) -> Result<Option<EibfsTraceOverlay>, EibfsError> {
        if self.recovery_mode {
            return Ok(None);
        }
        let nodes = self
            .graph
            .node_indices()
            .map(|node| {
                let state = &self.nodes[node.as_usize()];
                let membership = match state.state {
                    ForestState::Free => EibfsTraceMembership::Free,
                    ForestState::Source | ForestState::SourceOrphan => EibfsTraceMembership::Source,
                    ForestState::Sink | ForestState::SinkOrphan => EibfsTraceMembership::Sink,
                };
                let root_kind = if state.parent.is_some() || state.state.orphan().is_some() {
                    EibfsTraceRootKind::None
                } else if node == self.source {
                    EibfsTraceRootKind::Source
                } else if node == self.sink {
                    EibfsTraceRootKind::Sink
                } else if membership == EibfsTraceMembership::Source
                    && self.excess[node.as_usize()] > 0
                {
                    EibfsTraceRootKind::Excess
                } else if membership == EibfsTraceMembership::Sink
                    && self.excess[node.as_usize()] < 0
                {
                    EibfsTraceRootKind::Deficit
                } else {
                    EibfsTraceRootKind::None
                };
                Ok(EibfsTraceNodeState {
                    node: self
                        .graph
                        .node(node)
                        .ok_or(EibfsError::ForestInvariant)?
                        .id()
                        .clone(),
                    source_label: state.source_label,
                    sink_label: state.sink_label,
                    membership,
                    root_kind,
                    orphan: state.state.orphan().is_some(),
                    imbalance: self.excess[node.as_usize()],
                })
            })
            .collect::<Result<Vec<_>, EibfsError>>()?;
        let mut forest_arcs = Vec::new();
        for child in self.graph.node_indices() {
            let state = &self.nodes[child.as_usize()];
            let Some(side) = state.state.normal() else {
                continue;
            };
            let Some(id) = state.parent.as_ref() else {
                continue;
            };
            let arc = self.residual.arc(id).ok_or(EibfsError::ForestInvariant)?;
            let parent = match side {
                ForestSide::Source if arc.to == child => arc.from,
                ForestSide::Sink if arc.from == child => arc.to,
                ForestSide::Source | ForestSide::Sink => {
                    return Err(EibfsError::ForestInvariant);
                }
            };
            forest_arcs.push(EibfsTraceForestArc {
                parent: self
                    .graph
                    .node(parent)
                    .ok_or(EibfsError::ForestInvariant)?
                    .id()
                    .clone(),
                child: self
                    .graph
                    .node(child)
                    .ok_or(EibfsError::ForestInvariant)?
                    .id()
                    .clone(),
                side: match side {
                    ForestSide::Source => EibfsTraceMembership::Source,
                    ForestSide::Sink => EibfsTraceMembership::Sink,
                },
                admissible_residual: id.clone(),
            });
        }
        forest_arcs.sort_unstable_by(|left, right| {
            (&left.child, &left.parent, &left.admissible_residual).cmp(&(
                &right.child,
                &right.parent,
                &right.admissible_residual,
            ))
        });
        Ok(Some(EibfsTraceOverlay {
            phase_direction: match self.direction {
                PhaseDirection::Forward => EibfsTracePhaseDirection::Forward,
                PhaseDirection::Reverse => EibfsTracePhaseDirection::Reverse,
            },
            source_depth: self.source_depth,
            sink_depth: self.sink_depth,
            nodes,
            forest_arcs,
        }))
    }

    fn record(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        metadata: FlowTraceEventMetadata,
        search_order: Vec<NodeIndex>,
        active_path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), EibfsError> {
        let Some(recorder) = recorder else {
            return Ok(());
        };
        check_eibfs_trace_projection_budget(self.graph, recorder.event_count() + 1)?;
        let mut focus = search_order
            .iter()
            .map(|index| {
                self.graph
                    .node(*index)
                    .map(|node| FlowTraceEntityRef::Node(node.id().clone()))
                    .ok_or(EibfsError::ForestInvariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        focus.extend(
            active_path
                .iter()
                .cloned()
                .map(FlowTraceEntityRef::ResidualArc),
        );
        let mut snapshot = FlowTraceSnapshot::capture(
            self.graph,
            &self.residual,
            self.labels()?,
            search_order,
            active_path,
            self.excess.clone(),
            trace_metrics(&self.metrics),
        );
        if let Some(overlay) = self.eibfs_overlay()? {
            snapshot = snapshot.with_eibfs_overlay(overlay);
        }
        if let Some(overlay) = self.dynamic_overlay.clone() {
            snapshot = snapshot.with_dynamic_eibfs_overlay(overlay);
        }
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
        Ok(())
    }

    fn count_scan(&mut self, kind: ScanKind) -> Result<(), EibfsError> {
        if self
            .metrics
            .residual_arc_scans
            .checked_sub(self.work_scan_start)
            .ok_or(EibfsError::ArithmeticOverflow)?
            >= EIBFS_MAX_RESIDUAL_ARC_SCANS
        {
            return Err(EibfsError::WorkLimit);
        }
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        let counter = match kind {
            ScanKind::Growth => &mut self.metrics.growth_arc_scans,
            ScanKind::Adoption => &mut self.metrics.adoption_arc_scans,
            ScanKind::Recovery => &mut self.metrics.recovery_arc_scans,
            ScanKind::DynamicRepair => &mut self.metrics.dynamic_repair_arc_scans,
        };
        *counter = counter
            .checked_add(1)
            .ok_or(EibfsError::ArithmeticOverflow)?;
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), EibfsError> {
        if self
            .metrics
            .state_transitions
            .checked_sub(self.work_transition_start)
            .ok_or(EibfsError::ArithmeticOverflow)?
            >= EIBFS_MAX_STATE_TRANSITIONS
        {
            return Err(EibfsError::WorkLimit);
        }
        self.metrics.state_transitions = add_u64(self.metrics.state_transitions, 1)?;
        Ok(())
    }

    fn begin_work_epoch(&mut self) {
        self.work_scan_start = self.metrics.residual_arc_scans;
        self.work_transition_start = self.metrics.state_transitions;
        self.work_bridge_start = self.metrics.bridge_pushes;
    }
}

fn check_eibfs_trace_projection_budget(
    graph: &FlowNetwork,
    next_event_count: usize,
) -> Result<(), EibfsError> {
    let overlay_units_per_boundary = graph
        .nodes()
        .len()
        .checked_mul(4)
        .ok_or(EibfsError::ArithmeticOverflow)?;
    let projected_boundaries = next_event_count
        .checked_add(1)
        .ok_or(EibfsError::ArithmeticOverflow)?;
    let projected_units = overlay_units_per_boundary
        .checked_mul(projected_boundaries)
        .ok_or(EibfsError::ArithmeticOverflow)?;
    if projected_units > EIBFS_MAX_TRACE_PROJECTION_UNITS {
        return Err(EibfsError::Trace(FlowTraceError::EventLimit));
    }
    Ok(())
}

struct PositiveFlowPath {
    forward_arcs: Vec<ResidualArcId>,
    reverse_arcs: Vec<ResidualArcId>,
    bottleneck: u64,
}

#[derive(Clone, Copy)]
enum BridgeCase {
    TerminalTerminal,
    SourceRoot,
    SinkRoot,
    NonterminalRoots,
}

impl BridgeCase {
    const fn catalog_id(self) -> &'static str {
        match self {
            Self::TerminalTerminal => "eibfs.push-bridge-terminal-terminal",
            Self::SourceRoot => "eibfs.push-bridge-source-root",
            Self::SinkRoot => "eibfs.push-bridge-sink-root",
            Self::NonterminalRoots => "eibfs.push-bridge-nonterminal-roots",
        }
    }
}

#[derive(Clone, Copy)]
enum ScanKind {
    Growth,
    Adoption,
    Recovery,
    DynamicRepair,
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

fn positive_cap(value: i128, cap: u64) -> u64 {
    if value <= 0 {
        return 0;
    }
    u64::try_from(value).unwrap_or(u64::MAX).min(cap)
}

fn negative_cap(value: i128, cap: u64) -> Result<u64, EibfsError> {
    let magnitude = value.checked_neg().ok_or(EibfsError::ArithmeticOverflow)?;
    Ok(positive_cap(magnitude, cap))
}

fn add_u64(value: u64, amount: u64) -> Result<u64, EibfsError> {
    value
        .checked_add(amount)
        .ok_or(EibfsError::ArithmeticOverflow)
}

fn to_i128(value: usize) -> Result<i128, EibfsError> {
    i128::try_from(value).map_err(|_| EibfsError::ArithmeticOverflow)
}

const fn trace_metrics(metrics: &EibfsMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.phases as u128,
        relaxation_passes: metrics.forward_phases as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.bridge_pushes as u128,
        path_searches: metrics.tree_path_pushes as u128,
        scaling_phases: metrics.reverse_phases as u128,
        blocking_flow_phases: metrics.tree_attachments as u128,
        relabels: metrics.orphan_relabels as u128,
        retreats: metrics.tree_removals as u128,
        reverse_bfs_runs: metrics.adoption_arc_scans,
        gap_terminations: metrics.orphan_creations as u128,
        pushes: metrics.orphan_visits as u128,
        saturating_pushes: metrics.saturated_tree_arcs as u128,
        nonsaturating_pushes: metrics.side_migrations as u128,
        discharges: metrics.recovery_cancellations as u128,
        active_vertex_selections: metrics.state_transitions as u128,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    type BridgeCase = (u64, fn(&EibfsMetrics) -> u64, &'static str);

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

    fn deterministic_multigraph(seed: u64) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "n1", "n2", "n3", "n4", "n5", "t"];
        let materialized = (0_u64..36)
            .map(|ordinal| {
                let mixed = seed
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(ordinal.wrapping_mul(0xBF58_476D_1CE4_E5B9));
                let from =
                    nodes[usize::try_from(mixed % nodes.len() as u64).expect("bounded node index")];
                let to = nodes[usize::try_from((mixed >> 11) % nodes.len() as u64)
                    .expect("bounded node index")];
                (
                    format!("e{ordinal:03}"),
                    from.to_owned(),
                    to.to_owned(),
                    (mixed >> 23) % 11,
                )
            })
            .collect::<Vec<_>>();
        let refs = materialized
            .iter()
            .map(|(id, from, to, capacity)| (id.as_str(), from.as_str(), to.as_str(), *capacity))
            .collect::<Vec<_>>();
        graph(&nodes, &refs)
    }

    fn admission_graph(
        node_count: usize,
        edge_count: usize,
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        assert!(node_count >= 2);
        let node_ids = std::iter::once("s".to_owned())
            .chain((1..node_count - 1).map(|index| format!("n{index:04}")))
            .chain(std::iter::once("t".to_owned()))
            .collect::<Vec<_>>();
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect(),
            (0..edge_count)
                .map(|index| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("e{index:05}")).expect("edge id"),
                    from: NodeId::parse("s").expect("source"),
                    to: NodeId::parse("t").expect("sink"),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                })
                .collect(),
        )
        .expect("admission graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    fn bridge_amount_from_snapshot(
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        before: &FlowTraceSnapshot,
        after: &FlowTraceSnapshot,
    ) -> (&'static str, u64) {
        let overlay = before.eibfs_overlay.as_ref().expect("pre-bridge overlay");
        let bridge = after.active_path.as_slice();
        assert_eq!(bridge.len(), 1, "one connecting residual arc");
        let bridge = &bridge[0];
        let edge = graph
            .edge(
                graph
                    .edge_index(bridge.original_edge())
                    .expect("bridge edge index"),
            )
            .expect("bridge edge");
        let (source_endpoint, sink_endpoint) = match bridge.direction() {
            ResidualDirection::Forward => (edge.from(), edge.to()),
            ResidualDirection::Reverse => (edge.to(), edge.from()),
        };
        let node_id = |node: NodeIndex| graph.node(node).expect("bridge endpoint").id().clone();
        let residual_capacity = |id: &ResidualArcId| {
            before
                .residual_capacities
                .iter()
                .find_map(|(candidate, capacity)| (candidate == id).then_some(*capacity))
                .expect("residual capacity")
        };
        let root_and_bottleneck = |start: NodeIndex, side: EibfsTraceMembership| -> (NodeId, u64) {
            let mut cursor = node_id(start);
            let mut bottleneck = u64::MAX;
            let mut steps = 0;
            while let Some(relation) = overlay
                .forest_arcs
                .iter()
                .find(|relation| relation.child == cursor && relation.side == side)
            {
                bottleneck = bottleneck.min(residual_capacity(&relation.admissible_residual));
                cursor = relation.parent.clone();
                steps += 1;
                assert!(steps <= graph.nodes().len(), "forest cycle");
            }
            (cursor, bottleneck)
        };
        let (source_root, source_bottleneck) =
            root_and_bottleneck(source_endpoint, EibfsTraceMembership::Source);
        let (sink_root, sink_bottleneck) =
            root_and_bottleneck(sink_endpoint, EibfsTraceMembership::Sink);
        let source_terminal = node_id(source);
        let sink_terminal = node_id(sink);
        let bridge_capacity = residual_capacity(bridge);
        let root_imbalance = |root: &NodeId| {
            overlay
                .nodes
                .iter()
                .find_map(|node| (&node.node == root).then_some(node.imbalance))
                .expect("root imbalance")
        };
        match (source_root == source_terminal, sink_root == sink_terminal) {
            (true, true) => ("eibfs.push-bridge-terminal-terminal", bridge_capacity),
            (true, false) => (
                "eibfs.push-bridge-source-root",
                bridge_capacity.min(source_bottleneck),
            ),
            (false, true) => (
                "eibfs.push-bridge-sink-root",
                bridge_capacity.min(sink_bottleneck),
            ),
            (false, false) => {
                let source_excess = u64::try_from(root_imbalance(&source_root))
                    .expect("positive source root excess");
                let sink_deficit = u64::try_from(
                    root_imbalance(&sink_root)
                        .checked_neg()
                        .expect("finite sink deficit"),
                )
                .expect("negative sink root deficit");
                (
                    "eibfs.push-bridge-nonterminal-roots",
                    bridge_capacity
                        .min(source_bottleneck)
                        .min(sink_bottleneck)
                        .min(source_excess)
                        .min(sink_deficit),
                )
            }
        }
    }

    fn assert_metric_accounting(metrics: &EibfsMetrics) {
        assert_eq!(
            metrics.phases,
            metrics.forward_phases + metrics.reverse_phases
        );
        assert_eq!(
            metrics.residual_arc_scans,
            metrics.growth_arc_scans
                + metrics.adoption_arc_scans
                + metrics.recovery_arc_scans
                + metrics.dynamic_repair_arc_scans
        );
        assert_eq!(
            metrics.bridge_pushes,
            metrics.terminal_terminal_bridges
                + metrics.source_root_bridges
                + metrics.sink_root_bridges
                + metrics.nonterminal_root_bridges
        );
    }

    #[test]
    fn recovers_an_overpushed_terminal_root_case_to_a_feasible_flow() {
        let (graph, source, sink) = graph(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 2),
                ("ab", "a", "b", 10),
                ("bt", "b", "t", 10),
            ],
        );
        let result = trace_eibfs(&graph, source, sink).expect("EIBFS");
        assert_eq!(result.result.certificate.value, 2);
        assert!(result.result.metrics.side_migrations > 0);
        assert!(result.result.metrics.recovery_cancellations > 0);
        assert_eq!(result.result.metrics.recovered_units, 8);
        assert!(
            result
                .events
                .iter()
                .any(|event| { event.catalog_id == "eibfs.cancel-same-cut-positive-flow" })
        );
        let source_side = result
            .result
            .certificate
            .source_side
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut snapshot = result.base_snapshot.clone();
        let mut recovered = 0_u128;
        for event in &result.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("recovery replay");
            if event.catalog_id != "eibfs.cancel-same-cut-positive-flow" {
                continue;
            }
            let detail = event.detail.as_ref().expect("recovery amount");
            assert_eq!(detail.label, "amount");
            recovered += u128::try_from(detail.value).expect("positive recovery amount");
            for id in &snapshot.active_path {
                let edge = graph
                    .edge(graph.edge_index(id.original_edge()).expect("recovery edge"))
                    .expect("recovery edge declaration");
                let from = graph.node(edge.from()).expect("tail").id();
                let to = graph.node(edge.to()).expect("head").id();
                assert_eq!(
                    source_side.contains(from),
                    source_side.contains(to),
                    "recovery must remain inside one frozen-cut side"
                );
            }
        }
        assert_eq!(recovered, 8);
        assert_metric_accounting(&result.result.metrics);
    }

    #[test]
    fn traces_a_drain_that_invalidates_a_deeper_nonterminal_root() {
        let (graph, source, sink) = graph(
            &["s", "a", "c", "d", "q", "t"],
            &[
                ("sa", "s", "a", 2),
                ("aq", "a", "q", 10),
                ("qt", "q", "t", 10),
                ("sc", "s", "c", 10),
                ("cd", "c", "d", 10),
                ("dq", "d", "q", 10),
            ],
        );
        let fast = solve_eibfs(&graph, source, sink).expect("fast EIBFS");
        let traced = trace_eibfs(&graph, source, sink).expect("traced EIBFS");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(traced.result, fast);
        assert_eq!(traced.result.certificate.value, oracle.certificate.value);
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "eibfs.drain-sink-excess"
                && event
                    .patches
                    .iter()
                    .any(|patch| matches!(patch, crate::trace::FlowTracePatch::EibfsOverlay { after: Some(overlay), .. } if overlay.nodes.iter().any(|node| node.node.as_str() == "a" && node.orphan)))
        }));
    }

    #[test]
    fn solves_parallel_opposite_zero_capacity_and_self_loop_edges() {
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
        let eibfs = solve_eibfs(&graph, source, sink).expect("EIBFS");
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        assert_eq!(eibfs.certificate.value, 4);
        assert_eq!(eibfs.certificate.value, oracle.certificate.value);
    }

    #[test]
    fn accumulates_parallel_u64_max_capacities_in_checked_wide_arithmetic() {
        let (graph, source, sink) = graph(
            &["s", "t"],
            &[("e0", "s", "t", u64::MAX), ("e1", "s", "t", u64::MAX)],
        );
        let result = solve_eibfs(&graph, source, sink).expect("wide EIBFS value");
        assert_eq!(result.certificate.value, i128::from(u64::MAX) * 2);
        assert_eq!(result.flows, [u64::MAX, u64::MAX]);
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
        let fast = solve_eibfs(&graph, source, sink).expect("fast");
        let traced = trace_eibfs(&graph, source, sink).expect("trace");
        let repeated = trace_eibfs(&graph, source, sink).expect("repeated trace");
        assert_eq!(traced.result, fast);
        assert_eq!(traced, repeated);
        let phases = traced
            .events
            .iter()
            .filter(|event| {
                event.catalog_id == "eibfs.start-forward-phase"
                    || event.catalog_id == "eibfs.start-reverse-phase"
            })
            .map(|event| event.catalog_id.as_str())
            .collect::<Vec<_>>();
        assert!(phases.len() >= 2);
        for (ordinal, phase) in phases.iter().enumerate() {
            assert_eq!(
                *phase,
                if ordinal % 2 == 0 {
                    "eibfs.start-forward-phase"
                } else {
                    "eibfs.start-reverse-phase"
                }
            );
        }
        assert_metric_accounting(&traced.result.metrics);
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
    fn rejects_a_current_arc_outside_the_stable_parent_adjacency() {
        let (graph, source, sink) = graph(
            &["s", "a", "t"],
            &[("sa", "s", "a", 1), ("at", "a", "t", 1)],
        );
        let residual = ResidualState::at_lower_bounds(&graph);
        let mut engine = EibfsEngine::new(&graph, source, sink, residual).expect("engine");
        let parent = ResidualArcId::new(
            EdgeId::parse("sa").expect("edge id"),
            ResidualDirection::Forward,
        );
        let a = graph
            .node_index(&NodeId::parse("a").expect("node id"))
            .expect("a");
        engine.attach_free(source, a, parent).expect("attach a");
        engine.validate_stable_forest().expect("valid current arc");
        engine.nodes[a.as_usize()].current_arc = engine.incoming[a.as_usize()].len() + 1;
        assert_eq!(
            engine.validate_stable_forest(),
            Err(EibfsError::ForestInvariant)
        );
    }

    #[test]
    fn eager_trace_projection_budget_is_checked_before_snapshot_cloning() {
        let (graph, _, _) = deterministic_multigraph(0);
        let units_per_boundary = graph.nodes().len() * 4;
        let admitted_events = EIBFS_MAX_TRACE_PROJECTION_UNITS / units_per_boundary - 1;
        check_eibfs_trace_projection_budget(&graph, admitted_events).expect("exact budget");
        assert_eq!(
            check_eibfs_trace_projection_budget(&graph, admitted_events + 1),
            Err(EibfsError::Trace(FlowTraceError::EventLimit))
        );
    }

    #[test]
    fn admission_limits_are_exact_at_both_node_and_edge_boundaries() {
        let (nodes_at_limit, source, sink) = admission_graph(EIBFS_MAX_NODES, 0);
        validate_eibfs_graph(&nodes_at_limit, source, sink).expect("node limit admitted");
        let (too_many_nodes, source, sink) = admission_graph(EIBFS_MAX_NODES + 1, 0);
        assert_eq!(
            validate_eibfs_graph(&too_many_nodes, source, sink),
            Err(EibfsError::AdmissionLimit)
        );

        let (edges_at_limit, source, sink) = admission_graph(2, EIBFS_MAX_EDGES);
        validate_eibfs_graph(&edges_at_limit, source, sink).expect("edge limit admitted");
        let (too_many_edges, source, sink) = admission_graph(2, EIBFS_MAX_EDGES + 1);
        assert_eq!(
            validate_eibfs_graph(&too_many_edges, source, sink),
            Err(EibfsError::AdmissionLimit)
        );
        assert_eq!(
            validate_eibfs_graph(&edges_at_limit, source, source),
            Err(EibfsError::GraphRequirement("distinct source and sink"))
        );
    }

    #[test]
    fn rejects_nonzero_supply_and_lower_bound_contracts() {
        let supply_graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("source"), 1),
                FlowNode::new(NodeId::parse("t").expect("sink"), -1),
            ],
            Vec::new(),
        )
        .expect("supply graph");
        let source = supply_graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = supply_graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        assert_eq!(
            validate_eibfs_graph(&supply_graph, source, sink),
            Err(EibfsError::GraphRequirement("zero supplies"))
        );

        let lower_graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("source"), 0),
                FlowNode::new(NodeId::parse("t").expect("sink"), 0),
            ],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("edge"),
                from: NodeId::parse("s").expect("source"),
                to: NodeId::parse("t").expect("sink"),
                lower: 1,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("lower graph");
        let source = lower_graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = lower_graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        assert_eq!(
            validate_eibfs_graph(&lower_graph, source, sink),
            Err(EibfsError::GraphRequirement("zero lower bounds"))
        );
    }

    #[test]
    fn fixed_fixtures_exercise_all_four_bridge_amount_cases() {
        let cases: [BridgeCase; 4] = [
            (
                1,
                |metrics: &EibfsMetrics| metrics.terminal_terminal_bridges,
                "eibfs.push-bridge-terminal-terminal",
            ),
            (
                1,
                |metrics: &EibfsMetrics| metrics.source_root_bridges,
                "eibfs.push-bridge-source-root",
            ),
            (
                53,
                |metrics: &EibfsMetrics| metrics.sink_root_bridges,
                "eibfs.push-bridge-sink-root",
            ),
            (
                10,
                |metrics: &EibfsMetrics| metrics.nonterminal_root_bridges,
                "eibfs.push-bridge-nonterminal-roots",
            ),
        ];
        for (seed, metric, event_id) in cases {
            let (graph, source, sink) = deterministic_multigraph(seed);
            let traced = trace_eibfs(&graph, source, sink).expect("EIBFS trace");
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                traced.result.certificate.value, oracle.certificate.value,
                "seed {seed}"
            );
            assert!(
                metric(&traced.result.metrics) > 0,
                "seed {seed}: {event_id}"
            );
            assert!(
                traced
                    .events
                    .iter()
                    .any(|event| event.catalog_id == event_id),
                "seed {seed}: {event_id}"
            );
            let mut snapshot = traced.base_snapshot.clone();
            let mut exact_amount_checked = false;
            for event in &traced.events {
                let before = snapshot.clone();
                apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                    .expect("bridge replay");
                if event.catalog_id != event_id {
                    continue;
                }
                let (expected_case, expected_amount) =
                    bridge_amount_from_snapshot(&graph, source, sink, &before, &snapshot);
                assert_eq!(expected_case, event_id, "seed {seed}");
                let detail = event.detail.as_ref().expect("bridge amount detail");
                assert_eq!(detail.label, "amount");
                assert_eq!(detail.value, i128::from(expected_amount), "seed {seed}");
                exact_amount_checked = true;
                break;
            }
            assert!(exact_amount_checked, "seed {seed}: {event_id}");
        }
    }

    #[test]
    fn agrees_with_edmonds_karp_on_bounded_deterministic_graphs() {
        for seed in 0_u64..128 {
            let node_ids = ["s", "a", "b", "c", "d", "t"];
            let mut edges = Vec::new();
            let mut ordinal = 0_u64;
            for (from_index, from) in node_ids.iter().enumerate() {
                for (to_index, to) in node_ids.iter().enumerate() {
                    let mixed = seed
                        .wrapping_mul(0x9E37_79B9)
                        .wrapping_add((from_index as u64) * 17)
                        .wrapping_add((to_index as u64) * 31);
                    if mixed % 4 == 0 {
                        let id = format!("e{ordinal:02}");
                        ordinal += 1;
                        edges.push((id, (*from).to_owned(), (*to).to_owned(), mixed % 9));
                    }
                }
            }
            let refs = edges
                .iter()
                .map(|(id, from, to, capacity)| {
                    (id.as_str(), from.as_str(), to.as_str(), *capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&node_ids, &refs);
            let eibfs = solve_eibfs(&graph, source, sink)
                .unwrap_or_else(|error| panic!("EIBFS seed {seed}: {error:?}"));
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                eibfs.certificate.value, oracle.certificate.value,
                "seed {seed}"
            );
            assert_metric_accounting(&eibfs.metrics);
        }
    }

    #[test]
    fn agrees_with_edmonds_karp_on_every_four_node_unit_digraph() {
        let nodes = ["s", "a", "b", "t"];
        let candidates = nodes
            .iter()
            .enumerate()
            .flat_map(|(from_index, from)| {
                nodes
                    .iter()
                    .enumerate()
                    .filter(move |(to_index, _)| *to_index != from_index)
                    .map(move |(_, to)| (*from, *to))
            })
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 12);
        for mask in 0_u16..(1_u16 << candidates.len()) {
            let materialized = candidates
                .iter()
                .enumerate()
                .filter(|(ordinal, _)| mask & (1_u16 << ordinal) != 0)
                .map(|(ordinal, (from, to))| {
                    (
                        format!("e{ordinal:02}"),
                        (*from).to_owned(),
                        (*to).to_owned(),
                        1,
                    )
                })
                .collect::<Vec<_>>();
            let refs = materialized
                .iter()
                .map(|(id, from, to, capacity)| {
                    (id.as_str(), from.as_str(), to.as_str(), *capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = graph(&nodes, &refs);
            let eibfs = solve_eibfs(&graph, source, sink)
                .unwrap_or_else(|error| panic!("EIBFS mask {mask:#05x}: {error:?}"));
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                eibfs.certificate.value, oracle.certificate.value,
                "mask {mask:#05x}"
            );
            assert_metric_accounting(&eibfs.metrics);
        }
    }

    #[test]
    fn agrees_with_edmonds_karp_on_deterministic_multigraphs() {
        for seed in 0_u64..1_000 {
            let (graph, source, sink) = deterministic_multigraph(seed);
            let eibfs = solve_eibfs(&graph, source, sink)
                .unwrap_or_else(|error| panic!("EIBFS seed {seed}: {error:?}"));
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            assert_eq!(
                eibfs.certificate.value, oracle.certificate.value,
                "seed {seed}"
            );
            assert_metric_accounting(&eibfs.metrics);
        }
    }
}
