//! Exact bounded demonstrator for weighted push-relabel on shortcut graphs.
//!
//! Bernstein--Blikstad--Li--Saranurak--Tu build a weak directed expander
//! hierarchy, add one bidirectional Steiner star per hierarchy component, and
//! run the relabel-prioritized weighted push-relabel kernel of BBST24 on the
//! resulting shortcut graph.  This realization keeps those literal objects:
//! hierarchy-respecting order, original weights `|tau(u)-tau(v)|`, shortcut
//! weights `|C|`, unit capacity scale `psi`, the `9h` death rule, modified
//! residual distances, and a sparse distance-layer cut.
//!
//! The randomized cut-matching hierarchy and its flow-unfolding data structure
//! are intentionally replaced by a deterministic one-level SCC hierarchy.
//! The shortcut call is therefore a checked source-kernel demonstration, not a
//! claim of the paper's `~O(n^2 log U)` implementation.  Exactness follows the
//! source outer loop: rebuild the original residual graph, run the same weighted
//! push-relabel kernel, apply its integral residual flow, and stop only when a
//! residual call routes zero.  The final flow is independently checked by the
//! max-flow/min-cut certificate.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Original-node ceiling for the explicit shortcut graph.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES: usize = 8;
/// Original-edge ceiling for the explicit shortcut graph.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_EDGES: usize = 12;
/// Capacity ceiling keeping the source demand and trace bounded.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_CAPACITY: u64 = 64;
/// Relabel increments executed by the literal kernel.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_RELABEL_STEPS: u64 = 1_000_000;
/// Augmenting paths in the source kernel.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_AUGMENTATIONS: u64 = 8_192;
/// Public reversible semantic boundaries.
pub const WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_TRACE_EVENTS: usize = 32_768;
/// Maximum literal primitive inspections represented by one visual checkpoint.
const WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE: u64 = 512;
/// Literal relabel increments represented by one visual checkpoint after the dense prefix.
const WEIGHTED_PUSH_RELABEL_SHORTCUT_RELABEL_CHECKPOINT_STRIDE: u64 = 128;
/// Initial literal operations published without sampling so short traces remain fully legible.
const WEIGHTED_PUSH_RELABEL_SHORTCUT_DENSE_TRACE_PREFIX: u64 = 32;

/// Semantic source and bounded residual-completion boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedPushRelabelShortcutStage {
    /// Valid original graph before hierarchy construction.
    Ready,
    /// Canonical one-level SCC hierarchy and respecting order are installed.
    BuildWeakHierarchy,
    /// Bidirectional Steiner stars are materialized.
    BuildShortcutGraph,
    /// Original and shortcut edge weights are assigned.
    AssignWeights,
    /// The `n^3 U` source/sink demand and source height are public.
    InitializeDemand,
    /// One complete aggressive relabel closure is public.
    RelabelSweep,
    /// A source-time checkpoint after one or more literal weighted label increments.
    RelabelCheckpoint,
    /// A source-time checkpoint after one or more concrete augmented-edge inspections.
    InspectPrimitiveArcCheckpoint,
    /// One admissible simple path was augmented in the shortcut graph.
    AugmentPath,
    /// Routed value and average weighted length were measured.
    MeasureShortFlow,
    /// Modified zero-forward weights and residual distances were computed.
    ComputeDistanceLayers,
    /// The minimum residual distance-layer cut was selected.
    SelectSparseCut,
    /// A concrete original-residual arc inspection inside exact completion.
    CompletionInspectPrimitiveArcCheckpoint,
    /// A concrete original-node relabel inside exact completion.
    CompletionRelabelCheckpoint,
    /// One exact original-residual path was augmented.
    CompletionAugmentPath,
    /// One original-residual kernel call was applied to the exact flow.
    CompletionResidualRound,
    /// Source-defined weighted residual rounds completed on the original graph.
    CompleteResidualRounds,
    /// Independent maximum-flow/minimum-cut checking passed.
    CheckCertificate,
    /// Certified exact maximum flow is public.
    Optimal,
}

/// Source role of a directed edge in the augmented graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedPushRelabelShortcutEdgeKind {
    /// An immutable user-declared edge.
    Original,
    /// One direction of a bidirectional Steiner-star connection.
    Shortcut,
}

/// Direction of one residual arc of an augmented directed edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightedPushRelabelShortcutDirection {
    /// Adds flow to the augmented directed edge.
    Forward,
    /// Cancels flow on the augmented directed edge.
    Reverse,
}

/// Stable augmented residual identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutArcId {
    /// Augmented directed-edge ordinal.
    pub edge: usize,
    /// Residual direction.
    pub direction: WeightedPushRelabelShortcutDirection,
}

/// Original or Steiner node projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutNodeState {
    /// Dense augmented node ordinal; original nodes precede Steiner roots.
    pub node: usize,
    /// Original node identity, absent for a Steiner root.
    pub original_node: Option<NodeIndex>,
    /// One-level SCC component ordinal.
    pub component: usize,
    /// One-based hierarchy-respecting order, zero for Steiner roots.
    pub order: usize,
    /// Current weighted push-relabel level.
    pub label: u64,
    /// Whether the node remains below the source `9h` death boundary.
    pub alive: bool,
    /// Membership in the selected modified-distance cut.
    pub sparse_cut_side: bool,
    /// Membership in the certified original minimum cut.
    pub source_side: bool,
}

/// One augmented directed edge and its current source-kernel flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutEdgeState {
    /// Stable augmented edge ordinal.
    pub ordinal: usize,
    /// Original identity, absent for shortcut edges.
    pub original_edge: Option<EdgeId>,
    /// Augmented tail ordinal.
    pub from: usize,
    /// Augmented head ordinal.
    pub to: usize,
    /// Capacity after applying the public `psi=1` scale.
    pub capacity: u64,
    /// Current source-kernel flow.
    pub flow: u64,
    /// Original or shortcut role.
    pub kind: WeightedPushRelabelShortcutEdgeKind,
    /// Hierarchy component owning a shortcut, absent for original edges.
    pub shortcut_component: Option<usize>,
    /// Source-defined positive weight.
    pub weight: u64,
}

/// One stable residual direction of an augmented edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutResidualArcState {
    /// Stable residual identity.
    pub id: WeightedPushRelabelShortcutArcId,
    /// Residual tail.
    pub from: usize,
    /// Residual head.
    pub to: usize,
    /// Current residual capacity.
    pub capacity: u64,
    /// Source-defined weight inherited from the augmented edge.
    pub weight: u64,
    /// Persistent admissibility flag from the literal relabel schedule.
    pub admissible: bool,
    /// Membership in the active augmenting path or current primitive inspection.
    pub active: bool,
}

/// Exact source-kernel and bounded-completion counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutMetrics {
    /// Weak hierarchy constructions.
    pub hierarchy_builds: u64,
    /// Steiner roots created.
    pub shortcut_stars: u64,
    /// Directed shortcut edges created.
    pub shortcut_edges: u64,
    /// Literal `level += 1` operations.
    pub relabel_steps: u64,
    /// Residual or augmented arcs inspected by relabel, refresh, path, and completion scans.
    pub primitive_arc_inspections: u64,
    /// Public compressed relabel closures.
    pub relabel_sweeps: u64,
    /// Admissibility flag changes.
    pub admissible_updates: u64,
    /// Source-kernel augmenting paths.
    pub augmentations: u64,
    /// Shortcut residual arcs traversed by those paths.
    pub shortcut_traversals: u64,
    /// Total routed units in the shortcut call.
    pub routed_units: u128,
    /// Residual arcs scanned by modified-distance Dijkstra.
    pub distance_arc_scans: u64,
    /// Candidate distance-layer cuts inspected.
    pub sparse_cut_checks: u64,
    /// Original residual graphs processed by the source-defined outer loop.
    pub residual_rounds: u64,
    /// Literal relabel increments in the exact original-residual calls.
    pub completion_relabel_steps: u64,
    /// Path augmentations in the exact original-residual calls.
    pub completion_augmentations: u64,
    /// Independent terminal checks.
    pub certificate_checks: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete reversible state at one semantic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutSnapshot {
    /// Semantic stage.
    pub stage: WeightedPushRelabelShortcutStage,
    /// Number of hierarchy levels in the bounded construction.
    pub hierarchy_levels: usize,
    /// Source shortcut capacity scale numerator (`psi=1`).
    pub psi_numerator: u64,
    /// Source shortcut capacity scale denominator (`psi=1`).
    pub psi_denominator: u64,
    /// Source height parameter.
    pub height: u64,
    /// Artificial source and sink demand `n^3 U`.
    pub demand: u64,
    /// Amount routed by the shortcut call.
    pub routed: u64,
    /// Numerator of average weighted path length before reduction.
    pub weighted_length: u128,
    /// Positive denominator of average weighted path length.
    pub weighted_length_units: u64,
    /// Selected residual distance-layer threshold.
    pub sparse_cut_level: u64,
    /// Residual capacity crossing the selected layer.
    pub sparse_cut_capacity: u128,
    /// Bottleneck on the active path, zero off augmentation boundaries.
    pub active_bottleneck: u64,
    /// Original and Steiner node projections.
    pub nodes: Vec<WeightedPushRelabelShortcutNodeState>,
    /// Augmented directed edges.
    pub edges: Vec<WeightedPushRelabelShortcutEdgeState>,
    /// Both stable residual directions per augmented edge.
    pub residual_arcs: Vec<WeightedPushRelabelShortcutResidualArcState>,
    /// Active path in augmented residual identities.
    pub active_path: Vec<WeightedPushRelabelShortcutArcId>,
    /// Augmented edge inspected by the current primitive step.
    pub inspected_edge: Option<usize>,
    /// Residual direction when the inspection is direction-specific.
    pub inspected_direction: Option<WeightedPushRelabelShortcutDirection>,
    /// Augmented node whose weighted label was incremented.
    pub active_relabel_node: Option<usize>,
    /// Exact counters.
    pub metrics: WeightedPushRelabelShortcutMetrics,
    /// Exact original-edge flows after the residual rounds.
    pub exact_flows: Option<Vec<u64>>,
}

/// One reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutTraceEvent {
    /// Stable catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: WeightedPushRelabelShortcutSnapshot,
    /// State after the transition.
    pub after: WeightedPushRelabelShortcutSnapshot,
}

/// Certified exact bounded result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutResult {
    /// Original-edge flows in canonical order.
    pub flows: Vec<u64>,
    /// Independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact counters.
    pub metrics: WeightedPushRelabelShortcutMetrics,
    /// Terminal public state.
    pub final_snapshot: WeightedPushRelabelShortcutSnapshot,
}

/// Exact result plus all reversible semantic boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedPushRelabelShortcutTraceResult {
    /// Same result returned by the fast profile.
    pub result: WeightedPushRelabelShortcutResult,
    /// Ready boundary.
    pub base_snapshot: WeightedPushRelabelShortcutSnapshot,
    /// Deterministic transition sequence.
    pub events: Vec<WeightedPushRelabelShortcutTraceEvent>,
    /// Terminal boundary.
    pub final_snapshot: WeightedPushRelabelShortcutSnapshot,
}

/// Admission, construction, residual-completion, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WeightedPushRelabelShortcutError {
    /// Input exceeds the explicit interactive band.
    #[error("graph exceeds weighted push-relabel shortcut admission limits")]
    AdmissionLimit,
    /// Source-kernel domain was violated.
    #[error(
        "weighted push-relabel shortcuts require distinct terminals, zero lower bounds and supplies, no self-loops, and positive capacities"
    )]
    GraphRequirement,
    /// Exact arithmetic overflowed.
    #[error("weighted push-relabel shortcut arithmetic overflow")]
    ArithmeticOverflow,
    /// Literal bounded work ceiling was reached.
    #[error("weighted push-relabel shortcut work limit exceeded")]
    WorkLimit,
    /// A hierarchy, shortcut, label, path, or cut invariant failed.
    #[error("weighted push-relabel shortcut invariant failed")]
    Invariant,
    /// Independent terminal verification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Public trace failed deterministic re-execution.
    #[error("weighted push-relabel shortcut trace verification failed")]
    TraceVerification,
}

/// Solves the exact bounded realization.
///
/// # Errors
///
/// Rejects unsupported graphs, work exhaustion, arithmetic/invariant failure,
/// exact-repair failure, or a rejected independent certificate.
pub fn solve_weighted_push_relabel_shortcut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<WeightedPushRelabelShortcutResult, WeightedPushRelabelShortcutError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records hierarchy, shortcut, relabel, path, sparse-cut, and repair stages.
///
/// # Errors
///
/// Returns the same failures as [`solve_weighted_push_relabel_shortcut`].
pub fn trace_weighted_push_relabel_shortcut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<WeightedPushRelabelShortcutTraceResult, WeightedPushRelabelShortcutError> {
    let run = solve_internal(graph, source, sink, true)?;
    Ok(WeightedPushRelabelShortcutTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    })
}

/// Re-executes the deterministic bounded construction and checks every state.
///
/// # Errors
///
/// Rejects malformed snapshots, broken chains, or any re-execution divergence.
pub fn verify_weighted_push_relabel_shortcut_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &WeightedPushRelabelShortcutTraceResult,
) -> Result<(), WeightedPushRelabelShortcutError> {
    validate_snapshot(graph, source, sink, &trace.base_snapshot)?;
    validate_snapshot(graph, source, sink, &trace.final_snapshot)?;
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != "weighted-push-relabel" || &event.before != cursor {
            return Err(WeightedPushRelabelShortcutError::TraceVerification);
        }
        let inspection_delta = event
            .after
            .metrics
            .primitive_arc_inspections
            .checked_sub(event.before.metrics.primitive_arc_inspections)
            .ok_or(WeightedPushRelabelShortcutError::TraceVerification)?;
        let inspection_boundary = matches!(
            event.after.stage,
            WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
                | WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
        );
        if inspection_boundary != (inspection_delta > 0)
            || inspection_delta > WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE
            || (inspection_boundary && event.after.inspected_edge.is_none())
        {
            return Err(WeightedPushRelabelShortcutError::TraceVerification);
        }
        validate_snapshot(graph, source, sink, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot || trace.result.final_snapshot != trace.final_snapshot {
        return Err(WeightedPushRelabelShortcutError::TraceVerification);
    }
    if trace_weighted_push_relabel_shortcut(graph, source, sink)? != *trace {
        return Err(WeightedPushRelabelShortcutError::TraceVerification);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct WorkEdge {
    original_edge: Option<EdgeId>,
    from: usize,
    to: usize,
    capacity: u64,
    flow: u64,
    kind: WeightedPushRelabelShortcutEdgeKind,
    shortcut_component: Option<usize>,
    weight: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OriginalResidualEdge {
    original_ordinal: usize,
    original_direction: WeightedPushRelabelShortcutDirection,
    from: usize,
    to: usize,
    capacity: u64,
    flow: u64,
    weight: u64,
}

#[derive(Clone, Debug)]
struct OriginalResidualKernel {
    edges: Vec<OriginalResidualEdge>,
    labels: Vec<u64>,
    alive: Vec<bool>,
    admissible: Vec<bool>,
    source: usize,
    sink: usize,
    height: u64,
    demand: u64,
    routed: u64,
    relabel_steps: u64,
    primitive_arc_inspections: u64,
    augmentations: u64,
    base_flows: Vec<u64>,
    trace_primitive_offset: u64,
    trace_relabel_offset: u64,
    trace: bool,
    last_inspected_original_ordinal: Option<usize>,
    last_inspected_direction: Option<WeightedPushRelabelShortcutDirection>,
    checkpoints: Vec<CompletionCheckpoint>,
}

struct OriginalResidualKernelConfig {
    node_count: usize,
    source: usize,
    sink: usize,
    height: u64,
    demand: u64,
    base_flows: Vec<u64>,
    trace_primitive_offset: u64,
    trace_relabel_offset: u64,
    trace: bool,
}

#[derive(Clone, Debug)]
enum CompletionCheckpointKind {
    Inspect {
        original_ordinal: usize,
        direction: Option<WeightedPushRelabelShortcutDirection>,
    },
    Relabel {
        node: usize,
    },
    Augment {
        path: Vec<WeightedPushRelabelShortcutArcId>,
        bottleneck: u64,
    },
}

#[derive(Clone, Debug)]
struct CompletionCheckpoint {
    kind: CompletionCheckpointKind,
    flows: Vec<u64>,
    labels: Vec<u64>,
    alive: Vec<bool>,
    primitive_arc_inspections: u64,
    relabel_steps: u64,
    augmentations: u64,
}

#[derive(Clone, Debug)]
struct Hierarchy {
    component: Vec<usize>,
    order: Vec<usize>,
    component_nodes: Vec<Vec<usize>>,
}

struct InternalRun {
    result: WeightedPushRelabelShortcutResult,
    base_snapshot: WeightedPushRelabelShortcutSnapshot,
    events: Vec<WeightedPushRelabelShortcutTraceEvent>,
    final_snapshot: WeightedPushRelabelShortcutSnapshot,
}

struct Runner<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    hierarchy: Hierarchy,
    edges: Vec<WorkEdge>,
    labels: Vec<u64>,
    alive: Vec<bool>,
    admissible: Vec<bool>,
    sparse_cut_side: Vec<bool>,
    source_side: Vec<bool>,
    stage: WeightedPushRelabelShortcutStage,
    height: u64,
    demand: u64,
    routed: u64,
    weighted_length: u128,
    sparse_cut_level: u64,
    sparse_cut_capacity: u128,
    active_path: Vec<WeightedPushRelabelShortcutArcId>,
    active_bottleneck: u64,
    inspected_edge: Option<usize>,
    inspected_direction: Option<WeightedPushRelabelShortcutDirection>,
    active_relabel_node: Option<usize>,
    exact_flows: Option<Vec<u64>>,
    metrics: WeightedPushRelabelShortcutMetrics,
    trace: bool,
    base_snapshot: WeightedPushRelabelShortcutSnapshot,
    current_snapshot: WeightedPushRelabelShortcutSnapshot,
    events: Vec<WeightedPushRelabelShortcutTraceEvent>,
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: bool,
) -> Result<InternalRun, WeightedPushRelabelShortcutError> {
    validate_input(graph, source, sink)?;
    let hierarchy = build_hierarchy(graph)?;
    let mut edges = graph
        .edges()
        .iter()
        .map(|edge| WorkEdge {
            original_edge: Some(edge.id().clone()),
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            capacity: edge.capacity(),
            flow: 0,
            kind: WeightedPushRelabelShortcutEdgeKind::Original,
            shortcut_component: None,
            weight: hierarchy.order[edge.from().as_usize()]
                .abs_diff(hierarchy.order[edge.to().as_usize()]) as u64,
        })
        .collect::<Vec<_>>();
    add_shortcuts(graph, &hierarchy, &mut edges)?;
    let node_count = graph.nodes().len() + shortcut_star_count(&hierarchy, graph);
    let (height, demand) = source_kernel_parameters(graph)?;
    let empty = empty_snapshot(graph);
    let mut runner = Runner {
        graph,
        source,
        sink,
        hierarchy,
        admissible: vec![false; edges.len() * 2],
        edges,
        labels: vec![0; node_count],
        alive: vec![true; node_count],
        sparse_cut_side: vec![false; node_count],
        source_side: vec![false; node_count],
        stage: WeightedPushRelabelShortcutStage::Ready,
        height,
        demand,
        routed: 0,
        weighted_length: 0,
        sparse_cut_level: 0,
        sparse_cut_capacity: 0,
        active_path: Vec::new(),
        active_bottleneck: 0,
        inspected_edge: None,
        inspected_direction: None,
        active_relabel_node: None,
        exact_flows: None,
        metrics: WeightedPushRelabelShortcutMetrics::default(),
        trace,
        base_snapshot: empty.clone(),
        current_snapshot: empty,
        events: Vec::new(),
    };
    runner.run_source_kernel()?;
    let exact_flows = complete_with_weighted_residual_rounds(&mut runner)?;
    for edge in &mut runner.edges {
        edge.flow = if let Some(original_edge) = &edge.original_edge {
            let ordinal = graph
                .edge_index(original_edge)
                .ok_or(WeightedPushRelabelShortcutError::Invariant)?
                .as_usize();
            *exact_flows
                .get(ordinal)
                .ok_or(WeightedPushRelabelShortcutError::Invariant)?
        } else if edge.kind == WeightedPushRelabelShortcutEdgeKind::Shortcut {
            0
        } else {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        };
    }
    runner.admissible.fill(false);
    runner.active_path.clear();
    runner.active_bottleneck = 0;
    runner.exact_flows = Some(exact_flows.clone());
    let certificate = check_max_flow(graph, source, sink, &exact_flows)?;
    runner.install_source_side(&certificate);
    runner.emit(WeightedPushRelabelShortcutStage::CompleteResidualRounds)?;
    runner.metrics.certificate_checks = runner
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    runner.emit(WeightedPushRelabelShortcutStage::CheckCertificate)?;
    runner.emit(WeightedPushRelabelShortcutStage::Optimal)?;
    let final_snapshot = runner.current_snapshot.clone();
    let result = WeightedPushRelabelShortcutResult {
        flows: exact_flows,
        certificate,
        metrics: runner.metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot: runner.base_snapshot,
        events: runner.events,
        final_snapshot,
    })
}

fn source_kernel_parameters(
    graph: &FlowNetwork,
) -> Result<(u64, u64), WeightedPushRelabelShortcutError> {
    let max_capacity = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .ok_or(WeightedPushRelabelShortcutError::GraphRequirement)?;
    let demand = u64::try_from(graph.nodes().len())
        .ok()
        .and_then(|n| n.checked_pow(3))
        .and_then(|n| n.checked_mul(max_capacity))
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    let total_capacity = graph.edges().iter().try_fold(0_u64, |sum, edge| {
        sum.checked_add(edge.capacity())
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)
    })?;
    let log_m = u64::from(total_capacity.max(2).ilog2() + 1);
    let n = u64::try_from(graph.nodes().len())
        .map_err(|_| WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    let height = n
        .checked_mul(
            6_u64
                .checked_add(
                    100_u64
                        .checked_mul(log_m)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?,
                )
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?,
        )
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    Ok((height, demand))
}

/// Runs the source outer loop on the original residual graph.  The full paper
/// unfolds each approximate shortcut flow before applying it.  This bounded
/// one-level hierarchy has no congestion-preserving unfolding oracle, so the
/// same weighted push-relabel kernel is invoked directly on each original
/// residual graph.  With `h = n * max(w)`, every simple residual path is in
/// range; a zero routed value is still accepted only after the independent
/// maximum-flow certificate succeeds.
fn complete_with_weighted_residual_rounds(
    runner: &mut Runner<'_>,
) -> Result<Vec<u64>, WeightedPushRelabelShortcutError> {
    let graph = runner.graph;
    let hierarchy = runner.hierarchy.clone();
    let mut flows = vec![0_u64; graph.edges().len()];
    let demand = graph.edges().iter().try_fold(0_u64, |sum, edge| {
        sum.checked_add(edge.capacity())
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)
    })?;
    let max_rounds = demand
        .checked_add(1)
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;

    loop {
        runner.metrics.residual_rounds = runner
            .metrics
            .residual_rounds
            .checked_add(1)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        if runner.metrics.residual_rounds > max_rounds {
            return Err(WeightedPushRelabelShortcutError::WorkLimit);
        }
        let residual_edges = build_original_residual_edges(graph, &hierarchy, &flows)?;
        if residual_edges.is_empty() {
            runner.install_completion_state(&flows, None, None)?;
            runner.active_path.clear();
            runner.active_bottleneck = 0;
            runner.emit(WeightedPushRelabelShortcutStage::CompletionResidualRound)?;
            break;
        }
        let max_weight = residual_edges
            .iter()
            .map(|edge| edge.weight)
            .max()
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        let height = u64::try_from(graph.nodes().len())
            .ok()
            .and_then(|n| n.checked_mul(max_weight))
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        let kernel =
            run_original_residual_kernel(runner, residual_edges, height.max(1), demand, &flows)?;
        for edge in &kernel.edges {
            let original_flow = flows
                .get_mut(edge.original_ordinal)
                .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            match edge.original_direction {
                WeightedPushRelabelShortcutDirection::Forward => {
                    *original_flow = original_flow
                        .checked_add(edge.flow)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                }
                WeightedPushRelabelShortcutDirection::Reverse => {
                    *original_flow = original_flow
                        .checked_sub(edge.flow)
                        .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
                }
            }
        }
        if flows
            .iter()
            .zip(graph.edges())
            .any(|(flow, edge)| *flow > edge.capacity())
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        runner.install_completion_state(&flows, Some(&kernel.labels), Some(&kernel.alive))?;
        runner.active_path.clear();
        runner.active_bottleneck = 0;
        runner.emit(WeightedPushRelabelShortcutStage::CompletionResidualRound)?;
        if kernel.routed == 0 {
            break;
        }
    }
    Ok(flows)
}

fn run_original_residual_kernel(
    runner: &mut Runner<'_>,
    residual_edges: Vec<OriginalResidualEdge>,
    height: u64,
    demand: u64,
    flows: &[u64],
) -> Result<OriginalResidualKernel, WeightedPushRelabelShortcutError> {
    let primitive_base = runner.metrics.primitive_arc_inspections;
    let relabel_base = runner.metrics.completion_relabel_steps;
    let augmentation_base = runner.metrics.completion_augmentations;
    runner.height = height;
    let mut kernel = OriginalResidualKernel::new(
        residual_edges,
        OriginalResidualKernelConfig {
            node_count: runner.graph.nodes().len(),
            source: runner.source.as_usize(),
            sink: runner.sink.as_usize(),
            height,
            demand,
            base_flows: flows.to_vec(),
            trace_primitive_offset: primitive_base,
            trace_relabel_offset: relabel_base,
            trace: true,
        },
    );
    kernel.run()?;
    for checkpoint in &kernel.checkpoints {
        runner.publish_completion_checkpoint(
            checkpoint,
            primitive_base,
            relabel_base,
            augmentation_base,
        )?;
    }
    runner.metrics.primitive_arc_inspections = primitive_base
        .checked_add(kernel.primitive_arc_inspections)
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    runner.metrics.completion_relabel_steps = relabel_base
        .checked_add(kernel.relabel_steps)
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    runner.metrics.completion_augmentations =
        augmentation_base
            .checked_add(kernel.augmentations)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    if runner.metrics.completion_relabel_steps > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_RELABEL_STEPS
        || runner.metrics.completion_augmentations
            > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_AUGMENTATIONS
    {
        return Err(WeightedPushRelabelShortcutError::WorkLimit);
    }
    Ok(kernel)
}

fn build_original_residual_edges(
    graph: &FlowNetwork,
    hierarchy: &Hierarchy,
    flows: &[u64],
) -> Result<Vec<OriginalResidualEdge>, WeightedPushRelabelShortcutError> {
    if flows.len() != graph.edges().len() {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    let mut residual = Vec::with_capacity(graph.edges().len() * 2);
    for (original_ordinal, (edge, flow)) in graph.edges().iter().zip(flows).enumerate() {
        let weight = u64::try_from(
            hierarchy.order[edge.from().as_usize()].abs_diff(hierarchy.order[edge.to().as_usize()]),
        )
        .map_err(|_| WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        if weight == 0 || *flow > edge.capacity() {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        if *flow < edge.capacity() {
            residual.push(OriginalResidualEdge {
                original_ordinal,
                original_direction: WeightedPushRelabelShortcutDirection::Forward,
                from: edge.from().as_usize(),
                to: edge.to().as_usize(),
                capacity: edge.capacity() - *flow,
                flow: 0,
                weight,
            });
        }
        if *flow > 0 {
            residual.push(OriginalResidualEdge {
                original_ordinal,
                original_direction: WeightedPushRelabelShortcutDirection::Reverse,
                from: edge.to().as_usize(),
                to: edge.from().as_usize(),
                capacity: *flow,
                flow: 0,
                weight,
            });
        }
    }
    Ok(residual)
}

impl OriginalResidualKernel {
    fn new(edges: Vec<OriginalResidualEdge>, config: OriginalResidualKernelConfig) -> Self {
        Self {
            admissible: vec![false; edges.len() * 2],
            edges,
            labels: vec![0; config.node_count],
            alive: vec![true; config.node_count],
            source: config.source,
            sink: config.sink,
            height: config.height,
            demand: config.demand,
            routed: 0,
            relabel_steps: 0,
            primitive_arc_inspections: 0,
            augmentations: 0,
            base_flows: config.base_flows,
            trace_primitive_offset: config.trace_primitive_offset,
            trace_relabel_offset: config.trace_relabel_offset,
            trace: config.trace,
            last_inspected_original_ordinal: None,
            last_inspected_direction: None,
            checkpoints: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        while self.alive[self.source] && self.routed < self.demand {
            self.relabel_closure()?;
            if !self.alive[self.source] {
                break;
            }
            let path = self.admissible_path()?;
            self.flush_primitive_checkpoint()?;
            let bottleneck = path
                .iter()
                .try_fold(self.demand - self.routed, |limit, arc| {
                    Ok::<_, WeightedPushRelabelShortcutError>(
                        limit.min(self.residual_capacity(*arc)?),
                    )
                })?;
            if path.is_empty() || bottleneck == 0 {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
            for arc in &path {
                self.augment(*arc, bottleneck)?;
            }
            self.routed = self
                .routed
                .checked_add(bottleneck)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.augmentations = self
                .augmentations
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if self.augmentations > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_AUGMENTATIONS {
                return Err(WeightedPushRelabelShortcutError::WorkLimit);
            }
            self.record_augment_checkpoint(&path, bottleneck)?;
        }
        self.flush_primitive_checkpoint()?;
        Ok(())
    }

    fn relabel_closure(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        loop {
            let mut candidate = None;
            for node in 0..self.labels.len() {
                if self.alive[node] && node != self.sink && !self.has_admissible_outgoing(node)? {
                    candidate = Some(node);
                    break;
                }
            }
            let Some(node) = candidate else {
                self.flush_primitive_checkpoint()?;
                return Ok(());
            };
            let next_relabel = self
                .relabel_steps
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            let global_next_relabel = self
                .trace_relabel_offset
                .checked_add(next_relabel)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if self.trace
                && should_publish_scalar_checkpoint(
                    global_next_relabel,
                    WEIGHTED_PUSH_RELABEL_SHORTCUT_RELABEL_CHECKPOINT_STRIDE,
                )
            {
                self.flush_primitive_checkpoint()?;
            }
            self.labels[node] = self.labels[node]
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.relabel_steps = self
                .relabel_steps
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if self.relabel_steps > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_RELABEL_STEPS {
                return Err(WeightedPushRelabelShortcutError::WorkLimit);
            }
            if self.labels[node] > self.height.saturating_mul(9) {
                self.alive[node] = false;
                self.record_relabel_checkpoint(node)?;
                continue;
            }
            self.record_relabel_checkpoint(node)?;
            for ordinal in 0..self.edges.len() {
                self.inspect_primitive_arc(ordinal, None)?;
                let edge = self.edges[ordinal];
                if (edge.from == node || edge.to == node)
                    && self.labels[node].is_multiple_of(edge.weight)
                {
                    self.refresh_arc(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Forward,
                    })?;
                    self.refresh_arc(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Reverse,
                    })?;
                }
            }
        }
    }

    fn refresh_arc(
        &mut self,
        arc: WeightedPushRelabelShortcutArcId,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        self.inspect_primitive_arc(arc.edge, Some(arc.direction))?;
        let (from, to) = self.arc_endpoints(arc);
        let weight = self.edges[arc.edge].weight;
        self.admissible[arc_index(arc)] = self.residual_capacity(arc)? > 0
            && self.labels[from].saturating_sub(self.labels[to]) >= weight.saturating_mul(2);
        Ok(())
    }

    fn has_admissible_outgoing(
        &mut self,
        node: usize,
    ) -> Result<bool, WeightedPushRelabelShortcutError> {
        for ordinal in 0..self.edges.len() {
            self.inspect_primitive_arc(ordinal, None)?;
            let edge = self.edges[ordinal];
            if (edge.from == node && self.admissible[ordinal * 2] && edge.flow < edge.capacity)
                || (edge.to == node && self.admissible[ordinal * 2 + 1] && edge.flow > 0)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn admissible_path(
        &mut self,
    ) -> Result<Vec<WeightedPushRelabelShortcutArcId>, WeightedPushRelabelShortcutError> {
        let mut node = self.source;
        let mut path = Vec::new();
        let mut seen = vec![false; self.labels.len()];
        while node != self.sink {
            if seen[node] || !self.alive[node] {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
            seen[node] = true;
            let mut next = None;
            for ordinal in 0..self.edges.len() {
                self.inspect_primitive_arc(ordinal, None)?;
                let edge = self.edges[ordinal];
                if edge.from == node && self.admissible[ordinal * 2] && edge.flow < edge.capacity {
                    next = Some(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Forward,
                    });
                } else if edge.to == node && self.admissible[ordinal * 2 + 1] && edge.flow > 0 {
                    next = Some(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Reverse,
                    });
                }
                if next.is_some() {
                    break;
                }
            }
            let Some(arc) = next else {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            };
            node = self.arc_endpoints(arc).1;
            path.push(arc);
            if path.len() > self.labels.len() {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
        }
        Ok(path)
    }

    fn inspect_primitive_arc(
        &mut self,
        edge: usize,
        direction: Option<WeightedPushRelabelShortcutDirection>,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        let residual = *self
            .edges
            .get(edge)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        self.primitive_arc_inspections = self
            .primitive_arc_inspections
            .checked_add(1)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        let global = self
            .trace_primitive_offset
            .checked_add(self.primitive_arc_inspections)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.last_inspected_original_ordinal = Some(residual.original_ordinal);
        self.last_inspected_direction = direction.map(|kernel_direction| {
            compose_original_residual_direction(residual.original_direction, kernel_direction)
        });
        let published = self
            .trace_primitive_offset
            .checked_add(self.published_primitive_arc_inspections())
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        if self.trace
            && should_publish_weighted_checkpoint(
                global,
                published,
                WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE,
            )
        {
            self.flush_primitive_checkpoint()?;
        }
        Ok(())
    }

    fn published_primitive_arc_inspections(&self) -> u64 {
        self.checkpoints
            .last()
            .map_or(0, |checkpoint| checkpoint.primitive_arc_inspections)
    }

    fn flush_primitive_checkpoint(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        if !self.trace
            || self.primitive_arc_inspections == self.published_primitive_arc_inspections()
        {
            return Ok(());
        }
        let original_ordinal = self
            .last_inspected_original_ordinal
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        self.record_checkpoint(CompletionCheckpointKind::Inspect {
            original_ordinal,
            direction: self.last_inspected_direction,
        })
    }

    fn record_relabel_checkpoint(
        &mut self,
        node: usize,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        let global = self
            .trace_relabel_offset
            .checked_add(self.relabel_steps)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        if !self.trace
            || !should_publish_scalar_checkpoint(
                global,
                WEIGHTED_PUSH_RELABEL_SHORTCUT_RELABEL_CHECKPOINT_STRIDE,
            )
        {
            return Ok(());
        }
        if self.primitive_arc_inspections != self.published_primitive_arc_inspections() {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        self.record_checkpoint(CompletionCheckpointKind::Relabel { node })
    }

    fn record_augment_checkpoint(
        &mut self,
        path: &[WeightedPushRelabelShortcutArcId],
        bottleneck: u64,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        if !self.trace {
            return Ok(());
        }
        let original_path = path
            .iter()
            .map(|arc| {
                let residual = self
                    .edges
                    .get(arc.edge)
                    .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
                Ok(WeightedPushRelabelShortcutArcId {
                    edge: residual.original_ordinal,
                    direction: compose_original_residual_direction(
                        residual.original_direction,
                        arc.direction,
                    ),
                })
            })
            .collect::<Result<Vec<_>, WeightedPushRelabelShortcutError>>()?;
        self.record_checkpoint(CompletionCheckpointKind::Augment {
            path: original_path,
            bottleneck,
        })
    }

    fn record_checkpoint(
        &mut self,
        kind: CompletionCheckpointKind,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        self.checkpoints.push(CompletionCheckpoint {
            kind,
            flows: self.project_original_flows()?,
            labels: self.labels.clone(),
            alive: self.alive.clone(),
            primitive_arc_inspections: self.primitive_arc_inspections,
            relabel_steps: self.relabel_steps,
            augmentations: self.augmentations,
        });
        Ok(())
    }

    fn project_original_flows(&self) -> Result<Vec<u64>, WeightedPushRelabelShortcutError> {
        let mut flows = self.base_flows.clone();
        for edge in &self.edges {
            let flow = flows
                .get_mut(edge.original_ordinal)
                .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            match edge.original_direction {
                WeightedPushRelabelShortcutDirection::Forward => {
                    *flow = flow
                        .checked_add(edge.flow)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                }
                WeightedPushRelabelShortcutDirection::Reverse => {
                    *flow = flow
                        .checked_sub(edge.flow)
                        .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
                }
            }
        }
        Ok(flows)
    }

    fn augment(
        &mut self,
        arc: WeightedPushRelabelShortcutArcId,
        amount: u64,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        let edge = self
            .edges
            .get_mut(arc.edge)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => {
                edge.flow = edge
                    .flow
                    .checked_add(amount)
                    .filter(|flow| *flow <= edge.capacity)
                    .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            }
            WeightedPushRelabelShortcutDirection::Reverse => {
                edge.flow = edge
                    .flow
                    .checked_sub(amount)
                    .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            }
        }
        if self.residual_capacity(arc)? == 0 {
            self.admissible[arc_index(arc)] = false;
        }
        Ok(())
    }

    fn residual_capacity(
        &self,
        arc: WeightedPushRelabelShortcutArcId,
    ) -> Result<u64, WeightedPushRelabelShortcutError> {
        let edge = self
            .edges
            .get(arc.edge)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => edge
                .capacity
                .checked_sub(edge.flow)
                .ok_or(WeightedPushRelabelShortcutError::Invariant),
            WeightedPushRelabelShortcutDirection::Reverse => Ok(edge.flow),
        }
    }

    fn arc_endpoints(&self, arc: WeightedPushRelabelShortcutArcId) -> (usize, usize) {
        let edge = self.edges[arc.edge];
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => (edge.from, edge.to),
            WeightedPushRelabelShortcutDirection::Reverse => (edge.to, edge.from),
        }
    }
}

const fn compose_original_residual_direction(
    original: WeightedPushRelabelShortcutDirection,
    kernel: WeightedPushRelabelShortcutDirection,
) -> WeightedPushRelabelShortcutDirection {
    match (original, kernel) {
        (
            WeightedPushRelabelShortcutDirection::Forward,
            WeightedPushRelabelShortcutDirection::Forward,
        )
        | (
            WeightedPushRelabelShortcutDirection::Reverse,
            WeightedPushRelabelShortcutDirection::Reverse,
        ) => WeightedPushRelabelShortcutDirection::Forward,
        (
            WeightedPushRelabelShortcutDirection::Forward,
            WeightedPushRelabelShortcutDirection::Reverse,
        )
        | (
            WeightedPushRelabelShortcutDirection::Reverse,
            WeightedPushRelabelShortcutDirection::Forward,
        ) => WeightedPushRelabelShortcutDirection::Reverse,
    }
}

impl Runner<'_> {
    fn publish_completion_checkpoint(
        &mut self,
        checkpoint: &CompletionCheckpoint,
        primitive_base: u64,
        relabel_base: u64,
        augmentation_base: u64,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        self.metrics.primitive_arc_inspections = primitive_base
            .checked_add(checkpoint.primitive_arc_inspections)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.metrics.completion_relabel_steps = relabel_base
            .checked_add(checkpoint.relabel_steps)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.metrics.completion_augmentations = augmentation_base
            .checked_add(checkpoint.augmentations)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.install_completion_state(
            &checkpoint.flows,
            Some(&checkpoint.labels),
            Some(&checkpoint.alive),
        )?;
        self.active_path.clear();
        self.active_bottleneck = 0;
        self.inspected_edge = None;
        self.inspected_direction = None;
        self.active_relabel_node = None;
        self.stage = match &checkpoint.kind {
            CompletionCheckpointKind::Inspect {
                original_ordinal,
                direction,
            } => {
                self.inspected_edge = Some(*original_ordinal);
                self.inspected_direction = *direction;
                WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
            }
            CompletionCheckpointKind::Relabel { node } => {
                self.active_relabel_node = Some(*node);
                WeightedPushRelabelShortcutStage::CompletionRelabelCheckpoint
            }
            CompletionCheckpointKind::Augment { path, bottleneck } => {
                self.active_path.clone_from(path);
                self.active_bottleneck = *bottleneck;
                for arc in path {
                    self.admissible[arc_index(*arc)] = self.residual_capacity(*arc)? > 0;
                }
                WeightedPushRelabelShortcutStage::CompletionAugmentPath
            }
        };
        self.commit_event()
    }

    fn install_completion_state(
        &mut self,
        flows: &[u64],
        labels: Option<&[u64]>,
        alive: Option<&[bool]>,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        if flows.len() != self.graph.edges().len()
            || labels.is_some_and(|values| values.len() != self.graph.nodes().len())
            || alive.is_some_and(|values| values.len() != self.graph.nodes().len())
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        for (ordinal, edge) in self.edges.iter_mut().enumerate() {
            edge.flow = if ordinal < flows.len() {
                flows[ordinal]
            } else {
                0
            };
        }
        if let Some(values) = labels {
            self.labels[..values.len()].copy_from_slice(values);
        }
        if let Some(values) = alive {
            self.alive[..values.len()].copy_from_slice(values);
        }
        for node in self.graph.nodes().len()..self.labels.len() {
            self.labels[node] = 0;
            self.alive[node] = true;
        }
        self.admissible.fill(false);
        Ok(())
    }

    fn run_source_kernel(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        self.metrics.hierarchy_builds = 1;
        self.emit(WeightedPushRelabelShortcutStage::BuildWeakHierarchy)?;
        self.metrics.shortcut_stars =
            u64::try_from(shortcut_star_count(&self.hierarchy, self.graph))
                .map_err(|_| WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.metrics.shortcut_edges = u64::try_from(
            self.edges
                .iter()
                .filter(|edge| edge.kind == WeightedPushRelabelShortcutEdgeKind::Shortcut)
                .count(),
        )
        .map_err(|_| WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.emit(WeightedPushRelabelShortcutStage::BuildShortcutGraph)?;
        self.emit(WeightedPushRelabelShortcutStage::AssignWeights)?;
        self.emit(WeightedPushRelabelShortcutStage::InitializeDemand)?;

        while self.alive[self.source.as_usize()] && self.routed < self.demand {
            self.active_path.clear();
            self.active_bottleneck = 0;
            let relabeled = self.relabel_closure()?;
            if relabeled > 0 {
                self.metrics.relabel_sweeps = self
                    .metrics
                    .relabel_sweeps
                    .checked_add(1)
                    .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                self.active_path.clear();
                self.active_bottleneck = 0;
                self.emit(WeightedPushRelabelShortcutStage::RelabelSweep)?;
            }
            if !self.alive[self.source.as_usize()] {
                break;
            }
            let path = self.trace_admissible_path()?;
            self.flush_primitive_arc_checkpoint()?;
            if path.is_empty() {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
            let bottleneck = path
                .iter()
                .try_fold(self.demand - self.routed, |current, arc| {
                    Ok::<_, WeightedPushRelabelShortcutError>(
                        current.min(self.residual_capacity(*arc)?),
                    )
                })?;
            if bottleneck == 0 {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
            let mut path_weight = 0_u64;
            for arc in &path {
                path_weight = path_weight
                    .checked_add(self.edges[arc.edge].weight)
                    .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                if self.edges[arc.edge].kind == WeightedPushRelabelShortcutEdgeKind::Shortcut {
                    self.metrics.shortcut_traversals = self
                        .metrics
                        .shortcut_traversals
                        .checked_add(1)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                }
                self.augment_arc(*arc, bottleneck)?;
            }
            self.routed = self
                .routed
                .checked_add(bottleneck)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.weighted_length = self
                .weighted_length
                .checked_add(u128::from(path_weight) * u128::from(bottleneck))
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.metrics.augmentations = self
                .metrics
                .augmentations
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if self.metrics.augmentations > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_AUGMENTATIONS {
                return Err(WeightedPushRelabelShortcutError::WorkLimit);
            }
            self.metrics.routed_units = self
                .metrics
                .routed_units
                .checked_add(u128::from(bottleneck))
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.active_path = path;
            self.active_bottleneck = bottleneck;
            self.emit(WeightedPushRelabelShortcutStage::AugmentPath)?;
        }

        self.active_path.clear();
        self.active_bottleneck = 0;
        self.emit(WeightedPushRelabelShortcutStage::MeasureShortFlow)?;
        let distances = self.modified_distances()?;
        self.emit(WeightedPushRelabelShortcutStage::ComputeDistanceLayers)?;
        self.select_sparse_cut(&distances)?;
        self.emit(WeightedPushRelabelShortcutStage::SelectSparseCut)?;
        Ok(())
    }

    fn relabel_closure(&mut self) -> Result<u64, WeightedPushRelabelShortcutError> {
        let mut local_steps = 0_u64;
        loop {
            let mut candidate = None;
            for node in 0..self.labels.len() {
                if self.alive[node]
                    && node != self.sink.as_usize()
                    && !self.has_admissible_outgoing(node)?
                {
                    candidate = Some(node);
                    break;
                }
            }
            let Some(node) = candidate else {
                self.flush_primitive_arc_checkpoint()?;
                return Ok(local_steps);
            };
            let next_relabel = self
                .metrics
                .relabel_steps
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if should_publish_scalar_checkpoint(
                next_relabel,
                WEIGHTED_PUSH_RELABEL_SHORTCUT_RELABEL_CHECKPOINT_STRIDE,
            ) {
                self.flush_primitive_arc_checkpoint()?;
            }
            self.labels[node] = self.labels[node]
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            local_steps = local_steps
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            self.metrics.relabel_steps = self
                .metrics
                .relabel_steps
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if self.metrics.relabel_steps > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_RELABEL_STEPS {
                return Err(WeightedPushRelabelShortcutError::WorkLimit);
            }
            if self.labels[node] > self.height.saturating_mul(9) {
                self.alive[node] = false;
                self.emit_relabel_checkpoint(node)?;
                continue;
            }
            self.emit_relabel_checkpoint(node)?;
            for ordinal in 0..self.edges.len() {
                self.inspect_primitive_arc(ordinal, None)?;
                let edge = &self.edges[ordinal];
                if (edge.from == node || edge.to == node)
                    && self.labels[node].is_multiple_of(edge.weight)
                {
                    self.refresh_arc(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Forward,
                    })?;
                    self.refresh_arc(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Reverse,
                    })?;
                }
            }
        }
    }

    fn refresh_arc(
        &mut self,
        arc: WeightedPushRelabelShortcutArcId,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        self.inspect_primitive_arc(arc.edge, Some(arc.direction))?;
        let (from, to) = self.arc_endpoints(arc);
        let weight = self.edges[arc.edge].weight;
        let should = self.residual_capacity(arc)? > 0
            && self.labels[from].saturating_sub(self.labels[to]) >= weight.saturating_mul(2);
        let index = arc_index(arc);
        if self.admissible[index] != should {
            self.admissible[index] = should;
            self.metrics.admissible_updates = self
                .metrics
                .admissible_updates
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        }
        Ok(())
    }

    fn has_admissible_outgoing(
        &mut self,
        node: usize,
    ) -> Result<bool, WeightedPushRelabelShortcutError> {
        for ordinal in 0..self.edges.len() {
            self.inspect_primitive_arc(ordinal, None)?;
            let edge = &self.edges[ordinal];
            if (edge.from == node && self.admissible[ordinal * 2] && edge.flow < edge.capacity)
                || (edge.to == node && self.admissible[ordinal * 2 + 1] && edge.flow > 0)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn trace_admissible_path(
        &mut self,
    ) -> Result<Vec<WeightedPushRelabelShortcutArcId>, WeightedPushRelabelShortcutError> {
        let mut node = self.source.as_usize();
        let mut path = Vec::new();
        let mut seen = vec![false; self.labels.len()];
        while node != self.sink.as_usize() {
            if seen[node] || !self.alive[node] {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
            seen[node] = true;
            let mut next = None;
            for ordinal in 0..self.edges.len() {
                self.inspect_primitive_arc(ordinal, None)?;
                let edge = &self.edges[ordinal];
                if edge.from == node && self.admissible[ordinal * 2] && edge.flow < edge.capacity {
                    next = Some(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Forward,
                    });
                } else if edge.to == node && self.admissible[ordinal * 2 + 1] && edge.flow > 0 {
                    next = Some(WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction: WeightedPushRelabelShortcutDirection::Reverse,
                    });
                }
                if next.is_some() {
                    break;
                }
            }
            let Some(arc) = next else {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            };
            node = self.arc_endpoints(arc).1;
            path.push(arc);
            if path.len() > self.labels.len() {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
        }
        Ok(path)
    }

    fn augment_arc(
        &mut self,
        arc: WeightedPushRelabelShortcutArcId,
        amount: u64,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        let edge = self
            .edges
            .get_mut(arc.edge)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => {
                edge.flow = edge
                    .flow
                    .checked_add(amount)
                    .filter(|flow| *flow <= edge.capacity)
                    .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            }
            WeightedPushRelabelShortcutDirection::Reverse => {
                edge.flow = edge
                    .flow
                    .checked_sub(amount)
                    .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
            }
        }
        if self.residual_capacity(arc)? == 0 {
            self.admissible[arc_index(arc)] = false;
        }
        Ok(())
    }

    fn residual_capacity(
        &self,
        arc: WeightedPushRelabelShortcutArcId,
    ) -> Result<u64, WeightedPushRelabelShortcutError> {
        let edge = self
            .edges
            .get(arc.edge)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => edge
                .capacity
                .checked_sub(edge.flow)
                .ok_or(WeightedPushRelabelShortcutError::Invariant),
            WeightedPushRelabelShortcutDirection::Reverse => Ok(edge.flow),
        }
    }

    fn arc_endpoints(&self, arc: WeightedPushRelabelShortcutArcId) -> (usize, usize) {
        let edge = &self.edges[arc.edge];
        match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => (edge.from, edge.to),
            WeightedPushRelabelShortcutDirection::Reverse => (edge.to, edge.from),
        }
    }

    fn modified_distances(&mut self) -> Result<Vec<u64>, WeightedPushRelabelShortcutError> {
        let mut distances = vec![u64::MAX; self.labels.len()];
        let source = self.source.as_usize();
        distances[source] = 0;
        let mut heap = BinaryHeap::from([(Reverse(0_u64), source)]);
        while let Some((Reverse(distance), node)) = heap.pop() {
            if distance != distances[node] {
                continue;
            }
            for ordinal in 0..self.edges.len() {
                for direction in [
                    WeightedPushRelabelShortcutDirection::Forward,
                    WeightedPushRelabelShortcutDirection::Reverse,
                ] {
                    self.inspect_primitive_arc(ordinal, Some(direction))?;
                    let arc = WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction,
                    };
                    let (from, to) = self.arc_endpoints(arc);
                    if from != node || self.residual_capacity(arc)? == 0 {
                        continue;
                    }
                    self.metrics.distance_arc_scans = self
                        .metrics
                        .distance_arc_scans
                        .checked_add(1)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                    let edge = &self.edges[ordinal];
                    let weight = if direction == WeightedPushRelabelShortcutDirection::Forward
                        && edge.kind == WeightedPushRelabelShortcutEdgeKind::Original
                        && self.hierarchy.order[from] < self.hierarchy.order[to]
                    {
                        0
                    } else {
                        edge.weight
                    };
                    let candidate = distance
                        .checked_add(weight)
                        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                    if candidate < distances[to] {
                        distances[to] = candidate;
                        heap.push((Reverse(candidate), to));
                    }
                }
            }
        }
        Ok(distances)
    }

    fn select_sparse_cut(
        &mut self,
        distances: &[u64],
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        let max_finite = distances
            .iter()
            .copied()
            .filter(|distance| *distance != u64::MAX)
            .max()
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?
            .min(self.height);
        let mut best: Option<(u128, u64, Vec<bool>)> = None;
        for level in 0..=max_finite {
            let side = distances
                .iter()
                .map(|distance| *distance <= level)
                .collect::<Vec<_>>();
            if !side[self.source.as_usize()] || side[self.sink.as_usize()] {
                continue;
            }
            let mut capacity = 0_u128;
            for ordinal in 0..self.edges.len() {
                for direction in [
                    WeightedPushRelabelShortcutDirection::Forward,
                    WeightedPushRelabelShortcutDirection::Reverse,
                ] {
                    let arc = WeightedPushRelabelShortcutArcId {
                        edge: ordinal,
                        direction,
                    };
                    let (from, to) = self.arc_endpoints(arc);
                    if side[from] && !side[to] {
                        capacity = capacity
                            .checked_add(u128::from(self.residual_capacity(arc)?))
                            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
                    }
                }
            }
            self.metrics.sparse_cut_checks = self
                .metrics
                .sparse_cut_checks
                .checked_add(1)
                .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            if best.as_ref().is_none_or(|(current, current_level, _)| {
                (capacity, level) < (*current, *current_level)
            }) {
                best = Some((capacity, level, side));
            }
        }
        if let Some((capacity, level, side)) = best {
            self.sparse_cut_capacity = capacity;
            self.sparse_cut_level = level;
            self.sparse_cut_side = side;
        } else {
            self.sparse_cut_level = max_finite;
            self.sparse_cut_side = distances
                .iter()
                .map(|distance| *distance != u64::MAX)
                .collect();
            self.sparse_cut_capacity = 0;
        }
        Ok(())
    }

    fn install_source_side(&mut self, certificate: &MaxFlowCertificate) {
        for (ordinal, node) in self.graph.nodes().iter().enumerate() {
            self.source_side[ordinal] = certificate.source_side.contains(node.id());
        }
    }

    fn emit(
        &mut self,
        stage: WeightedPushRelabelShortcutStage,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        self.flush_primitive_arc_checkpoint()?;
        self.stage = stage;
        self.inspected_edge = None;
        self.inspected_direction = None;
        self.active_relabel_node = None;
        self.commit_event()
    }

    fn emit_relabel_checkpoint(
        &mut self,
        node: usize,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        if !should_publish_scalar_checkpoint(
            self.metrics.relabel_steps,
            WEIGHTED_PUSH_RELABEL_SHORTCUT_RELABEL_CHECKPOINT_STRIDE,
        ) {
            return Ok(());
        }
        if self.metrics.primitive_arc_inspections
            != self.current_snapshot.metrics.primitive_arc_inspections
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        self.stage = WeightedPushRelabelShortcutStage::RelabelCheckpoint;
        self.inspected_edge = None;
        self.inspected_direction = None;
        self.active_relabel_node = Some(node);
        self.commit_event()
    }

    fn inspect_primitive_arc(
        &mut self,
        edge: usize,
        direction: Option<WeightedPushRelabelShortcutDirection>,
    ) -> Result<(), WeightedPushRelabelShortcutError> {
        if edge >= self.edges.len() {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        self.metrics.primitive_arc_inspections = self
            .metrics
            .primitive_arc_inspections
            .checked_add(1)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        self.inspected_edge = Some(edge);
        self.inspected_direction = direction;
        if !should_publish_weighted_checkpoint(
            self.metrics.primitive_arc_inspections,
            self.current_snapshot.metrics.primitive_arc_inspections,
            WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE,
        ) {
            return Ok(());
        }
        self.flush_primitive_arc_checkpoint()
    }

    fn flush_primitive_arc_checkpoint(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        if self.metrics.primitive_arc_inspections
            == self.current_snapshot.metrics.primitive_arc_inspections
        {
            return Ok(());
        }
        if self.inspected_edge.is_none() {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        self.stage = WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint;
        self.active_relabel_node = None;
        self.commit_event()
    }

    fn commit_event(&mut self) -> Result<(), WeightedPushRelabelShortcutError> {
        self.metrics.state_transitions = self
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        if usize::try_from(self.metrics.state_transitions).map_or(true, |count| {
            count > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_TRACE_EVENTS
        }) {
            #[cfg(test)]
            eprintln!(
                "weighted push-relabel trace boundary limit: transitions={} inspections={} relabels={} completion-relabels={}",
                self.metrics.state_transitions,
                self.metrics.primitive_arc_inspections,
                self.metrics.relabel_steps,
                self.metrics.completion_relabel_steps,
            );
            return Err(WeightedPushRelabelShortcutError::WorkLimit);
        }
        let next = self.snapshot()?;
        validate_snapshot(self.graph, self.source, self.sink, &next)?;
        if self.trace {
            self.events.push(WeightedPushRelabelShortcutTraceEvent {
                catalog_id: "weighted-push-relabel",
                before: self.current_snapshot.clone(),
                after: next.clone(),
            });
        }
        self.current_snapshot = next;
        Ok(())
    }

    fn snapshot(
        &self,
    ) -> Result<WeightedPushRelabelShortcutSnapshot, WeightedPushRelabelShortcutError> {
        let original_count = self.graph.nodes().len();
        let mut root_component = vec![None; self.labels.len()];
        let mut root = original_count;
        for (component, nodes) in self.hierarchy.component_nodes.iter().enumerate() {
            if component_has_internal_tail(self.graph, &self.hierarchy, component) {
                root_component[root] = Some(component);
                root += 1;
            }
            if nodes.is_empty() {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
        }
        let nodes = (0..self.labels.len())
            .map(|node| {
                let original_node = (node < original_count)
                    .then(|| NodeIndex::try_from_usize(node))
                    .flatten();
                let component = if node < original_count {
                    self.hierarchy.component[node]
                } else {
                    root_component[node].ok_or(WeightedPushRelabelShortcutError::Invariant)?
                };
                Ok(WeightedPushRelabelShortcutNodeState {
                    node,
                    original_node,
                    component,
                    order: original_node.map_or(0, |index| self.hierarchy.order[index.as_usize()]),
                    label: self.labels[node],
                    alive: self.alive[node],
                    sparse_cut_side: self.sparse_cut_side[node],
                    source_side: self.source_side[node],
                })
            })
            .collect::<Result<Vec<_>, WeightedPushRelabelShortcutError>>()?;
        let edges = self
            .edges
            .iter()
            .enumerate()
            .map(|(ordinal, edge)| WeightedPushRelabelShortcutEdgeState {
                ordinal,
                original_edge: edge.original_edge.clone(),
                from: edge.from,
                to: edge.to,
                capacity: edge.capacity,
                flow: edge.flow,
                kind: edge.kind,
                shortcut_component: edge.shortcut_component,
                weight: edge.weight,
            })
            .collect();
        let mut residual_arcs = Vec::with_capacity(self.edges.len() * 2);
        for ordinal in 0..self.edges.len() {
            for direction in [
                WeightedPushRelabelShortcutDirection::Forward,
                WeightedPushRelabelShortcutDirection::Reverse,
            ] {
                let id = WeightedPushRelabelShortcutArcId {
                    edge: ordinal,
                    direction,
                };
                let (from, to) = self.arc_endpoints(id);
                residual_arcs.push(WeightedPushRelabelShortcutResidualArcState {
                    id,
                    from,
                    to,
                    capacity: self.residual_capacity(id)?,
                    weight: self.edges[ordinal].weight,
                    admissible: self.admissible[arc_index(id)],
                    active: self.active_path.contains(&id)
                        || (self.inspected_edge == Some(ordinal)
                            && self
                                .inspected_direction
                                .is_none_or(|value| value == direction)),
                });
            }
        }
        Ok(WeightedPushRelabelShortcutSnapshot {
            stage: self.stage,
            hierarchy_levels: 1,
            psi_numerator: 1,
            psi_denominator: 1,
            height: self.height,
            demand: self.demand,
            routed: self.routed,
            weighted_length: self.weighted_length,
            weighted_length_units: self.routed.max(1),
            sparse_cut_level: self.sparse_cut_level,
            sparse_cut_capacity: self.sparse_cut_capacity,
            active_bottleneck: self.active_bottleneck,
            nodes,
            edges,
            residual_arcs,
            active_path: self.active_path.clone(),
            inspected_edge: self.inspected_edge,
            inspected_direction: self.inspected_direction,
            active_relabel_node: self.active_relabel_node,
            metrics: self.metrics,
            exact_flows: self.exact_flows.clone(),
        })
    }
}

const fn arc_index(arc: WeightedPushRelabelShortcutArcId) -> usize {
    arc.edge * 2
        + match arc.direction {
            WeightedPushRelabelShortcutDirection::Forward => 0,
            WeightedPushRelabelShortcutDirection::Reverse => 1,
        }
}

const fn should_publish_weighted_checkpoint(work: u64, published: u64, stride: u64) -> bool {
    work <= WEIGHTED_PUSH_RELABEL_SHORTCUT_DENSE_TRACE_PREFIX
        || work.saturating_sub(published) >= stride
}

const fn should_publish_scalar_checkpoint(work: u64, stride: u64) -> bool {
    work <= WEIGHTED_PUSH_RELABEL_SHORTCUT_DENSE_TRACE_PREFIX || work.is_multiple_of(stride)
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), WeightedPushRelabelShortcutError> {
    if graph.nodes().len() > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES
        || graph.edges().len() > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_EDGES
    {
        return Err(WeightedPushRelabelShortcutError::AdmissionLimit);
    }
    if source == sink
        || graph.nodes().is_empty()
        || graph.edges().is_empty()
        || graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.capacity() == 0
                || edge.capacity() > WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_CAPACITY
                || edge.from() == edge.to()
        })
    {
        return Err(WeightedPushRelabelShortcutError::GraphRequirement);
    }
    Ok(())
}

fn build_hierarchy(graph: &FlowNetwork) -> Result<Hierarchy, WeightedPushRelabelShortcutError> {
    let n = graph.nodes().len();
    let mut adjacency = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for edge in graph.edges() {
        adjacency[edge.from().as_usize()].push(edge.to().as_usize());
        reverse[edge.to().as_usize()].push(edge.from().as_usize());
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    for neighbors in &mut reverse {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    let mut visited = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for start in 0..n {
        if !visited[start] {
            dfs_finish(start, &adjacency, &mut visited, &mut finish);
        }
    }
    let mut component = vec![usize::MAX; n];
    let mut component_nodes = Vec::new();
    for &start in finish.iter().rev() {
        if component[start] != usize::MAX {
            continue;
        }
        let ordinal = component_nodes.len();
        let mut nodes = Vec::new();
        let mut queue = VecDeque::from([start]);
        component[start] = ordinal;
        while let Some(node) = queue.pop_front() {
            nodes.push(node);
            for &next in &reverse[node] {
                if component[next] == usize::MAX {
                    component[next] = ordinal;
                    queue.push_back(next);
                }
            }
        }
        nodes.sort_unstable();
        component_nodes.push(nodes);
    }
    let count = component_nodes.len();
    let mut dag = vec![Vec::new(); count];
    let mut indegree = vec![0_usize; count];
    for edge in graph.edges() {
        let from = component[edge.from().as_usize()];
        let to = component[edge.to().as_usize()];
        if from != to && !dag[from].contains(&to) {
            dag[from].push(to);
            indegree[to] += 1;
        }
    }
    for next in &mut dag {
        next.sort_unstable();
    }
    let mut ready = BinaryHeap::new();
    for (ordinal, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.push(Reverse(ordinal));
        }
    }
    let mut order = vec![0_usize; n];
    let mut rank = 1_usize;
    let mut seen_components = 0_usize;
    while let Some(Reverse(current)) = ready.pop() {
        seen_components += 1;
        for &node in &component_nodes[current] {
            order[node] = rank;
            rank += 1;
        }
        for &next in &dag[current] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.push(Reverse(next));
            }
        }
    }
    if seen_components != count || order.contains(&0) {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    Ok(Hierarchy {
        component,
        order,
        component_nodes,
    })
}

fn dfs_finish(
    node: usize,
    adjacency: &[Vec<usize>],
    visited: &mut [bool],
    finish: &mut Vec<usize>,
) {
    visited[node] = true;
    for &next in &adjacency[node] {
        if !visited[next] {
            dfs_finish(next, adjacency, visited, finish);
        }
    }
    finish.push(node);
}

fn component_has_internal_tail(
    graph: &FlowNetwork,
    hierarchy: &Hierarchy,
    component: usize,
) -> bool {
    graph.edges().iter().any(|edge| {
        hierarchy.component[edge.from().as_usize()] == component
            && hierarchy.component[edge.to().as_usize()] == component
    })
}

fn shortcut_star_count(hierarchy: &Hierarchy, graph: &FlowNetwork) -> usize {
    hierarchy
        .component_nodes
        .iter()
        .enumerate()
        .filter(|(component, _)| component_has_internal_tail(graph, hierarchy, *component))
        .count()
}

fn add_shortcuts(
    graph: &FlowNetwork,
    hierarchy: &Hierarchy,
    edges: &mut Vec<WorkEdge>,
) -> Result<(), WeightedPushRelabelShortcutError> {
    let mut next_root = graph.nodes().len();
    for (component, nodes) in hierarchy.component_nodes.iter().enumerate() {
        let mut capacity_by_tail = vec![0_u64; graph.nodes().len()];
        for edge in graph.edges() {
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            if hierarchy.component[from] == component && hierarchy.component[to] == component {
                capacity_by_tail[from] = capacity_by_tail[from]
                    .checked_add(edge.capacity())
                    .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
            }
        }
        if capacity_by_tail.iter().all(|capacity| *capacity == 0) {
            continue;
        }
        let root = next_root;
        next_root += 1;
        let weight = u64::try_from(nodes.len())
            .map_err(|_| WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
        for (tail, capacity) in capacity_by_tail.into_iter().enumerate() {
            if capacity == 0 {
                continue;
            }
            for (from, to) in [(tail, root), (root, tail)] {
                edges.push(WorkEdge {
                    original_edge: None,
                    from,
                    to,
                    capacity,
                    flow: 0,
                    kind: WeightedPushRelabelShortcutEdgeKind::Shortcut,
                    shortcut_component: Some(component),
                    weight,
                });
            }
        }
    }
    Ok(())
}

fn empty_snapshot(graph: &FlowNetwork) -> WeightedPushRelabelShortcutSnapshot {
    WeightedPushRelabelShortcutSnapshot {
        stage: WeightedPushRelabelShortcutStage::Ready,
        hierarchy_levels: 0,
        psi_numerator: 1,
        psi_denominator: 1,
        height: 0,
        demand: 0,
        routed: 0,
        weighted_length: 0,
        weighted_length_units: 1,
        sparse_cut_level: 0,
        sparse_cut_capacity: 0,
        active_bottleneck: 0,
        nodes: graph
            .node_indices()
            .map(|node| WeightedPushRelabelShortcutNodeState {
                node: node.as_usize(),
                original_node: Some(node),
                component: node.as_usize(),
                order: node.as_usize() + 1,
                label: 0,
                alive: true,
                sparse_cut_side: false,
                source_side: false,
            })
            .collect(),
        edges: Vec::new(),
        residual_arcs: Vec::new(),
        active_path: Vec::new(),
        inspected_edge: None,
        inspected_direction: None,
        active_relabel_node: None,
        metrics: WeightedPushRelabelShortcutMetrics::default(),
        exact_flows: None,
    }
}

#[allow(clippy::too_many_lines)]
fn validate_snapshot(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &WeightedPushRelabelShortcutSnapshot,
) -> Result<(), WeightedPushRelabelShortcutError> {
    if snapshot.psi_numerator != 1
        || snapshot.psi_denominator != 1
        || snapshot.weighted_length_units == 0
        || snapshot.routed > snapshot.demand
        || matches!(
            snapshot.stage,
            WeightedPushRelabelShortcutStage::AugmentPath
                | WeightedPushRelabelShortcutStage::CompletionAugmentPath
        ) != (snapshot.active_bottleneck > 0 && !snapshot.active_path.is_empty())
    {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    let inspection_stage = matches!(
        snapshot.stage,
        WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
            | WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
    );
    let relabel_step = matches!(
        snapshot.stage,
        WeightedPushRelabelShortcutStage::RelabelCheckpoint
            | WeightedPushRelabelShortcutStage::CompletionRelabelCheckpoint
    );
    if inspection_stage != snapshot.inspected_edge.is_some()
        || snapshot
            .inspected_edge
            .is_some_and(|edge| edge >= snapshot.edges.len())
        || (!inspection_stage && snapshot.inspected_direction.is_some())
        || relabel_step != snapshot.active_relabel_node.is_some()
        || snapshot
            .active_relabel_node
            .is_some_and(|node| node >= snapshot.nodes.len())
    {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    if snapshot.stage == WeightedPushRelabelShortcutStage::Ready {
        if snapshot.nodes.len() != graph.nodes().len()
            || !snapshot.edges.is_empty()
            || !snapshot.residual_arcs.is_empty()
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        return Ok(());
    }
    if snapshot.hierarchy_levels != 1
        || snapshot.nodes.len() < graph.nodes().len()
        || snapshot.edges.len() < graph.edges().len()
        || snapshot.residual_arcs.len() != snapshot.edges.len() * 2
        || snapshot.height == 0
        || snapshot.demand == 0
    {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    let max_label = snapshot
        .height
        .checked_mul(9)
        .and_then(|value| value.checked_add(1))
        .ok_or(WeightedPushRelabelShortcutError::ArithmeticOverflow)?;
    for (ordinal, node) in snapshot.nodes.iter().enumerate() {
        if node.node != ordinal
            || node.label > max_label
            || node.alive != (node.label <= snapshot.height * 9)
            || (ordinal < graph.nodes().len()) != node.original_node.is_some()
            || (node.original_node.is_some() && node.order == 0)
            || (node.original_node.is_none() && node.order != 0)
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
    }
    for (ordinal, edge) in snapshot.edges.iter().enumerate() {
        if edge.ordinal != ordinal
            || edge.from >= snapshot.nodes.len()
            || edge.to >= snapshot.nodes.len()
            || edge.capacity == 0
            || edge.flow > edge.capacity
            || edge.weight == 0
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        if ordinal < graph.edges().len() {
            let original = &graph.edges()[ordinal];
            if edge.kind != WeightedPushRelabelShortcutEdgeKind::Original
                || edge.original_edge.as_ref() != Some(original.id())
                || edge.from != original.from().as_usize()
                || edge.to != original.to().as_usize()
                || edge.capacity != original.capacity()
                || edge.shortcut_component.is_some()
            {
                return Err(WeightedPushRelabelShortcutError::Invariant);
            }
        } else if edge.kind != WeightedPushRelabelShortcutEdgeKind::Shortcut
            || edge.original_edge.is_some()
            || edge.shortcut_component.is_none()
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        let forward = &snapshot.residual_arcs[ordinal * 2];
        let reverse = &snapshot.residual_arcs[ordinal * 2 + 1];
        if forward.id
            != (WeightedPushRelabelShortcutArcId {
                edge: ordinal,
                direction: WeightedPushRelabelShortcutDirection::Forward,
            })
            || reverse.id
                != (WeightedPushRelabelShortcutArcId {
                    edge: ordinal,
                    direction: WeightedPushRelabelShortcutDirection::Reverse,
                })
            || forward.from != edge.from
            || forward.to != edge.to
            || reverse.from != edge.to
            || reverse.to != edge.from
            || forward.capacity != edge.capacity - edge.flow
            || reverse.capacity != edge.flow
            || forward.weight != edge.weight
            || reverse.weight != edge.weight
            || forward.active
                != (snapshot.active_path.contains(&forward.id)
                    || (snapshot.inspected_edge == Some(ordinal)
                        && snapshot
                            .inspected_direction
                            .is_none_or(|value| value == forward.id.direction)))
            || reverse.active
                != (snapshot.active_path.contains(&reverse.id)
                    || (snapshot.inspected_edge == Some(ordinal)
                        && snapshot
                            .inspected_direction
                            .is_none_or(|value| value == reverse.id.direction)))
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
    }
    let mut active_path_node = snapshot.active_path.first().map(|_| source.as_usize());
    for id in &snapshot.active_path {
        let arc = snapshot
            .residual_arcs
            .get(arc_index(*id))
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        let from = snapshot
            .nodes
            .get(arc.from)
            .ok_or(WeightedPushRelabelShortcutError::Invariant)?;
        if active_path_node != Some(arc.from)
            || (!arc.admissible && arc.capacity != 0)
            || !from.alive
        {
            return Err(WeightedPushRelabelShortcutError::Invariant);
        }
        active_path_node = Some(arc.to);
    }
    if active_path_node.is_some_and(|node| node != sink.as_usize()) {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    let completed = matches!(
        snapshot.stage,
        WeightedPushRelabelShortcutStage::CompleteResidualRounds
            | WeightedPushRelabelShortcutStage::CheckCertificate
            | WeightedPushRelabelShortcutStage::Optimal
    );
    if completed != snapshot.exact_flows.is_some() {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    if let Some(flows) = &snapshot.exact_flows
        && (flows.len() != graph.edges().len()
            || flows.iter().zip(graph.edges()).zip(&snapshot.edges).any(
                |((flow, original), state)| *flow > original.capacity() || *flow != state.flow,
            ))
    {
        return Err(WeightedPushRelabelShortcutError::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};

    fn fixture() -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect::<Vec<_>>();
        let edges = [
            ("sa", "s", "a", 6),
            ("sb", "s", "b", 4),
            ("ab", "a", "b", 7),
            ("ba", "b", "a", 7),
            ("at", "a", "t", 4),
            ("bt", "b", "t", 6),
        ]
        .into_iter()
        .map(|(id, from, to, capacity)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost: 0,
        })
        .collect();
        let graph = FlowNetwork::new(nodes, edges).expect("network");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "the fixture verifies shortcut construction and every complexity-faithful completion boundary"
    )]
    fn builds_source_shortcuts_and_publishes_certified_weighted_completion() {
        let (graph, source, sink) = fixture();
        let trace = trace_weighted_push_relabel_shortcut(&graph, source, sink).expect("trace");
        verify_weighted_push_relabel_shortcut_trace(&graph, source, sink, &trace).expect("replay");
        assert_eq!(trace.result.certificate.value, 10);
        assert_eq!(trace.result.certificate.cut_bound, 10);
        assert!(trace.events.iter().any(|event| {
            event.after.stage == WeightedPushRelabelShortcutStage::BuildShortcutGraph
                && event.after.edges.iter().any(|edge| {
                    edge.kind == WeightedPushRelabelShortcutEdgeKind::Shortcut && edge.weight == 2
                })
        }));
        assert!(
            trace.events.iter().any(|event| {
                event.after.stage == WeightedPushRelabelShortcutStage::RelabelSweep
            })
        );
        assert!(
            trace.events.iter().any(|event| {
                event.after.stage == WeightedPushRelabelShortcutStage::AugmentPath
            })
        );
        assert!(trace.result.metrics.residual_rounds >= 2);
        assert!(trace.result.metrics.completion_augmentations > 0);
        let completion_events = trace
            .events
            .iter()
            .skip_while(|event| {
                event.after.stage
                    != WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
                    && event.after.stage
                        != WeightedPushRelabelShortcutStage::CompletionRelabelCheckpoint
                    && event.after.stage != WeightedPushRelabelShortcutStage::CompletionAugmentPath
                    && event.after.stage
                        != WeightedPushRelabelShortcutStage::CompletionResidualRound
            })
            .collect::<Vec<_>>();
        assert!(!completion_events.is_empty());
        assert!(completion_events.iter().any(|event| {
            event.after.stage
                == WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
                && event.after.inspected_edge.is_some()
        }));
        assert!(completion_events.iter().any(|event| {
            event.after.stage == WeightedPushRelabelShortcutStage::CompletionAugmentPath
                && !event.after.active_path.is_empty()
                && event.after.active_bottleneck > 0
        }));
        assert!(completion_events.iter().all(|event| {
            event
                .after
                .metrics
                .primitive_arc_inspections
                .saturating_sub(event.before.metrics.primitive_arc_inspections)
                <= WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE
        }));
        assert!(trace.events.iter().all(|event| {
            let delta = event
                .after
                .metrics
                .primitive_arc_inspections
                .saturating_sub(event.before.metrics.primitive_arc_inspections);
            let inspection_boundary = matches!(
                event.after.stage,
                WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
                    | WeightedPushRelabelShortcutStage::CompletionInspectPrimitiveArcCheckpoint
            );
            inspection_boundary == (delta > 0)
                && delta <= WEIGHTED_PUSH_RELABEL_SHORTCUT_INSPECTION_CHECKPOINT_STRIDE
                && (!inspection_boundary || event.after.inspected_edge.is_some())
        }));
        let inspection_checkpoints = trace
            .events
            .iter()
            .filter(|event| {
                event.after.stage == WeightedPushRelabelShortcutStage::InspectPrimitiveArcCheckpoint
            })
            .collect::<Vec<_>>();
        assert!(inspection_checkpoints.len() >= 32);
        assert!(inspection_checkpoints.iter().all(|event| {
            event.after.inspected_edge.is_some()
                && event.after.metrics.primitive_arc_inspections
                    > event.before.metrics.primitive_arc_inspections
        }));
        let relabel_checkpoints = trace
            .events
            .iter()
            .filter(|event| {
                event.after.stage == WeightedPushRelabelShortcutStage::RelabelCheckpoint
            })
            .collect::<Vec<_>>();
        assert!(relabel_checkpoints.len() >= 32);
        assert!(relabel_checkpoints.iter().all(|event| {
            event.after.active_relabel_node.is_some()
                && event.after.metrics.relabel_steps > event.before.metrics.relabel_steps
        }));
        assert_eq!(
            trace.result.metrics.state_transitions,
            u64::try_from(trace.events.len()).expect("bounded event count")
        );
        assert!(trace.events.iter().any(|event| {
            event.after.stage == WeightedPushRelabelShortcutStage::CompleteResidualRounds
                && event.after.exact_flows.as_ref() == Some(&trace.result.flows)
        }));
    }

    #[test]
    fn fast_trace_and_terminal_certificate_are_identical() {
        let (graph, source, sink) = fixture();
        let fast = solve_weighted_push_relabel_shortcut(&graph, source, sink).expect("fast");
        let trace = trace_weighted_push_relabel_shortcut(&graph, source, sink).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(
            fast.final_snapshot.stage,
            WeightedPushRelabelShortcutStage::Optimal
        );
    }

    #[test]
    fn weighted_residual_outer_loop_certifies_all_four_node_edge_subsets() {
        let node_ids = ["s", "a", "b", "t"];
        let candidates = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("ab", "a", "b"),
            ("ba", "b", "a"),
            ("at", "a", "t"),
            ("bt", "b", "t"),
        ];
        for mask in 1_u64..(1_u64 << candidates.len()) {
            let nodes = node_ids
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect::<Vec<_>>();
            let edges = candidates
                .iter()
                .enumerate()
                .filter(|(ordinal, _)| mask & (1_u64 << ordinal) != 0)
                .map(|(ordinal, (id, from, to))| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("from"),
                    to: NodeId::parse(to).expect("to"),
                    lower: 0,
                    capacity: 1 + u64::try_from(ordinal % 3).expect("small ordinal"),
                    cost: 0,
                })
                .collect::<Vec<_>>();
            let graph = FlowNetwork::new(nodes, edges).expect("network");
            let source = graph
                .node_index(&NodeId::parse("s").expect("source id"))
                .expect("source");
            let sink = graph
                .node_index(&NodeId::parse("t").expect("sink id"))
                .expect("sink");
            let result = solve_weighted_push_relabel_shortcut(&graph, source, sink)
                .unwrap_or_else(|error| panic!("mask {mask:#08b}: {error}"));
            check_max_flow(&graph, source, sink, &result.flows)
                .unwrap_or_else(|error| panic!("mask {mask:#08b}: {error}"));
            assert!(result.metrics.residual_rounds > 0, "mask {mask:#08b}");
        }
    }

    #[test]
    fn admission_and_graph_contract_fail_closed() {
        let (graph, source, sink) = fixture();
        assert_eq!(
            solve_weighted_push_relabel_shortcut(&graph, source, source),
            Err(WeightedPushRelabelShortcutError::GraphRequirement)
        );
        let mut nodes = graph.nodes().to_vec();
        while nodes.len() <= WEIGHTED_PUSH_RELABEL_SHORTCUT_MAX_NODES {
            nodes.push(FlowNode::new(
                NodeId::parse(&format!("x{}", nodes.len())).expect("node id"),
                0,
            ));
        }
        let oversized = FlowNetwork::new(
            nodes,
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("oversized-edge").expect("edge id"),
                from: NodeId::parse("s").expect("from"),
                to: NodeId::parse("t").expect("to"),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .expect("network");
        assert_eq!(
            solve_weighted_push_relabel_shortcut(&oversized, source, sink),
            Err(WeightedPushRelabelShortcutError::AdmissionLimit)
        );
    }

    #[test]
    fn forged_trace_is_rejected() {
        let (graph, source, sink) = fixture();
        let mut trace = trace_weighted_push_relabel_shortcut(&graph, source, sink).expect("trace");
        trace.events[0].after.psi_denominator = 2;
        assert_eq!(
            verify_weighted_push_relabel_shortcut_trace(&graph, source, sink, &trace),
            Err(WeightedPushRelabelShortcutError::Invariant)
        );
    }

    #[test]
    fn premature_exact_flow_publication_is_rejected() {
        let (graph, source, sink) = fixture();
        let mut trace = trace_weighted_push_relabel_shortcut(&graph, source, sink).expect("trace");
        trace.events[0].after.exact_flows = Some(vec![0; graph.edges().len()]);
        assert_eq!(
            verify_weighted_push_relabel_shortcut_trace(&graph, source, sink, &trace),
            Err(WeightedPushRelabelShortcutError::Invariant)
        );
    }
}
