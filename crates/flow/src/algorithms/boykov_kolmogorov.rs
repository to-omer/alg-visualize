//! Boykov–Kolmogorov two-tree augmenting-path maximum flow.
//!
//! This implementation follows Sections 3.1–3.3 of Boykov and Kolmogorov
//! (IEEE TPAMI 26(9), 2004): source and sink search trees are retained across
//! augmentations, saturated parent arcs create orphans, and an adoption stage
//! restores both trees before growth resumes. Stable residual identities and
//! FIFO queues make the paper's deliberately open tie choices reproducible.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot, apply_trace_event,
};

/// Conservative node band for a complete two-tree trace.
pub const BOYKOV_KOLMOGOROV_MAX_NODES: usize = 256;
/// Conservative edge band for a complete two-tree trace.
pub const BOYKOV_KOLMOGOROV_MAX_EDGES: usize = 2_048;
/// Hard ceiling for positive residual-arc inspections.
pub const BOYKOV_KOLMOGOROV_MAX_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for tree mutations, augmentations, and orphan transitions.
pub const BOYKOV_KOLMOGOROV_MAX_TRANSITIONS: u64 = 100_000;
/// Hard ceiling for successful path augmentations.
pub const BOYKOV_KOLMOGOROV_MAX_AUGMENTATIONS: u64 = 10_000;

/// Exact deterministic counters for the retained two-tree state machine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoykovKolmogorovMetrics {
    /// Active vertices removed from the FIFO growth queue.
    pub active_visits: u64,
    /// Positive residual arcs inspected during growth.
    pub growth_arc_scans: u128,
    /// Positive residual arcs inspected while finding new orphan parents.
    pub adoption_arc_scans: u128,
    /// Free vertices attached to one of the retained trees.
    pub tree_attachments: u64,
    /// Vertices whose complete growth adjacency was exhausted.
    pub passive_vertices: u64,
    /// Source-to-sink paths augmented.
    pub augmentations: u64,
    /// Sum of residual arcs in augmented paths.
    pub augmented_path_arcs: u128,
    /// Parent arcs invalidated by augmentation or cascading removal.
    pub orphan_creations: u64,
    /// FIFO orphan records processed.
    pub orphan_visits: u64,
    /// Orphans attached to a new terminal-originating parent.
    pub adoptions: u64,
    /// Orphans removed from a tree after no valid parent exists.
    pub tree_removals: u64,
    /// Same-tree neighbors reactivated after an orphan becomes free.
    pub reactivations: u64,
    /// Deterministic logical mutations charged against the work ceiling.
    pub state_transitions: u64,
}

/// Certified Boykov–Kolmogorov result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoykovKolmogorovResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Solver-independent maximum-flow/minimum-cut witness.
    pub certificate: MaxFlowCertificate,
    /// Source-specific deterministic counters.
    pub metrics: BoykovKolmogorovMetrics,
}

/// Certified result plus a complete reversible Grow/Augment/Adopt trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoykovKolmogorovTraceResult {
    /// Same result produced by the non-tracing profile.
    pub result: BoykovKolmogorovResult,
    /// Zero-flow boundary before the two roots are installed.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible source-specific event stream.
    pub events: Vec<FlowTraceEvent>,
    /// Independently certified final boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Admission, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BoykovKolmogorovError {
    /// Input exceeds the bounded complete-trace band.
    #[error("graph exceeds Boykov-Kolmogorov admission limits")]
    AdmissionLimit,
    /// The static zero-flow problem contract is not satisfied.
    #[error("Boykov-Kolmogorov graph requirement is not satisfied: {0}")]
    GraphRequirement(&'static str),
    /// A deterministic work ceiling was reached.
    #[error("Boykov-Kolmogorov work limit reached")]
    WorkLimit,
    /// Checked arithmetic overflowed.
    #[error("Boykov-Kolmogorov arithmetic overflow")]
    ArithmeticOverflow,
    /// A retained-tree, parent-origin, or queue invariant failed.
    #[error("Boykov-Kolmogorov tree invariant failed: {0}")]
    TreeInvariant(&'static str),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The independent maximum-flow checker rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Independent source-event and reversible-boundary checker failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BoykovKolmogorovTraceCheckError {
    /// The underlying problem does not satisfy the source contract.
    #[error(transparent)]
    Input(#[from] BoykovKolmogorovError),
    /// An event does not belong to the Grow/Augment/Adopt grammar.
    #[error("unexpected Boykov-Kolmogorov event sequence")]
    UnexpectedEvent,
    /// A replay boundary violates the retained two-tree shape.
    #[error("Boykov-Kolmogorov replay invariant failed")]
    Invariant,
    /// Forward or reverse replay does not reach the declared boundary.
    #[error("Boykov-Kolmogorov replay boundary mismatch")]
    BoundaryMismatch,
    /// The independent maximum-flow checker rejected the final flow.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// A reversible patch is malformed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Validates the zero-flow static BK contract.
///
/// # Errors
///
/// Requires distinct in-range terminals, zero supplies, zero lower bounds,
/// and the conservative visualization band. Parallel, antiparallel,
/// zero-capacity, and self-loop edges are accepted.
pub fn validate_boykov_kolmogorov_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), BoykovKolmogorovError> {
    if graph.nodes().len() > BOYKOV_KOLMOGOROV_MAX_NODES
        || graph.edges().len() > BOYKOV_KOLMOGOROV_MAX_EDGES
    {
        return Err(BoykovKolmogorovError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(BoykovKolmogorovError::GraphRequirement(
            "distinct source and sink",
        ));
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(BoykovKolmogorovError::GraphRequirement("zero supplies"));
    }
    if graph.edges().iter().any(|edge| edge.lower() != 0) {
        return Err(BoykovKolmogorovError::GraphRequirement("zero lower bounds"));
    }
    Ok(())
}

/// Solves maximum flow with the retained Boykov–Kolmogorov two-tree method.
///
/// # Errors
///
/// Rejects inputs outside [`validate_boykov_kolmogorov_graph`], bounded-work
/// exhaustion, retained-tree failures, and independently rejected results.
pub fn solve_boykov_kolmogorov(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BoykovKolmogorovResult, BoykovKolmogorovError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves BK while recording reversible Grow/Augment/Adopt boundaries.
///
/// # Errors
///
/// Returns the same failures as [`solve_boykov_kolmogorov`], plus trace
/// construction failures.
pub fn trace_boykov_kolmogorov(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BoykovKolmogorovTraceResult, BoykovKolmogorovError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(BoykovKolmogorovError::TreeInvariant("missing-trace"))?;
    Ok(BoykovKolmogorovTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Checks event identity, retained-forest boundaries, final certification, and
/// exact reverse replay without trusting the solver's internal tree state.
///
/// # Errors
///
/// Rejects malformed source-specific sequencing, invalid forest boundaries,
/// mismatched result/certificate boundaries, and non-reversible traces.
pub fn check_boykov_kolmogorov_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    run: &BoykovKolmogorovTraceResult,
) -> Result<(), BoykovKolmogorovTraceCheckError> {
    validate_boykov_kolmogorov_graph(graph, source, sink)?;
    validate_trace_base(graph, &run.base_snapshot)?;
    let mut replay = run.base_snapshot.clone();
    let mut initialized = false;
    let mut connected = false;
    let mut adopting = false;
    let mut optimal = false;
    for (index, event) in run.events.iter().enumerate() {
        match event.catalog_id.as_str() {
            "boykov-kolmogorov.initialize" if index == 0 && !initialized => {
                initialized = true;
            }
            "boykov-kolmogorov.grow-source-tree"
            | "boykov-kolmogorov.grow-sink-tree"
            | "boykov-kolmogorov.finish-active"
                if initialized && !connected && !adopting && !optimal => {}
            "boykov-kolmogorov.connect-trees"
                if initialized && !connected && !adopting && !optimal =>
            {
                connected = true;
            }
            "boykov-kolmogorov.augment" if initialized && connected && !adopting && !optimal => {
                connected = false;
                adopting = true;
            }
            "boykov-kolmogorov.adopt-source-orphan"
            | "boykov-kolmogorov.adopt-sink-orphan"
            | "boykov-kolmogorov.free-source-orphan"
            | "boykov-kolmogorov.free-sink-orphan"
                if initialized && !connected && adopting && !optimal => {}
            "boykov-kolmogorov.complete-adoption"
                if initialized && !connected && adopting && !optimal =>
            {
                adopting = false;
            }
            "boykov-kolmogorov.optimal" if initialized && !connected && !adopting && !optimal => {
                optimal = true;
            }
            _ => return Err(BoykovKolmogorovTraceCheckError::UnexpectedEvent),
        }
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)?;
        let stable = matches!(
            event.catalog_id.as_str(),
            "boykov-kolmogorov.initialize"
                | "boykov-kolmogorov.grow-source-tree"
                | "boykov-kolmogorov.grow-sink-tree"
                | "boykov-kolmogorov.finish-active"
                | "boykov-kolmogorov.connect-trees"
                | "boykov-kolmogorov.complete-adoption"
                | "boykov-kolmogorov.optimal"
        );
        validate_trace_forest(graph, source, sink, &replay, stable)?;
    }
    if !optimal || replay != run.final_snapshot || replay.flows != run.result.flows {
        return Err(BoykovKolmogorovTraceCheckError::BoundaryMismatch);
    }
    if check_max_flow(graph, source, sink, &replay.flows)? != run.result.certificate {
        return Err(BoykovKolmogorovTraceCheckError::BoundaryMismatch);
    }
    for event in run.events.iter().rev() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Reverse)?;
    }
    if replay != run.base_snapshot {
        return Err(BoykovKolmogorovTraceCheckError::BoundaryMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TreeSide {
    #[default]
    Free,
    Source,
    Sink,
}

#[derive(Clone, Debug, Default)]
struct TreeNode {
    side: TreeSide,
    parent: Option<ResidualArcId>,
    distance: usize,
    active: bool,
    orphan: bool,
}

struct AugmentingPath {
    arcs: Vec<ResidualArcId>,
    source_endpoint: NodeIndex,
    sink_endpoint: NodeIndex,
}

struct InternalRun {
    result: BoykovKolmogorovResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct Engine<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    residual: ResidualState<'graph>,
    nodes: Vec<TreeNode>,
    outgoing: Vec<Vec<ResidualArcId>>,
    incoming: Vec<Vec<ResidualArcId>>,
    neighbors: Vec<Vec<NodeIndex>>,
    active: VecDeque<NodeIndex>,
    active_set: BTreeSet<NodeIndex>,
    orphans: VecDeque<NodeIndex>,
    orphan_set: BTreeSet<NodeIndex>,
    metrics: BoykovKolmogorovMetrics,
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<InternalRun, BoykovKolmogorovError> {
    validate_boykov_kolmogorov_graph(graph, source, sink)?;
    let residual = ResidualState::at_lower_bounds(graph);
    let base = FlowTraceSnapshot::capture(
        graph,
        &residual,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    let mut recorder = if with_trace {
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };
    let mut engine = Engine::new(graph, source, sink, residual)?;
    engine.record(
        recorder.as_mut(),
        "boykov-kolmogorov.initialize",
        "boykov-kolmogorov:initialize-s-t-active-roots",
        TraceGranularityV1::Phase,
        engine.active.iter().copied().collect(),
        Vec::new(),
        Some(("active-roots", 2)),
        Some((vec![source, sink], Vec::new())),
    )?;

    while let Some(path) = engine.grow(recorder.as_mut())? {
        engine.augment(&path, recorder.as_mut())?;
        engine.adopt(recorder.as_mut())?;
        engine.record(
            recorder.as_mut(),
            "boykov-kolmogorov.complete-adoption",
            "boykov-kolmogorov:resume-growth-after-orphan-queue-empty",
            TraceGranularityV1::Phase,
            engine.active.iter().copied().collect(),
            Vec::new(),
            Some(("orphans", 0)),
            Some((Vec::new(), Vec::new())),
        )?;
        engine.validate_forest(false)?;
    }
    engine.validate_forest(false)?;
    let flows = engine.residual.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    let cut = certificate
        .source_side
        .iter()
        .filter_map(|id| graph.node_index(id))
        .collect::<Vec<_>>();
    engine.record(
        recorder.as_mut(),
        "boykov-kolmogorov.optimal",
        "boykov-kolmogorov:return-separated-trees-and-certified-cut",
        TraceGranularityV1::Phase,
        cut,
        Vec::new(),
        Some(("value", certificate.value)),
        None,
    )?;
    Ok(InternalRun {
        result: BoykovKolmogorovResult {
            flows,
            certificate,
            metrics: engine.metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

impl<'graph> Engine<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        residual: ResidualState<'graph>,
    ) -> Result<Self, BoykovKolmogorovError> {
        let count = graph.nodes().len();
        let mut outgoing = vec![Vec::new(); count];
        let mut incoming = vec![Vec::new(); count];
        let mut neighbors = vec![BTreeSet::new(); count];
        for edge in graph.edges() {
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let arc = residual
                    .arc(&id)
                    .ok_or(BoykovKolmogorovError::TreeInvariant("missing-residual"))?;
                outgoing[arc.from.as_usize()].push(id.clone());
                incoming[arc.to.as_usize()].push(id);
                if arc.from != arc.to {
                    neighbors[arc.from.as_usize()].insert(arc.to);
                    neighbors[arc.to.as_usize()].insert(arc.from);
                }
            }
        }
        for ids in outgoing.iter_mut().chain(incoming.iter_mut()) {
            ids.sort_unstable();
            ids.dedup();
        }
        let neighbors = neighbors
            .into_iter()
            .map(|set| set.into_iter().collect())
            .collect();
        let mut nodes = vec![TreeNode::default(); count];
        nodes[source.as_usize()] = TreeNode {
            side: TreeSide::Source,
            active: true,
            ..TreeNode::default()
        };
        nodes[sink.as_usize()] = TreeNode {
            side: TreeSide::Sink,
            active: true,
            ..TreeNode::default()
        };
        Ok(Self {
            graph,
            source,
            sink,
            residual,
            nodes,
            outgoing,
            incoming,
            neighbors,
            active: VecDeque::from([source, sink]),
            active_set: BTreeSet::from([source, sink]),
            orphans: VecDeque::new(),
            orphan_set: BTreeSet::new(),
            metrics: BoykovKolmogorovMetrics::default(),
        })
    }

    fn grow(
        &mut self,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<Option<AugmentingPath>, BoykovKolmogorovError> {
        while let Some(active) = self.active.pop_front() {
            self.active_set.remove(&active);
            if !self.nodes[active.as_usize()].active
                || self.nodes[active.as_usize()].orphan
                || self.nodes[active.as_usize()].side == TreeSide::Free
            {
                continue;
            }
            self.metrics.active_visits = checked_add(self.metrics.active_visits, 1)?;
            let side = self.nodes[active.as_usize()].side;
            let adjacency = match side {
                TreeSide::Source => self.outgoing[active.as_usize()].clone(),
                TreeSide::Sink => self.incoming[active.as_usize()].clone(),
                TreeSide::Free => unreachable!("free active vertex was filtered"),
            };
            for id in adjacency {
                let arc = self
                    .residual
                    .arc(&id)
                    .ok_or(BoykovKolmogorovError::TreeInvariant("missing-growth-arc"))?;
                if arc.capacity == 0 {
                    continue;
                }
                self.count_scan(false)?;
                let candidate = match side {
                    TreeSide::Source => arc.to,
                    TreeSide::Sink => arc.from,
                    TreeSide::Free => unreachable!(),
                };
                if candidate == active {
                    continue;
                }
                let candidate_side = self.nodes[candidate.as_usize()].side;
                if candidate_side == TreeSide::Free {
                    self.attach(active, candidate, side, id.clone())?;
                    self.record(
                        recorder.as_deref_mut(),
                        match side {
                            TreeSide::Source => "boykov-kolmogorov.grow-source-tree",
                            TreeSide::Sink => "boykov-kolmogorov.grow-sink-tree",
                            TreeSide::Free => unreachable!(),
                        },
                        "boykov-kolmogorov:acquire-free-neighbor",
                        TraceGranularityV1::Operation,
                        vec![active, candidate],
                        vec![id.clone()],
                        Some(("distance", self.distance_i128(candidate)?)),
                        Some((vec![candidate], vec![id])),
                    )?;
                } else if candidate_side != side {
                    let (source_endpoint, sink_endpoint) = match side {
                        TreeSide::Source => (active, candidate),
                        TreeSide::Sink => (candidate, active),
                        TreeSide::Free => unreachable!(),
                    };
                    let path = self.build_path(source_endpoint, sink_endpoint, id)?;
                    // Section 3.2.1 returns immediately on contact, before the
                    // active node is removed from A. Preserve that exact queue
                    // state across augmentation and adoption.
                    self.enqueue_active(active)?;
                    self.record(
                        recorder.as_deref_mut(),
                        "boykov-kolmogorov.connect-trees",
                        "boykov-kolmogorov:return-path-when-opposite-trees-touch",
                        TraceGranularityV1::Operation,
                        vec![source_endpoint, sink_endpoint],
                        path.arcs.clone(),
                        Some(("path-arcs", path.arcs.len() as i128)),
                        Some((vec![source_endpoint, sink_endpoint], path.arcs.clone())),
                    )?;
                    return Ok(Some(path));
                }
            }
            self.nodes[active.as_usize()].active = false;
            self.metrics.passive_vertices = checked_add(self.metrics.passive_vertices, 1)?;
            self.count_transition()?;
            self.record(
                recorder.as_deref_mut(),
                "boykov-kolmogorov.finish-active",
                "boykov-kolmogorov:remove-active-after-all-neighbors-scanned",
                TraceGranularityV1::Operation,
                vec![active],
                Vec::new(),
                None,
                Some((vec![active], Vec::new())),
            )?;
        }
        Ok(None)
    }

    fn attach(
        &mut self,
        parent: NodeIndex,
        child: NodeIndex,
        side: TreeSide,
        parent_arc: ResidualArcId,
    ) -> Result<(), BoykovKolmogorovError> {
        let distance = self.nodes[parent.as_usize()]
            .distance
            .checked_add(1)
            .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        self.nodes[child.as_usize()] = TreeNode {
            side,
            parent: Some(parent_arc),
            distance,
            active: true,
            orphan: false,
        };
        self.enqueue_active(child)?;
        self.metrics.tree_attachments = checked_add(self.metrics.tree_attachments, 1)?;
        self.count_transition()
    }

    fn build_path(
        &self,
        source_endpoint: NodeIndex,
        sink_endpoint: NodeIndex,
        bridge: ResidualArcId,
    ) -> Result<AugmentingPath, BoykovKolmogorovError> {
        let bridge_arc = self
            .residual
            .arc(&bridge)
            .ok_or(BoykovKolmogorovError::TreeInvariant("missing-bridge"))?;
        if bridge_arc.capacity == 0
            || bridge_arc.from != source_endpoint
            || bridge_arc.to != sink_endpoint
        {
            return Err(BoykovKolmogorovError::TreeInvariant("bridge-direction"));
        }
        let mut source_reverse = Vec::new();
        let mut current = source_endpoint;
        let mut seen = BTreeSet::new();
        while current != self.source {
            if !seen.insert(current) {
                return Err(BoykovKolmogorovError::TreeInvariant("source-cycle"));
            }
            let id = self.nodes[current.as_usize()].parent.clone().ok_or(
                BoykovKolmogorovError::TreeInvariant("source-parent-missing"),
            )?;
            let arc = self
                .residual
                .arc(&id)
                .ok_or(BoykovKolmogorovError::TreeInvariant("source-parent-arc"))?;
            if arc.capacity == 0 || arc.to != current {
                return Err(BoykovKolmogorovError::TreeInvariant(
                    "source-parent-direction",
                ));
            }
            source_reverse.push(id);
            current = arc.from;
        }
        source_reverse.reverse();
        let mut arcs = source_reverse;
        arcs.push(bridge);
        current = sink_endpoint;
        seen.clear();
        while current != self.sink {
            if !seen.insert(current) {
                return Err(BoykovKolmogorovError::TreeInvariant("sink-cycle"));
            }
            let id = self.nodes[current.as_usize()]
                .parent
                .clone()
                .ok_or(BoykovKolmogorovError::TreeInvariant("sink-parent-missing"))?;
            let arc = self
                .residual
                .arc(&id)
                .ok_or(BoykovKolmogorovError::TreeInvariant("sink-parent-arc"))?;
            if arc.capacity == 0 || arc.from != current {
                return Err(BoykovKolmogorovError::TreeInvariant(
                    "sink-parent-direction",
                ));
            }
            arcs.push(id);
            current = arc.to;
        }
        Ok(AugmentingPath {
            arcs,
            source_endpoint,
            sink_endpoint,
        })
    }

    fn augment(
        &mut self,
        path: &AugmentingPath,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), BoykovKolmogorovError> {
        if self.metrics.augmentations >= BOYKOV_KOLMOGOROV_MAX_AUGMENTATIONS {
            return Err(BoykovKolmogorovError::WorkLimit);
        }
        let delta = path
            .arcs
            .iter()
            .map(|id| {
                self.residual
                    .arc(id)
                    .filter(|arc| arc.capacity > 0)
                    .map(|arc| arc.capacity)
                    .ok_or(BoykovKolmogorovError::TreeInvariant(
                        "path-has-no-residual-capacity",
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .min()
            .ok_or(BoykovKolmogorovError::TreeInvariant("empty-path"))?;
        self.residual.augment(&path.arcs, delta)?;
        self.metrics.augmentations = checked_add(self.metrics.augmentations, 1)?;
        self.metrics.augmented_path_arcs = self
            .metrics
            .augmented_path_arcs
            .checked_add(path.arcs.len() as u128)
            .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        self.count_transition()?;

        let invalid = self
            .graph
            .node_indices()
            .filter(|&node| node != self.source && node != self.sink)
            .filter(|&node| {
                self.nodes[node.as_usize()]
                    .parent
                    .as_ref()
                    .and_then(|id| self.residual.arc(id))
                    .is_some_and(|arc| arc.capacity == 0)
            })
            .collect::<Vec<_>>();
        for orphan in invalid {
            self.make_orphan(orphan)?;
        }
        self.record(
            recorder,
            "boykov-kolmogorov.augment",
            "boykov-kolmogorov:augment-and-orphan-saturated-tree-arcs",
            TraceGranularityV1::Operation,
            std::iter::once(path.source_endpoint)
                .chain(std::iter::once(path.sink_endpoint))
                .chain(self.orphans.iter().copied())
                .collect(),
            path.arcs.clone(),
            Some(("delta", i128::from(delta))),
            Some((self.orphans.iter().copied().collect(), path.arcs.clone())),
        )
    }

    fn adopt(
        &mut self,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), BoykovKolmogorovError> {
        while let Some(orphan) = self.orphans.pop_front() {
            self.orphan_set.remove(&orphan);
            if !self.nodes[orphan.as_usize()].orphan {
                continue;
            }
            self.metrics.orphan_visits = checked_add(self.metrics.orphan_visits, 1)?;
            let side = self.nodes[orphan.as_usize()].side;
            let parent = self.best_origin_parent(orphan, side)?;
            if let Some((id, parent, distance)) = parent {
                self.nodes[orphan.as_usize()].parent = Some(id.clone());
                self.nodes[orphan.as_usize()].distance = distance;
                self.nodes[orphan.as_usize()].orphan = false;
                self.refresh_distances()?;
                self.metrics.adoptions = checked_add(self.metrics.adoptions, 1)?;
                self.count_transition()?;
                self.record(
                    recorder.as_deref_mut(),
                    match side {
                        TreeSide::Source => "boykov-kolmogorov.adopt-source-orphan",
                        TreeSide::Sink => "boykov-kolmogorov.adopt-sink-orphan",
                        TreeSide::Free => {
                            return Err(BoykovKolmogorovError::TreeInvariant(
                                "free-orphan-adopted",
                            ));
                        }
                    },
                    "boykov-kolmogorov:choose-nearest-terminal-origin-parent",
                    TraceGranularityV1::Operation,
                    std::iter::once(orphan)
                        .chain(std::iter::once(parent))
                        .chain(self.orphans.iter().copied())
                        .collect(),
                    vec![id.clone()],
                    Some(("distance", distance as i128)),
                    Some((vec![orphan], vec![id])),
                )?;
            } else {
                self.free_orphan(orphan, side, recorder.as_deref_mut())?;
            }
        }
        Ok(())
    }

    fn best_origin_parent(
        &mut self,
        orphan: NodeIndex,
        side: TreeSide,
    ) -> Result<Option<(ResidualArcId, NodeIndex, usize)>, BoykovKolmogorovError> {
        let adjacency = match side {
            TreeSide::Source => self.incoming[orphan.as_usize()].clone(),
            TreeSide::Sink => self.outgoing[orphan.as_usize()].clone(),
            TreeSide::Free => {
                return Err(BoykovKolmogorovError::TreeInvariant("free-orphan"));
            }
        };
        let mut best = None::<(usize, NodeIndex, ResidualArcId)>;
        for id in adjacency {
            let arc = self
                .residual
                .arc(&id)
                .ok_or(BoykovKolmogorovError::TreeInvariant("adoption-arc"))?;
            if arc.capacity == 0 {
                continue;
            }
            self.count_scan(true)?;
            let parent = match side {
                TreeSide::Source if arc.to == orphan => arc.from,
                TreeSide::Sink if arc.from == orphan => arc.to,
                TreeSide::Source | TreeSide::Sink => {
                    return Err(BoykovKolmogorovError::TreeInvariant("adoption-direction"));
                }
                TreeSide::Free => unreachable!(),
            };
            if parent == orphan || self.nodes[parent.as_usize()].side != side {
                continue;
            }
            let Some(parent_distance) = self.origin_distance(parent, side)? else {
                continue;
            };
            let distance = parent_distance
                .checked_add(1)
                .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
            let candidate = (distance, parent, id);
            if best.as_ref().is_none_or(|current| candidate < *current) {
                best = Some(candidate);
            }
        }
        Ok(best.map(|(distance, parent, id)| (id, parent, distance)))
    }

    fn origin_distance(
        &self,
        start: NodeIndex,
        side: TreeSide,
    ) -> Result<Option<usize>, BoykovKolmogorovError> {
        let terminal = match side {
            TreeSide::Source => self.source,
            TreeSide::Sink => self.sink,
            TreeSide::Free => return Ok(None),
        };
        let mut current = start;
        let mut distance = 0_usize;
        let mut seen = BTreeSet::new();
        while current != terminal {
            if !seen.insert(current) {
                return Err(BoykovKolmogorovError::TreeInvariant("origin-cycle"));
            }
            let state = &self.nodes[current.as_usize()];
            if state.side != side || state.orphan {
                return Ok(None);
            }
            let Some(id) = state.parent.as_ref() else {
                return Ok(None);
            };
            let arc = self
                .residual
                .arc(id)
                .ok_or(BoykovKolmogorovError::TreeInvariant("origin-parent-arc"))?;
            if arc.capacity == 0 {
                return Ok(None);
            }
            current = match side {
                TreeSide::Source if arc.to == current => arc.from,
                TreeSide::Sink if arc.from == current => arc.to,
                TreeSide::Source | TreeSide::Sink => {
                    return Err(BoykovKolmogorovError::TreeInvariant(
                        "origin-parent-direction",
                    ));
                }
                TreeSide::Free => unreachable!(),
            };
            distance = distance
                .checked_add(1)
                .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        }
        Ok(Some(distance))
    }

    fn free_orphan(
        &mut self,
        orphan: NodeIndex,
        side: TreeSide,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<(), BoykovKolmogorovError> {
        let neighbors = self.neighbors[orphan.as_usize()].clone();
        for neighbor in neighbors {
            if self.nodes[neighbor.as_usize()].side != side {
                continue;
            }
            if self.parent_node(neighbor, side)? == Some(orphan) {
                self.make_orphan(neighbor)?;
            }
            if self.tree_capacity(neighbor, orphan, side) > 0 {
                let was_active = self.nodes[neighbor.as_usize()].active;
                self.enqueue_active(neighbor)?;
                if !was_active {
                    self.metrics.reactivations = checked_add(self.metrics.reactivations, 1)?;
                }
            }
        }
        let old_distance = self.nodes[orphan.as_usize()].distance;
        self.nodes[orphan.as_usize()] = TreeNode::default();
        self.active_set.remove(&orphan);
        self.metrics.tree_removals = checked_add(self.metrics.tree_removals, 1)?;
        self.count_transition()?;
        self.record(
            recorder,
            match side {
                TreeSide::Source => "boykov-kolmogorov.free-source-orphan",
                TreeSide::Sink => "boykov-kolmogorov.free-sink-orphan",
                TreeSide::Free => {
                    return Err(BoykovKolmogorovError::TreeInvariant(
                        "free-orphan-without-side",
                    ));
                }
            },
            "boykov-kolmogorov:free-orphan-and-orphan-children",
            TraceGranularityV1::Operation,
            std::iter::once(orphan)
                .chain(self.orphans.iter().copied())
                .collect(),
            Vec::new(),
            Some(("old-distance", old_distance as i128)),
            Some((vec![orphan], Vec::new())),
        )
    }

    fn make_orphan(&mut self, node: NodeIndex) -> Result<(), BoykovKolmogorovError> {
        if node == self.source || node == self.sink {
            return Err(BoykovKolmogorovError::TreeInvariant("terminal-orphan"));
        }
        if self.nodes[node.as_usize()].side == TreeSide::Free {
            return Ok(());
        }
        self.nodes[node.as_usize()].parent = None;
        self.nodes[node.as_usize()].orphan = true;
        if self.orphan_set.insert(node) {
            self.orphans.push_back(node);
            self.metrics.orphan_creations = checked_add(self.metrics.orphan_creations, 1)?;
            self.count_transition()?;
        }
        Ok(())
    }

    fn parent_node(
        &self,
        child: NodeIndex,
        side: TreeSide,
    ) -> Result<Option<NodeIndex>, BoykovKolmogorovError> {
        let Some(id) = self.nodes[child.as_usize()].parent.as_ref() else {
            return Ok(None);
        };
        let arc = self
            .residual
            .arc(id)
            .ok_or(BoykovKolmogorovError::TreeInvariant("parent-arc"))?;
        match side {
            TreeSide::Source if arc.to == child => Ok(Some(arc.from)),
            TreeSide::Sink if arc.from == child => Ok(Some(arc.to)),
            TreeSide::Source | TreeSide::Sink => {
                Err(BoykovKolmogorovError::TreeInvariant("parent-direction"))
            }
            TreeSide::Free => Ok(None),
        }
    }

    fn tree_capacity(&self, parent: NodeIndex, child: NodeIndex, side: TreeSide) -> u64 {
        let adjacency = match side {
            TreeSide::Source => &self.outgoing[parent.as_usize()],
            TreeSide::Sink => &self.incoming[parent.as_usize()],
            TreeSide::Free => return 0,
        };
        adjacency
            .iter()
            .filter_map(|id| self.residual.arc(id))
            .filter(|arc| arc.capacity > 0)
            .filter(|arc| match side {
                TreeSide::Source => arc.to == child,
                TreeSide::Sink => arc.from == child,
                TreeSide::Free => false,
            })
            .map(|arc| arc.capacity)
            .max()
            .unwrap_or(0)
    }

    fn enqueue_active(&mut self, node: NodeIndex) -> Result<(), BoykovKolmogorovError> {
        if self.nodes[node.as_usize()].side == TreeSide::Free {
            return Err(BoykovKolmogorovError::TreeInvariant("activate-free-node"));
        }
        self.nodes[node.as_usize()].active = true;
        if self.active_set.insert(node) {
            self.active.push_back(node);
        }
        Ok(())
    }

    fn count_scan(&mut self, adoption: bool) -> Result<(), BoykovKolmogorovError> {
        let total = self
            .metrics
            .growth_arc_scans
            .checked_add(self.metrics.adoption_arc_scans)
            .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        if total >= BOYKOV_KOLMOGOROV_MAX_ARC_SCANS {
            return Err(BoykovKolmogorovError::WorkLimit);
        }
        if adoption {
            self.metrics.adoption_arc_scans = self
                .metrics
                .adoption_arc_scans
                .checked_add(1)
                .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        } else {
            self.metrics.growth_arc_scans = self
                .metrics
                .growth_arc_scans
                .checked_add(1)
                .ok_or(BoykovKolmogorovError::ArithmeticOverflow)?;
        }
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), BoykovKolmogorovError> {
        if self.metrics.state_transitions >= BOYKOV_KOLMOGOROV_MAX_TRANSITIONS {
            return Err(BoykovKolmogorovError::WorkLimit);
        }
        self.metrics.state_transitions = checked_add(self.metrics.state_transitions, 1)?;
        Ok(())
    }

    fn validate_forest(&self, allow_orphans: bool) -> Result<(), BoykovKolmogorovError> {
        if self.nodes[self.source.as_usize()].side != TreeSide::Source
            || self.nodes[self.source.as_usize()].parent.is_some()
            || self.nodes[self.source.as_usize()].orphan
            || self.nodes[self.sink.as_usize()].side != TreeSide::Sink
            || self.nodes[self.sink.as_usize()].parent.is_some()
            || self.nodes[self.sink.as_usize()].orphan
        {
            return Err(BoykovKolmogorovError::TreeInvariant("terminal-roots"));
        }
        for node in self.graph.node_indices() {
            let state = &self.nodes[node.as_usize()];
            if state.side == TreeSide::Free {
                if state.parent.is_some() || state.orphan {
                    return Err(BoykovKolmogorovError::TreeInvariant("free-state"));
                }
                continue;
            }
            if node == self.source || node == self.sink {
                continue;
            }
            if state.orphan {
                if !allow_orphans || state.parent.is_some() {
                    return Err(BoykovKolmogorovError::TreeInvariant("orphan-state"));
                }
                continue;
            }
            let Some(distance) = self.origin_distance(node, state.side)? else {
                return Err(BoykovKolmogorovError::TreeInvariant("no-terminal-origin"));
            };
            if distance != state.distance {
                return Err(BoykovKolmogorovError::TreeInvariant("stale-distance"));
            }
        }
        Ok(())
    }

    fn refresh_distances(&mut self) -> Result<(), BoykovKolmogorovError> {
        let distances = self
            .graph
            .node_indices()
            .map(|node| {
                let state = &self.nodes[node.as_usize()];
                if state.side == TreeSide::Free || state.orphan {
                    Ok(None)
                } else {
                    self.origin_distance(node, state.side)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (node, distance) in self.graph.node_indices().zip(distances) {
            if let Some(distance) = distance {
                self.nodes[node.as_usize()].distance = distance;
            }
        }
        Ok(())
    }

    fn labels(&self) -> Result<Vec<Option<i128>>, BoykovKolmogorovError> {
        self.nodes
            .iter()
            .map(|node| match node.side {
                TreeSide::Free => Ok(None),
                TreeSide::Source => i128::try_from(node.distance)
                    .map(Some)
                    .map_err(|_| BoykovKolmogorovError::ArithmeticOverflow),
                TreeSide::Sink => i128::try_from(node.distance)
                    .ok()
                    .and_then(|value| value.checked_add(1))
                    .and_then(i128::checked_neg)
                    .map(Some)
                    .ok_or(BoykovKolmogorovError::ArithmeticOverflow),
            })
            .collect()
    }

    fn forest_arcs(&self) -> Vec<ResidualArcId> {
        self.graph
            .node_indices()
            .filter_map(|node| {
                let state = &self.nodes[node.as_usize()];
                if state.orphan {
                    return None;
                }
                let parent = state.parent.as_ref()?;
                Some(match state.side {
                    TreeSide::Source => parent.clone(),
                    TreeSide::Sink => reverse_residual_id(parent),
                    TreeSide::Free => return None,
                })
            })
            .collect()
    }

    fn distance_i128(&self, node: NodeIndex) -> Result<i128, BoykovKolmogorovError> {
        i128::try_from(self.nodes[node.as_usize()].distance)
            .map_err(|_| BoykovKolmogorovError::ArithmeticOverflow)
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        catalog_id: &'static str,
        pseudocode_line: &'static str,
        granularity: TraceGranularityV1,
        search_order: Vec<NodeIndex>,
        active_path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
        exact_focus: Option<(Vec<NodeIndex>, Vec<ResidualArcId>)>,
    ) -> Result<(), BoykovKolmogorovError> {
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
        .with_forest_overlay(self.graph, self.forest_arcs(), Vec::new());
        let metadata = FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity: granularity,
            pseudocode_line,
        };
        if let Some((nodes, arcs)) = exact_focus {
            let mut focus = nodes
                .into_iter()
                .map(|node| {
                    self.graph
                        .node(node)
                        .map(|node| FlowTraceEntityRef::Node(node.id().clone()))
                        .ok_or(BoykovKolmogorovError::TreeInvariant("focus-node"))
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

fn trace_metrics(metrics: BoykovKolmogorovMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: u128::from(metrics.active_visits),
        relaxation_passes: u128::from(metrics.passive_vertices),
        residual_arc_scans: metrics
            .growth_arc_scans
            .saturating_add(metrics.adoption_arc_scans),
        augmentations: u128::from(metrics.augmentations),
        path_searches: metrics.augmented_path_arcs,
        scaling_phases: u128::from(metrics.tree_attachments),
        blocking_flow_phases: u128::from(metrics.orphan_creations),
        pushes: u128::from(metrics.adoptions),
        relabels: u128::from(metrics.tree_removals),
        retreats: u128::from(metrics.orphan_visits),
        reverse_bfs_runs: metrics.adoption_arc_scans,
        gap_terminations: u128::from(metrics.orphan_creations),
        saturating_pushes: u128::from(metrics.orphan_creations),
        nonsaturating_pushes: u128::from(metrics.adoptions),
        discharges: u128::from(metrics.reactivations),
        active_vertex_selections: u128::from(metrics.state_transitions),
    }
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

fn checked_add(value: u64, amount: u64) -> Result<u64, BoykovKolmogorovError> {
    value
        .checked_add(amount)
        .ok_or(BoykovKolmogorovError::ArithmeticOverflow)
}

fn validate_trace_base(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), BoykovKolmogorovTraceCheckError> {
    if snapshot.flows != vec![0; graph.edges().len()]
        || snapshot.node_labels != vec![None; graph.nodes().len()]
        || !snapshot.forest_arcs.is_empty()
    {
        return Err(BoykovKolmogorovTraceCheckError::Invariant);
    }
    Ok(())
}

fn validate_trace_forest(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
    require_terminal_origin: bool,
) -> Result<(), BoykovKolmogorovTraceCheckError> {
    if snapshot.node_labels.len() != graph.nodes().len() {
        return Err(BoykovKolmogorovTraceCheckError::Invariant);
    }
    let state = ResidualState::from_flows(graph, &snapshot.flows)
        .map_err(|_| BoykovKolmogorovTraceCheckError::Invariant)?;
    let mut parent = vec![None::<NodeIndex>; graph.nodes().len()];
    for id in &snapshot.forest_arcs {
        let visual_arc = state
            .arc(id)
            .ok_or(BoykovKolmogorovTraceCheckError::Invariant)?;
        let from_label = snapshot.node_labels[visual_arc.from.as_usize()]
            .ok_or(BoykovKolmogorovTraceCheckError::Invariant)?;
        let to_label = snapshot.node_labels[visual_arc.to.as_usize()]
            .ok_or(BoykovKolmogorovTraceCheckError::Invariant)?;
        let (child, expected_parent, admissible) = if from_label >= 0 && to_label == from_label + 1
        {
            (visual_arc.to, visual_arc.from, visual_arc)
        } else if from_label < 0 && to_label + 1 == from_label {
            // Sink-tree forest arcs are visual parent->child, opposite the
            // admissible residual direction retained by the solver.
            let admissible = state
                .arc(&reverse_residual_id(id))
                .ok_or(BoykovKolmogorovTraceCheckError::Invariant)?;
            (visual_arc.to, visual_arc.from, admissible)
        } else {
            return Err(BoykovKolmogorovTraceCheckError::Invariant);
        };
        if admissible.capacity == 0 {
            return Err(BoykovKolmogorovTraceCheckError::Invariant);
        }
        if parent[child.as_usize()].replace(expected_parent).is_some() {
            return Err(BoykovKolmogorovTraceCheckError::Invariant);
        }
    }
    if snapshot.node_labels[source.as_usize()] != Some(0)
        || snapshot.node_labels[sink.as_usize()] != Some(-1)
    {
        return Err(BoykovKolmogorovTraceCheckError::Invariant);
    }
    for node in graph.node_indices() {
        let Some(label) = snapshot.node_labels[node.as_usize()] else {
            if parent[node.as_usize()].is_some() {
                return Err(BoykovKolmogorovTraceCheckError::Invariant);
            }
            continue;
        };
        if node == source || node == sink {
            continue;
        }
        if require_terminal_origin && parent[node.as_usize()].is_none() {
            return Err(BoykovKolmogorovTraceCheckError::Invariant);
        }
        let mut current = node;
        let mut seen = BTreeSet::new();
        while let Some(next) = parent[current.as_usize()] {
            if !seen.insert(current) {
                return Err(BoykovKolmogorovTraceCheckError::Invariant);
            }
            current = next;
        }
        if require_terminal_origin
            && ((label >= 0 && current != source) || (label < 0 && current != sink))
        {
            return Err(BoykovKolmogorovTraceCheckError::Invariant);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn network(capacities: [u64; 5]) -> FlowNetwork {
        network_with_first_lower(capacities, 0)
    }

    fn network_with_first_lower(capacities: [u64; 5], first_lower: u64) -> FlowNetwork {
        let node_ids = ["s", "a", "b", "t"];
        let nodes = node_ids
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect::<Vec<_>>();
        let edge = |id: &str, from: &str, to: &str, lower: u64, capacity: u64| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower,
            capacity,
            cost: 0,
        };
        FlowNetwork::new(
            nodes,
            vec![
                edge("e0", "s", "a", first_lower, capacities[0]),
                edge("e1", "s", "b", 0, capacities[1]),
                edge("e2", "a", "b", 0, capacities[2]),
                edge("e3", "a", "t", 0, capacities[3]),
                edge("e4", "b", "t", 0, capacities[4]),
            ],
        )
        .expect("network")
    }

    fn terminals(graph: &FlowNetwork) -> (NodeIndex, NodeIndex) {
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (source, sink)
    }

    #[test]
    fn fast_trace_certificate_and_reverse_replay_agree() {
        let graph = network([8, 5, 4, 4, 9]);
        let (source, sink) = terminals(&graph);
        let fast = solve_boykov_kolmogorov(&graph, source, sink).expect("fast");
        let traced = trace_boykov_kolmogorov(&graph, source, sink).expect("trace");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.value, 13);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "boykov-kolmogorov.connect-trees")
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "boykov-kolmogorov.augment")
        );
        check_boykov_kolmogorov_trace(&graph, source, sink, &traced).expect("checked trace");
    }

    #[test]
    fn exhaustive_small_capacities_match_edmonds_karp() {
        for code in 0_u64..3_u64.pow(5) {
            let mut value = code;
            let mut capacities = [0_u64; 5];
            for capacity in &mut capacities {
                *capacity = value % 3;
                value /= 3;
            }
            let graph = network(capacities);
            let (source, sink) = terminals(&graph);
            let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            let actual = solve_boykov_kolmogorov(&graph, source, sink).expect("BK");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "{capacities:?}"
            );
        }
    }

    #[test]
    fn rejects_nonzero_lower_bound() {
        let graph = network_with_first_lower([1, 1, 1, 1, 1], 1);
        let (source, sink) = terminals(&graph);
        assert!(matches!(
            solve_boykov_kolmogorov(&graph, source, sink),
            Err(BoykovKolmogorovError::GraphRequirement("zero lower bounds"))
        ));
    }

    #[test]
    fn checker_rejects_event_identity_and_boundary_corruption() {
        let graph = network([8, 5, 4, 4, 9]);
        let (source, sink) = terminals(&graph);
        let traced = trace_boykov_kolmogorov(&graph, source, sink).expect("trace");
        let mut wrong_event = traced.clone();
        wrong_event.events[0].catalog_id = "ibfs.initialize-two-trees".to_owned();
        assert_eq!(
            check_boykov_kolmogorov_trace(&graph, source, sink, &wrong_event),
            Err(BoykovKolmogorovTraceCheckError::UnexpectedEvent)
        );
        let mut wrong_boundary = traced.clone();
        wrong_boundary.final_snapshot.flows[0] = 0;
        assert_eq!(
            check_boykov_kolmogorov_trace(&graph, source, sink, &wrong_boundary),
            Err(BoykovKolmogorovTraceCheckError::BoundaryMismatch)
        );
    }
}
