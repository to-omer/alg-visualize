//! Exact bounded dynamic core-graph transition primitive.
//!
//! This module realizes Definition "Core Graph" and the topology-update
//! semantics of the Dynamic Core Graphs lemma in van den Brand et al. A
//! checked dynamic low-stretch-forest transcript supplies the monotone roots,
//! decremental forest, and immutable stretch overestimates. Each new root is
//! emitted as a core-vertex split, while original edge insertions, deletions,
//! and vertex splits retain their stable edge identities.
//!
//! For an original edge `e = (u, v)`, the core edge has endpoints equal to the
//! current forest-component roots, length `stretch_overestimate[e] * length[e]`,
//! and gradient `gradient[e] + <gradient, path_T[v, u]>`. The static reference
//! tree keeps its initial gradients even if one of its edges is later deleted,
//! matching the source convention that the gradient support is `E(G) ∪ E(T)`.
//! The checked forest snapshot separately publishes immutable reference-tree
//! support rows, so a dynamic source row may move during a vertex split or
//! reuse its stable ID after deletion without changing the static tree path.
//!
//! The implementation is deliberately bounded and explicit. It consumes the
//! source Heavy-Light auxiliary-tree LSF realization, independently rebuilds
//! `F_T(R, pi)` while deriving root splits, and records incidence-level split
//! encodings (including loops). It does not claim LSST construction, dynamic
//! recourse, sparsifier construction, or asymptotic runtime bounds.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use thiserror::Error;

use super::{
    DYNAMIC_LSF_MAX_EDGES, DYNAMIC_LSF_MAX_NODES, DYNAMIC_LSF_MAX_OPERATIONS,
    DYNAMIC_LSF_MAX_RATIONAL_BITS, DynamicLowStretchForestEdge, DynamicLowStretchForestEncodedSide,
    DynamicLowStretchForestError, DynamicLowStretchForestEventKind,
    DynamicLowStretchForestIncidence, DynamicLowStretchForestIncidenceEndpoint,
    DynamicLowStretchForestInput, DynamicLowStretchForestOperation,
    DynamicLowStretchForestSnapshot, DynamicLowStretchForestStageBatch,
    DynamicLowStretchForestStageEventKind, DynamicLowStretchForestStageTraceResult,
    DynamicLowStretchForestStageUpdate, DynamicLowStretchForestTraceResult,
    check_dynamic_low_stretch_forest_stage_trace, check_dynamic_low_stretch_forest_trace,
    trace_dynamic_low_stretch_forest, trace_dynamic_low_stretch_forest_stages,
};

/// Maximum stable vertices.
pub const DYNAMIC_CORE_MAX_NODES: usize = DYNAMIC_LSF_MAX_NODES;
/// Maximum stable original/core edge slots.
pub const DYNAMIC_CORE_MAX_EDGES: usize = DYNAMIC_LSF_MAX_EDGES;
/// Maximum source topology operations.
pub const DYNAMIC_CORE_MAX_OPERATIONS: usize = DYNAMIC_LSF_MAX_OPERATIONS;
/// Maximum reversible boundaries, including completion.
pub const DYNAMIC_CORE_MAX_TRACE_EVENTS: usize = DYNAMIC_CORE_MAX_OPERATIONS + 1;
/// Maximum numerator or denominator width.
pub const DYNAMIC_CORE_MAX_RATIONAL_BITS: u64 = DYNAMIC_LSF_MAX_RATIONAL_BITS;

const CATALOG_ID: &str = "dynamic-core-graph";

/// Initial dynamic graph, low-stretch forest, and exact edge gradients.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphInput {
    /// Source topology and fixed reference tree.
    pub forest: DynamicLowStretchForestInput,
    /// Initial gradient in each active stable edge slot; inactive slots are `None`.
    pub initial_gradients: Vec<Option<BigRational>>,
}

/// One source topology operation with any newly inserted gradient.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCoreGraphOperation {
    /// Insert one inactive stable edge slot.
    Insert {
        /// Complete source edge row.
        edge: DynamicLowStretchForestEdge,
        /// Exact source gradient of the new edge.
        gradient: BigRational,
    },
    /// Delete one active stable edge.
    Delete {
        /// Stable source/core edge slot.
        edge: usize,
    },
    /// Explicitly reinsert one currently active edge without changing attributes.
    Reinsert {
        /// Stable source/core edge slot.
        edge: usize,
    },
    /// Split a source vertex and move the encoded smaller off-tree side.
    VertexSplit {
        /// Existing vertex retained in the reference tree.
        vertex: usize,
        /// Next consecutive stable vertex.
        new_vertex: usize,
        /// Strictly increasing active off-tree edge IDs whose endpoint moves.
        moved_edges: Vec<usize>,
    },
}

/// One complete source edge row including exact dynamic attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphStageEdge {
    /// Stable source edge slot.
    pub edge: usize,
    /// Current source tail.
    pub from: usize,
    /// Current source head.
    pub to: usize,
    /// Exact current source length.
    pub length: BigRational,
    /// Exact current source gradient.
    pub gradient: BigRational,
}

/// One ordered source update inside an atomic dynamic-core stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCoreGraphStageUpdate {
    /// Insert one complete source row into an inactive stable slot.
    Insert {
        /// Complete inserted row.
        edge: DynamicCoreGraphStageEdge,
    },
    /// Delete one complete active source row.
    Delete {
        /// Complete row at deletion time.
        edge: DynamicCoreGraphStageEdge,
    },
    /// Reinsert an active source edge without changing topology.
    Reinsert {
        /// Complete row immediately before reinsertion.
        before: DynamicCoreGraphStageEdge,
        /// Complete current row after reinsertion.
        after: DynamicCoreGraphStageEdge,
    },
    /// Replace length and/or gradient without resetting the insertion epoch.
    ReplaceAttributes {
        /// Complete row before replacement.
        before: DynamicCoreGraphStageEdge,
        /// Complete row after replacement; stable ID and endpoints are unchanged.
        after: DynamicCoreGraphStageEdge,
    },
    /// Split one actual new side while separately retaining the smaller encoding.
    VertexSplit {
        /// Existing source vertex whose identity is retained.
        retained_vertex: usize,
        /// Next consecutive actual new source vertex.
        new_vertex: usize,
        /// Exact source incidences moving to the new vertex.
        new_side_incidences: Vec<DynamicCoreIncidence>,
        /// Canonical smaller side used for encoded-size accounting.
        encoded_side: DynamicCoreEncodedSide,
        /// Canonical strictly ordered incidences of the encoded side.
        encoded_incidences: Vec<DynamicCoreIncidence>,
    },
}

/// One outer source stage containing an ordered atomic core input batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphStageBatch {
    /// Exact next outer stage.
    pub outer_stage: u64,
    /// Ordered source records.
    pub updates: Vec<DynamicCoreGraphStageUpdate>,
}

/// Which endpoint incidence of a stable edge is moved by a core split.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DynamicCoreIncidenceEndpoint {
    /// Tail incidence.
    Tail,
    /// Head incidence.
    Head,
}

/// One stable edge incidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DynamicCoreIncidence {
    /// Stable source/core edge slot.
    pub edge: usize,
    /// Tail or head incidence.
    pub endpoint: DynamicCoreIncidenceEndpoint,
}

/// Which side is listed by the source smaller-side split encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicCoreEncodedSide {
    /// Incidences that remain on the old core vertex are listed.
    Retained,
    /// Incidences moved to the new core vertex are listed.
    New,
}

/// One exact core edge row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreEdge {
    /// Stable identity inherited from the source edge.
    pub edge: usize,
    /// Current contracted tail root.
    pub from: usize,
    /// Current contracted head root.
    pub to: usize,
    /// Exact `stretch_overestimate * source_length`.
    pub length: BigRational,
    /// Exact source gradient plus the signed static-tree return path.
    pub gradient: BigRational,
}

/// One source-level update emitted for the dynamic core graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCoreUpdate {
    /// A forest component is split by adding one root.
    VertexSplit {
        /// Existing core vertex that is partitioned.
        retained_vertex: usize,
        /// Newly materialized core vertex/root.
        new_vertex: usize,
        /// Exact incidences whose core endpoint moves to `new_vertex`.
        new_side_incidences: Vec<DynamicCoreIncidence>,
        /// Canonical smaller side used by the encoded dynamic update.
        encoded_side: DynamicCoreEncodedSide,
        /// Canonical strictly ordered smaller-side incidences.
        encoded_incidences: Vec<DynamicCoreIncidence>,
    },
    /// An original edge and its image are inserted.
    EdgeInserted {
        /// Complete inserted core row.
        edge: DynamicCoreEdge,
    },
    /// An original edge and its image are deleted.
    EdgeDeleted {
        /// Complete deleted core row after any preceding root splits.
        edge: DynamicCoreEdge,
    },
    /// An active edge was explicitly reinserted without a topology change.
    EdgeReinserted {
        /// Core row after any preceding root splits and before reinsertion.
        before: DynamicCoreEdge,
        /// Current core row after resetting the source insertion epoch/stretch.
        after: DynamicCoreEdge,
    },
    /// A source vertex split changed the static-tree return-path gradient.
    GradientReplaced {
        /// Stable core edge slot.
        edge: usize,
        /// Gradient before the endpoint move.
        before: BigRational,
        /// Gradient after the endpoint move.
        after: BigRational,
    },
    /// One selected/core edge length changed without a topology update.
    LengthReplaced {
        /// Stable core edge slot.
        edge: usize,
        /// Exact core length before replacement.
        before: BigRational,
        /// Exact core length after replacement.
        after: BigRational,
    },
}

/// Exact work and update-shape counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicCoreGraphMetrics {
    /// Source topology operations consumed.
    pub source_updates: u64,
    /// Core vertex-split updates emitted.
    pub vertex_splits: u64,
    /// Incidences whose endpoint actually moved to a new core vertex.
    pub endpoint_moves: u64,
    /// Incidences included in canonical smaller-side encodings.
    pub encoded_incidences: u64,
    /// Core edge insertions emitted.
    pub edge_insertions: u64,
    /// Core edge deletions emitted.
    pub edge_deletions: u64,
    /// Explicit active-edge reinsertions emitted.
    pub edge_reinsertions: u64,
    /// Core gradient replacements emitted.
    pub gradient_replacements: u64,
    /// Core length replacements emitted.
    pub length_replacements: u64,
    /// Active-edge Definition Core Graph checks performed.
    pub definition_checks: u64,
    /// Reversible public transitions.
    pub state_transitions: u64,
}

/// Complete exact core graph state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphSnapshot {
    /// Active source vertex count.
    pub active_node_count: usize,
    /// Active contracted core vertices, equal to the LSF roots.
    pub core_vertices: Vec<usize>,
    /// Stable exact core edge rows.
    pub edge_slots: Vec<Option<DynamicCoreEdge>>,
    /// Current source gradients; deleted/inactive source slots are `None`.
    pub source_gradients: Vec<Option<BigRational>>,
    /// Completed source topology stages.
    pub stage: u64,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: DynamicCoreGraphMetrics,
}

/// Meaning of one reversible core transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCoreGraphEventKind {
    /// One source topology operation and its induced core update batch.
    Updated {
        /// Source operation.
        operation: Box<DynamicCoreGraphOperation>,
        /// Checked low-stretch-forest transition used by this stage.
        forest_event: DynamicLowStretchForestEventKind,
        /// Ordered core update batch.
        core_updates: Vec<DynamicCoreUpdate>,
    },
    /// Every supplied source operation completed.
    Completed,
}

/// One fully reversible core boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source/core transition.
    pub kind: DynamicCoreGraphEventKind,
    /// State before the transition.
    pub before: DynamicCoreGraphSnapshot,
    /// State after the transition.
    pub after: DynamicCoreGraphSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphResult {
    /// Terminal core graph state.
    pub final_snapshot: DynamicCoreGraphSnapshot,
}

/// Complete low-stretch-forest and core-graph transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphTraceResult {
    /// Independently checkable LSF component transcript.
    pub forest_trace: DynamicLowStretchForestTraceResult,
    /// Initial core graph state.
    pub base_snapshot: DynamicCoreGraphSnapshot,
    /// One core event per operation followed by completion.
    pub events: Vec<DynamicCoreGraphTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicCoreGraphResult,
}

/// Meaning of one atomic dynamic-core stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicCoreGraphStageEventKind {
    /// One source batch and its induced ordered core batch.
    Updated {
        /// Exact atomic source batch.
        batch: DynamicCoreGraphStageBatch,
        /// Checked atomic LSF transition used by the stage.
        forest_event: DynamicLowStretchForestStageEventKind,
        /// Ordered core update batch.
        core_updates: Vec<DynamicCoreUpdate>,
    },
    /// Every supplied source stage completed.
    Completed,
}

/// One reversible atomic dynamic-core boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphStageTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source/core stage transition.
    pub kind: DynamicCoreGraphStageEventKind,
    /// Core state before the whole source batch.
    pub before: DynamicCoreGraphSnapshot,
    /// Core state after the whole source batch.
    pub after: DynamicCoreGraphSnapshot,
}

/// Complete atomic LSF and dynamic-core transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCoreGraphStageTraceResult {
    /// Independently checkable atomic LSF component transcript.
    pub forest_trace: DynamicLowStretchForestStageTraceResult,
    /// Initial core graph state.
    pub base_snapshot: DynamicCoreGraphSnapshot,
    /// One event per source stage followed by completion.
    pub events: Vec<DynamicCoreGraphStageTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicCoreGraphResult,
}

/// Explicit bounded dynamic-core failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicCoreGraphError {
    /// Gradient shape or source/core operation input is malformed.
    #[error("dynamic core graph input is invalid")]
    InvalidInput,
    /// Input or exact arithmetic exceeds the published bounded realization.
    #[error("dynamic core graph exceeds its admission band")]
    AdmissionLimit,
    /// The composed low-stretch-forest primitive failed.
    #[error("dynamic core graph low-stretch-forest component failed: {0}")]
    Forest(#[from] DynamicLowStretchForestError),
    /// A Definition Core Graph or update-batch invariant failed.
    #[error("dynamic core graph invariant failed")]
    InvariantViolation,
    /// Checked metric arithmetic overflowed.
    #[error("dynamic core graph arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact independent replay.
    #[error("dynamic core graph trace verification failed")]
    TraceVerification,
}

#[derive(Clone)]
struct StaticReference {
    initial_node_count: usize,
    root: usize,
    parent: Vec<Option<usize>>,
    parent_edge: Vec<Option<usize>>,
    depth: Vec<usize>,
    pi_rank: Vec<Option<usize>>,
    tree_gradients: Vec<Option<BigRational>>,
    tree_edges: Vec<Option<DynamicLowStretchForestEdge>>,
}

struct CoreTransitionContext<'a> {
    reference: &'a StaticReference,
    forest_before: &'a DynamicLowStretchForestSnapshot,
    forest_after: &'a DynamicLowStretchForestSnapshot,
    operation: &'a DynamicCoreGraphOperation,
    core_before: &'a DynamicCoreGraphSnapshot,
}

struct InternalRun {
    forest_trace: DynamicLowStretchForestTraceResult,
    base_snapshot: DynamicCoreGraphSnapshot,
    events: Vec<DynamicCoreGraphTraceEvent>,
    result: DynamicCoreGraphResult,
}

/// Executes the bounded dynamic core-graph transition system.
///
/// # Errors
///
/// Rejects malformed/out-of-band data, a failed LSF component, an invalid
/// source-derived core update, or exact arithmetic overflow.
pub fn execute_dynamic_core_graph(
    input: &DynamicCoreGraphInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicCoreGraphResult, DynamicCoreGraphError> {
    run_internal(input, operations, false).map(|run| run.result)
}

/// Records every source topology and completion boundary.
///
/// # Errors
///
/// Returns any execution or independent replay-checker failure.
pub fn trace_dynamic_core_graph(
    input: &DynamicCoreGraphInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicCoreGraphTraceResult, DynamicCoreGraphError> {
    let run = run_internal(input, operations, true)?;
    let trace = DynamicCoreGraphTraceResult {
        forest_trace: run.forest_trace,
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_core_graph_trace(input, operations, &trace)?;
    Ok(trace)
}

/// Independently replays the LSF component and every core update batch.
///
/// The checker never calls the dynamic-core runner or its production batch
/// derivation. It verifies the LSF transcript, reconstructs Definition Core
/// Graph rows, derives the canonical split batches again, applies the supplied
/// batches to the prior core state, and checks all metrics and boundaries.
///
/// # Errors
///
/// Rejects invalid source input or any component/core transcript drift.
pub fn check_dynamic_core_graph_trace(
    input: &DynamicCoreGraphInput,
    operations: &[DynamicCoreGraphOperation],
    trace: &DynamicCoreGraphTraceResult,
) -> Result<(), DynamicCoreGraphError> {
    let forest_operations = validate_and_convert(input, operations)?;
    check_dynamic_low_stretch_forest_trace(&input.forest, &forest_operations, &trace.forest_trace)?;
    let reference = build_reference(input)?;
    let mut gradients = input.initial_gradients.clone();
    let mut snapshot = build_snapshot(
        &reference,
        &trace.forest_trace.base_snapshot,
        &gradients,
        DynamicCoreGraphMetrics::default(),
        false,
    )?;
    snapshot.metrics.definition_checks = active_count(&snapshot)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != operations
                .len()
                .checked_add(1)
                .ok_or(DynamicCoreGraphError::TraceVerification)?
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }

    for (index, operation) in operations.iter().enumerate() {
        let event = trace
            .events
            .get(index)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let forest_event = trace
            .forest_trace
            .events
            .get(index)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        let after_gradients = audit_gradient_transition(&gradients, operation)?;
        let expected_updates = audit_update_batch(
            &reference,
            &forest_event.before,
            &forest_event.after,
            operation,
            &snapshot,
        )?;
        let mut replayed = snapshot.clone();
        audit_apply_core_updates(&mut replayed, &expected_updates)?;
        let metrics = audit_metrics(snapshot.metrics, &expected_updates)?;
        let mut expected_after = build_snapshot(
            &reference,
            &forest_event.after,
            &after_gradients,
            metrics,
            false,
        )?;
        expected_after.metrics.definition_checks = expected_after
            .metrics
            .definition_checks
            .checked_add(active_count(&expected_after)?)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        expected_after.metrics.state_transitions = expected_after
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        expected_after.metrics.source_updates = expected_after
            .metrics
            .source_updates
            .checked_add(1)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        expected_after.stage = snapshot
            .stage
            .checked_add(1)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        replayed
            .core_vertices
            .clone_from(&expected_after.core_vertices);
        replayed.active_node_count = expected_after.active_node_count;
        replayed.source_gradients.clone_from(&after_gradients);
        replayed.stage = expected_after.stage;
        replayed.metrics = expected_after.metrics;
        if replayed.edge_slots != expected_after.edge_slots {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        let expected_kind = DynamicCoreGraphEventKind::Updated {
            operation: Box::new(operation.clone()),
            forest_event: forest_event.kind.clone(),
            core_updates: expected_updates,
        };
        if event.kind != expected_kind || event.after != expected_after {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        gradients = after_gradients;
        snapshot = expected_after;
    }

    audit_completion(&snapshot, trace)
}

/// Executes atomic source batches with one LSF/Core boundary per outer stage.
///
/// # Errors
///
/// Rejects malformed/out-of-band source rows, incidence splits, attribute
/// changes, stage ordering, component failures, invariant failures, or overflow.
pub fn execute_dynamic_core_graph_stages(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicCoreGraphResult, DynamicCoreGraphError> {
    run_stage_internal(input, batches, false).map(|run| run.result)
}

/// Executes atomic source batches and records LSF/Core stage boundaries.
///
/// # Errors
///
/// Returns any component execution or independent core replay failure.
pub fn trace_dynamic_core_graph_stages(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicCoreGraphStageTraceResult, DynamicCoreGraphError> {
    let run = run_stage_internal(input, batches, true)?;
    let trace = DynamicCoreGraphStageTraceResult {
        forest_trace: run.forest_trace,
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_core_graph_stage_trace(input, batches, &trace)?;
    Ok(trace)
}

/// Independently replays atomic LSF and core batches.
///
/// # Errors
///
/// Rejects invalid input or any component, update order, attribute, mapping,
/// metric, Definition Core Graph, stage, or completion drift.
pub fn check_dynamic_core_graph_stage_trace(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicCoreGraphStageTraceResult,
) -> Result<(), DynamicCoreGraphError> {
    let forest_batches = audit_convert_stage_batches(input, batches)?;
    check_dynamic_low_stretch_forest_stage_trace(
        &input.forest,
        &forest_batches,
        &trace.forest_trace,
    )?;
    let reference = build_reference(input)?;
    let mut snapshot = build_snapshot(
        &reference,
        &trace.forest_trace.base_snapshot,
        &input.initial_gradients,
        DynamicCoreGraphMetrics::default(),
        false,
    )?;
    snapshot.metrics.definition_checks = active_count(&snapshot)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != batches
                .len()
                .checked_add(1)
                .ok_or(DynamicCoreGraphError::TraceVerification)?
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    for (index, batch) in batches.iter().enumerate() {
        let event = trace
            .events
            .get(index)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let forest_event = trace
            .forest_trace
            .events
            .get(index)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        let (updates, gradients) = audit_stage_update_batch(
            &reference,
            &forest_event.before,
            &forest_event.after,
            batch,
            &snapshot,
        )?;
        let mut replayed = snapshot.clone();
        audit_apply_core_updates(&mut replayed, &updates)?;
        let mut metrics = audit_metrics(snapshot.metrics, &updates)?;
        metrics.source_updates = metrics
            .source_updates
            .checked_add(
                u64::try_from(batch.updates.len())
                    .map_err(|_| DynamicCoreGraphError::TraceVerification)?,
            )
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        metrics.state_transitions = metrics
            .state_transitions
            .checked_add(1)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let mut expected =
            build_snapshot(&reference, &forest_event.after, &gradients, metrics, false)?;
        expected.stage = batch.outer_stage;
        expected.metrics.definition_checks = expected
            .metrics
            .definition_checks
            .checked_add(active_count(&expected)?)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        replayed.core_vertices.clone_from(&expected.core_vertices);
        replayed.active_node_count = expected.active_node_count;
        replayed.source_gradients.clone_from(&gradients);
        replayed.stage = expected.stage;
        replayed.metrics = expected.metrics;
        if replayed.edge_slots != expected.edge_slots {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        let kind = DynamicCoreGraphStageEventKind::Updated {
            batch: batch.clone(),
            forest_event: forest_event.kind.clone(),
            core_updates: updates,
        };
        if event.kind != kind || event.after != expected {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        snapshot = expected;
    }
    audit_stage_completion(&snapshot, trace)
}

struct InternalStageRun {
    forest_trace: DynamicLowStretchForestStageTraceResult,
    base_snapshot: DynamicCoreGraphSnapshot,
    events: Vec<DynamicCoreGraphStageTraceEvent>,
    result: DynamicCoreGraphResult,
}

fn run_stage_internal(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
    record: bool,
) -> Result<InternalStageRun, DynamicCoreGraphError> {
    let forest_batches = convert_stage_batches(input, batches)?;
    let forest_trace = trace_dynamic_low_stretch_forest_stages(&input.forest, &forest_batches)?;
    let reference = build_reference(input)?;
    let mut snapshot = build_snapshot(
        &reference,
        &forest_trace.base_snapshot,
        &input.initial_gradients,
        DynamicCoreGraphMetrics::default(),
        false,
    )?;
    snapshot.metrics.definition_checks = active_count(&snapshot)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { batches.len() + 1 } else { 0 });
    for (index, batch) in batches.iter().enumerate() {
        let forest_event = &forest_trace.events[index];
        let before = snapshot.clone();
        let (updates, gradients) = derive_stage_update_batch(
            &reference,
            &forest_event.before,
            &forest_event.after,
            batch,
            &snapshot,
        )?;
        let mut metrics = apply_metrics(snapshot.metrics, &updates)?;
        metrics.source_updates = metrics
            .source_updates
            .checked_add(
                u64::try_from(batch.updates.len())
                    .map_err(|_| DynamicCoreGraphError::ArithmeticOverflow)?,
            )
            .ok_or(DynamicCoreGraphError::ArithmeticOverflow)?;
        metrics.state_transitions = increment(metrics.state_transitions)?;
        snapshot = build_snapshot(&reference, &forest_event.after, &gradients, metrics, false)?;
        snapshot.stage = batch.outer_stage;
        snapshot.metrics.definition_checks = snapshot
            .metrics
            .definition_checks
            .checked_add(active_count(&snapshot)?)
            .ok_or(DynamicCoreGraphError::ArithmeticOverflow)?;
        verify_snapshot(&snapshot)?;
        if record {
            events.push(DynamicCoreGraphStageTraceEvent {
                catalog_id: CATALOG_ID,
                kind: DynamicCoreGraphStageEventKind::Updated {
                    batch: batch.clone(),
                    forest_event: forest_event.kind.clone(),
                    core_updates: updates,
                },
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicCoreGraphStageTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicCoreGraphStageEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalStageRun {
        forest_trace,
        base_snapshot,
        events,
        result: DynamicCoreGraphResult {
            final_snapshot: snapshot,
        },
    })
}

fn convert_stage_batches(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<Vec<DynamicLowStretchForestStageBatch>, DynamicCoreGraphError> {
    validate_and_convert(input, &[])?;
    if batches.len() > DYNAMIC_CORE_MAX_OPERATIONS {
        return Err(DynamicCoreGraphError::AdmissionLimit);
    }
    let mut source_rows = initial_stage_rows(input)?;
    let mut stage = 0_u64;
    let mut forest_batches = Vec::with_capacity(batches.len());
    for batch in batches {
        if stage.checked_add(1) != Some(batch.outer_stage) {
            return Err(DynamicCoreGraphError::InvalidInput);
        }
        let mut forest_updates = Vec::new();
        for update in &batch.updates {
            convert_stage_update(input, &mut source_rows, update, &mut forest_updates)?;
        }
        forest_batches.push(DynamicLowStretchForestStageBatch {
            outer_stage: batch.outer_stage,
            updates: forest_updates,
        });
        stage = batch.outer_stage;
    }
    Ok(forest_batches)
}

fn initial_stage_rows(
    input: &DynamicCoreGraphInput,
) -> Result<Vec<Option<DynamicCoreGraphStageEdge>>, DynamicCoreGraphError> {
    input
        .forest
        .edge_slots
        .iter()
        .zip(&input.initial_gradients)
        .map(|(edge, gradient)| match (edge, gradient) {
            (Some(edge), Some(gradient)) => Ok(Some(DynamicCoreGraphStageEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
                gradient: gradient.clone(),
            })),
            (None, None) => Ok(None),
            _ => Err(DynamicCoreGraphError::InvalidInput),
        })
        .collect()
}

fn convert_stage_update(
    input: &DynamicCoreGraphInput,
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    update: &DynamicCoreGraphStageUpdate,
    forest_updates: &mut Vec<DynamicLowStretchForestStageUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match update {
        DynamicCoreGraphStageUpdate::Insert { edge } => {
            validate_core_stage_edge(edge, source_rows.len(), input.forest.maximum_node_count)?;
            let slot = source_rows
                .get_mut(edge.edge)
                .ok_or(DynamicCoreGraphError::InvalidInput)?;
            if slot.is_some() {
                return Err(DynamicCoreGraphError::InvalidInput);
            }
            *slot = Some(edge.clone());
            forest_updates.push(DynamicLowStretchForestStageUpdate::Insert {
                edge: stage_edge_to_forest(edge),
            });
        }
        DynamicCoreGraphStageUpdate::Delete { edge } => {
            let slot = source_rows
                .get_mut(edge.edge)
                .ok_or(DynamicCoreGraphError::InvalidInput)?;
            if slot.as_ref() != Some(edge) {
                return Err(DynamicCoreGraphError::InvalidInput);
            }
            *slot = None;
            forest_updates.push(DynamicLowStretchForestStageUpdate::Delete {
                edge: stage_edge_to_forest(edge),
            });
        }
        DynamicCoreGraphStageUpdate::Reinsert { before, after } => {
            validate_core_stage_replacement(source_rows, before, after)?;
            source_rows[before.edge] = Some(after.clone());
            forest_updates.push(DynamicLowStretchForestStageUpdate::Reinsert {
                before: stage_edge_to_forest(before),
                after: stage_edge_to_forest(after),
            });
        }
        DynamicCoreGraphStageUpdate::ReplaceAttributes { before, after } => {
            validate_core_stage_replacement(source_rows, before, after)?;
            source_rows[before.edge] = Some(after.clone());
            if before.length != after.length {
                forest_updates.push(DynamicLowStretchForestStageUpdate::ReplaceLength {
                    edge: before.edge,
                    before: before.length.clone(),
                    after: after.length.clone(),
                });
            }
        }
        DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => {
            apply_core_stage_source_split(
                source_rows,
                *retained_vertex,
                *new_vertex,
                new_side_incidences,
            )?;
            forest_updates.push(DynamicLowStretchForestStageUpdate::VertexSplit {
                retained_vertex: *retained_vertex,
                new_vertex: *new_vertex,
                new_side_incidences: new_side_incidences
                    .iter()
                    .copied()
                    .map(core_incidence_to_forest)
                    .collect(),
                encoded_side: core_side_to_forest(*encoded_side),
                encoded_incidences: encoded_incidences
                    .iter()
                    .copied()
                    .map(core_incidence_to_forest)
                    .collect(),
            });
        }
    }
    Ok(())
}

fn validate_core_stage_edge(
    edge: &DynamicCoreGraphStageEdge,
    slots: usize,
    maximum_nodes: usize,
) -> Result<(), DynamicCoreGraphError> {
    if edge.edge >= slots || edge.from >= maximum_nodes || edge.to >= maximum_nodes {
        return Err(DynamicCoreGraphError::InvalidInput);
    }
    if edge.length <= BigRational::zero()
        || rational_too_wide(&edge.length)
        || rational_too_wide(&edge.gradient)
    {
        return Err(DynamicCoreGraphError::AdmissionLimit);
    }
    Ok(())
}

fn validate_core_stage_replacement(
    source_rows: &[Option<DynamicCoreGraphStageEdge>],
    before: &DynamicCoreGraphStageEdge,
    after: &DynamicCoreGraphStageEdge,
) -> Result<(), DynamicCoreGraphError> {
    validate_core_stage_edge(after, source_rows.len(), DYNAMIC_CORE_MAX_NODES)?;
    if before.edge != after.edge
        || before.from != after.from
        || before.to != after.to
        || source_rows.get(before.edge).and_then(Option::as_ref) != Some(before)
    {
        return Err(DynamicCoreGraphError::InvalidInput);
    }
    Ok(())
}

fn stage_edge_to_forest(edge: &DynamicCoreGraphStageEdge) -> DynamicLowStretchForestEdge {
    DynamicLowStretchForestEdge {
        edge: edge.edge,
        from: edge.from,
        to: edge.to,
        length: edge.length.clone(),
    }
}

fn core_incidence_to_forest(incidence: DynamicCoreIncidence) -> DynamicLowStretchForestIncidence {
    DynamicLowStretchForestIncidence {
        edge: incidence.edge,
        endpoint: match incidence.endpoint {
            DynamicCoreIncidenceEndpoint::Tail => DynamicLowStretchForestIncidenceEndpoint::Tail,
            DynamicCoreIncidenceEndpoint::Head => DynamicLowStretchForestIncidenceEndpoint::Head,
        },
    }
}

fn core_side_to_forest(side: DynamicCoreEncodedSide) -> DynamicLowStretchForestEncodedSide {
    match side {
        DynamicCoreEncodedSide::Retained => DynamicLowStretchForestEncodedSide::Retained,
        DynamicCoreEncodedSide::New => DynamicLowStretchForestEncodedSide::New,
    }
}

fn apply_core_stage_source_split(
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidences: &[DynamicCoreIncidence],
) -> Result<(), DynamicCoreGraphError> {
    for incidence in incidences {
        let row = source_rows
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
            .ok_or(DynamicCoreGraphError::InvalidInput)?;
        let endpoint = match incidence.endpoint {
            DynamicCoreIncidenceEndpoint::Tail => &mut row.from,
            DynamicCoreIncidenceEndpoint::Head => &mut row.to,
        };
        if *endpoint != retained_vertex {
            return Err(DynamicCoreGraphError::InvalidInput);
        }
        *endpoint = new_vertex;
    }
    Ok(())
}

fn audit_convert_stage_batches(
    input: &DynamicCoreGraphInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<Vec<DynamicLowStretchForestStageBatch>, DynamicCoreGraphError> {
    validate_and_convert(input, &[])?;
    if batches.len() > DYNAMIC_CORE_MAX_OPERATIONS {
        return Err(DynamicCoreGraphError::AdmissionLimit);
    }
    let mut rows = audit_initial_stage_rows(input)?;
    let mut expected_stage = 1_u64;
    let mut converted = Vec::with_capacity(batches.len());
    for batch in batches {
        if batch.outer_stage != expected_stage {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        let mut updates = Vec::new();
        for source in &batch.updates {
            audit_convert_stage_update(&mut rows, source, &mut updates)?;
        }
        converted.push(DynamicLowStretchForestStageBatch {
            outer_stage: batch.outer_stage,
            updates,
        });
        expected_stage = expected_stage
            .checked_add(1)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
    }
    Ok(converted)
}

fn audit_initial_stage_rows(
    input: &DynamicCoreGraphInput,
) -> Result<Vec<Option<DynamicCoreGraphStageEdge>>, DynamicCoreGraphError> {
    let mut rows = Vec::with_capacity(input.forest.edge_slots.len());
    if input.forest.edge_slots.len() != input.initial_gradients.len() {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    for slot in 0..input.forest.edge_slots.len() {
        match (
            &input.forest.edge_slots[slot],
            &input.initial_gradients[slot],
        ) {
            (Some(edge), Some(gradient)) => rows.push(Some(DynamicCoreGraphStageEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
                gradient: gradient.clone(),
            })),
            (None, None) => rows.push(None),
            _ => return Err(DynamicCoreGraphError::TraceVerification),
        }
    }
    Ok(rows)
}

fn audit_convert_stage_update(
    rows: &mut [Option<DynamicCoreGraphStageEdge>],
    source: &DynamicCoreGraphStageUpdate,
    converted: &mut Vec<DynamicLowStretchForestStageUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match source {
        DynamicCoreGraphStageUpdate::Insert { edge } => {
            audit_core_stage_edge(edge, rows.len())?;
            let Some(slot) = rows.get_mut(edge.edge) else {
                return Err(DynamicCoreGraphError::TraceVerification);
            };
            if slot.is_some() {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
            *slot = Some(edge.clone());
            converted.push(DynamicLowStretchForestStageUpdate::Insert {
                edge: audit_stage_edge_to_forest(edge),
            });
        }
        DynamicCoreGraphStageUpdate::Delete { edge } => {
            let Some(slot) = rows.get_mut(edge.edge) else {
                return Err(DynamicCoreGraphError::TraceVerification);
            };
            if slot.as_ref() != Some(edge) {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
            *slot = None;
            converted.push(DynamicLowStretchForestStageUpdate::Delete {
                edge: audit_stage_edge_to_forest(edge),
            });
        }
        DynamicCoreGraphStageUpdate::Reinsert { before, after } => {
            audit_core_stage_replacement(rows, before, after)?;
            rows[before.edge] = Some(after.clone());
            converted.push(DynamicLowStretchForestStageUpdate::Reinsert {
                before: audit_stage_edge_to_forest(before),
                after: audit_stage_edge_to_forest(after),
            });
        }
        DynamicCoreGraphStageUpdate::ReplaceAttributes { before, after } => {
            audit_core_stage_replacement(rows, before, after)?;
            rows[before.edge] = Some(after.clone());
            if before.length != after.length {
                converted.push(DynamicLowStretchForestStageUpdate::ReplaceLength {
                    edge: before.edge,
                    before: before.length.clone(),
                    after: after.length.clone(),
                });
            }
        }
        DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => {
            for incidence in new_side_incidences {
                let Some(row) = rows.get_mut(incidence.edge).and_then(Option::as_mut) else {
                    return Err(DynamicCoreGraphError::TraceVerification);
                };
                let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
                    &mut row.from
                } else {
                    &mut row.to
                };
                if *endpoint != *retained_vertex {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                *endpoint = *new_vertex;
            }
            converted.push(DynamicLowStretchForestStageUpdate::VertexSplit {
                retained_vertex: *retained_vertex,
                new_vertex: *new_vertex,
                new_side_incidences: new_side_incidences
                    .iter()
                    .map(|incidence| DynamicLowStretchForestIncidence {
                        edge: incidence.edge,
                        endpoint: if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
                            DynamicLowStretchForestIncidenceEndpoint::Tail
                        } else {
                            DynamicLowStretchForestIncidenceEndpoint::Head
                        },
                    })
                    .collect(),
                encoded_side: if *encoded_side == DynamicCoreEncodedSide::Retained {
                    DynamicLowStretchForestEncodedSide::Retained
                } else {
                    DynamicLowStretchForestEncodedSide::New
                },
                encoded_incidences: encoded_incidences
                    .iter()
                    .map(|incidence| DynamicLowStretchForestIncidence {
                        edge: incidence.edge,
                        endpoint: if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
                            DynamicLowStretchForestIncidenceEndpoint::Tail
                        } else {
                            DynamicLowStretchForestIncidenceEndpoint::Head
                        },
                    })
                    .collect(),
            });
        }
    }
    Ok(())
}

fn audit_core_stage_edge(
    edge: &DynamicCoreGraphStageEdge,
    slots: usize,
) -> Result<(), DynamicCoreGraphError> {
    if edge.edge >= slots
        || edge.from >= DYNAMIC_CORE_MAX_NODES
        || edge.to >= DYNAMIC_CORE_MAX_NODES
        || edge.length <= BigRational::zero()
        || audit_rational_too_wide(&edge.length)
        || audit_rational_too_wide(&edge.gradient)
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(())
}

fn audit_core_stage_replacement(
    rows: &[Option<DynamicCoreGraphStageEdge>],
    before: &DynamicCoreGraphStageEdge,
    after: &DynamicCoreGraphStageEdge,
) -> Result<(), DynamicCoreGraphError> {
    audit_core_stage_edge(after, rows.len())?;
    if before.edge != after.edge
        || before.from != after.from
        || before.to != after.to
        || rows.get(before.edge).and_then(Option::as_ref) != Some(before)
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(())
}

fn audit_stage_edge_to_forest(edge: &DynamicCoreGraphStageEdge) -> DynamicLowStretchForestEdge {
    DynamicLowStretchForestEdge {
        edge: edge.edge,
        from: edge.from,
        to: edge.to,
        length: edge.length.clone(),
    }
}

fn derive_stage_update_batch(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    batch: &DynamicCoreGraphStageBatch,
    core_before: &DynamicCoreGraphSnapshot,
) -> Result<(Vec<DynamicCoreUpdate>, Vec<Option<BigRational>>), DynamicCoreGraphError> {
    let mut updates = Vec::new();
    let mut working_roots = forest_before.roots.clone();
    let mut working_edges = core_before.edge_slots.clone();
    let new_source_vertices = batch
        .updates
        .iter()
        .filter_map(|update| match update {
            DynamicCoreGraphStageUpdate::VertexSplit { new_vertex, .. } => Some(*new_vertex),
            _ => None,
        })
        .collect::<Vec<_>>();
    derive_stage_root_splits(
        reference,
        forest_before,
        forest_after,
        &new_source_vertices,
        &mut working_roots,
        &mut working_edges,
        &mut updates,
    )?;
    let mut source_rows = source_rows_from_forest(forest_before, &core_before.source_gradients)?;
    let mut gradients = core_before.source_gradients.clone();
    let mut stretches = forest_before.stretch_overestimates.clone();
    for source_update in &batch.updates {
        derive_stage_source_update(
            reference,
            forest_after,
            source_update,
            &mut source_rows,
            &mut gradients,
            &mut stretches,
            &mut working_edges,
            &mut updates,
        )?;
    }
    let expected = build_snapshot(
        reference,
        forest_after,
        &gradients,
        core_before.metrics,
        false,
    )?;
    if working_edges != expected.edge_slots
        || source_rows_to_forest(&source_rows) != forest_after.edge_slots
        || stretches != forest_after.stretch_overestimates
    {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    Ok((updates, gradients))
}

fn derive_stage_root_splits(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    excluded_new_vertices: &[usize],
    working_roots: &mut Vec<usize>,
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let mut additions = forest_after
        .roots
        .iter()
        .copied()
        .filter(|root| !forest_before.roots.contains(root) && !excluded_new_vertices.contains(root))
        .collect::<Vec<_>>();
    additions.sort_unstable_by_key(|root| (reference.depth[*root], *root));
    for root in additions {
        let before_map =
            component_roots(reference, working_roots, forest_before.active_node_count)?;
        let retained = before_map[root];
        insert_sorted(working_roots, root);
        let after_map = component_roots(reference, working_roots, forest_before.active_node_count)?;
        let moved = moved_incidences_for_component_change(
            &forest_before.edge_slots,
            &before_map,
            &after_map,
            retained,
            root,
        );
        let split = make_split_update(retained, root, moved, working_edges)?;
        apply_split_to_edges(working_edges, &split)?;
        updates.push(split);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn derive_stage_source_update(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    source_update: &DynamicCoreGraphStageUpdate,
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &mut [Option<BigRational>],
    stretches: &mut [Option<BigRational>],
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match source_update {
        DynamicCoreGraphStageUpdate::Insert { edge } => {
            source_rows[edge.edge] = Some(edge.clone());
            gradients[edge.edge] = Some(edge.gradient.clone());
            stretches[edge.edge] = Some(BigRational::from_integer(BigInt::from(1)));
            let core = stage_core_row(
                reference,
                forest_after,
                edge,
                &BigRational::from_integer(BigInt::from(1)),
            )?;
            working_edges[edge.edge] = Some(core.clone());
            updates.push(DynamicCoreUpdate::EdgeInserted { edge: core });
        }
        DynamicCoreGraphStageUpdate::Delete { edge } => {
            let removed = working_edges[edge.edge]
                .take()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?;
            source_rows[edge.edge] = None;
            gradients[edge.edge] = None;
            stretches[edge.edge] = None;
            updates.push(DynamicCoreUpdate::EdgeDeleted { edge: removed });
        }
        DynamicCoreGraphStageUpdate::Reinsert { before, after } => {
            let core_before = working_edges[before.edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .clone();
            source_rows[before.edge] = Some(after.clone());
            gradients[before.edge] = Some(after.gradient.clone());
            stretches[before.edge] = Some(BigRational::from_integer(BigInt::from(1)));
            let core_after = stage_core_row(
                reference,
                forest_after,
                after,
                &BigRational::from_integer(BigInt::from(1)),
            )?;
            working_edges[before.edge] = Some(core_after.clone());
            updates.push(DynamicCoreUpdate::EdgeReinserted {
                before: core_before,
                after: core_after,
            });
        }
        DynamicCoreGraphStageUpdate::ReplaceAttributes { before, after } => {
            derive_stage_attribute_replacement(
                reference,
                forest_after,
                before,
                after,
                source_rows,
                gradients,
                stretches,
                working_edges,
                updates,
            )?;
        }
        DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            ..
        } => derive_stage_vertex_split(
            reference,
            *retained_vertex,
            *new_vertex,
            new_side_incidences,
            source_rows,
            gradients,
            working_edges,
            updates,
        )?,
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn derive_stage_attribute_replacement(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    before: &DynamicCoreGraphStageEdge,
    after: &DynamicCoreGraphStageEdge,
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &mut [Option<BigRational>],
    stretches: &[Option<BigRational>],
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let core_before = working_edges[before.edge]
        .as_ref()
        .ok_or(DynamicCoreGraphError::InvariantViolation)?
        .clone();
    source_rows[before.edge] = Some(after.clone());
    gradients[before.edge] = Some(after.gradient.clone());
    let stretch = stretches[before.edge]
        .as_ref()
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let core_after = stage_core_row(reference, forest_after, after, stretch)?;
    if core_before.length != core_after.length {
        updates.push(DynamicCoreUpdate::LengthReplaced {
            edge: before.edge,
            before: core_before.length.clone(),
            after: core_after.length.clone(),
        });
    }
    if core_before.gradient != core_after.gradient {
        updates.push(DynamicCoreUpdate::GradientReplaced {
            edge: before.edge,
            before: core_before.gradient.clone(),
            after: core_after.gradient.clone(),
        });
    }
    working_edges[before.edge] = Some(core_after);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn derive_stage_vertex_split(
    reference: &StaticReference,
    retained_vertex: usize,
    new_vertex: usize,
    incidences: &[DynamicCoreIncidence],
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &[Option<BigRational>],
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let split = make_split_update(
        retained_vertex,
        new_vertex,
        incidences.to_vec(),
        working_edges,
    )?;
    apply_split_to_edges(working_edges, &split)?;
    updates.push(split);
    let mut changed_edges = Vec::new();
    for incidence in incidences {
        apply_stage_source_incidence(source_rows, retained_vertex, new_vertex, *incidence)?;
        changed_edges.push(incidence.edge);
    }
    changed_edges.sort_unstable();
    changed_edges.dedup();
    for edge in changed_edges {
        let before_gradient = working_edges[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?
            .gradient
            .clone();
        let source = source_rows[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let source_gradient = gradients[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let after_gradient =
            source_gradient + tree_path_gradient(reference, source.to, source.from)?;
        if before_gradient != after_gradient {
            working_edges[edge]
                .as_mut()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .gradient = after_gradient.clone();
            updates.push(DynamicCoreUpdate::GradientReplaced {
                edge,
                before: before_gradient,
                after: after_gradient,
            });
        }
    }
    Ok(())
}

fn apply_stage_source_incidence(
    rows: &mut [Option<DynamicCoreGraphStageEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidence: DynamicCoreIncidence,
) -> Result<(), DynamicCoreGraphError> {
    let row = rows
        .get_mut(incidence.edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
        &mut row.from
    } else {
        &mut row.to
    };
    if *endpoint != retained_vertex {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    *endpoint = new_vertex;
    Ok(())
}

fn stage_core_row(
    reference: &StaticReference,
    forest: &DynamicLowStretchForestSnapshot,
    source: &DynamicCoreGraphStageEdge,
    stretch: &BigRational,
) -> Result<DynamicCoreEdge, DynamicCoreGraphError> {
    let from = *forest
        .component_roots
        .get(source.from)
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let to = *forest
        .component_roots
        .get(source.to)
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let gradient = &source.gradient + tree_path_gradient(reference, source.to, source.from)?;
    let length = stretch * &source.length;
    if rational_too_wide(&gradient) || rational_too_wide(&length) {
        return Err(DynamicCoreGraphError::AdmissionLimit);
    }
    Ok(DynamicCoreEdge {
        edge: source.edge,
        from,
        to,
        length,
        gradient,
    })
}

fn source_rows_from_forest(
    forest: &DynamicLowStretchForestSnapshot,
    gradients: &[Option<BigRational>],
) -> Result<Vec<Option<DynamicCoreGraphStageEdge>>, DynamicCoreGraphError> {
    forest
        .edge_slots
        .iter()
        .zip(gradients)
        .map(|(edge, gradient)| match (edge, gradient) {
            (Some(edge), Some(gradient)) => Ok(Some(DynamicCoreGraphStageEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
                gradient: gradient.clone(),
            })),
            (None, None) => Ok(None),
            _ => Err(DynamicCoreGraphError::InvariantViolation),
        })
        .collect()
}

fn source_rows_to_forest(
    source_rows: &[Option<DynamicCoreGraphStageEdge>],
) -> Vec<Option<DynamicLowStretchForestEdge>> {
    source_rows
        .iter()
        .map(|row| row.as_ref().map(stage_edge_to_forest))
        .collect()
}

fn audit_stage_update_batch(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    batch: &DynamicCoreGraphStageBatch,
    core_before: &DynamicCoreGraphSnapshot,
) -> Result<(Vec<DynamicCoreUpdate>, Vec<Option<BigRational>>), DynamicCoreGraphError> {
    let mut expected = Vec::new();
    let mut roots = forest_before.roots.clone();
    let mut edges = core_before.edge_slots.clone();
    let excluded = batch
        .updates
        .iter()
        .filter_map(|update| match update {
            DynamicCoreGraphStageUpdate::VertexSplit { new_vertex, .. } => Some(*new_vertex),
            _ => None,
        })
        .collect::<Vec<_>>();
    audit_stage_root_splits(
        reference,
        forest_before,
        forest_after,
        &excluded,
        &mut roots,
        &mut edges,
        &mut expected,
    )?;
    let mut source_rows =
        audit_source_rows_from_forest(forest_before, &core_before.source_gradients)?;
    let mut gradients = core_before.source_gradients.clone();
    let mut stretches = forest_before.stretch_overestimates.clone();
    for source in &batch.updates {
        audit_stage_source_update(
            reference,
            forest_after,
            source,
            &mut source_rows,
            &mut gradients,
            &mut stretches,
            &mut edges,
            &mut expected,
        )?;
    }
    let rebuilt = build_snapshot(
        reference,
        forest_after,
        &gradients,
        core_before.metrics,
        false,
    )?;
    if edges != rebuilt.edge_slots
        || audit_source_rows_to_forest(&source_rows) != forest_after.edge_slots
        || stretches != forest_after.stretch_overestimates
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok((expected, gradients))
}

fn audit_stage_root_splits(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    excluded: &[usize],
    roots: &mut Vec<usize>,
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let mut additions = forest_after
        .roots
        .iter()
        .copied()
        .filter(|root| !forest_before.roots.contains(root) && !excluded.contains(root))
        .collect::<Vec<_>>();
    additions.sort_unstable_by_key(|root| (reference.depth[*root], *root));
    for new_root in additions {
        let before_map = component_roots(reference, roots, forest_before.active_node_count)?;
        let retained = before_map[new_root];
        insert_sorted(roots, new_root);
        let after_map = component_roots(reference, roots, forest_before.active_node_count)?;
        let mut moved = Vec::new();
        for row in forest_before.edge_slots.iter().flatten() {
            if before_map[row.from] == retained && after_map[row.from] == new_root {
                moved.push(DynamicCoreIncidence {
                    edge: row.edge,
                    endpoint: DynamicCoreIncidenceEndpoint::Tail,
                });
            }
            if before_map[row.to] == retained && after_map[row.to] == new_root {
                moved.push(DynamicCoreIncidence {
                    edge: row.edge,
                    endpoint: DynamicCoreIncidenceEndpoint::Head,
                });
            }
        }
        moved.sort_unstable();
        let split = audit_make_split(retained, new_root, moved, edges)?;
        audit_apply_split(edges, &split)?;
        updates.push(split);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn audit_stage_source_update(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    source: &DynamicCoreGraphStageUpdate,
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &mut [Option<BigRational>],
    stretches: &mut [Option<BigRational>],
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match source {
        DynamicCoreGraphStageUpdate::Insert { edge } => {
            if source_rows[edge.edge].is_some() || edges[edge.edge].is_some() {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
            source_rows[edge.edge] = Some(edge.clone());
            gradients[edge.edge] = Some(edge.gradient.clone());
            stretches[edge.edge] = Some(BigRational::from_integer(BigInt::from(1)));
            let row = audit_stage_core_row(
                reference,
                forest_after,
                edge,
                &BigRational::from_integer(BigInt::from(1)),
            )?;
            edges[edge.edge] = Some(row.clone());
            updates.push(DynamicCoreUpdate::EdgeInserted { edge: row });
        }
        DynamicCoreGraphStageUpdate::Delete { edge } => {
            let removed = edges[edge.edge]
                .take()
                .ok_or(DynamicCoreGraphError::TraceVerification)?;
            source_rows[edge.edge] = None;
            gradients[edge.edge] = None;
            stretches[edge.edge] = None;
            updates.push(DynamicCoreUpdate::EdgeDeleted { edge: removed });
        }
        DynamicCoreGraphStageUpdate::Reinsert { before, after } => {
            let old = edges[before.edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .clone();
            source_rows[before.edge] = Some(after.clone());
            gradients[before.edge] = Some(after.gradient.clone());
            stretches[before.edge] = Some(BigRational::from_integer(BigInt::from(1)));
            let new = audit_stage_core_row(
                reference,
                forest_after,
                after,
                &BigRational::from_integer(BigInt::from(1)),
            )?;
            edges[before.edge] = Some(new.clone());
            updates.push(DynamicCoreUpdate::EdgeReinserted {
                before: old,
                after: new,
            });
        }
        DynamicCoreGraphStageUpdate::ReplaceAttributes { before, after } => {
            audit_stage_attribute_replacement(
                reference,
                forest_after,
                before,
                after,
                source_rows,
                gradients,
                stretches,
                edges,
                updates,
            )?;
        }
        DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            ..
        } => audit_stage_vertex_split(
            reference,
            *retained_vertex,
            *new_vertex,
            new_side_incidences,
            source_rows,
            gradients,
            edges,
            updates,
        )?,
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn audit_stage_attribute_replacement(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    before: &DynamicCoreGraphStageEdge,
    after: &DynamicCoreGraphStageEdge,
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &mut [Option<BigRational>],
    stretches: &[Option<BigRational>],
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let old = edges[before.edge]
        .as_ref()
        .ok_or(DynamicCoreGraphError::TraceVerification)?
        .clone();
    source_rows[before.edge] = Some(after.clone());
    gradients[before.edge] = Some(after.gradient.clone());
    let stretch = stretches[before.edge]
        .as_ref()
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    let new = audit_stage_core_row(reference, forest_after, after, stretch)?;
    if old.length != new.length {
        updates.push(DynamicCoreUpdate::LengthReplaced {
            edge: before.edge,
            before: old.length.clone(),
            after: new.length.clone(),
        });
    }
    if old.gradient != new.gradient {
        updates.push(DynamicCoreUpdate::GradientReplaced {
            edge: before.edge,
            before: old.gradient.clone(),
            after: new.gradient.clone(),
        });
    }
    edges[before.edge] = Some(new);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn audit_stage_vertex_split(
    reference: &StaticReference,
    retained_vertex: usize,
    new_vertex: usize,
    incidences: &[DynamicCoreIncidence],
    source_rows: &mut [Option<DynamicCoreGraphStageEdge>],
    gradients: &[Option<BigRational>],
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    let split = audit_make_split(retained_vertex, new_vertex, incidences.to_vec(), edges)?;
    audit_apply_split(edges, &split)?;
    updates.push(split);
    let mut changed = Vec::new();
    for incidence in incidences {
        let row = source_rows
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
            &mut row.from
        } else {
            &mut row.to
        };
        if *endpoint != retained_vertex {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        *endpoint = new_vertex;
        changed.push(incidence.edge);
    }
    changed.sort_unstable();
    changed.dedup();
    for edge in changed {
        let old = edges[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?
            .gradient
            .clone();
        let source = source_rows[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let base = gradients[edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let new = base + tree_path_gradient(reference, source.to, source.from)?;
        if old != new {
            edges[edge]
                .as_mut()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .gradient = new.clone();
            updates.push(DynamicCoreUpdate::GradientReplaced {
                edge,
                before: old,
                after: new,
            });
        }
    }
    Ok(())
}

fn audit_stage_core_row(
    reference: &StaticReference,
    forest: &DynamicLowStretchForestSnapshot,
    source: &DynamicCoreGraphStageEdge,
    stretch: &BigRational,
) -> Result<DynamicCoreEdge, DynamicCoreGraphError> {
    let from = forest
        .component_roots
        .get(source.from)
        .copied()
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    let to = forest
        .component_roots
        .get(source.to)
        .copied()
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    let gradient = source.gradient.clone() + tree_path_gradient(reference, source.to, source.from)?;
    let length = stretch.clone() * source.length.clone();
    if audit_rational_too_wide(&gradient) || audit_rational_too_wide(&length) {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(DynamicCoreEdge {
        edge: source.edge,
        from,
        to,
        length,
        gradient,
    })
}

fn audit_source_rows_from_forest(
    forest: &DynamicLowStretchForestSnapshot,
    gradients: &[Option<BigRational>],
) -> Result<Vec<Option<DynamicCoreGraphStageEdge>>, DynamicCoreGraphError> {
    if forest.edge_slots.len() != gradients.len() {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    let mut rows = Vec::with_capacity(forest.edge_slots.len());
    for (edge, gradient) in forest.edge_slots.iter().zip(gradients) {
        match (edge, gradient) {
            (Some(edge), Some(gradient)) => rows.push(Some(DynamicCoreGraphStageEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
                gradient: gradient.clone(),
            })),
            (None, None) => rows.push(None),
            _ => return Err(DynamicCoreGraphError::TraceVerification),
        }
    }
    Ok(rows)
}

fn audit_source_rows_to_forest(
    rows: &[Option<DynamicCoreGraphStageEdge>],
) -> Vec<Option<DynamicLowStretchForestEdge>> {
    rows.iter()
        .map(|row| {
            row.as_ref().map(|edge| DynamicLowStretchForestEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
            })
        })
        .collect()
}

fn audit_stage_completion(
    snapshot: &DynamicCoreGraphSnapshot,
    trace: &DynamicCoreGraphStageTraceResult,
) -> Result<(), DynamicCoreGraphError> {
    let completion = trace
        .events
        .last()
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.complete = true;
    expected.metrics.state_transitions = expected
        .metrics
        .state_transitions
        .checked_add(1)
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != DynamicCoreGraphStageEventKind::Completed
        || completion.before != *snapshot
        || completion.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(())
}

fn audit_completion(
    snapshot: &DynamicCoreGraphSnapshot,
    trace: &DynamicCoreGraphTraceResult,
) -> Result<(), DynamicCoreGraphError> {
    let completion = trace
        .events
        .last()
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.complete = true;
    expected.metrics.state_transitions = expected
        .metrics
        .state_transitions
        .checked_add(1)
        .ok_or(DynamicCoreGraphError::TraceVerification)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != DynamicCoreGraphEventKind::Completed
        || completion.before != *snapshot
        || completion.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    input: &DynamicCoreGraphInput,
    operations: &[DynamicCoreGraphOperation],
    record: bool,
) -> Result<InternalRun, DynamicCoreGraphError> {
    let forest_operations = validate_and_convert(input, operations)?;
    let forest_trace = trace_dynamic_low_stretch_forest(&input.forest, &forest_operations)?;
    let reference = build_reference(input)?;
    let mut gradients = input.initial_gradients.clone();
    let mut snapshot = build_snapshot(
        &reference,
        &forest_trace.base_snapshot,
        &gradients,
        DynamicCoreGraphMetrics::default(),
        false,
    )?;
    snapshot.metrics.definition_checks = active_count(&snapshot)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { operations.len() + 1 } else { 0 });

    for (index, operation) in operations.iter().enumerate() {
        let forest_event = &forest_trace.events[index];
        let before = snapshot.clone();
        let updates = derive_update_batch(
            &reference,
            &forest_event.before,
            &forest_event.after,
            operation,
            &snapshot,
        )?;
        gradients = apply_gradient_transition(&gradients, operation)?;
        let mut metrics = apply_metrics(snapshot.metrics, &updates)?;
        metrics.source_updates = increment(metrics.source_updates)?;
        metrics.state_transitions = increment(metrics.state_transitions)?;
        snapshot = build_snapshot(&reference, &forest_event.after, &gradients, metrics, false)?;
        snapshot.stage = increment(before.stage)?;
        snapshot.metrics.definition_checks = snapshot
            .metrics
            .definition_checks
            .checked_add(active_count(&snapshot)?)
            .ok_or(DynamicCoreGraphError::ArithmeticOverflow)?;
        verify_snapshot(&snapshot)?;
        if record {
            events.push(DynamicCoreGraphTraceEvent {
                catalog_id: CATALOG_ID,
                kind: DynamicCoreGraphEventKind::Updated {
                    operation: Box::new(operation.clone()),
                    forest_event: forest_event.kind.clone(),
                    core_updates: updates,
                },
                before,
                after: snapshot.clone(),
            });
        }
    }

    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicCoreGraphTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicCoreGraphEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalRun {
        forest_trace,
        base_snapshot,
        events,
        result: DynamicCoreGraphResult {
            final_snapshot: snapshot,
        },
    })
}

fn validate_and_convert(
    input: &DynamicCoreGraphInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<Vec<DynamicLowStretchForestOperation>, DynamicCoreGraphError> {
    if input.forest.maximum_node_count > DYNAMIC_CORE_MAX_NODES
        || input.forest.edge_slots.len() > DYNAMIC_CORE_MAX_EDGES
        || operations.len() > DYNAMIC_CORE_MAX_OPERATIONS
        || input.initial_gradients.len() != input.forest.edge_slots.len()
        || input
            .initial_gradients
            .iter()
            .zip(&input.forest.edge_slots)
            .any(|(gradient, edge)| gradient.is_some() != edge.is_some())
    {
        return Err(DynamicCoreGraphError::InvalidInput);
    }
    for gradient in input.initial_gradients.iter().flatten() {
        if rational_too_wide(gradient) {
            return Err(DynamicCoreGraphError::AdmissionLimit);
        }
    }
    let mut converted = Vec::with_capacity(operations.len());
    for operation in operations {
        match operation {
            DynamicCoreGraphOperation::Insert { edge, gradient } => {
                if rational_too_wide(gradient) {
                    return Err(DynamicCoreGraphError::AdmissionLimit);
                }
                converted.push(DynamicLowStretchForestOperation::Insert { edge: edge.clone() });
            }
            DynamicCoreGraphOperation::Delete { edge } => {
                converted.push(DynamicLowStretchForestOperation::Delete { edge: *edge });
            }
            DynamicCoreGraphOperation::Reinsert { edge } => {
                converted.push(DynamicLowStretchForestOperation::Reinsert { edge: *edge });
            }
            DynamicCoreGraphOperation::VertexSplit {
                vertex,
                new_vertex,
                moved_edges,
            } => converted.push(DynamicLowStretchForestOperation::VertexSplit {
                vertex: *vertex,
                new_vertex: *new_vertex,
                moved_edges: moved_edges.clone(),
            }),
        }
    }
    Ok(converted)
}

fn build_reference(
    input: &DynamicCoreGraphInput,
) -> Result<StaticReference, DynamicCoreGraphError> {
    let node_count = input.forest.initial_node_count;
    let mut adjacency = vec![Vec::<(usize, usize)>::new(); node_count];
    let mut tree_gradients = vec![None; input.forest.edge_slots.len()];
    let mut tree_edges = vec![None; input.forest.edge_slots.len()];
    for &edge_id in &input.forest.reference_tree_edges {
        let edge = input
            .forest
            .edge_slots
            .get(edge_id)
            .and_then(Option::as_ref)
            .ok_or(DynamicCoreGraphError::InvalidInput)?;
        let gradient = input
            .initial_gradients
            .get(edge_id)
            .and_then(Option::as_ref)
            .ok_or(DynamicCoreGraphError::InvalidInput)?;
        adjacency[edge.from].push((edge.to, edge_id));
        adjacency[edge.to].push((edge.from, edge_id));
        tree_gradients[edge_id] = Some(gradient.clone());
        tree_edges[edge_id] = Some(edge.clone());
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable_by_key(|&(node, edge)| (edge, node));
    }
    let mut parent = vec![None; input.forest.maximum_node_count];
    let mut parent_edge = vec![None; input.forest.maximum_node_count];
    let mut depth = vec![0_usize; input.forest.maximum_node_count];
    let mut seen = vec![false; node_count];
    let mut stack = vec![input.forest.reference_root];
    if input.forest.reference_root >= node_count {
        return Err(DynamicCoreGraphError::InvalidInput);
    }
    seen[input.forest.reference_root] = true;
    while let Some(node) = stack.pop() {
        for &(next, edge) in adjacency[node].iter().rev() {
            if seen[next] {
                continue;
            }
            seen[next] = true;
            parent[next] = Some(node);
            parent_edge[next] = Some(edge);
            depth[next] = depth[node]
                .checked_add(1)
                .ok_or(DynamicCoreGraphError::ArithmeticOverflow)?;
            stack.push(next);
        }
    }
    if seen.iter().any(|seen| !seen) {
        return Err(DynamicCoreGraphError::InvalidInput);
    }
    let mut congestion = Vec::new();
    for &tree_edge in &input.forest.reference_tree_edges {
        let mut total = BigRational::zero();
        for edge in input.forest.edge_slots.iter().flatten() {
            if core_path_contains_tree_edge(&parent, &parent_edge, &depth, edge, tree_edge)? {
                total += BigRational::one() / &edge.length;
            }
        }
        congestion.push((total, tree_edge));
    }
    congestion.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    let mut pi_rank = vec![None; input.forest.edge_slots.len()];
    for (rank, (_, edge)) in congestion.into_iter().enumerate() {
        pi_rank[edge] = Some(rank);
    }
    Ok(StaticReference {
        initial_node_count: node_count,
        root: input.forest.reference_root,
        parent,
        parent_edge,
        depth,
        pi_rank,
        tree_gradients,
        tree_edges,
    })
}

fn core_path_contains_tree_edge(
    parent: &[Option<usize>],
    parent_edge: &[Option<usize>],
    depth: &[usize],
    edge: &DynamicLowStretchForestEdge,
    target: usize,
) -> Result<bool, DynamicCoreGraphError> {
    let mut left = edge.from;
    let mut right = edge.to;
    while depth[left] > depth[right] {
        if parent_edge[left] == Some(target) {
            return Ok(true);
        }
        left = parent[left].ok_or(DynamicCoreGraphError::InvalidInput)?;
    }
    while depth[right] > depth[left] {
        if parent_edge[right] == Some(target) {
            return Ok(true);
        }
        right = parent[right].ok_or(DynamicCoreGraphError::InvalidInput)?;
    }
    while left != right {
        if parent_edge[left] == Some(target) || parent_edge[right] == Some(target) {
            return Ok(true);
        }
        left = parent[left].ok_or(DynamicCoreGraphError::InvalidInput)?;
        right = parent[right].ok_or(DynamicCoreGraphError::InvalidInput)?;
    }
    Ok(false)
}

fn build_snapshot(
    reference: &StaticReference,
    forest: &DynamicLowStretchForestSnapshot,
    gradients: &[Option<BigRational>],
    metrics: DynamicCoreGraphMetrics,
    complete: bool,
) -> Result<DynamicCoreGraphSnapshot, DynamicCoreGraphError> {
    if gradients.len() != forest.edge_slots.len() {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    let mut edge_slots = vec![None; forest.edge_slots.len()];
    for edge in forest.edge_slots.iter().flatten() {
        let source_gradient = gradients[edge.edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let stretch = forest.stretch_overestimates[edge.edge]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let from = *forest
            .component_roots
            .get(edge.from)
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let to = *forest
            .component_roots
            .get(edge.to)
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let gradient = source_gradient + tree_path_gradient(reference, edge.to, edge.from)?;
        let length = stretch * &edge.length;
        if rational_too_wide(&gradient) || rational_too_wide(&length) {
            return Err(DynamicCoreGraphError::AdmissionLimit);
        }
        edge_slots[edge.edge] = Some(DynamicCoreEdge {
            edge: edge.edge,
            from,
            to,
            length,
            gradient,
        });
    }
    if gradients
        .iter()
        .zip(&forest.edge_slots)
        .any(|(gradient, edge)| gradient.is_some() != edge.is_some())
    {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    let snapshot = DynamicCoreGraphSnapshot {
        active_node_count: forest.active_node_count,
        core_vertices: forest.roots.clone(),
        edge_slots,
        source_gradients: gradients.to_vec(),
        stage: forest.stage,
        complete,
        metrics,
    };
    verify_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn tree_path_gradient(
    reference: &StaticReference,
    from: usize,
    to: usize,
) -> Result<BigRational, DynamicCoreGraphError> {
    if from >= reference.initial_node_count || to >= reference.initial_node_count {
        return Ok(BigRational::zero());
    }
    let mut left = from;
    let mut right = to;
    let mut total = BigRational::zero();
    while reference.depth[left] > reference.depth[right] {
        total += upward_gradient(reference, left)?;
        left = reference.parent[left].ok_or(DynamicCoreGraphError::InvariantViolation)?;
    }
    while reference.depth[right] > reference.depth[left] {
        total -= upward_gradient(reference, right)?;
        right = reference.parent[right].ok_or(DynamicCoreGraphError::InvariantViolation)?;
    }
    while left != right {
        total += upward_gradient(reference, left)?;
        total -= upward_gradient(reference, right)?;
        left = reference.parent[left].ok_or(DynamicCoreGraphError::InvariantViolation)?;
        right = reference.parent[right].ok_or(DynamicCoreGraphError::InvariantViolation)?;
    }
    Ok(total)
}

fn upward_gradient(
    reference: &StaticReference,
    child: usize,
) -> Result<BigRational, DynamicCoreGraphError> {
    let parent = reference.parent[child].ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let edge_id = reference.parent_edge[child].ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let edge = reference.tree_edges[edge_id]
        .as_ref()
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    let gradient = reference.tree_gradients[edge_id]
        .as_ref()
        .ok_or(DynamicCoreGraphError::InvariantViolation)?;
    if edge.from == child && edge.to == parent {
        Ok(gradient.clone())
    } else if edge.from == parent && edge.to == child {
        Ok(-gradient)
    } else {
        Err(DynamicCoreGraphError::InvariantViolation)
    }
}

fn derive_update_batch(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    operation: &DynamicCoreGraphOperation,
    core_before: &DynamicCoreGraphSnapshot,
) -> Result<Vec<DynamicCoreUpdate>, DynamicCoreGraphError> {
    let context = CoreTransitionContext {
        reference,
        forest_before,
        forest_after,
        operation,
        core_before,
    };
    let mut updates = Vec::new();
    let mut working_roots = forest_before.roots.clone();
    let mut working_edges = core_before.edge_slots.clone();
    let mut roots_added: Vec<_> = forest_after
        .roots
        .iter()
        .copied()
        .filter(|root| !forest_before.roots.contains(root))
        .collect();
    let split_new_vertex = match operation {
        DynamicCoreGraphOperation::VertexSplit { new_vertex, .. } => Some(*new_vertex),
        _ => None,
    };
    roots_added.retain(|root| Some(*root) != split_new_vertex);
    roots_added.sort_unstable_by_key(|root| (reference.depth[*root], *root));
    for root in roots_added {
        let old_components =
            component_roots(reference, &working_roots, forest_before.active_node_count)?;
        let retained = old_components[root];
        insert_sorted(&mut working_roots, root);
        let new_components =
            component_roots(reference, &working_roots, forest_before.active_node_count)?;
        let moved = moved_incidences_for_component_change(
            &forest_before.edge_slots,
            &old_components,
            &new_components,
            retained,
            root,
        );
        let update = make_split_update(retained, root, moved, &working_edges)?;
        apply_split_to_edges(&mut working_edges, &update)?;
        updates.push(update);
    }

    derive_source_update(&context, &working_roots, &mut working_edges, &mut updates)?;
    let gradients = apply_gradient_transition(&core_before.source_gradients, operation)?;
    let expected = build_snapshot(
        reference,
        forest_after,
        &gradients,
        core_before.metrics,
        false,
    )?;
    if working_edges != expected.edge_slots {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    Ok(updates)
}

fn derive_source_update(
    context: &CoreTransitionContext<'_>,
    working_roots: &[usize],
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match context.operation {
        DynamicCoreGraphOperation::Insert { edge, gradient } => {
            let mut gradients = context.core_before.source_gradients.clone();
            gradients[edge.edge] = Some(gradient.clone());
            let after = build_snapshot(
                context.reference,
                context.forest_after,
                &gradients,
                context.core_before.metrics,
                false,
            )?;
            let inserted = after.edge_slots[edge.edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .clone();
            working_edges[edge.edge] = Some(inserted.clone());
            updates.push(DynamicCoreUpdate::EdgeInserted { edge: inserted });
        }
        DynamicCoreGraphOperation::Delete { edge } => {
            let deleted = working_edges[*edge]
                .take()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?;
            updates.push(DynamicCoreUpdate::EdgeDeleted { edge: deleted });
        }
        DynamicCoreGraphOperation::Reinsert { edge } => {
            let before = working_edges[*edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .clone();
            let after = build_snapshot(
                context.reference,
                context.forest_after,
                &context.core_before.source_gradients,
                context.core_before.metrics,
                false,
            )?;
            let inserted = after.edge_slots[*edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .clone();
            working_edges[*edge] = Some(inserted.clone());
            updates.push(DynamicCoreUpdate::EdgeReinserted {
                before,
                after: inserted,
            });
        }
        DynamicCoreGraphOperation::VertexSplit {
            vertex,
            new_vertex,
            moved_edges,
        } => {
            let components = component_roots(
                context.reference,
                working_roots,
                context.forest_before.active_node_count,
            )?;
            let retained = components[*vertex];
            let moved =
                source_split_incidences(&context.forest_before.edge_slots, *vertex, moved_edges)?;
            let split = make_split_update(retained, *new_vertex, moved, working_edges)?;
            apply_split_to_edges(working_edges, &split)?;
            updates.push(split);
            derive_split_gradient_updates(
                context.reference,
                context.forest_after,
                moved_edges,
                &context.core_before.source_gradients,
                working_edges,
                updates,
            )?;
        }
    }
    Ok(())
}

fn derive_split_gradient_updates(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    moved_edges: &[usize],
    source_gradients: &[Option<BigRational>],
    working_edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    for &edge_id in moved_edges {
        let before = working_edges[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?
            .gradient
            .clone();
        let after_edge = forest_after.edge_slots[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let source_gradient = source_gradients[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let after =
            source_gradient + tree_path_gradient(reference, after_edge.to, after_edge.from)?;
        if before != after {
            working_edges[edge_id]
                .as_mut()
                .ok_or(DynamicCoreGraphError::InvariantViolation)?
                .gradient = after.clone();
            updates.push(DynamicCoreUpdate::GradientReplaced {
                edge: edge_id,
                before,
                after,
            });
        }
    }
    Ok(())
}

fn audit_update_batch(
    reference: &StaticReference,
    forest_before: &DynamicLowStretchForestSnapshot,
    forest_after: &DynamicLowStretchForestSnapshot,
    operation: &DynamicCoreGraphOperation,
    core_before: &DynamicCoreGraphSnapshot,
) -> Result<Vec<DynamicCoreUpdate>, DynamicCoreGraphError> {
    let context = CoreTransitionContext {
        reference,
        forest_before,
        forest_after,
        operation,
        core_before,
    };
    let mut updates = Vec::new();
    let mut roots = forest_before.roots.clone();
    let mut edges = core_before.edge_slots.clone();
    let excluded = match operation {
        DynamicCoreGraphOperation::VertexSplit { new_vertex, .. } => Some(*new_vertex),
        _ => None,
    };
    let mut additions: Vec<usize> = forest_after
        .roots
        .iter()
        .copied()
        .filter(|root| !forest_before.roots.contains(root) && Some(*root) != excluded)
        .collect();
    additions.sort_unstable_by_key(|root| (reference.depth[*root], *root));
    for new_root in additions {
        let before_map = component_roots(reference, &roots, forest_before.active_node_count)?;
        let old_root = before_map[new_root];
        insert_sorted(&mut roots, new_root);
        let after_map = component_roots(reference, &roots, forest_before.active_node_count)?;
        let mut moved = Vec::new();
        for source_edge in forest_before.edge_slots.iter().flatten() {
            if before_map[source_edge.from] == old_root && after_map[source_edge.from] == new_root {
                moved.push(DynamicCoreIncidence {
                    edge: source_edge.edge,
                    endpoint: DynamicCoreIncidenceEndpoint::Tail,
                });
            }
            if before_map[source_edge.to] == old_root && after_map[source_edge.to] == new_root {
                moved.push(DynamicCoreIncidence {
                    edge: source_edge.edge,
                    endpoint: DynamicCoreIncidenceEndpoint::Head,
                });
            }
        }
        moved.sort_unstable();
        let split = audit_make_split(old_root, new_root, moved, &edges)?;
        audit_apply_split(&mut edges, &split)?;
        updates.push(split);
    }
    audit_source_update(&context, &roots, &mut edges, &mut updates)?;
    let after_gradients = audit_gradient_transition(&core_before.source_gradients, operation)?;
    let expected = build_snapshot(
        reference,
        forest_after,
        &after_gradients,
        core_before.metrics,
        false,
    )?;
    if edges != expected.edge_slots {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    Ok(updates)
}

fn audit_source_update(
    context: &CoreTransitionContext<'_>,
    roots: &[usize],
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    match context.operation {
        DynamicCoreGraphOperation::Insert { edge, gradient } => {
            let mut after_gradients = context.core_before.source_gradients.clone();
            after_gradients[edge.edge] = Some(gradient.clone());
            let row = build_snapshot(
                context.reference,
                context.forest_after,
                &after_gradients,
                context.core_before.metrics,
                false,
            )?
            .edge_slots[edge.edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .clone();
            edges[edge.edge] = Some(row.clone());
            updates.push(DynamicCoreUpdate::EdgeInserted { edge: row });
        }
        DynamicCoreGraphOperation::Delete { edge } => {
            let row = edges[*edge]
                .take()
                .ok_or(DynamicCoreGraphError::TraceVerification)?;
            updates.push(DynamicCoreUpdate::EdgeDeleted { edge: row });
        }
        DynamicCoreGraphOperation::Reinsert { edge } => {
            let before = edges[*edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .clone();
            let row = build_snapshot(
                context.reference,
                context.forest_after,
                &context.core_before.source_gradients,
                context.core_before.metrics,
                false,
            )?
            .edge_slots[*edge]
                .as_ref()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .clone();
            edges[*edge] = Some(row.clone());
            updates.push(DynamicCoreUpdate::EdgeReinserted { before, after: row });
        }
        DynamicCoreGraphOperation::VertexSplit {
            vertex,
            new_vertex,
            moved_edges,
        } => {
            let map = component_roots(
                context.reference,
                roots,
                context.forest_before.active_node_count,
            )?;
            let old_root = map[*vertex];
            let moved =
                source_split_incidences(&context.forest_before.edge_slots, *vertex, moved_edges)?;
            let split = audit_make_split(old_root, *new_vertex, moved, edges)?;
            audit_apply_split(edges, &split)?;
            updates.push(split);
            audit_split_gradient_updates(
                context.reference,
                context.forest_after,
                moved_edges,
                &context.core_before.source_gradients,
                edges,
                updates,
            )?;
        }
    }
    Ok(())
}

fn audit_split_gradient_updates(
    reference: &StaticReference,
    forest_after: &DynamicLowStretchForestSnapshot,
    moved_edges: &[usize],
    source_gradients: &[Option<BigRational>],
    edges: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicCoreUpdate>,
) -> Result<(), DynamicCoreGraphError> {
    for &edge_id in moved_edges {
        let before = edges[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?
            .gradient
            .clone();
        let source_edge = forest_after.edge_slots[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let source_gradient = source_gradients[edge_id]
            .as_ref()
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let after =
            source_gradient + tree_path_gradient(reference, source_edge.to, source_edge.from)?;
        if before != after {
            edges[edge_id]
                .as_mut()
                .ok_or(DynamicCoreGraphError::TraceVerification)?
                .gradient = after.clone();
            updates.push(DynamicCoreUpdate::GradientReplaced {
                edge: edge_id,
                before,
                after,
            });
        }
    }
    Ok(())
}

fn make_split_update(
    retained_vertex: usize,
    new_vertex: usize,
    mut new_side: Vec<DynamicCoreIncidence>,
    edges: &[Option<DynamicCoreEdge>],
) -> Result<DynamicCoreUpdate, DynamicCoreGraphError> {
    new_side.sort_unstable();
    new_side.dedup();
    let incident = incidences_at(edges, retained_vertex);
    if new_side
        .iter()
        .any(|incidence| !incident.contains(incidence))
    {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    let retained: Vec<_> = incident
        .iter()
        .copied()
        .filter(|incidence| !new_side.contains(incidence))
        .collect();
    let (encoded_side, encoded_incidences) = if new_side.len() <= retained.len() {
        (DynamicCoreEncodedSide::New, new_side.clone())
    } else {
        (DynamicCoreEncodedSide::Retained, retained)
    };
    Ok(DynamicCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences: new_side,
        encoded_side,
        encoded_incidences,
    })
}

fn audit_make_split(
    retained_vertex: usize,
    new_vertex: usize,
    mut moved: Vec<DynamicCoreIncidence>,
    edges: &[Option<DynamicCoreEdge>],
) -> Result<DynamicCoreUpdate, DynamicCoreGraphError> {
    moved.sort_unstable();
    moved.dedup();
    let all = audit_incidences_at(edges, retained_vertex);
    if moved
        .iter()
        .any(|incidence| all.binary_search(incidence).is_err())
    {
        return Err(DynamicCoreGraphError::TraceVerification);
    }
    let stay: Vec<_> = all
        .iter()
        .copied()
        .filter(|incidence| moved.binary_search(incidence).is_err())
        .collect();
    let (side, encoding) = if moved.len() <= stay.len() {
        (DynamicCoreEncodedSide::New, moved.clone())
    } else {
        (DynamicCoreEncodedSide::Retained, stay)
    };
    Ok(DynamicCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences: moved,
        encoded_side: side,
        encoded_incidences: encoding,
    })
}

fn apply_split_to_edges(
    edges: &mut [Option<DynamicCoreEdge>],
    update: &DynamicCoreUpdate,
) -> Result<(), DynamicCoreGraphError> {
    let DynamicCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences,
        ..
    } = update
    else {
        return Err(DynamicCoreGraphError::InvariantViolation);
    };
    for incidence in new_side_incidences {
        let edge = edges
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        let endpoint = match incidence.endpoint {
            DynamicCoreIncidenceEndpoint::Tail => &mut edge.from,
            DynamicCoreIncidenceEndpoint::Head => &mut edge.to,
        };
        if *endpoint != *retained_vertex {
            return Err(DynamicCoreGraphError::InvariantViolation);
        }
        *endpoint = *new_vertex;
    }
    Ok(())
}

fn audit_apply_split(
    edges: &mut [Option<DynamicCoreEdge>],
    update: &DynamicCoreUpdate,
) -> Result<(), DynamicCoreGraphError> {
    let DynamicCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences,
        ..
    } = update
    else {
        return Err(DynamicCoreGraphError::TraceVerification);
    };
    for incidence in new_side_incidences {
        let row = edges
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
            .ok_or(DynamicCoreGraphError::TraceVerification)?;
        let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
            &mut row.from
        } else {
            &mut row.to
        };
        if *endpoint != *retained_vertex {
            return Err(DynamicCoreGraphError::TraceVerification);
        }
        *endpoint = *new_vertex;
    }
    Ok(())
}

fn audit_apply_core_updates(
    snapshot: &mut DynamicCoreGraphSnapshot,
    updates: &[DynamicCoreUpdate],
) -> Result<(), DynamicCoreGraphError> {
    for update in updates {
        match update {
            DynamicCoreUpdate::VertexSplit { new_vertex, .. } => {
                audit_apply_split(&mut snapshot.edge_slots, update)?;
                insert_sorted(&mut snapshot.core_vertices, *new_vertex);
            }
            DynamicCoreUpdate::EdgeInserted { edge } => {
                let slot = snapshot
                    .edge_slots
                    .get_mut(edge.edge)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                if slot.is_some() {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                *slot = Some(edge.clone());
            }
            DynamicCoreUpdate::EdgeDeleted { edge } => {
                let slot = snapshot
                    .edge_slots
                    .get_mut(edge.edge)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                if slot.as_ref() != Some(edge) {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                *slot = None;
            }
            DynamicCoreUpdate::EdgeReinserted { before, after } => {
                if before.edge != after.edge {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                let slot = snapshot
                    .edge_slots
                    .get_mut(before.edge)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                if slot.as_ref() != Some(before) {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                *slot = Some(after.clone());
            }
            DynamicCoreUpdate::GradientReplaced {
                edge,
                before,
                after,
            } => {
                let row = snapshot
                    .edge_slots
                    .get_mut(*edge)
                    .and_then(Option::as_mut)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                if &row.gradient != before {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                row.gradient = after.clone();
            }
            DynamicCoreUpdate::LengthReplaced {
                edge,
                before,
                after,
            } => {
                let row = snapshot
                    .edge_slots
                    .get_mut(*edge)
                    .and_then(Option::as_mut)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                if &row.length != before {
                    return Err(DynamicCoreGraphError::TraceVerification);
                }
                row.length = after.clone();
            }
        }
    }
    Ok(())
}

fn moved_incidences_for_component_change(
    edges: &[Option<DynamicLowStretchForestEdge>],
    before: &[usize],
    after: &[usize],
    retained: usize,
    new_vertex: usize,
) -> Vec<DynamicCoreIncidence> {
    let mut moved = Vec::new();
    for edge in edges.iter().flatten() {
        if before[edge.from] == retained && after[edge.from] == new_vertex {
            moved.push(DynamicCoreIncidence {
                edge: edge.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        }
        if before[edge.to] == retained && after[edge.to] == new_vertex {
            moved.push(DynamicCoreIncidence {
                edge: edge.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        }
    }
    moved.sort_unstable();
    moved
}

fn source_split_incidences(
    edges: &[Option<DynamicLowStretchForestEdge>],
    vertex: usize,
    moved_edges: &[usize],
) -> Result<Vec<DynamicCoreIncidence>, DynamicCoreGraphError> {
    let mut incidences = Vec::with_capacity(moved_edges.len());
    for &edge_id in moved_edges {
        let edge = edges
            .get(edge_id)
            .and_then(Option::as_ref)
            .ok_or(DynamicCoreGraphError::InvariantViolation)?;
        if edge.from == vertex && edge.to != vertex {
            incidences.push(DynamicCoreIncidence {
                edge: edge_id,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        } else if edge.to == vertex && edge.from != vertex {
            incidences.push(DynamicCoreIncidence {
                edge: edge_id,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        } else {
            return Err(DynamicCoreGraphError::InvariantViolation);
        }
    }
    Ok(incidences)
}

fn component_roots(
    reference: &StaticReference,
    roots: &[usize],
    active_node_count: usize,
) -> Result<Vec<usize>, DynamicCoreGraphError> {
    if active_node_count < reference.initial_node_count
        || active_node_count > reference.parent.len()
        || roots.binary_search(&reference.root).is_err()
    {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    let roots = roots.iter().copied().collect::<BTreeSet<_>>();
    let mut removed = BTreeSet::new();
    for &root in roots.range(..reference.initial_node_count) {
        if root == reference.root {
            continue;
        }
        let mut cursor = root;
        let mut best = None;
        loop {
            let edge =
                reference.parent_edge[cursor].ok_or(DynamicCoreGraphError::InvariantViolation)?;
            let rank = reference.pi_rank[edge].ok_or(DynamicCoreGraphError::InvariantViolation)?;
            if best.is_none_or(|(best_rank, best_edge)| (rank, edge) < (best_rank, best_edge)) {
                best = Some((rank, edge));
            }
            cursor = reference.parent[cursor].ok_or(DynamicCoreGraphError::InvariantViolation)?;
            if roots.contains(&cursor) {
                break;
            }
        }
        if !removed.insert(best.ok_or(DynamicCoreGraphError::InvariantViolation)?.1) {
            return Err(DynamicCoreGraphError::InvariantViolation);
        }
    }

    let mut adjacency = vec![Vec::new(); reference.initial_node_count];
    for node in 0..reference.initial_node_count {
        let Some(parent) = reference.parent[node] else {
            continue;
        };
        let edge = reference.parent_edge[node].ok_or(DynamicCoreGraphError::InvariantViolation)?;
        if !removed.contains(&edge) {
            adjacency[node].push(parent);
            adjacency[parent].push(node);
        }
    }
    let mut result = vec![usize::MAX; active_node_count];
    let mut seen = vec![false; reference.initial_node_count];
    for start in 0..reference.initial_node_count {
        if seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        seen[start] = true;
        while let Some(node) = stack.pop() {
            component.push(node);
            for &next in &adjacency[node] {
                if !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        let component_roots = component
            .iter()
            .copied()
            .filter(|node| roots.contains(node))
            .collect::<Vec<_>>();
        if component_roots.len() != 1 {
            return Err(DynamicCoreGraphError::InvariantViolation);
        }
        for node in component {
            result[node] = component_roots[0];
        }
    }
    for (node, component_root) in result
        .iter_mut()
        .enumerate()
        .take(active_node_count)
        .skip(reference.initial_node_count)
    {
        if !roots.contains(&node) {
            return Err(DynamicCoreGraphError::InvariantViolation);
        }
        *component_root = node;
    }
    Ok(result)
}

fn incidences_at(edges: &[Option<DynamicCoreEdge>], vertex: usize) -> Vec<DynamicCoreIncidence> {
    let mut result = Vec::new();
    for edge in edges.iter().flatten() {
        if edge.from == vertex {
            result.push(DynamicCoreIncidence {
                edge: edge.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        }
        if edge.to == vertex {
            result.push(DynamicCoreIncidence {
                edge: edge.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn audit_incidences_at(
    edges: &[Option<DynamicCoreEdge>],
    vertex: usize,
) -> Vec<DynamicCoreIncidence> {
    let mut result = Vec::new();
    for (edge_id, row) in edges.iter().enumerate() {
        let Some(row) = row else { continue };
        if row.from == vertex {
            result.push(DynamicCoreIncidence {
                edge: edge_id,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        }
        if row.to == vertex {
            result.push(DynamicCoreIncidence {
                edge: edge_id,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn apply_gradient_transition(
    before: &[Option<BigRational>],
    operation: &DynamicCoreGraphOperation,
) -> Result<Vec<Option<BigRational>>, DynamicCoreGraphError> {
    let mut after = before.to_vec();
    match operation {
        DynamicCoreGraphOperation::Insert { edge, gradient } => {
            if after[edge.edge].is_some() {
                return Err(DynamicCoreGraphError::InvariantViolation);
            }
            after[edge.edge] = Some(gradient.clone());
        }
        DynamicCoreGraphOperation::Delete { edge } => {
            if after.get(*edge).and_then(Option::as_ref).is_none() {
                return Err(DynamicCoreGraphError::InvariantViolation);
            }
            after[*edge] = None;
        }
        DynamicCoreGraphOperation::Reinsert { edge } => {
            if after.get(*edge).and_then(Option::as_ref).is_none() {
                return Err(DynamicCoreGraphError::InvariantViolation);
            }
        }
        DynamicCoreGraphOperation::VertexSplit { .. } => {}
    }
    Ok(after)
}

fn audit_gradient_transition(
    before: &[Option<BigRational>],
    operation: &DynamicCoreGraphOperation,
) -> Result<Vec<Option<BigRational>>, DynamicCoreGraphError> {
    let mut after = before.to_vec();
    match operation {
        DynamicCoreGraphOperation::Insert { edge, gradient } => {
            let slot = after
                .get_mut(edge.edge)
                .ok_or(DynamicCoreGraphError::TraceVerification)?;
            if slot.is_some() {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
            *slot = Some(gradient.clone());
        }
        DynamicCoreGraphOperation::Delete { edge } => {
            let slot = after
                .get_mut(*edge)
                .ok_or(DynamicCoreGraphError::TraceVerification)?;
            if slot.take().is_none() {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
        }
        DynamicCoreGraphOperation::Reinsert { edge } => {
            if after.get(*edge).and_then(Option::as_ref).is_none() {
                return Err(DynamicCoreGraphError::TraceVerification);
            }
        }
        DynamicCoreGraphOperation::VertexSplit { .. } => {}
    }
    Ok(after)
}

fn apply_metrics(
    before: DynamicCoreGraphMetrics,
    updates: &[DynamicCoreUpdate],
) -> Result<DynamicCoreGraphMetrics, DynamicCoreGraphError> {
    let mut metrics = before;
    for update in updates {
        match update {
            DynamicCoreUpdate::VertexSplit {
                new_side_incidences,
                encoded_incidences,
                ..
            } => {
                metrics.vertex_splits = increment(metrics.vertex_splits)?;
                metrics.endpoint_moves =
                    add_usize(metrics.endpoint_moves, new_side_incidences.len())?;
                metrics.encoded_incidences =
                    add_usize(metrics.encoded_incidences, encoded_incidences.len())?;
            }
            DynamicCoreUpdate::EdgeInserted { .. } => {
                metrics.edge_insertions = increment(metrics.edge_insertions)?;
            }
            DynamicCoreUpdate::EdgeDeleted { .. } => {
                metrics.edge_deletions = increment(metrics.edge_deletions)?;
            }
            DynamicCoreUpdate::EdgeReinserted { .. } => {
                metrics.edge_reinsertions = increment(metrics.edge_reinsertions)?;
            }
            DynamicCoreUpdate::GradientReplaced { .. } => {
                metrics.gradient_replacements = increment(metrics.gradient_replacements)?;
            }
            DynamicCoreUpdate::LengthReplaced { .. } => {
                metrics.length_replacements = increment(metrics.length_replacements)?;
            }
        }
    }
    Ok(metrics)
}

fn audit_metrics(
    before: DynamicCoreGraphMetrics,
    updates: &[DynamicCoreUpdate],
) -> Result<DynamicCoreGraphMetrics, DynamicCoreGraphError> {
    let mut metrics = before;
    for update in updates {
        match update {
            DynamicCoreUpdate::VertexSplit {
                new_side_incidences,
                encoded_incidences,
                ..
            } => {
                metrics.vertex_splits = metrics
                    .vertex_splits
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                metrics.endpoint_moves = metrics
                    .endpoint_moves
                    .checked_add(
                        u64::try_from(new_side_incidences.len())
                            .map_err(|_| DynamicCoreGraphError::TraceVerification)?,
                    )
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
                metrics.encoded_incidences = metrics
                    .encoded_incidences
                    .checked_add(
                        u64::try_from(encoded_incidences.len())
                            .map_err(|_| DynamicCoreGraphError::TraceVerification)?,
                    )
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
            DynamicCoreUpdate::EdgeInserted { .. } => {
                metrics.edge_insertions = metrics
                    .edge_insertions
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
            DynamicCoreUpdate::EdgeDeleted { .. } => {
                metrics.edge_deletions = metrics
                    .edge_deletions
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
            DynamicCoreUpdate::EdgeReinserted { .. } => {
                metrics.edge_reinsertions = metrics
                    .edge_reinsertions
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
            DynamicCoreUpdate::GradientReplaced { .. } => {
                metrics.gradient_replacements = metrics
                    .gradient_replacements
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
            DynamicCoreUpdate::LengthReplaced { .. } => {
                metrics.length_replacements = metrics
                    .length_replacements
                    .checked_add(1)
                    .ok_or(DynamicCoreGraphError::TraceVerification)?;
            }
        }
    }
    Ok(metrics)
}

fn verify_snapshot(snapshot: &DynamicCoreGraphSnapshot) -> Result<(), DynamicCoreGraphError> {
    if snapshot.active_node_count == 0
        || snapshot.active_node_count > DYNAMIC_CORE_MAX_NODES
        || snapshot.edge_slots.len() > DYNAMIC_CORE_MAX_EDGES
        || snapshot.source_gradients.len() != snapshot.edge_slots.len()
        || snapshot.core_vertices.is_empty()
        || snapshot
            .core_vertices
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || snapshot
            .core_vertices
            .iter()
            .any(|&vertex| vertex >= snapshot.active_node_count)
    {
        return Err(DynamicCoreGraphError::InvariantViolation);
    }
    for (index, edge) in snapshot.edge_slots.iter().enumerate() {
        match (edge, &snapshot.source_gradients[index]) {
            (Some(edge), Some(gradient))
                if edge.edge == index
                    && snapshot.core_vertices.binary_search(&edge.from).is_ok()
                    && snapshot.core_vertices.binary_search(&edge.to).is_ok()
                    && edge.length > BigRational::zero()
                    && !rational_too_wide(&edge.length)
                    && !rational_too_wide(&edge.gradient)
                    && !rational_too_wide(gradient) => {}
            (None, None) => {}
            _ => return Err(DynamicCoreGraphError::InvariantViolation),
        }
    }
    Ok(())
}

fn active_count(snapshot: &DynamicCoreGraphSnapshot) -> Result<u64, DynamicCoreGraphError> {
    u64::try_from(snapshot.edge_slots.iter().flatten().count())
        .map_err(|_| DynamicCoreGraphError::ArithmeticOverflow)
}

fn insert_sorted(values: &mut Vec<usize>, value: usize) {
    if let Err(index) = values.binary_search(&value) {
        values.insert(index, value);
    }
}

fn increment(value: u64) -> Result<u64, DynamicCoreGraphError> {
    value
        .checked_add(1)
        .ok_or(DynamicCoreGraphError::ArithmeticOverflow)
}

fn add_usize(value: u64, additional: usize) -> Result<u64, DynamicCoreGraphError> {
    value
        .checked_add(
            u64::try_from(additional).map_err(|_| DynamicCoreGraphError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicCoreGraphError::ArithmeticOverflow)
}

fn rational_too_wide(value: &BigRational) -> bool {
    bigint_bits(value.numer()) > DYNAMIC_CORE_MAX_RATIONAL_BITS
        || bigint_bits(value.denom()) > DYNAMIC_CORE_MAX_RATIONAL_BITS
}

fn audit_rational_too_wide(value: &BigRational) -> bool {
    let numerator_bits = value.numer().bits();
    let denominator_bits = value.denom().bits();
    numerator_bits > DYNAMIC_CORE_MAX_RATIONAL_BITS
        || denominator_bits > DYNAMIC_CORE_MAX_RATIONAL_BITS
}

fn bigint_bits(value: &BigInt) -> u64 {
    value.magnitude().bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn edge(edge: usize, from: usize, to: usize, length: i64) -> DynamicLowStretchForestEdge {
        DynamicLowStretchForestEdge {
            edge,
            from,
            to,
            length: rational(length),
        }
    }

    fn input() -> DynamicCoreGraphInput {
        DynamicCoreGraphInput {
            forest: DynamicLowStretchForestInput {
                initial_node_count: 4,
                maximum_node_count: 5,
                edge_slots: vec![
                    Some(edge(0, 0, 1, 1)),
                    Some(edge(1, 1, 2, 1)),
                    Some(edge(2, 1, 3, 1)),
                    Some(edge(3, 2, 3, 2)),
                    None,
                    Some(edge(5, 1, 3, 2)),
                ],
                reference_tree_edges: vec![0, 1, 2],
                reference_root: 0,
                initial_root_seeds: vec![0],
                initial_stretch_overestimates: None,
            },
            initial_gradients: vec![
                Some(rational(2)),
                Some(rational(3)),
                Some(rational(5)),
                Some(rational(7)),
                None,
                Some(rational(11)),
            ],
        }
    }

    #[test]
    fn core_definition_contracts_endpoints_and_adds_signed_tree_path_gradient() {
        let result = execute_dynamic_core_graph(&input(), &[]).expect("core");
        let snapshot = result.final_snapshot;
        assert_eq!(snapshot.core_vertices, vec![0]);
        for edge_id in [0, 1, 2] {
            let edge = snapshot.edge_slots[edge_id].as_ref().expect("tree edge");
            assert_eq!((edge.from, edge.to), (0, 0));
            assert_eq!(edge.gradient, BigRational::zero());
        }
        let edge3 = snapshot.edge_slots[3].as_ref().expect("edge 3");
        assert_eq!(edge3.gradient, rational(5));
        let edge5 = snapshot.edge_slots[5].as_ref().expect("edge 5");
        assert_eq!(edge5.gradient, rational(6));
        assert!(edge3.length > rational(2));
    }

    #[test]
    fn static_tree_path_gradient_does_not_assume_root_zero() {
        let mut rooted_at_one = input();
        rooted_at_one.forest.reference_root = 1;
        rooted_at_one.forest.initial_root_seeds = vec![1];
        let snapshot = execute_dynamic_core_graph(&rooted_at_one, &[])
            .expect("core")
            .final_snapshot;
        assert_eq!(snapshot.core_vertices, vec![1]);
        for edge_id in [0, 1, 2] {
            assert_eq!(
                snapshot.edge_slots[edge_id]
                    .as_ref()
                    .expect("tree edge")
                    .gradient,
                BigRational::zero()
            );
        }
        assert_eq!(
            snapshot.edge_slots[3]
                .as_ref()
                .expect("off-tree edge")
                .gradient,
            rational(5)
        );
    }

    #[test]
    fn insertion_emits_root_splits_before_the_exact_core_edge() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &trace.events[0].kind else {
            panic!("update");
        };
        assert!(matches!(
            core_updates[0],
            DynamicCoreUpdate::VertexSplit { new_vertex: 2, .. }
        ));
        assert_eq!(core_updates.len(), 2);
        let DynamicCoreUpdate::EdgeInserted { edge } = core_updates.last().expect("insert") else {
            panic!("insert");
        };
        assert_eq!((edge.from, edge.to), (2, 0));
        assert_eq!(edge.length, rational(3));
        assert_eq!(edge.gradient, rational(1));
    }

    #[test]
    fn deletion_splits_the_last_endpoint_component_then_removes_the_core_edge() {
        let operations = vec![DynamicCoreGraphOperation::Delete { edge: 2 }];
        let trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &trace.events[0].kind else {
            panic!("update");
        };
        assert!(
            core_updates.iter().any(|update| matches!(
                update,
                DynamicCoreUpdate::VertexSplit { new_vertex: 3, .. }
            ))
        );
        assert!(
            matches!(core_updates.last(), Some(DynamicCoreUpdate::EdgeDeleted { edge }) if edge.edge == 2)
        );
        assert!(trace.result.final_snapshot.edge_slots[2].is_none());
    }

    #[test]
    fn source_vertex_split_changes_endpoint_and_static_tree_return_gradient() {
        let operations = vec![DynamicCoreGraphOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![5],
        }];
        let trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &trace.events[0].kind else {
            panic!("update");
        };
        assert!(core_updates.iter().any(|update| matches!(
            update,
            DynamicCoreUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                ..
            }
        )));
        assert!(core_updates.iter().any(|update| matches!(
            update,
            DynamicCoreUpdate::GradientReplaced { edge: 5, before, after }
                if *before == rational(6) && *after == rational(11)
        )));
        let final_edge = trace.result.final_snapshot.edge_slots[5]
            .as_ref()
            .expect("edge");
        assert_eq!((final_edge.from, final_edge.to), (4, 1));
        assert_eq!(final_edge.gradient, rational(11));
    }

    #[test]
    fn source_vertex_split_moves_a_tree_source_row_without_moving_static_support() {
        let operations = vec![DynamicCoreGraphOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![1],
        }];
        let trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        let forest = &trace.forest_trace.result.final_snapshot;
        assert_eq!(forest.edge_slots[1].as_ref().expect("dynamic").from, 4);
        assert_eq!(
            forest.reference_tree_support[1].as_ref().expect("support"),
            &edge(1, 1, 2, 1)
        );
        check_dynamic_core_graph_trace(&input(), &operations, &trace).expect("check");
    }

    #[test]
    fn reinsertion_is_topology_invariant_and_emits_one_exact_row_replacement() {
        let operations = vec![DynamicCoreGraphOperation::Reinsert { edge: 3 }];
        let trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &trace.events[0].kind else {
            panic!("update");
        };
        let DynamicCoreUpdate::EdgeReinserted { before, after } =
            core_updates.last().expect("reinsertion")
        else {
            panic!("reinsertion");
        };
        assert_eq!(before.edge, 3);
        assert_eq!(after.edge, 3);
        assert_eq!((before.from, before.to), (2, 3));
        assert_eq!((after.from, after.to), (2, 3));
        assert_eq!(before.gradient, after.gradient);
        assert!(before.length >= after.length);
        assert_eq!(after.length, rational(2));
        assert!(!core_updates.iter().any(|update| matches!(
            update,
            DynamicCoreUpdate::EdgeInserted { .. } | DynamicCoreUpdate::EdgeDeleted { .. }
        )));
        assert_eq!(trace.result.final_snapshot.metrics.edge_reinsertions, 1);
        check_dynamic_core_graph_trace(&input(), &operations, &trace).expect("check");
    }

    fn stage_edge(
        edge: usize,
        from: usize,
        to: usize,
        length: i64,
        gradient: i64,
    ) -> DynamicCoreGraphStageEdge {
        DynamicCoreGraphStageEdge {
            edge,
            from,
            to,
            length: rational(length),
            gradient: rational(gradient),
        }
    }

    #[test]
    fn atomic_stage_preserves_insert_then_reinsert_order_and_epoch() {
        let inserted = stage_edge(4, 2, 0, 3, -4);
        let current = stage_edge(4, 2, 0, 2, -3);
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![
                DynamicCoreGraphStageUpdate::Insert {
                    edge: inserted.clone(),
                },
                DynamicCoreGraphStageUpdate::Reinsert {
                    before: inserted,
                    after: current,
                },
            ],
        }];
        let fast = execute_dynamic_core_graph_stages(&input(), &batches).expect("fast");
        let trace = trace_dynamic_core_graph_stages(&input(), &batches).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.final_snapshot.stage, 1);
        assert_eq!(
            trace.forest_trace.result.final_snapshot.insertion_epoch[4],
            Some(1)
        );
        assert_eq!(fast.final_snapshot.metrics.source_updates, 2);
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        let insert = core_updates
            .iter()
            .position(|update| matches!(update, DynamicCoreUpdate::EdgeInserted { edge } if edge.edge == 4))
            .expect("insert");
        let reinsert = core_updates
            .iter()
            .position(|update| matches!(update, DynamicCoreUpdate::EdgeReinserted { after, .. } if after.edge == 4))
            .expect("reinsert");
        assert!(insert < reinsert);
        check_dynamic_core_graph_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_attribute_replacement_emits_exact_length_and_gradient_updates() {
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: stage_edge(3, 2, 3, 2, 7),
                after: stage_edge(3, 2, 3, 3, 8),
            }],
        }];
        let trace = trace_dynamic_core_graph_stages(&input(), &batches).expect("trace");
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        assert!(core_updates.iter().any(|update| matches!(
            update,
            DynamicCoreUpdate::LengthReplaced { edge: 3, before, after }
                if before < after
        )));
        assert!(core_updates.iter().any(|update| matches!(
            update,
            DynamicCoreUpdate::GradientReplaced { edge: 3, before, after }
                if before < after
        )));
        assert_eq!(trace.result.final_snapshot.metrics.length_replacements, 1);
        assert_eq!(trace.result.final_snapshot.metrics.gradient_replacements, 1);
        check_dynamic_core_graph_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_retained_encoding_keeps_actual_new_vertex_identity() {
        let moved = vec![
            DynamicCoreIncidence {
                edge: 1,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
            DynamicCoreIncidence {
                edge: 2,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
            DynamicCoreIncidence {
                edge: 5,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
        ];
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![DynamicCoreGraphStageUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                new_side_incidences: moved,
                encoded_side: DynamicCoreEncodedSide::Retained,
                encoded_incidences: vec![DynamicCoreIncidence {
                    edge: 0,
                    endpoint: DynamicCoreIncidenceEndpoint::Head,
                }],
            }],
        }];
        let trace = trace_dynamic_core_graph_stages(&input(), &batches).expect("trace");
        let final_snapshot = &trace.result.final_snapshot;
        assert!(final_snapshot.core_vertices.contains(&4));
        assert_eq!(
            trace.forest_trace.result.final_snapshot.edge_slots[1]
                .as_ref()
                .map(|edge| edge.from),
            Some(4)
        );
        assert_eq!(
            trace
                .forest_trace
                .result
                .final_snapshot
                .reference_tree_support[1]
                .as_ref()
                .expect("support"),
            &edge(1, 1, 2, 1)
        );
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        assert!(
            core_updates.iter().any(|update| matches!(
                update,
                DynamicCoreUpdate::VertexSplit { new_vertex: 4, .. }
            ))
        );
        check_dynamic_core_graph_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_active_graph_can_reuse_a_deleted_static_tree_edge_slot() {
        let tree_row = stage_edge(1, 1, 2, 1, 3);
        let batches = vec![
            DynamicCoreGraphStageBatch {
                outer_stage: 1,
                updates: vec![DynamicCoreGraphStageUpdate::Delete {
                    edge: tree_row.clone(),
                }],
            },
            DynamicCoreGraphStageBatch {
                outer_stage: 2,
                updates: vec![DynamicCoreGraphStageUpdate::Insert { edge: tree_row }],
            },
        ];
        let trace = trace_dynamic_core_graph_stages(&input(), &batches).expect("trace");
        assert!(trace.result.final_snapshot.edge_slots[1].is_some());
        assert_eq!(trace.result.final_snapshot.stage, 2);
        check_dynamic_core_graph_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_core_checker_rejects_update_and_metric_tampering() {
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: stage_edge(3, 2, 3, 2, 7),
                after: stage_edge(3, 2, 3, 3, 8),
            }],
        }];
        let trace = trace_dynamic_core_graph_stages(&input(), &batches).expect("trace");
        let mut tampered = trace.clone();
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } =
            &mut tampered.events[0].kind
        else {
            panic!("stage");
        };
        core_updates.pop();
        assert_eq!(
            check_dynamic_core_graph_stage_trace(&input(), &batches, &tampered),
            Err(DynamicCoreGraphError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.events[0].after.metrics.source_updates = 0;
        assert_eq!(
            check_dynamic_core_graph_stage_trace(&input(), &batches, &tampered),
            Err(DynamicCoreGraphError::TraceVerification)
        );
    }

    #[test]
    fn fast_trace_and_independent_checker_match_and_reject_batch_tampering() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let fast = execute_dynamic_core_graph(&input(), &operations).expect("fast");
        let mut trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        assert_eq!(fast, trace.result);
        check_dynamic_core_graph_trace(&input(), &operations, &trace).expect("check");
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &mut trace.events[0].kind
        else {
            panic!("update");
        };
        core_updates.pop();
        assert_eq!(
            check_dynamic_core_graph_trace(&input(), &operations, &trace),
            Err(DynamicCoreGraphError::TraceVerification)
        );
    }

    #[test]
    fn checker_rejects_definition_metric_and_forest_component_tampering() {
        let operations = vec![DynamicCoreGraphOperation::Delete { edge: 2 }];
        let mut trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        trace.events[0].after.edge_slots[3]
            .as_mut()
            .expect("edge")
            .gradient += rational(1);
        assert_eq!(
            check_dynamic_core_graph_trace(&input(), &operations, &trace),
            Err(DynamicCoreGraphError::TraceVerification)
        );

        let mut trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        trace.events[0].after.metrics.endpoint_moves += 1;
        assert_eq!(
            check_dynamic_core_graph_trace(&input(), &operations, &trace),
            Err(DynamicCoreGraphError::TraceVerification)
        );

        let mut trace = trace_dynamic_core_graph(&input(), &operations).expect("trace");
        trace.forest_trace.events[0].after.roots.pop();
        assert!(matches!(
            check_dynamic_core_graph_trace(&input(), &operations, &trace),
            Err(DynamicCoreGraphError::Forest(
                DynamicLowStretchForestError::TraceVerification
            ))
        ));
    }

    #[test]
    fn gradient_shape_width_and_source_topology_fail_closed() {
        let mut missing = input();
        missing.initial_gradients.pop();
        assert_eq!(
            execute_dynamic_core_graph(&missing, &[]),
            Err(DynamicCoreGraphError::InvalidInput)
        );

        let mut wide = input();
        wide.initial_gradients[0] = Some(BigRational::from_integer(BigInt::from(1) << 513));
        assert_eq!(
            execute_dynamic_core_graph(&wide, &[]),
            Err(DynamicCoreGraphError::AdmissionLimit)
        );

        let invalid = DynamicCoreGraphOperation::Insert {
            edge: edge(0, 0, 2, 1),
            gradient: rational(1),
        };
        assert!(matches!(
            execute_dynamic_core_graph(&input(), &[invalid]),
            Err(DynamicCoreGraphError::Forest(
                DynamicLowStretchForestError::InvalidInput
            ))
        ));

        let reused_reference = vec![
            DynamicCoreGraphOperation::Delete { edge: 0 },
            DynamicCoreGraphOperation::Insert {
                edge: edge(0, 0, 1, 1),
                gradient: rational(2),
            },
        ];
        let reused = trace_dynamic_core_graph(&input(), &reused_reference).expect("reuse");
        assert_eq!(
            reused
                .forest_trace
                .result
                .final_snapshot
                .reference_tree_support[0]
                .as_ref()
                .expect("support"),
            &edge(0, 0, 1, 1)
        );
        assert!(reused.result.final_snapshot.edge_slots[0].is_some());
        check_dynamic_core_graph_trace(&input(), &reused_reference, &reused).expect("check");
    }
}
