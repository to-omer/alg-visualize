//! Exact bounded dynamic rooted-forest transition primitive.
//!
//! This module realizes the source-defined update semantics behind the Dynamic
//! Low Stretch Forest lemma in van den Brand et al. and its CKLPPS appendix:
//! a fixed reference tree and congestion order define `F_T(R, pi)`; insertion
//! and deletion add both endpoint Heavy-Light ancestor closures to the monotone root set;
//! insertion receives stretch overestimate one; and a vertex split creates an
//! isolated new root while moving the explicitly encoded smaller incident
//! off-tree side. The rooted forest is therefore decremental.
//!
//! Each heavy chain is replaced by the source balanced depth-ordered BST, so
//! the auxiliary tree has a concrete bounded `O(log^2 n)` height and every
//! auxiliary ancestor closure is branch-free in the reference tree. The forest
//! removes the minimum-congestion-order edge on every path between adjacent
//! roots, exactly as in `F_T(R, pi)`. A vertex split may move an active graph
//! edge whose stable ID also supplied the initial reference tree: the dynamic
//! row moves to the new isolated root while an explicit immutable support row
//! keeps the initial reference-tree endpoint and length. Initial overestimates
//! use the source formula `2 * sum_i stretch(F_T(B_i, pi))` with auxiliary-depth
//! balls `B_i`. This bounded realization does not claim LSST construction, the
//! tree decomposition/large-stretch seed choice, polylog recourse, or runtime.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use thiserror::Error;

use super::hld_branch_free_tree::{
    HldBranchFreeTree, build_hld_branch_free_tree, check_hld_branch_free_tree, is_branch_free,
};

/// Maximum stable vertices, including vertices introduced by splits.
pub const DYNAMIC_LSF_MAX_NODES: usize = 8;
/// Maximum stable edge slots.
pub const DYNAMIC_LSF_MAX_EDGES: usize = 12;
/// Maximum topology operations.
pub const DYNAMIC_LSF_MAX_OPERATIONS: usize = 256;
/// Maximum reversible boundaries, including completion.
pub const DYNAMIC_LSF_MAX_TRACE_EVENTS: usize = 257;
/// Maximum numerator or denominator width.
pub const DYNAMIC_LSF_MAX_RATIONAL_BITS: u64 = 512;

const CATALOG_ID: &str = "dynamic-low-stretch-forest";

/// One active stable graph edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestEdge {
    /// Stable slot equal to its position in the edge-slot vector.
    pub edge: usize,
    /// Current tail.
    pub from: usize,
    /// Current head.
    pub to: usize,
    /// Exact positive immutable length during this primitive execution.
    pub length: BigRational,
}

/// Initial graph, reference tree, and future stable universes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestInput {
    /// Initially active vertices `0..initial_node_count`.
    pub initial_node_count: usize,
    /// Stable vertex universe; splits activate the next consecutive vertex.
    pub maximum_node_count: usize,
    /// Stable edge slots. `None` slots may be inserted later.
    pub edge_slots: Vec<Option<DynamicLowStretchForestEdge>>,
    /// Initial active edge IDs forming a tree on the initial vertices.
    pub reference_tree_edges: Vec<usize>,
    /// Root of the reference tree.
    pub reference_root: usize,
    /// Nonempty initial root seeds; their ancestor closure is used.
    pub initial_root_seeds: Vec<usize>,
    /// Optional source-certified initial overestimates in stable-slot order.
    /// When absent, the transparent tree-depth-ball bound is reconstructed.
    pub initial_stretch_overestimates: Option<Vec<Option<BigRational>>>,
}

/// One source topology update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicLowStretchForestOperation {
    /// Insert an inactive stable edge slot.
    Insert {
        /// Complete inserted edge row.
        edge: DynamicLowStretchForestEdge,
    },
    /// Delete one currently active edge.
    Delete {
        /// Stable edge slot.
        edge: usize,
    },
    /// Reinsert one currently active edge without changing its row.
    Reinsert {
        /// Stable edge slot whose insertion epoch is refreshed.
        edge: usize,
    },
    /// Split one vertex and move the explicitly encoded smaller off-tree side.
    VertexSplit {
        /// Existing vertex retained in the reference tree.
        vertex: usize,
        /// Must be the next consecutive inactive stable vertex.
        new_vertex: usize,
        /// Nonempty, strictly increasing active incident edge IDs to move.
        moved_edges: Vec<usize>,
    },
}

/// Tail or head incidence used by an atomic source vertex split.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DynamicLowStretchForestIncidenceEndpoint {
    /// Tail incidence.
    Tail,
    /// Head incidence.
    Head,
}

/// One stable active-edge incidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DynamicLowStretchForestIncidence {
    /// Stable edge slot.
    pub edge: usize,
    /// Tail or head incidence.
    pub endpoint: DynamicLowStretchForestIncidenceEndpoint,
}

/// Which side is listed by the canonical smaller-side split encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicLowStretchForestEncodedSide {
    /// Incidences remaining on the retained vertex are listed.
    Retained,
    /// Incidences moving to the actual new vertex are listed.
    New,
}

/// One ordered source update inside an atomic LSF stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicLowStretchForestStageUpdate {
    /// Insert one complete edge row into an inactive stable slot.
    Insert {
        /// Complete current source row.
        edge: DynamicLowStretchForestEdge,
    },
    /// Delete one active stable edge.
    Delete {
        /// Complete row at deletion time.
        edge: DynamicLowStretchForestEdge,
    },
    /// Explicitly reinsert an active edge and replace its current attributes.
    Reinsert {
        /// Complete row immediately before reinsertion.
        before: DynamicLowStretchForestEdge,
        /// Complete row after reinsertion; topology and stable ID are unchanged.
        after: DynamicLowStretchForestEdge,
    },
    /// Replace one active edge length without resetting its insertion epoch.
    ReplaceLength {
        /// Stable active edge slot.
        edge: usize,
        /// Exact current length.
        before: BigRational,
        /// Exact replacement length.
        after: BigRational,
    },
    /// Split one source vertex using actual incidences and a separate encoding.
    VertexSplit {
        /// Existing vertex whose identity is retained.
        retained_vertex: usize,
        /// Next consecutive actual new vertex.
        new_vertex: usize,
        /// Exact incidences moving to `new_vertex`.
        new_side_incidences: Vec<DynamicLowStretchForestIncidence>,
        /// Canonical smaller side used only for encoding accounting.
        encoded_side: DynamicLowStretchForestEncodedSide,
        /// Canonical strictly ordered incidences of the encoded side.
        encoded_incidences: Vec<DynamicLowStretchForestIncidence>,
    },
}

/// One outer source stage containing an ordered atomic update batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestStageBatch {
    /// Exact next outer stage.
    pub outer_stage: u64,
    /// Ordered source update records applied before one forest refresh.
    pub updates: Vec<DynamicLowStretchForestStageUpdate>,
}

/// Maximum records accepted in one atomic LSF stage.
pub const DYNAMIC_LSF_MAX_STAGE_UPDATES: usize = DYNAMIC_LSF_MAX_EDGES * 8;

/// Exact work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicLowStretchForestMetrics {
    /// Edge insertions.
    pub insertions: u64,
    /// Edge deletions.
    pub deletions: u64,
    /// Explicit reinsertions of active stable edges.
    pub reinsertions: u64,
    /// Existing active-edge length replacements.
    pub length_replacements: u64,
    /// Vertex splits.
    pub vertex_splits: u64,
    /// Incident edge endpoints moved by splits.
    pub moved_edges: u64,
    /// Distinct roots added after initialization.
    pub roots_added: u64,
    /// Reference-tree edges removed from the decremental forest.
    pub forest_edges_removed: u64,
    /// Active-edge stretch upper-bound checks.
    pub stretch_checks: u64,
    /// Reversible public transitions.
    pub state_transitions: u64,
}

/// Complete dynamic rooted-forest state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestSnapshot {
    /// Number of active vertices.
    pub active_node_count: usize,
    /// Current stable edge slots.
    pub edge_slots: Vec<Option<DynamicLowStretchForestEdge>>,
    /// Immutable initial rows supporting the fixed reference tree by stable ID.
    pub reference_tree_support: Vec<Option<DynamicLowStretchForestEdge>>,
    /// Monotone branch-free root set in increasing vertex order.
    pub roots: Vec<usize>,
    /// Current decremental subset of reference-tree edge IDs.
    pub forest_edges: Vec<usize>,
    /// Initial reference-tree edge IDs in increasing congestion/ID order.
    pub congestion_order: Vec<usize>,
    /// Heavy child in the static reference tree; future split vertices are `None`.
    pub heavy_child: Vec<Option<usize>>,
    /// Heavy-chain head for each static vertex; future split vertices are `None`.
    pub heavy_chain_head: Vec<Option<usize>>,
    /// Parent in the CKLPPS Heavy-Light auxiliary tree.
    pub auxiliary_parent: Vec<Option<usize>>,
    /// Depth in the CKLPPS Heavy-Light auxiliary tree; split vertices are absent.
    pub auxiliary_depth: Vec<Option<usize>>,
    /// Distinguished component root for each active vertex.
    pub component_roots: Vec<usize>,
    /// Current exact stretch for each active edge slot.
    pub current_stretches: Vec<Option<BigRational>>,
    /// Fixed/epoch exact stretch overestimate for each active edge slot.
    pub stretch_overestimates: Vec<Option<BigRational>>,
    /// Insertion epoch for each active edge slot.
    pub insertion_epoch: Vec<Option<u64>>,
    /// Completed topology stages.
    pub stage: u64,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: DynamicLowStretchForestMetrics,
}

/// Source meaning of one update boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicLowStretchForestEventKind {
    /// One edge was inserted after endpoint roots were added.
    Inserted {
        /// Stable edge slot.
        edge: usize,
        /// Roots newly introduced by this update.
        roots_added: Vec<usize>,
        /// Reference-tree edges newly removed from the forest.
        forest_edges_removed: Vec<usize>,
    },
    /// One edge was deleted after endpoint roots were added.
    Deleted {
        /// Stable edge slot.
        edge: usize,
        /// Roots newly introduced by this update.
        roots_added: Vec<usize>,
        /// Reference-tree edges newly removed from the forest.
        forest_edges_removed: Vec<usize>,
    },
    /// An active edge was explicitly reinserted after endpoint roots were added.
    Reinserted {
        /// Stable edge slot.
        edge: usize,
        /// Roots newly introduced by this update.
        roots_added: Vec<usize>,
        /// Reference-tree edges newly removed from the forest.
        forest_edges_removed: Vec<usize>,
    },
    /// One encoded smaller side moved to a new isolated root.
    VertexSplit {
        /// Original vertex.
        vertex: usize,
        /// Newly activated vertex.
        new_vertex: usize,
        /// Stable moved edge IDs.
        moved_edges: Vec<usize>,
        /// Roots newly introduced by this update.
        roots_added: Vec<usize>,
        /// Reference-tree edges newly removed from the forest.
        forest_edges_removed: Vec<usize>,
    },
    /// Every supplied topology update completed.
    Completed,
}

/// One fully reversible dynamic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level transition.
    pub kind: DynamicLowStretchForestEventKind,
    /// State before the transition.
    pub before: DynamicLowStretchForestSnapshot,
    /// State after the transition.
    pub after: DynamicLowStretchForestSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestResult {
    /// Terminal source state.
    pub final_snapshot: DynamicLowStretchForestSnapshot,
}

/// Complete reversible execution transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestTraceResult {
    /// Initial source state.
    pub base_snapshot: DynamicLowStretchForestSnapshot,
    /// One event per operation followed by completion.
    pub events: Vec<DynamicLowStretchForestTraceEvent>,
    /// Exact result.
    pub result: DynamicLowStretchForestResult,
}

/// Meaning of one atomic LSF stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicLowStretchForestStageEventKind {
    /// One ordered source batch was applied before a single forest refresh.
    Updated {
        /// Exact source batch.
        batch: DynamicLowStretchForestStageBatch,
        /// Roots introduced by the whole batch.
        roots_added: Vec<usize>,
        /// Reference-tree edges removed by the whole batch.
        forest_edges_removed: Vec<usize>,
    },
    /// Every supplied source stage completed.
    Completed,
}

/// One fully reversible atomic LSF stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestStageTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Atomic stage transition.
    pub kind: DynamicLowStretchForestStageEventKind,
    /// State before the whole source batch.
    pub before: DynamicLowStretchForestSnapshot,
    /// State after the whole source batch.
    pub after: DynamicLowStretchForestSnapshot,
}

/// Complete atomic-stage LSF transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLowStretchForestStageTraceResult {
    /// Initial source state.
    pub base_snapshot: DynamicLowStretchForestSnapshot,
    /// One event per atomic batch followed by completion.
    pub events: Vec<DynamicLowStretchForestStageTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicLowStretchForestResult,
}

/// Explicit bounded dynamic-LSF failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicLowStretchForestError {
    /// Initial graph, reference tree, roots, or operation shape is malformed.
    #[error("dynamic low-stretch forest input is invalid")]
    InvalidInput,
    /// Input or exact arithmetic exceeds the published bounded realization.
    #[error("dynamic low-stretch forest exceeds its admission band")]
    AdmissionLimit,
    /// A source forest, root, decrementality, or stretch invariant failed.
    #[error("dynamic low-stretch forest invariant failed")]
    InvariantViolation,
    /// Checked work accounting overflowed.
    #[error("dynamic low-stretch forest arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact stable replay.
    #[error("dynamic low-stretch forest trace verification failed")]
    TraceVerification,
}

#[derive(Clone)]
struct ReferenceTree {
    parent: Vec<Option<usize>>,
    parent_edge: Vec<Option<usize>>,
    depth: Vec<usize>,
    edge_lengths: Vec<Option<BigRational>>,
    pi_rank: Vec<Option<usize>>,
    support: Vec<Option<DynamicLowStretchForestEdge>>,
    auxiliary: HldBranchFreeTree,
}

struct InternalRun {
    base_snapshot: DynamicLowStretchForestSnapshot,
    events: Vec<DynamicLowStretchForestTraceEvent>,
    result: DynamicLowStretchForestResult,
}

/// Executes the bounded dynamic rooted-forest transition system.
///
/// # Errors
///
/// Rejects malformed/out-of-band input, invalid topology updates, a failed
/// source invariant, or exact work overflow.
pub fn execute_dynamic_low_stretch_forest(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
) -> Result<DynamicLowStretchForestResult, DynamicLowStretchForestError> {
    run_internal(input, operations, false).map(|run| run.result)
}

/// Records every topology update and completion boundary.
///
/// # Errors
///
/// Returns any execution or independent replay-checker failure.
pub fn trace_dynamic_low_stretch_forest(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
) -> Result<DynamicLowStretchForestTraceResult, DynamicLowStretchForestError> {
    let run = run_internal(input, operations, true)?;
    let trace = DynamicLowStretchForestTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_low_stretch_forest_trace(input, operations, &trace)?;
    Ok(trace)
}

/// Replays topology transitions and rechecks every exact forest invariant.
///
/// The checker does not call the production runner or production operation
/// dispatcher. It reconstructs each operation, then independently checks the
/// roots, decremental forest, component roots, stretches, epochs, and metrics.
///
/// # Errors
///
/// Rejects invalid source input or any transcript drift.
pub fn check_dynamic_low_stretch_forest_trace(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
    trace: &DynamicLowStretchForestTraceResult,
) -> Result<(), DynamicLowStretchForestError> {
    validate_input(input, operations)?;
    let reference = build_reference_tree(input)?;
    let mut snapshot = initial_snapshot(input, &reference)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != operations
                .len()
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::TraceVerification)?
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    for (operation, event) in operations.iter().zip(&trace.events) {
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicLowStretchForestError::TraceVerification);
        }
        let (kind, mut after) = audit_operation(&reference, operation, &snapshot)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        audit_snapshot(input, &reference, &after, &snapshot)?;
        if event.kind != kind || event.after != after {
            return Err(DynamicLowStretchForestError::TraceVerification);
        }
        snapshot = after;
    }
    let completion = trace
        .events
        .last()
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    let mut final_snapshot = snapshot.clone();
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.before != snapshot
        || completion.kind != DynamicLowStretchForestEventKind::Completed
        || completion.after != final_snapshot
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

/// Executes ordered update batches with one forest refresh per outer stage.
///
/// # Errors
///
/// Rejects malformed/out-of-band batches, invalid incidence partitions,
/// nonconsecutive outer stages, failed forest invariants, or arithmetic overflow.
pub fn execute_dynamic_low_stretch_forest_stages(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
) -> Result<DynamicLowStretchForestResult, DynamicLowStretchForestError> {
    run_stage_internal(input, batches, false).map(|run| run.result)
}

/// Executes atomic LSF stages and records every reversible outer boundary.
///
/// # Errors
///
/// Returns any atomic execution or independent replay-checker failure.
pub fn trace_dynamic_low_stretch_forest_stages(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
) -> Result<DynamicLowStretchForestStageTraceResult, DynamicLowStretchForestError> {
    let run = run_stage_internal(input, batches, true)?;
    let trace = DynamicLowStretchForestStageTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_low_stretch_forest_stage_trace(input, batches, &trace)?;
    Ok(trace)
}

/// Independently replays atomic source batches and checks every LSF invariant.
///
/// # Errors
///
/// Rejects malformed source input or any batch, mapping, metric, epoch, root,
/// forest, stretch, or completion drift in the supplied transcript.
pub fn check_dynamic_low_stretch_forest_stage_trace(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
    trace: &DynamicLowStretchForestStageTraceResult,
) -> Result<(), DynamicLowStretchForestError> {
    audit_validate_stage_batches(input, batches)?;
    let reference = build_reference_tree(input)?;
    let mut snapshot = initial_snapshot(input, &reference)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != batches
                .len()
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::TraceVerification)?
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    for (batch, event) in batches.iter().zip(&trace.events) {
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicLowStretchForestError::TraceVerification);
        }
        let (kind, mut after) = audit_stage_batch(&reference, batch, &snapshot)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        audit_snapshot(input, &reference, &after, &snapshot)?;
        if event.kind != kind || event.after != after {
            return Err(DynamicLowStretchForestError::TraceVerification);
        }
        snapshot = after;
    }
    let completion = trace
        .events
        .last()
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    let mut final_snapshot = snapshot.clone();
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.before != snapshot
        || completion.kind != DynamicLowStretchForestStageEventKind::Completed
        || completion.after != final_snapshot
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

struct InternalStageRun {
    base_snapshot: DynamicLowStretchForestSnapshot,
    events: Vec<DynamicLowStretchForestStageTraceEvent>,
    result: DynamicLowStretchForestResult,
}

fn run_stage_internal(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
    record: bool,
) -> Result<InternalStageRun, DynamicLowStretchForestError> {
    validate_stage_batches(input, batches)?;
    let reference = build_reference_tree(input)?;
    let mut snapshot = initial_snapshot(input, &reference)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { batches.len() + 1 } else { 0 });
    for batch in batches {
        let before = snapshot.clone();
        let kind = apply_stage_batch(&reference, batch, &mut snapshot)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        check_snapshot(input, &reference, &snapshot, &before)?;
        if record {
            events.push(DynamicLowStretchForestStageTraceEvent {
                catalog_id: CATALOG_ID,
                kind,
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicLowStretchForestStageTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicLowStretchForestStageEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalStageRun {
        base_snapshot,
        events,
        result: DynamicLowStretchForestResult {
            final_snapshot: snapshot,
        },
    })
}

fn apply_stage_batch(
    reference: &ReferenceTree,
    batch: &DynamicLowStretchForestStageBatch,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<DynamicLowStretchForestStageEventKind, DynamicLowStretchForestError> {
    let old_roots = snapshot.roots.clone();
    let old_forest = snapshot.forest_edges.clone();
    for update in &batch.updates {
        apply_stage_update(reference, batch.outer_stage, update, snapshot)?;
    }
    refresh_dynamic_state(reference, snapshot)?;
    let roots_added = ordered_difference(&snapshot.roots, &old_roots);
    let forest_edges_removed = ordered_difference(&old_forest, &snapshot.forest_edges);
    record_root_forest_metrics(snapshot, &roots_added, &forest_edges_removed)?;
    snapshot.stage = batch.outer_stage;
    Ok(DynamicLowStretchForestStageEventKind::Updated {
        batch: batch.clone(),
        roots_added,
        forest_edges_removed,
    })
}

fn apply_stage_update(
    reference: &ReferenceTree,
    outer_stage: u64,
    update: &DynamicLowStretchForestStageUpdate,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    match update {
        DynamicLowStretchForestStageUpdate::Insert { edge } => {
            add_endpoint_roots(reference, &mut snapshot.roots, edge.from, edge.to);
            snapshot.edge_slots[edge.edge] = Some(edge.clone());
            snapshot.stretch_overestimates[edge.edge] = Some(BigRational::one());
            snapshot.insertion_epoch[edge.edge] = Some(outer_stage);
            snapshot.metrics.insertions = increment(snapshot.metrics.insertions)?;
        }
        DynamicLowStretchForestStageUpdate::Delete { edge } => {
            add_endpoint_roots(reference, &mut snapshot.roots, edge.from, edge.to);
            snapshot.edge_slots[edge.edge] = None;
            snapshot.stretch_overestimates[edge.edge] = None;
            snapshot.insertion_epoch[edge.edge] = None;
            snapshot.metrics.deletions = increment(snapshot.metrics.deletions)?;
        }
        DynamicLowStretchForestStageUpdate::Reinsert { before, after } => {
            add_endpoint_roots(reference, &mut snapshot.roots, after.from, after.to);
            snapshot.edge_slots[before.edge] = Some(after.clone());
            snapshot.stretch_overestimates[before.edge] = Some(BigRational::one());
            snapshot.insertion_epoch[before.edge] = Some(outer_stage);
            snapshot.metrics.reinsertions = increment(snapshot.metrics.reinsertions)?;
        }
        DynamicLowStretchForestStageUpdate::ReplaceLength {
            edge,
            before: _,
            after,
        } => {
            snapshot.edge_slots[*edge]
                .as_mut()
                .ok_or(DynamicLowStretchForestError::InvariantViolation)?
                .length = after.clone();
            snapshot.metrics.length_replacements = increment(snapshot.metrics.length_replacements)?;
        }
        DynamicLowStretchForestStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            ..
        } => {
            add_ancestor_closure(reference, &mut snapshot.roots, *retained_vertex);
            insert_sorted_unique(&mut snapshot.roots, *new_vertex);
            for incidence in new_side_incidences {
                apply_stage_incidence(
                    &mut snapshot.edge_slots,
                    *retained_vertex,
                    *new_vertex,
                    *incidence,
                )?;
            }
            snapshot.active_node_count = snapshot
                .active_node_count
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
            snapshot.metrics.vertex_splits = increment(snapshot.metrics.vertex_splits)?;
            snapshot.metrics.moved_edges = snapshot
                .metrics
                .moved_edges
                .checked_add(
                    u64::try_from(new_side_incidences.len())
                        .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?,
                )
                .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn apply_stage_incidence(
    slots: &mut [Option<DynamicLowStretchForestEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidence: DynamicLowStretchForestIncidence,
) -> Result<(), DynamicLowStretchForestError> {
    let row = slots
        .get_mut(incidence.edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    let endpoint = match incidence.endpoint {
        DynamicLowStretchForestIncidenceEndpoint::Tail => &mut row.from,
        DynamicLowStretchForestIncidenceEndpoint::Head => &mut row.to,
    };
    if *endpoint != retained_vertex {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    *endpoint = new_vertex;
    Ok(())
}

fn validate_stage_batches(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
) -> Result<(), DynamicLowStretchForestError> {
    validate_input(input, &[])?;
    if batches.len() > DYNAMIC_LSF_MAX_OPERATIONS {
        return Err(DynamicLowStretchForestError::AdmissionLimit);
    }
    let mut slots = input.edge_slots.clone();
    let mut active_nodes = input.initial_node_count;
    let mut stage = 0_u64;
    for batch in batches {
        if batch.outer_stage != increment(stage)?
            || batch.updates.len() > DYNAMIC_LSF_MAX_STAGE_UPDATES
        {
            return Err(DynamicLowStretchForestError::InvalidInput);
        }
        for update in &batch.updates {
            validate_stage_update(input, &mut slots, &mut active_nodes, update)?;
        }
        stage = batch.outer_stage;
    }
    Ok(())
}

fn validate_stage_update(
    input: &DynamicLowStretchForestInput,
    slots: &mut [Option<DynamicLowStretchForestEdge>],
    active_nodes: &mut usize,
    update: &DynamicLowStretchForestStageUpdate,
) -> Result<(), DynamicLowStretchForestError> {
    match update {
        DynamicLowStretchForestStageUpdate::Insert { edge } => {
            validate_stage_edge(edge, slots.len(), *active_nodes)?;
            if slots[edge.edge].is_some() {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            slots[edge.edge] = Some(edge.clone());
        }
        DynamicLowStretchForestStageUpdate::Delete { edge } => {
            if slots.get(edge.edge).and_then(Option::as_ref) != Some(edge) {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            slots[edge.edge] = None;
        }
        DynamicLowStretchForestStageUpdate::Reinsert { before, after } => {
            validate_stage_edge(after, slots.len(), *active_nodes)?;
            if before.edge != after.edge
                || before.from != after.from
                || before.to != after.to
                || slots.get(before.edge).and_then(Option::as_ref) != Some(before)
            {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            slots[before.edge] = Some(after.clone());
        }
        DynamicLowStretchForestStageUpdate::ReplaceLength {
            edge,
            before,
            after,
        } => {
            if after <= &BigRational::zero()
                || rational_too_wide(before)
                || rational_too_wide(after)
            {
                return Err(DynamicLowStretchForestError::AdmissionLimit);
            }
            let row = slots
                .get_mut(*edge)
                .and_then(Option::as_mut)
                .ok_or(DynamicLowStretchForestError::InvalidInput)?;
            if &row.length != before {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            row.length = after.clone();
        }
        DynamicLowStretchForestStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => {
            if *retained_vertex >= *active_nodes
                || *new_vertex != *active_nodes
                || *active_nodes >= input.maximum_node_count
            {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            validate_stage_split(
                slots,
                *retained_vertex,
                new_side_incidences,
                *encoded_side,
                encoded_incidences,
            )?;
            for incidence in new_side_incidences {
                apply_validation_incidence(slots, *retained_vertex, *new_vertex, *incidence)?;
            }
            *active_nodes = active_nodes
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn validate_stage_edge(
    edge: &DynamicLowStretchForestEdge,
    slot_count: usize,
    active_nodes: usize,
) -> Result<(), DynamicLowStretchForestError> {
    if edge.edge >= slot_count || edge.from >= active_nodes || edge.to >= active_nodes {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    if edge.length <= BigRational::zero() || rational_too_wide(&edge.length) {
        return Err(DynamicLowStretchForestError::AdmissionLimit);
    }
    Ok(())
}

fn validate_stage_split(
    slots: &[Option<DynamicLowStretchForestEdge>],
    retained_vertex: usize,
    new_side: &[DynamicLowStretchForestIncidence],
    encoded_side: DynamicLowStretchForestEncodedSide,
    encoded: &[DynamicLowStretchForestIncidence],
) -> Result<(), DynamicLowStretchForestError> {
    if new_side.windows(2).any(|pair| pair[0] >= pair[1])
        || encoded.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    let incident = stage_incidences_at(slots, retained_vertex);
    if new_side
        .iter()
        .any(|incidence| incident.binary_search(incidence).is_err())
    {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    let retained = incident
        .iter()
        .copied()
        .filter(|incidence| new_side.binary_search(incidence).is_err())
        .collect::<Vec<_>>();
    let (expected_side, expected) = if new_side.len() <= retained.len() {
        (DynamicLowStretchForestEncodedSide::New, new_side)
    } else {
        (
            DynamicLowStretchForestEncodedSide::Retained,
            retained.as_slice(),
        )
    };
    if encoded_side != expected_side || encoded != expected {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    Ok(())
}

fn stage_incidences_at(
    slots: &[Option<DynamicLowStretchForestEdge>],
    vertex: usize,
) -> Vec<DynamicLowStretchForestIncidence> {
    let mut result = Vec::new();
    for edge in slots.iter().flatten() {
        if edge.from == vertex {
            result.push(DynamicLowStretchForestIncidence {
                edge: edge.edge,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
            });
        }
        if edge.to == vertex {
            result.push(DynamicLowStretchForestIncidence {
                edge: edge.edge,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn apply_validation_incidence(
    slots: &mut [Option<DynamicLowStretchForestEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidence: DynamicLowStretchForestIncidence,
) -> Result<(), DynamicLowStretchForestError> {
    let row = slots
        .get_mut(incidence.edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicLowStretchForestError::InvalidInput)?;
    let endpoint = if incidence.endpoint == DynamicLowStretchForestIncidenceEndpoint::Tail {
        &mut row.from
    } else {
        &mut row.to
    };
    if *endpoint != retained_vertex {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    *endpoint = new_vertex;
    Ok(())
}

fn audit_validate_stage_batches(
    input: &DynamicLowStretchForestInput,
    batches: &[DynamicLowStretchForestStageBatch],
) -> Result<(), DynamicLowStretchForestError> {
    validate_input(input, &[])?;
    if batches.len() > DYNAMIC_LSF_MAX_OPERATIONS {
        return Err(DynamicLowStretchForestError::AdmissionLimit);
    }
    let mut stage = 0_u64;
    for batch in batches {
        if stage.checked_add(1) != Some(batch.outer_stage)
            || batch.updates.len() > DYNAMIC_LSF_MAX_STAGE_UPDATES
        {
            return Err(DynamicLowStretchForestError::TraceVerification);
        }
        stage = batch.outer_stage;
    }
    Ok(())
}

fn audit_stage_batch(
    reference: &ReferenceTree,
    batch: &DynamicLowStretchForestStageBatch,
    before: &DynamicLowStretchForestSnapshot,
) -> Result<
    (
        DynamicLowStretchForestStageEventKind,
        DynamicLowStretchForestSnapshot,
    ),
    DynamicLowStretchForestError,
> {
    if before.stage.checked_add(1) != Some(batch.outer_stage) {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    let mut after = before.clone();
    let old_roots = before.roots.clone();
    let old_forest = before.forest_edges.clone();
    for update in &batch.updates {
        audit_stage_update(reference, batch.outer_stage, update, &mut after)?;
    }
    audit_refresh(reference, &mut after)?;
    let roots_added = ordered_difference(&after.roots, &old_roots);
    let removed = ordered_difference(&old_forest, &after.forest_edges);
    audit_record_metrics(&mut after, &roots_added, &removed)?;
    after.stage = batch.outer_stage;
    Ok((
        DynamicLowStretchForestStageEventKind::Updated {
            batch: batch.clone(),
            roots_added,
            forest_edges_removed: removed,
        },
        after,
    ))
}

fn audit_stage_update(
    reference: &ReferenceTree,
    outer_stage: u64,
    update: &DynamicLowStretchForestStageUpdate,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    match update {
        DynamicLowStretchForestStageUpdate::Insert { edge } => {
            audit_stage_edge(edge, snapshot.edge_slots.len(), snapshot.active_node_count)?;
            if snapshot.edge_slots[edge.edge].is_some() {
                return Err(DynamicLowStretchForestError::TraceVerification);
            }
            audit_add_endpoint_roots(reference, &mut snapshot.roots, edge.from, edge.to);
            snapshot.edge_slots[edge.edge] = Some(edge.clone());
            snapshot.stretch_overestimates[edge.edge] = Some(BigRational::one());
            snapshot.insertion_epoch[edge.edge] = Some(outer_stage);
            snapshot.metrics.insertions = audit_increment(snapshot.metrics.insertions)?;
        }
        DynamicLowStretchForestStageUpdate::Delete { edge } => {
            if snapshot.edge_slots.get(edge.edge).and_then(Option::as_ref) != Some(edge) {
                return Err(DynamicLowStretchForestError::TraceVerification);
            }
            audit_add_endpoint_roots(reference, &mut snapshot.roots, edge.from, edge.to);
            snapshot.edge_slots[edge.edge] = None;
            snapshot.stretch_overestimates[edge.edge] = None;
            snapshot.insertion_epoch[edge.edge] = None;
            snapshot.metrics.deletions = audit_increment(snapshot.metrics.deletions)?;
        }
        DynamicLowStretchForestStageUpdate::Reinsert { before, after } => {
            audit_stage_edge(after, snapshot.edge_slots.len(), snapshot.active_node_count)?;
            if before.edge != after.edge
                || before.from != after.from
                || before.to != after.to
                || snapshot
                    .edge_slots
                    .get(before.edge)
                    .and_then(Option::as_ref)
                    != Some(before)
            {
                return Err(DynamicLowStretchForestError::TraceVerification);
            }
            audit_add_endpoint_roots(reference, &mut snapshot.roots, after.from, after.to);
            snapshot.edge_slots[before.edge] = Some(after.clone());
            snapshot.stretch_overestimates[before.edge] = Some(BigRational::one());
            snapshot.insertion_epoch[before.edge] = Some(outer_stage);
            snapshot.metrics.reinsertions = audit_increment(snapshot.metrics.reinsertions)?;
        }
        DynamicLowStretchForestStageUpdate::ReplaceLength {
            edge,
            before,
            after,
        } => {
            if after <= &BigRational::zero()
                || audit_rational_too_wide(before)
                || audit_rational_too_wide(after)
            {
                return Err(DynamicLowStretchForestError::TraceVerification);
            }
            let row = snapshot
                .edge_slots
                .get_mut(*edge)
                .and_then(Option::as_mut)
                .ok_or(DynamicLowStretchForestError::TraceVerification)?;
            if row.length != *before {
                return Err(DynamicLowStretchForestError::TraceVerification);
            }
            row.length = after.clone();
            snapshot.metrics.length_replacements =
                audit_increment(snapshot.metrics.length_replacements)?;
        }
        DynamicLowStretchForestStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => audit_stage_split_update(
            reference,
            snapshot,
            *retained_vertex,
            *new_vertex,
            new_side_incidences,
            *encoded_side,
            encoded_incidences,
        )?,
    }
    Ok(())
}

fn audit_stage_split_update(
    reference: &ReferenceTree,
    snapshot: &mut DynamicLowStretchForestSnapshot,
    retained_vertex: usize,
    new_vertex: usize,
    new_side_incidences: &[DynamicLowStretchForestIncidence],
    encoded_side: DynamicLowStretchForestEncodedSide,
    encoded_incidences: &[DynamicLowStretchForestIncidence],
) -> Result<(), DynamicLowStretchForestError> {
    if retained_vertex >= snapshot.active_node_count || new_vertex != snapshot.active_node_count {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    audit_validate_stage_split(
        &snapshot.edge_slots,
        retained_vertex,
        new_side_incidences,
        encoded_side,
        encoded_incidences,
    )?;
    audit_add_ancestor_closure(reference, &mut snapshot.roots, retained_vertex);
    insert_sorted_unique(&mut snapshot.roots, new_vertex);
    for incidence in new_side_incidences {
        audit_apply_stage_incidence(
            &mut snapshot.edge_slots,
            retained_vertex,
            new_vertex,
            *incidence,
        )?;
    }
    snapshot.active_node_count = snapshot
        .active_node_count
        .checked_add(1)
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    snapshot.metrics.vertex_splits = audit_increment(snapshot.metrics.vertex_splits)?;
    snapshot.metrics.moved_edges = snapshot
        .metrics
        .moved_edges
        .checked_add(
            u64::try_from(new_side_incidences.len())
                .map_err(|_| DynamicLowStretchForestError::TraceVerification)?,
        )
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    Ok(())
}

fn audit_stage_edge(
    edge: &DynamicLowStretchForestEdge,
    slots: usize,
    nodes: usize,
) -> Result<(), DynamicLowStretchForestError> {
    if edge.edge >= slots
        || edge.from >= nodes
        || edge.to >= nodes
        || edge.length <= BigRational::zero()
        || audit_rational_too_wide(&edge.length)
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

fn audit_validate_stage_split(
    slots: &[Option<DynamicLowStretchForestEdge>],
    retained_vertex: usize,
    new_side: &[DynamicLowStretchForestIncidence],
    encoded_side: DynamicLowStretchForestEncodedSide,
    encoded: &[DynamicLowStretchForestIncidence],
) -> Result<(), DynamicLowStretchForestError> {
    if new_side.windows(2).any(|pair| pair[0] >= pair[1])
        || encoded.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    let all = audit_stage_incidences_at(slots, retained_vertex);
    if new_side.iter().any(|incidence| !all.contains(incidence)) {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    let stay = all
        .iter()
        .copied()
        .filter(|incidence| !new_side.contains(incidence))
        .collect::<Vec<_>>();
    let (side, encoding) = if new_side.len() <= stay.len() {
        (DynamicLowStretchForestEncodedSide::New, new_side.to_vec())
    } else {
        (DynamicLowStretchForestEncodedSide::Retained, stay)
    };
    if side != encoded_side || encoding != encoded {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

fn audit_stage_incidences_at(
    slots: &[Option<DynamicLowStretchForestEdge>],
    vertex: usize,
) -> Vec<DynamicLowStretchForestIncidence> {
    let mut result = Vec::new();
    for edge in slots.iter().flatten() {
        if edge.from == vertex {
            result.push(DynamicLowStretchForestIncidence {
                edge: edge.edge,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
            });
        }
        if edge.to == vertex {
            result.push(DynamicLowStretchForestIncidence {
                edge: edge.edge,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn audit_apply_stage_incidence(
    slots: &mut [Option<DynamicLowStretchForestEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidence: DynamicLowStretchForestIncidence,
) -> Result<(), DynamicLowStretchForestError> {
    let row = slots
        .get_mut(incidence.edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    let endpoint = if incidence.endpoint == DynamicLowStretchForestIncidenceEndpoint::Tail {
        &mut row.from
    } else {
        &mut row.to
    };
    if *endpoint != retained_vertex {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    *endpoint = new_vertex;
    Ok(())
}

fn run_internal(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
    record: bool,
) -> Result<InternalRun, DynamicLowStretchForestError> {
    validate_input(input, operations)?;
    let reference = build_reference_tree(input)?;
    let mut snapshot = initial_snapshot(input, &reference)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { operations.len() + 1 } else { 0 });
    for operation in operations {
        let before = snapshot.clone();
        let kind = apply_operation(&reference, operation, &mut snapshot)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        check_snapshot(input, &reference, &snapshot, &before)?;
        if record {
            events.push(DynamicLowStretchForestTraceEvent {
                catalog_id: CATALOG_ID,
                kind,
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicLowStretchForestTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicLowStretchForestEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalRun {
        base_snapshot,
        events,
        result: DynamicLowStretchForestResult {
            final_snapshot: snapshot,
        },
    })
}

fn apply_operation(
    reference: &ReferenceTree,
    operation: &DynamicLowStretchForestOperation,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<DynamicLowStretchForestEventKind, DynamicLowStretchForestError> {
    match operation {
        DynamicLowStretchForestOperation::Insert { edge } => {
            let old_roots = snapshot.roots.clone();
            let old_forest = snapshot.forest_edges.clone();
            add_endpoint_roots(reference, &mut snapshot.roots, edge.from, edge.to);
            snapshot.edge_slots[edge.edge] = Some(edge.clone());
            snapshot.stretch_overestimates[edge.edge] = Some(BigRational::one());
            let next_stage = increment(snapshot.stage)?;
            snapshot.insertion_epoch[edge.edge] = Some(next_stage);
            snapshot.metrics.insertions = increment(snapshot.metrics.insertions)?;
            refresh_dynamic_state(reference, snapshot)?;
            let roots_added = ordered_difference(&snapshot.roots, &old_roots);
            let forest_edges_removed = ordered_difference(&old_forest, &snapshot.forest_edges);
            record_root_forest_metrics(snapshot, &roots_added, &forest_edges_removed)?;
            snapshot.stage = next_stage;
            Ok(DynamicLowStretchForestEventKind::Inserted {
                edge: edge.edge,
                roots_added,
                forest_edges_removed,
            })
        }
        DynamicLowStretchForestOperation::Delete { edge } => {
            let old_roots = snapshot.roots.clone();
            let old_forest = snapshot.forest_edges.clone();
            let deleted = snapshot.edge_slots[*edge]
                .as_ref()
                .ok_or(DynamicLowStretchForestError::InvalidInput)?
                .clone();
            add_endpoint_roots(reference, &mut snapshot.roots, deleted.from, deleted.to);
            snapshot.edge_slots[*edge] = None;
            snapshot.stretch_overestimates[*edge] = None;
            snapshot.insertion_epoch[*edge] = None;
            snapshot.metrics.deletions = increment(snapshot.metrics.deletions)?;
            refresh_dynamic_state(reference, snapshot)?;
            let roots_added = ordered_difference(&snapshot.roots, &old_roots);
            let forest_edges_removed = ordered_difference(&old_forest, &snapshot.forest_edges);
            record_root_forest_metrics(snapshot, &roots_added, &forest_edges_removed)?;
            snapshot.stage = increment(snapshot.stage)?;
            Ok(DynamicLowStretchForestEventKind::Deleted {
                edge: *edge,
                roots_added,
                forest_edges_removed,
            })
        }
        DynamicLowStretchForestOperation::Reinsert { edge } => {
            apply_reinsert(reference, *edge, snapshot)
        }
        DynamicLowStretchForestOperation::VertexSplit {
            vertex,
            new_vertex,
            moved_edges,
        } => {
            let old_roots = snapshot.roots.clone();
            let old_forest = snapshot.forest_edges.clone();
            add_ancestor_closure(reference, &mut snapshot.roots, *vertex);
            insert_sorted_unique(&mut snapshot.roots, *new_vertex);
            for &edge in moved_edges {
                move_edge_endpoint(&mut snapshot.edge_slots[edge], *vertex, *new_vertex)?;
            }
            snapshot.active_node_count = snapshot
                .active_node_count
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
            snapshot.metrics.vertex_splits = increment(snapshot.metrics.vertex_splits)?;
            snapshot.metrics.moved_edges = snapshot
                .metrics
                .moved_edges
                .checked_add(
                    u64::try_from(moved_edges.len())
                        .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?,
                )
                .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
            refresh_dynamic_state(reference, snapshot)?;
            let roots_added = ordered_difference(&snapshot.roots, &old_roots);
            let forest_edges_removed = ordered_difference(&old_forest, &snapshot.forest_edges);
            record_root_forest_metrics(snapshot, &roots_added, &forest_edges_removed)?;
            snapshot.stage = increment(snapshot.stage)?;
            Ok(DynamicLowStretchForestEventKind::VertexSplit {
                vertex: *vertex,
                new_vertex: *new_vertex,
                moved_edges: moved_edges.clone(),
                roots_added,
                forest_edges_removed,
            })
        }
    }
}

fn audit_operation(
    reference: &ReferenceTree,
    operation: &DynamicLowStretchForestOperation,
    before: &DynamicLowStretchForestSnapshot,
) -> Result<
    (
        DynamicLowStretchForestEventKind,
        DynamicLowStretchForestSnapshot,
    ),
    DynamicLowStretchForestError,
> {
    let mut after = before.clone();
    let old_roots = before.roots.clone();
    let old_forest = before.forest_edges.clone();
    let kind = match operation {
        DynamicLowStretchForestOperation::Insert { edge } => {
            audit_add_endpoint_roots(reference, &mut after.roots, edge.from, edge.to);
            after.edge_slots[edge.edge] = Some(edge.clone());
            after.stretch_overestimates[edge.edge] = Some(BigRational::one());
            let next_stage = audit_increment(after.stage)?;
            after.insertion_epoch[edge.edge] = Some(next_stage);
            after.metrics.insertions = audit_increment(after.metrics.insertions)?;
            audit_refresh(reference, &mut after)?;
            let roots_added = ordered_difference(&after.roots, &old_roots);
            let removed = ordered_difference(&old_forest, &after.forest_edges);
            audit_record_metrics(&mut after, &roots_added, &removed)?;
            after.stage = next_stage;
            DynamicLowStretchForestEventKind::Inserted {
                edge: edge.edge,
                roots_added,
                forest_edges_removed: removed,
            }
        }
        DynamicLowStretchForestOperation::Delete { edge } => {
            let deleted = after.edge_slots[*edge]
                .as_ref()
                .ok_or(DynamicLowStretchForestError::TraceVerification)?
                .clone();
            audit_add_endpoint_roots(reference, &mut after.roots, deleted.from, deleted.to);
            after.edge_slots[*edge] = None;
            after.stretch_overestimates[*edge] = None;
            after.insertion_epoch[*edge] = None;
            after.metrics.deletions = audit_increment(after.metrics.deletions)?;
            audit_refresh(reference, &mut after)?;
            let roots_added = ordered_difference(&after.roots, &old_roots);
            let removed = ordered_difference(&old_forest, &after.forest_edges);
            audit_record_metrics(&mut after, &roots_added, &removed)?;
            after.stage = audit_increment(after.stage)?;
            DynamicLowStretchForestEventKind::Deleted {
                edge: *edge,
                roots_added,
                forest_edges_removed: removed,
            }
        }
        DynamicLowStretchForestOperation::Reinsert { edge } => {
            audit_reinsert(reference, *edge, &mut after)?
        }
        DynamicLowStretchForestOperation::VertexSplit {
            vertex,
            new_vertex,
            moved_edges,
        } => {
            audit_add_ancestor_closure(reference, &mut after.roots, *vertex);
            insert_sorted_unique(&mut after.roots, *new_vertex);
            for &edge in moved_edges {
                audit_move_endpoint(&mut after.edge_slots[edge], *vertex, *new_vertex)?;
            }
            after.active_node_count = after
                .active_node_count
                .checked_add(1)
                .ok_or(DynamicLowStretchForestError::TraceVerification)?;
            after.metrics.vertex_splits = audit_increment(after.metrics.vertex_splits)?;
            let moved = u64::try_from(moved_edges.len())
                .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
            after.metrics.moved_edges = after
                .metrics
                .moved_edges
                .checked_add(moved)
                .ok_or(DynamicLowStretchForestError::TraceVerification)?;
            audit_refresh(reference, &mut after)?;
            let roots_added = ordered_difference(&after.roots, &old_roots);
            let removed = ordered_difference(&old_forest, &after.forest_edges);
            audit_record_metrics(&mut after, &roots_added, &removed)?;
            after.stage = audit_increment(after.stage)?;
            DynamicLowStretchForestEventKind::VertexSplit {
                vertex: *vertex,
                new_vertex: *new_vertex,
                moved_edges: moved_edges.clone(),
                roots_added,
                forest_edges_removed: removed,
            }
        }
    };
    Ok((kind, after))
}

fn apply_reinsert(
    reference: &ReferenceTree,
    edge: usize,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<DynamicLowStretchForestEventKind, DynamicLowStretchForestError> {
    let old_roots = snapshot.roots.clone();
    let old_forest = snapshot.forest_edges.clone();
    let row = snapshot.edge_slots[edge]
        .as_ref()
        .ok_or(DynamicLowStretchForestError::InvalidInput)?
        .clone();
    add_endpoint_roots(reference, &mut snapshot.roots, row.from, row.to);
    snapshot.stretch_overestimates[edge] = Some(BigRational::one());
    let next_stage = increment(snapshot.stage)?;
    snapshot.insertion_epoch[edge] = Some(next_stage);
    snapshot.metrics.reinsertions = increment(snapshot.metrics.reinsertions)?;
    refresh_dynamic_state(reference, snapshot)?;
    let roots_added = ordered_difference(&snapshot.roots, &old_roots);
    let forest_edges_removed = ordered_difference(&old_forest, &snapshot.forest_edges);
    record_root_forest_metrics(snapshot, &roots_added, &forest_edges_removed)?;
    snapshot.stage = next_stage;
    Ok(DynamicLowStretchForestEventKind::Reinserted {
        edge,
        roots_added,
        forest_edges_removed,
    })
}

fn audit_reinsert(
    reference: &ReferenceTree,
    edge: usize,
    after: &mut DynamicLowStretchForestSnapshot,
) -> Result<DynamicLowStretchForestEventKind, DynamicLowStretchForestError> {
    let old_roots = after.roots.clone();
    let old_forest = after.forest_edges.clone();
    let row = after.edge_slots[edge]
        .as_ref()
        .ok_or(DynamicLowStretchForestError::TraceVerification)?
        .clone();
    audit_add_endpoint_roots(reference, &mut after.roots, row.from, row.to);
    after.stretch_overestimates[edge] = Some(BigRational::one());
    let next_stage = audit_increment(after.stage)?;
    after.insertion_epoch[edge] = Some(next_stage);
    after.metrics.reinsertions = audit_increment(after.metrics.reinsertions)?;
    audit_refresh(reference, after)?;
    let roots_added = ordered_difference(&after.roots, &old_roots);
    let removed = ordered_difference(&old_forest, &after.forest_edges);
    audit_record_metrics(after, &roots_added, &removed)?;
    after.stage = next_stage;
    Ok(DynamicLowStretchForestEventKind::Reinserted {
        edge,
        roots_added,
        forest_edges_removed: removed,
    })
}

fn initial_snapshot(
    input: &DynamicLowStretchForestInput,
    reference: &ReferenceTree,
) -> Result<DynamicLowStretchForestSnapshot, DynamicLowStretchForestError> {
    let mut roots = Vec::new();
    for &seed in &input.initial_root_seeds {
        add_ancestor_closure(reference, &mut roots, seed);
    }
    let mut snapshot = DynamicLowStretchForestSnapshot {
        active_node_count: input.initial_node_count,
        edge_slots: input.edge_slots.clone(),
        reference_tree_support: reference.support.clone(),
        roots,
        forest_edges: Vec::new(),
        congestion_order: congestion_order(reference),
        heavy_child: pad_optional_vertices(
            reference.auxiliary.heavy_child.clone(),
            input.maximum_node_count,
        ),
        heavy_chain_head: pad_optional_vertices(
            reference
                .auxiliary
                .chain_head
                .iter()
                .copied()
                .map(Some)
                .collect(),
            input.maximum_node_count,
        ),
        auxiliary_parent: pad_optional_vertices(
            reference.auxiliary.auxiliary_parent.clone(),
            input.maximum_node_count,
        ),
        auxiliary_depth: pad_optional_vertices(
            reference
                .auxiliary
                .auxiliary_depth
                .iter()
                .copied()
                .map(Some)
                .collect(),
            input.maximum_node_count,
        ),
        component_roots: Vec::new(),
        current_stretches: vec![None; input.edge_slots.len()],
        stretch_overestimates: vec![None; input.edge_slots.len()],
        insertion_epoch: input
            .edge_slots
            .iter()
            .map(|edge| edge.as_ref().map(|_| 0))
            .collect(),
        stage: 0,
        complete: false,
        metrics: DynamicLowStretchForestMetrics::default(),
    };
    refresh_forest_components(reference, &mut snapshot)?;
    refresh_stretches(reference, &mut snapshot)?;
    if let Some(overestimates) = &input.initial_stretch_overestimates {
        snapshot.stretch_overestimates.clone_from(overestimates);
    } else {
        for edge in snapshot.edge_slots.iter().flatten() {
            snapshot.stretch_overestimates[edge.edge] =
                Some(initial_overestimate(reference, &snapshot, edge)?);
        }
    }
    snapshot.metrics.stretch_checks = u64::try_from(active_edge_count(&snapshot))
        .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?;
    verify_stretch_bounds(&snapshot)?;
    Ok(snapshot)
}

fn refresh_dynamic_state(
    reference: &ReferenceTree,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    refresh_forest_components(reference, snapshot)?;
    refresh_stretches(reference, snapshot)?;
    let checks = u64::try_from(active_edge_count(snapshot))
        .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?;
    snapshot.metrics.stretch_checks = snapshot
        .metrics
        .stretch_checks
        .checked_add(checks)
        .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
    verify_stretch_bounds(snapshot)
}

fn audit_refresh(
    reference: &ReferenceTree,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    refresh_forest_components(reference, snapshot)
        .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    refresh_stretches(reference, snapshot)
        .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    let checks = u64::try_from(active_edge_count(snapshot))
        .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    snapshot.metrics.stretch_checks = snapshot
        .metrics
        .stretch_checks
        .checked_add(checks)
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    verify_stretch_bounds(snapshot).map_err(|_| DynamicLowStretchForestError::TraceVerification)
}

fn refresh_forest_components(
    reference: &ReferenceTree,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    let roots = snapshot.roots.iter().copied().collect::<BTreeSet<_>>();
    let static_node_count = reference.auxiliary.auxiliary_parent.len();
    for node in static_node_count..snapshot.active_node_count {
        if !roots.contains(&node) || reference.parent[node].is_some() {
            return Err(DynamicLowStretchForestError::InvariantViolation);
        }
    }
    let (forest_edges, static_component_roots) = static_forest_for_roots(reference, &roots)?;
    let mut component_roots = vec![usize::MAX; snapshot.active_node_count];
    component_roots[..static_node_count].copy_from_slice(&static_component_roots);
    for (node, component_root) in component_roots
        .iter_mut()
        .enumerate()
        .take(snapshot.active_node_count)
        .skip(static_node_count)
    {
        *component_root = node;
    }
    snapshot.forest_edges = forest_edges;
    snapshot.component_roots = component_roots;
    Ok(())
}

fn static_forest_for_roots(
    reference: &ReferenceTree,
    roots: &BTreeSet<usize>,
) -> Result<(Vec<usize>, Vec<usize>), DynamicLowStretchForestError> {
    let static_node_count = reference.auxiliary.auxiliary_parent.len();
    let static_roots = roots
        .range(..static_node_count)
        .copied()
        .collect::<Vec<_>>();
    if static_roots.is_empty()
        || !is_branch_free(&reference.parent[..static_node_count], &static_roots)
    {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    let removed = removed_edges_for_adjacent_roots(reference, roots)?;
    let mut adjacency = vec![Vec::<usize>::new(); static_node_count];
    let mut forest_edges = Vec::new();
    for node in 0..static_node_count {
        let Some(parent) = reference.parent[node] else {
            continue;
        };
        let edge =
            reference.parent_edge[node].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
        if !removed.contains(&edge) {
            adjacency[node].push(parent);
            adjacency[parent].push(node);
            forest_edges.push(edge);
        }
    }
    for row in &mut adjacency {
        row.sort_unstable();
    }
    forest_edges.sort_unstable();

    let mut component_roots = vec![usize::MAX; static_node_count];
    let mut seen = vec![false; static_node_count];
    for start in 0..static_node_count {
        if seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        seen[start] = true;
        while let Some(node) = stack.pop() {
            component.push(node);
            for &next in adjacency[node].iter().rev() {
                if !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        let component_root = component
            .iter()
            .copied()
            .filter(|node| roots.contains(node))
            .collect::<Vec<_>>();
        if component_root.len() != 1 {
            return Err(DynamicLowStretchForestError::InvariantViolation);
        }
        for node in component {
            component_roots[node] = component_root[0];
        }
    }
    Ok((forest_edges, component_roots))
}

fn removed_edges_for_adjacent_roots(
    reference: &ReferenceTree,
    roots: &BTreeSet<usize>,
) -> Result<BTreeSet<usize>, DynamicLowStretchForestError> {
    let static_node_count = reference.auxiliary.auxiliary_parent.len();
    let mut removed = BTreeSet::new();
    for &root in roots.range(..static_node_count) {
        if root == reference.auxiliary.root {
            continue;
        }
        let mut cursor = root;
        let mut path = Vec::new();
        loop {
            let edge = reference.parent_edge[cursor]
                .ok_or(DynamicLowStretchForestError::InvariantViolation)?;
            path.push(edge);
            cursor =
                reference.parent[cursor].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
            if roots.contains(&cursor) {
                break;
            }
        }
        let selected = path
            .into_iter()
            .min_by_key(|&edge| {
                reference.pi_rank[edge].map_or((usize::MAX, edge), |rank| (rank, edge))
            })
            .ok_or(DynamicLowStretchForestError::InvariantViolation)?;
        if reference.pi_rank[selected].is_none() || !removed.insert(selected) {
            return Err(DynamicLowStretchForestError::InvariantViolation);
        }
    }
    if removed.len().checked_add(1) != Some(roots.range(..static_node_count).count()) {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    Ok(removed)
}

fn verify_hld_lsf_source_contract(
    reference: &ReferenceTree,
) -> Result<(), DynamicLowStretchForestError> {
    let node_count = reference.auxiliary.auxiliary_parent.len();
    let subset_count = 1_usize
        .checked_shl(
            u32::try_from(node_count)
                .map_err(|_| DynamicLowStretchForestError::InvariantViolation)?,
        )
        .ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    let mut behaviors = BTreeMap::<Vec<usize>, Vec<usize>>::new();
    for mask in 1_usize..subset_count {
        let seeds = (0..node_count)
            .filter(|&node| mask & (1_usize << node) != 0)
            .collect::<Vec<_>>();
        let mut roots = Vec::new();
        for seed in seeds {
            add_ancestor_closure(reference, &mut roots, seed);
        }
        if behaviors.contains_key(&roots) {
            continue;
        }
        let root_set = roots.iter().copied().collect::<BTreeSet<_>>();
        let (_, component_roots) = static_forest_for_roots(reference, &root_set)?;
        for (node, &component_root) in component_roots.iter().enumerate() {
            if !is_auxiliary_ancestor(reference, component_root, node) {
                return Err(DynamicLowStretchForestError::InvariantViolation);
            }
        }
        behaviors.insert(roots, component_roots);
    }

    let mut determined = BTreeMap::<(usize, Vec<usize>), usize>::new();
    for (roots, component_roots) in behaviors {
        for (node, &component_root) in component_roots.iter().enumerate() {
            let ancestor_intersection = roots
                .iter()
                .copied()
                .filter(|&root| is_auxiliary_ancestor(reference, root, node))
                .collect::<Vec<_>>();
            match determined.insert((node, ancestor_intersection), component_root) {
                Some(previous) if previous != component_root => {
                    return Err(DynamicLowStretchForestError::InvariantViolation);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn is_auxiliary_ancestor(reference: &ReferenceTree, ancestor: usize, node: usize) -> bool {
    let mut cursor = Some(node);
    while let Some(current) = cursor {
        if current == ancestor {
            return true;
        }
        cursor = reference.auxiliary.auxiliary_parent[current];
    }
    false
}

fn refresh_stretches(
    reference: &ReferenceTree,
    snapshot: &mut DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    let mut stretches = vec![None; snapshot.edge_slots.len()];
    for edge in snapshot.edge_slots.iter().flatten() {
        stretches[edge.edge] = Some(forest_stretch(reference, snapshot, edge)?);
    }
    snapshot.current_stretches = stretches;
    Ok(())
}

fn forest_stretch(
    reference: &ReferenceTree,
    snapshot: &DynamicLowStretchForestSnapshot,
    edge: &DynamicLowStretchForestEdge,
) -> Result<BigRational, DynamicLowStretchForestError> {
    let from_root = snapshot.component_roots[edge.from];
    let to_root = snapshot.component_roots[edge.to];
    let route_length = if from_root == to_root {
        tree_path_length(reference, edge.from, edge.to)?
    } else {
        tree_path_length(reference, edge.from, from_root)?
            + tree_path_length(reference, edge.to, to_root)?
    };
    Ok(BigRational::one() + route_length / &edge.length)
}

fn initial_overestimate(
    reference: &ReferenceTree,
    base: &DynamicLowStretchForestSnapshot,
    edge: &DynamicLowStretchForestEdge,
) -> Result<BigRational, DynamicLowStretchForestError> {
    let maximum_depth = reference
        .auxiliary
        .auxiliary_depth
        .iter()
        .copied()
        .max()
        .unwrap_or(0);
    let mut sum = BigRational::zero();
    for radius in 0..=maximum_depth {
        let roots = reference
            .auxiliary
            .auxiliary_depth
            .iter()
            .enumerate()
            .filter_map(|(node, &depth)| (depth <= radius).then_some(node))
            .collect::<Vec<_>>();
        let mut state = base.clone();
        state.roots = roots;
        refresh_forest_components(reference, &mut state)?;
        sum += forest_stretch(reference, &state, edge)?;
    }
    Ok(sum * BigInt::from(2_u8))
}

fn pad_optional_vertices<T>(
    mut values: Vec<Option<T>>,
    maximum_node_count: usize,
) -> Vec<Option<T>> {
    values.resize_with(maximum_node_count, || None);
    values
}

fn tree_path_length(
    reference: &ReferenceTree,
    from: usize,
    to: usize,
) -> Result<BigRational, DynamicLowStretchForestError> {
    if from == to {
        return Ok(BigRational::zero());
    }
    if from >= reference.parent.len() || to >= reference.parent.len() {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    let mut left = from;
    let mut right = to;
    let mut length = BigRational::zero();
    while reference.depth[left] > reference.depth[right] {
        length += parent_length(reference, left)?;
        left = reference.parent[left].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    }
    while reference.depth[right] > reference.depth[left] {
        length += parent_length(reference, right)?;
        right = reference.parent[right].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    }
    while left != right {
        length += parent_length(reference, left)?;
        length += parent_length(reference, right)?;
        left = reference.parent[left].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
        right = reference.parent[right].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    }
    Ok(length)
}

fn parent_length(
    reference: &ReferenceTree,
    node: usize,
) -> Result<BigRational, DynamicLowStretchForestError> {
    let edge =
        reference.parent_edge[node].ok_or(DynamicLowStretchForestError::InvariantViolation)?;
    reference.edge_lengths[edge]
        .clone()
        .ok_or(DynamicLowStretchForestError::InvariantViolation)
}

fn verify_stretch_bounds(
    snapshot: &DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    for index in 0..snapshot.edge_slots.len() {
        match (
            &snapshot.edge_slots[index],
            &snapshot.current_stretches[index],
            &snapshot.stretch_overestimates[index],
            snapshot.insertion_epoch[index],
        ) {
            (Some(_), Some(actual), Some(upper), Some(_))
                if upper >= actual && upper > &BigRational::zero() => {}
            (None, None, None, None) => {}
            _ => return Err(DynamicLowStretchForestError::InvariantViolation),
        }
    }
    Ok(())
}

fn build_reference_tree(
    input: &DynamicLowStretchForestInput,
) -> Result<ReferenceTree, DynamicLowStretchForestError> {
    let mut adjacency = vec![Vec::<(usize, usize)>::new(); input.initial_node_count];
    let mut edge_lengths = vec![None; input.edge_slots.len()];
    let mut support = vec![None; input.edge_slots.len()];
    for &edge_id in &input.reference_tree_edges {
        let edge = input.edge_slots[edge_id]
            .as_ref()
            .ok_or(DynamicLowStretchForestError::InvalidInput)?;
        adjacency[edge.from].push((edge.to, edge_id));
        adjacency[edge.to].push((edge.from, edge_id));
        edge_lengths[edge_id] = Some(edge.length.clone());
        support[edge_id] = Some(edge.clone());
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable_by_key(|&(node, edge)| (edge, node));
    }
    let mut parent = vec![None; input.maximum_node_count];
    let mut parent_edge = vec![None; input.maximum_node_count];
    let mut depth = vec![0; input.maximum_node_count];
    let mut stack = vec![input.reference_root];
    let mut seen = vec![false; input.initial_node_count];
    seen[input.reference_root] = true;
    while let Some(node) = stack.pop() {
        for &(next, edge) in adjacency[node].iter().rev() {
            if seen[next] {
                continue;
            }
            seen[next] = true;
            parent[next] = Some(node);
            parent_edge[next] = Some(edge);
            depth[next] = depth[node] + 1;
            stack.push(next);
        }
    }
    if seen.iter().any(|seen| !seen) {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    let mut congestion = Vec::new();
    for &tree_edge in &input.reference_tree_edges {
        let mut total = BigRational::zero();
        for edge in input.edge_slots.iter().flatten() {
            if path_contains_tree_edge(&parent, &parent_edge, &depth, edge, tree_edge)? {
                total += BigRational::one() / &edge.length;
            }
        }
        congestion.push((total, tree_edge));
    }
    congestion.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    let mut pi_rank = vec![None; input.edge_slots.len()];
    for (rank, (_, edge)) in congestion.into_iter().enumerate() {
        pi_rank[edge] = Some(rank);
    }
    let auxiliary =
        build_hld_branch_free_tree(&parent[..input.initial_node_count], input.reference_root)
            .map_err(|_| DynamicLowStretchForestError::InvariantViolation)?;
    check_hld_branch_free_tree(&parent[..input.initial_node_count], &auxiliary)
        .map_err(|_| DynamicLowStretchForestError::InvariantViolation)?;
    let reference = ReferenceTree {
        parent,
        parent_edge,
        depth,
        edge_lengths,
        pi_rank,
        support,
        auxiliary,
    };
    verify_hld_lsf_source_contract(&reference)?;
    Ok(reference)
}

fn path_contains_tree_edge(
    parent: &[Option<usize>],
    parent_edge: &[Option<usize>],
    depth: &[usize],
    edge: &DynamicLowStretchForestEdge,
    target: usize,
) -> Result<bool, DynamicLowStretchForestError> {
    let mut left = edge.from;
    let mut right = edge.to;
    while depth[left] > depth[right] {
        if parent_edge[left] == Some(target) {
            return Ok(true);
        }
        left = parent[left].ok_or(DynamicLowStretchForestError::InvalidInput)?;
    }
    while depth[right] > depth[left] {
        if parent_edge[right] == Some(target) {
            return Ok(true);
        }
        right = parent[right].ok_or(DynamicLowStretchForestError::InvalidInput)?;
    }
    while left != right {
        if parent_edge[left] == Some(target) || parent_edge[right] == Some(target) {
            return Ok(true);
        }
        left = parent[left].ok_or(DynamicLowStretchForestError::InvalidInput)?;
        right = parent[right].ok_or(DynamicLowStretchForestError::InvalidInput)?;
    }
    Ok(false)
}

fn add_endpoint_roots(reference: &ReferenceTree, roots: &mut Vec<usize>, from: usize, to: usize) {
    add_ancestor_closure(reference, roots, from);
    add_ancestor_closure(reference, roots, to);
}

fn audit_add_endpoint_roots(
    reference: &ReferenceTree,
    roots: &mut Vec<usize>,
    from: usize,
    to: usize,
) {
    audit_add_ancestor_closure(reference, roots, from);
    audit_add_ancestor_closure(reference, roots, to);
}

fn add_ancestor_closure(reference: &ReferenceTree, roots: &mut Vec<usize>, mut node: usize) {
    if node >= reference.auxiliary.auxiliary_parent.len() {
        insert_sorted_unique(roots, node);
        return;
    }
    loop {
        insert_sorted_unique(roots, node);
        let Some(parent) = reference.auxiliary.auxiliary_parent[node] else {
            break;
        };
        node = parent;
    }
}

fn audit_add_ancestor_closure(reference: &ReferenceTree, roots: &mut Vec<usize>, mut node: usize) {
    if node >= reference.auxiliary.auxiliary_parent.len() {
        insert_sorted_unique(roots, node);
        return;
    }
    loop {
        insert_sorted_unique(roots, node);
        match reference.auxiliary.auxiliary_parent[node] {
            Some(parent) => node = parent,
            None => break,
        }
    }
}

fn insert_sorted_unique(values: &mut Vec<usize>, value: usize) {
    if let Err(index) = values.binary_search(&value) {
        values.insert(index, value);
    }
}

fn move_edge_endpoint(
    slot: &mut Option<DynamicLowStretchForestEdge>,
    vertex: usize,
    new_vertex: usize,
) -> Result<(), DynamicLowStretchForestError> {
    let edge = slot
        .as_mut()
        .ok_or(DynamicLowStretchForestError::InvalidInput)?;
    match (edge.from == vertex, edge.to == vertex) {
        (true, false) => edge.from = new_vertex,
        (false, true) => edge.to = new_vertex,
        _ => return Err(DynamicLowStretchForestError::InvalidInput),
    }
    Ok(())
}

fn audit_move_endpoint(
    slot: &mut Option<DynamicLowStretchForestEdge>,
    vertex: usize,
    new_vertex: usize,
) -> Result<(), DynamicLowStretchForestError> {
    let edge = slot
        .as_mut()
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    if edge.from == vertex && edge.to != vertex {
        edge.from = new_vertex;
    } else if edge.to == vertex && edge.from != vertex {
        edge.to = new_vertex;
    } else {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

fn ordered_difference(left: &[usize], right: &[usize]) -> Vec<usize> {
    left.iter()
        .copied()
        .filter(|value| right.binary_search(value).is_err())
        .collect()
}

fn record_root_forest_metrics(
    snapshot: &mut DynamicLowStretchForestSnapshot,
    roots: &[usize],
    forest_edges: &[usize],
) -> Result<(), DynamicLowStretchForestError> {
    snapshot.metrics.roots_added = snapshot
        .metrics
        .roots_added
        .checked_add(
            u64::try_from(roots.len())
                .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
    snapshot.metrics.forest_edges_removed = snapshot
        .metrics
        .forest_edges_removed
        .checked_add(
            u64::try_from(forest_edges.len())
                .map_err(|_| DynamicLowStretchForestError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)?;
    Ok(())
}

fn audit_record_metrics(
    snapshot: &mut DynamicLowStretchForestSnapshot,
    roots: &[usize],
    forest_edges: &[usize],
) -> Result<(), DynamicLowStretchForestError> {
    let roots =
        u64::try_from(roots.len()).map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    let removed = u64::try_from(forest_edges.len())
        .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    snapshot.metrics.roots_added = snapshot
        .metrics
        .roots_added
        .checked_add(roots)
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    snapshot.metrics.forest_edges_removed = snapshot
        .metrics
        .forest_edges_removed
        .checked_add(removed)
        .ok_or(DynamicLowStretchForestError::TraceVerification)?;
    Ok(())
}

fn validate_input(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
) -> Result<(), DynamicLowStretchForestError> {
    if input.initial_node_count == 0
        || input.initial_node_count > input.maximum_node_count
        || input.maximum_node_count > DYNAMIC_LSF_MAX_NODES
        || input.edge_slots.is_empty()
        || input.edge_slots.len() > DYNAMIC_LSF_MAX_EDGES
        || input.reference_root >= input.initial_node_count
        || input.initial_root_seeds.is_empty()
        || input
            .initial_root_seeds
            .iter()
            .any(|&node| node >= input.initial_node_count)
    {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    if operations.len() > DYNAMIC_LSF_MAX_OPERATIONS {
        return Err(DynamicLowStretchForestError::AdmissionLimit);
    }
    let expected_tree_edges = input.initial_node_count - 1;
    if input.reference_tree_edges.len() != expected_tree_edges {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    let mut tree_ids = input.reference_tree_edges.clone();
    tree_ids.sort_unstable();
    tree_ids.dedup();
    if tree_ids.len() != expected_tree_edges
        || tree_ids.iter().any(|&edge| edge >= input.edge_slots.len())
    {
        return Err(DynamicLowStretchForestError::InvalidInput);
    }
    for (slot, edge) in input.edge_slots.iter().enumerate() {
        if let Some(edge) = edge {
            if edge.edge != slot
                || edge.from >= input.initial_node_count
                || edge.to >= input.initial_node_count
                || edge.length <= BigRational::zero()
            {
                return Err(DynamicLowStretchForestError::InvalidInput);
            }
            if rational_too_wide(&edge.length) {
                return Err(DynamicLowStretchForestError::AdmissionLimit);
            }
        }
    }
    if let Some(overestimates) = &input.initial_stretch_overestimates {
        if overestimates.len() != input.edge_slots.len() {
            return Err(DynamicLowStretchForestError::InvalidInput);
        }
        for (edge, overestimate) in input.edge_slots.iter().zip(overestimates) {
            match (edge, overestimate) {
                (Some(_), Some(value)) if value > &BigRational::zero() => {
                    if rational_too_wide(value) {
                        return Err(DynamicLowStretchForestError::AdmissionLimit);
                    }
                }
                (None, None) => {}
                _ => return Err(DynamicLowStretchForestError::InvalidInput),
            }
        }
    }
    validate_operation_sequence(input, operations)
}

fn validate_operation_sequence(
    input: &DynamicLowStretchForestInput,
    operations: &[DynamicLowStretchForestOperation],
) -> Result<(), DynamicLowStretchForestError> {
    let mut slots = input.edge_slots.clone();
    let mut active_nodes = input.initial_node_count;
    for operation in operations {
        match operation {
            DynamicLowStretchForestOperation::Insert { edge } => {
                if edge.edge >= slots.len()
                    || slots[edge.edge].is_some()
                    || edge.from >= active_nodes
                    || edge.to >= active_nodes
                    || edge.length <= BigRational::zero()
                {
                    return Err(DynamicLowStretchForestError::InvalidInput);
                }
                if rational_too_wide(&edge.length) {
                    return Err(DynamicLowStretchForestError::AdmissionLimit);
                }
                slots[edge.edge] = Some(edge.clone());
            }
            DynamicLowStretchForestOperation::Delete { edge } => {
                if *edge >= slots.len() || slots[*edge].is_none() {
                    return Err(DynamicLowStretchForestError::InvalidInput);
                }
                slots[*edge] = None;
            }
            DynamicLowStretchForestOperation::Reinsert { edge } => {
                if *edge >= slots.len() || slots[*edge].is_none() {
                    return Err(DynamicLowStretchForestError::InvalidInput);
                }
            }
            DynamicLowStretchForestOperation::VertexSplit {
                vertex,
                new_vertex,
                moved_edges,
            } => {
                if *vertex >= active_nodes
                    || *new_vertex != active_nodes
                    || active_nodes >= input.maximum_node_count
                    || moved_edges.is_empty()
                    || moved_edges.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err(DynamicLowStretchForestError::InvalidInput);
                }
                let incident = slots
                    .iter()
                    .flatten()
                    .filter(|edge| edge.from == *vertex || edge.to == *vertex)
                    .count();
                if moved_edges.len() > incident.saturating_sub(moved_edges.len()) {
                    return Err(DynamicLowStretchForestError::InvalidInput);
                }
                for &edge_id in moved_edges {
                    let edge = slots
                        .get_mut(edge_id)
                        .and_then(Option::as_mut)
                        .ok_or(DynamicLowStretchForestError::InvalidInput)?;
                    if edge.from == *vertex && edge.to != *vertex {
                        edge.from = *new_vertex;
                    } else if edge.to == *vertex && edge.from != *vertex {
                        edge.to = *new_vertex;
                    } else {
                        return Err(DynamicLowStretchForestError::InvalidInput);
                    }
                }
                active_nodes += 1;
            }
        }
    }
    Ok(())
}

fn check_snapshot(
    input: &DynamicLowStretchForestInput,
    reference: &ReferenceTree,
    snapshot: &DynamicLowStretchForestSnapshot,
    before: &DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    structural_snapshot_check(input, reference, snapshot)?;
    if before
        .roots
        .iter()
        .any(|root| snapshot.roots.binary_search(root).is_err())
        || snapshot
            .forest_edges
            .iter()
            .any(|edge| before.forest_edges.binary_search(edge).is_err())
    {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    Ok(())
}

fn audit_snapshot(
    input: &DynamicLowStretchForestInput,
    reference: &ReferenceTree,
    snapshot: &DynamicLowStretchForestSnapshot,
    before: &DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    structural_snapshot_check(input, reference, snapshot)
        .map_err(|_| DynamicLowStretchForestError::TraceVerification)?;
    if before
        .roots
        .iter()
        .any(|root| snapshot.roots.binary_search(root).is_err())
        || snapshot
            .forest_edges
            .iter()
            .any(|edge| before.forest_edges.binary_search(edge).is_err())
    {
        return Err(DynamicLowStretchForestError::TraceVerification);
    }
    Ok(())
}

fn structural_snapshot_check(
    input: &DynamicLowStretchForestInput,
    reference: &ReferenceTree,
    snapshot: &DynamicLowStretchForestSnapshot,
) -> Result<(), DynamicLowStretchForestError> {
    if snapshot.active_node_count < input.initial_node_count
        || snapshot.active_node_count > input.maximum_node_count
        || snapshot.component_roots.len() != snapshot.active_node_count
        || snapshot.edge_slots.len() != input.edge_slots.len()
        || snapshot.reference_tree_support != reference.support
        || snapshot.current_stretches.len() != input.edge_slots.len()
        || snapshot.stretch_overestimates.len() != input.edge_slots.len()
        || snapshot.insertion_epoch.len() != input.edge_slots.len()
        || snapshot.heavy_child.len() != input.maximum_node_count
        || snapshot.heavy_chain_head.len() != input.maximum_node_count
        || snapshot.auxiliary_parent.len() != input.maximum_node_count
        || snapshot.auxiliary_depth.len() != input.maximum_node_count
        || snapshot.roots.windows(2).any(|pair| pair[0] >= pair[1])
        || snapshot
            .roots
            .iter()
            .any(|&root| root >= snapshot.active_node_count)
        || snapshot.congestion_order != congestion_order(reference)
        || snapshot.heavy_child
            != pad_optional_vertices(
                reference.auxiliary.heavy_child.clone(),
                input.maximum_node_count,
            )
        || snapshot.heavy_chain_head
            != pad_optional_vertices(
                reference
                    .auxiliary
                    .chain_head
                    .iter()
                    .copied()
                    .map(Some)
                    .collect(),
                input.maximum_node_count,
            )
        || snapshot.auxiliary_parent
            != pad_optional_vertices(
                reference.auxiliary.auxiliary_parent.clone(),
                input.maximum_node_count,
            )
        || snapshot.auxiliary_depth
            != pad_optional_vertices(
                reference
                    .auxiliary
                    .auxiliary_depth
                    .iter()
                    .copied()
                    .map(Some)
                    .collect(),
                input.maximum_node_count,
            )
    {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    for &root in &snapshot.roots {
        if root >= reference.auxiliary.auxiliary_parent.len() {
            continue;
        }
        let mut cursor = root;
        while let Some(parent) = reference.auxiliary.auxiliary_parent[cursor] {
            if snapshot.roots.binary_search(&parent).is_err() {
                return Err(DynamicLowStretchForestError::InvariantViolation);
            }
            cursor = parent;
        }
    }
    let static_roots = snapshot
        .roots
        .iter()
        .copied()
        .take_while(|&root| root < input.initial_node_count)
        .collect::<Vec<_>>();
    if !is_branch_free(&reference.parent[..input.initial_node_count], &static_roots) {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    let mut rebuilt = snapshot.clone();
    refresh_forest_components(reference, &mut rebuilt)?;
    refresh_stretches(reference, &mut rebuilt)?;
    if rebuilt.forest_edges != snapshot.forest_edges
        || rebuilt.component_roots != snapshot.component_roots
        || rebuilt.current_stretches != snapshot.current_stretches
    {
        return Err(DynamicLowStretchForestError::InvariantViolation);
    }
    verify_stretch_bounds(snapshot)
}

fn active_edge_count(snapshot: &DynamicLowStretchForestSnapshot) -> usize {
    snapshot.edge_slots.iter().flatten().count()
}

fn congestion_order(reference: &ReferenceTree) -> Vec<usize> {
    let mut order = reference
        .pi_rank
        .iter()
        .enumerate()
        .filter_map(|(edge, rank)| rank.map(|rank| (rank, edge)))
        .collect::<Vec<_>>();
    order.sort_unstable();
    order.into_iter().map(|(_, edge)| edge).collect()
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_LSF_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_LSF_MAX_RATIONAL_BITS
}

fn audit_rational_too_wide(value: &BigRational) -> bool {
    let numerator_bits = value.numer().bits();
    let denominator_bits = value.denom().bits();
    numerator_bits > DYNAMIC_LSF_MAX_RATIONAL_BITS
        || denominator_bits > DYNAMIC_LSF_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, DynamicLowStretchForestError> {
    value
        .checked_add(1)
        .ok_or(DynamicLowStretchForestError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicLowStretchForestError> {
    value
        .checked_add(1)
        .ok_or(DynamicLowStretchForestError::TraceVerification)
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

    fn input() -> DynamicLowStretchForestInput {
        DynamicLowStretchForestInput {
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
        }
    }

    #[test]
    fn insertion_and_deletion_add_hld_ancestor_roots_and_keep_forest_decremental() {
        let operations = vec![
            DynamicLowStretchForestOperation::Insert {
                edge: edge(4, 2, 0, 3),
            },
            DynamicLowStretchForestOperation::Delete { edge: 2 },
        ];
        let trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        assert_eq!(trace.base_snapshot.forest_edges, vec![0, 1, 2]);
        assert_eq!(trace.events[0].after.roots, vec![0, 2]);
        assert_eq!(trace.events[0].after.forest_edges, vec![1, 2]);
        assert_ne!(
            trace.base_snapshot.auxiliary_parent,
            vec![None, Some(0), Some(1), Some(1), None]
        );
        assert_eq!(
            trace.events[0].after.stretch_overestimates[4],
            Some(BigRational::one())
        );
        assert_eq!(
            trace.events[0].after.current_stretches[4],
            Some(BigRational::one())
        );
        assert_eq!(trace.result.final_snapshot.roots, vec![0, 1, 2, 3]);
        assert!(trace.result.final_snapshot.forest_edges.is_empty());
    }

    #[test]
    fn vertex_split_moves_only_encoded_smaller_side_to_isolated_root() {
        let operations = vec![DynamicLowStretchForestOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![5],
        }];
        let result = execute_dynamic_low_stretch_forest(&input(), &operations).expect("split");
        let snapshot = result.final_snapshot;
        assert_eq!(snapshot.active_node_count, 5);
        assert_eq!(snapshot.edge_slots[5].as_ref().expect("edge").from, 4);
        assert_eq!(snapshot.roots, vec![0, 1, 2, 4]);
        assert_eq!(snapshot.component_roots[4], 4);
        assert_eq!(snapshot.metrics.vertex_splits, 1);
        assert_eq!(snapshot.metrics.moved_edges, 1);
    }

    #[test]
    fn active_edge_reinsertion_preserves_the_row_and_resets_its_epoch() {
        let before = execute_dynamic_low_stretch_forest(&input(), &[])
            .expect("initial")
            .final_snapshot;
        let operations = vec![DynamicLowStretchForestOperation::Reinsert { edge: 3 }];
        let trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        let after = &trace.result.final_snapshot;
        assert_eq!(after.edge_slots[3], before.edge_slots[3]);
        assert_eq!(after.insertion_epoch[3], Some(1));
        assert_eq!(after.stretch_overestimates[3], Some(BigRational::one()));
        assert_eq!(after.metrics.reinsertions, 1);
        assert!(matches!(
            trace.events[0].kind,
            DynamicLowStretchForestEventKind::Reinserted { edge: 3, .. }
        ));
        check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace).expect("check");

        assert_eq!(
            execute_dynamic_low_stretch_forest(
                &input(),
                &[DynamicLowStretchForestOperation::Reinsert { edge: 4 }]
            ),
            Err(DynamicLowStretchForestError::InvalidInput)
        );
    }

    #[test]
    fn atomic_stage_keeps_insert_and_forced_reinsert_in_one_epoch() {
        let inserted = edge(4, 2, 0, 3);
        let batches = vec![DynamicLowStretchForestStageBatch {
            outer_stage: 1,
            updates: vec![
                DynamicLowStretchForestStageUpdate::Insert {
                    edge: inserted.clone(),
                },
                DynamicLowStretchForestStageUpdate::Reinsert {
                    before: inserted.clone(),
                    after: inserted,
                },
            ],
        }];
        let fast = execute_dynamic_low_stretch_forest_stages(&input(), &batches).expect("fast");
        let trace = trace_dynamic_low_stretch_forest_stages(&input(), &batches).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.final_snapshot.stage, 1);
        assert_eq!(fast.final_snapshot.insertion_epoch[4], Some(1));
        assert_eq!(fast.final_snapshot.metrics.insertions, 1);
        assert_eq!(fast.final_snapshot.metrics.reinsertions, 1);
        assert_eq!(fast.final_snapshot.metrics.state_transitions, 2);
        let DynamicLowStretchForestStageEventKind::Updated { batch, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        assert_eq!(batch.updates.len(), 2);
        check_dynamic_low_stretch_forest_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_split_separates_actual_new_side_from_retained_encoding() {
        let moved = vec![
            DynamicLowStretchForestIncidence {
                edge: 1,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
            },
            DynamicLowStretchForestIncidence {
                edge: 2,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
            },
            DynamicLowStretchForestIncidence {
                edge: 5,
                endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
            },
        ];
        let encoded = vec![DynamicLowStretchForestIncidence {
            edge: 0,
            endpoint: DynamicLowStretchForestIncidenceEndpoint::Head,
        }];
        let batches = vec![DynamicLowStretchForestStageBatch {
            outer_stage: 1,
            updates: vec![DynamicLowStretchForestStageUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                new_side_incidences: moved,
                encoded_side: DynamicLowStretchForestEncodedSide::Retained,
                encoded_incidences: encoded,
            }],
        }];
        let trace = trace_dynamic_low_stretch_forest_stages(&input(), &batches).expect("trace");
        let after = &trace.result.final_snapshot;
        assert_eq!(after.edge_slots[0].as_ref().map(|edge| edge.to), Some(1));
        for edge_id in [1, 2, 5] {
            assert_eq!(
                after.edge_slots[edge_id].as_ref().map(|edge| edge.from),
                Some(4)
            );
        }
        assert_eq!(after.metrics.moved_edges, 3);
        check_dynamic_low_stretch_forest_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_split_moves_one_self_loop_incidence_and_accepts_empty_side() {
        let mut self_loop = input();
        self_loop.edge_slots[5] = Some(edge(5, 1, 1, 2));
        self_loop.maximum_node_count = 6;
        let batches = vec![DynamicLowStretchForestStageBatch {
            outer_stage: 1,
            updates: vec![
                DynamicLowStretchForestStageUpdate::VertexSplit {
                    retained_vertex: 1,
                    new_vertex: 4,
                    new_side_incidences: Vec::new(),
                    encoded_side: DynamicLowStretchForestEncodedSide::New,
                    encoded_incidences: Vec::new(),
                },
                DynamicLowStretchForestStageUpdate::VertexSplit {
                    retained_vertex: 1,
                    new_vertex: 5,
                    new_side_incidences: vec![DynamicLowStretchForestIncidence {
                        edge: 5,
                        endpoint: DynamicLowStretchForestIncidenceEndpoint::Head,
                    }],
                    encoded_side: DynamicLowStretchForestEncodedSide::New,
                    encoded_incidences: vec![DynamicLowStretchForestIncidence {
                        edge: 5,
                        endpoint: DynamicLowStretchForestIncidenceEndpoint::Head,
                    }],
                },
            ],
        }];
        let result = execute_dynamic_low_stretch_forest_stages(&self_loop, &batches)
            .expect("incidence splits");
        assert_eq!(result.final_snapshot.active_node_count, 6);
        assert_eq!(
            result.final_snapshot.edge_slots[5]
                .as_ref()
                .map(|edge| (edge.from, edge.to)),
            Some((1, 5))
        );
    }

    #[test]
    fn atomic_stage_checker_rejects_metric_and_encoding_tampering() {
        let batches = vec![DynamicLowStretchForestStageBatch {
            outer_stage: 1,
            updates: vec![DynamicLowStretchForestStageUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                new_side_incidences: vec![DynamicLowStretchForestIncidence {
                    edge: 5,
                    endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
                }],
                encoded_side: DynamicLowStretchForestEncodedSide::New,
                encoded_incidences: vec![DynamicLowStretchForestIncidence {
                    edge: 5,
                    endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
                }],
            }],
        }];
        let trace = trace_dynamic_low_stretch_forest_stages(&input(), &batches).expect("trace");
        let mut tampered = trace.clone();
        tampered.events[0].after.metrics.moved_edges = 0;
        assert_eq!(
            check_dynamic_low_stretch_forest_stage_trace(&input(), &batches, &tampered),
            Err(DynamicLowStretchForestError::TraceVerification)
        );

        let mut malformed = batches;
        let DynamicLowStretchForestStageUpdate::VertexSplit { encoded_side, .. } =
            &mut malformed[0].updates[0]
        else {
            panic!("split");
        };
        *encoded_side = DynamicLowStretchForestEncodedSide::Retained;
        assert_eq!(
            execute_dynamic_low_stretch_forest_stages(&input(), &malformed),
            Err(DynamicLowStretchForestError::InvalidInput)
        );
    }

    #[test]
    fn fast_trace_and_checker_match_and_reject_stretch_tampering() {
        let operations = vec![DynamicLowStretchForestOperation::Insert {
            edge: edge(4, 2, 0, 3),
        }];
        let fast = execute_dynamic_low_stretch_forest(&input(), &operations).expect("fast");
        let mut trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        assert_eq!(fast, trace.result);
        check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace).expect("check");
        trace.events[0].after.current_stretches[4] = Some(rational(2));
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace),
            Err(DynamicLowStretchForestError::TraceVerification)
        );
    }

    #[test]
    fn checker_rejects_root_epoch_and_metric_tampering() {
        let operations = vec![DynamicLowStretchForestOperation::Insert {
            edge: edge(4, 2, 0, 3),
        }];
        let mut trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        trace.events[0].after.roots.pop();
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace),
            Err(DynamicLowStretchForestError::TraceVerification)
        );

        let mut trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        trace.events[0].after.auxiliary_parent[1] = Some(0);
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace),
            Err(DynamicLowStretchForestError::TraceVerification)
        );

        let mut trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        trace.events[0].after.insertion_epoch[4] = Some(99);
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace),
            Err(DynamicLowStretchForestError::TraceVerification)
        );

        let mut trace = trace_dynamic_low_stretch_forest(&input(), &operations).expect("trace");
        trace.events[0].after.metrics.roots_added = 0;
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(&input(), &operations, &trace),
            Err(DynamicLowStretchForestError::TraceVerification)
        );
    }

    #[test]
    fn invalid_reuse_large_side_and_nonincident_move_fail_closed() {
        let occupied = DynamicLowStretchForestOperation::Insert {
            edge: edge(0, 0, 3, 1),
        };
        assert_eq!(
            execute_dynamic_low_stretch_forest(&input(), &[occupied]),
            Err(DynamicLowStretchForestError::InvalidInput)
        );
        let mut many_off_tree_edges = input();
        many_off_tree_edges.edge_slots.extend([
            Some(edge(6, 1, 0, 2)),
            Some(edge(7, 1, 2, 2)),
            Some(edge(8, 1, 3, 2)),
        ]);
        let large_side = DynamicLowStretchForestOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![5, 6, 7, 8],
        };
        assert_eq!(
            execute_dynamic_low_stretch_forest(&many_off_tree_edges, &[large_side]),
            Err(DynamicLowStretchForestError::InvalidInput)
        );
        let nonincident = DynamicLowStretchForestOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![3],
        };
        assert_eq!(
            execute_dynamic_low_stretch_forest(&input(), &[nonincident]),
            Err(DynamicLowStretchForestError::InvalidInput)
        );
    }

    #[test]
    fn split_moves_dynamic_reference_edge_row_but_keeps_static_tree_support() {
        let operation = DynamicLowStretchForestOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![1],
        };
        let trace = trace_dynamic_low_stretch_forest(&input(), &[operation]).expect("split");
        let snapshot = &trace.result.final_snapshot;
        assert_eq!(snapshot.edge_slots[1].as_ref().expect("dynamic").from, 4);
        assert_eq!(
            snapshot.reference_tree_support[1]
                .as_ref()
                .expect("support"),
            &edge(1, 1, 2, 1)
        );
        assert_eq!(snapshot.component_roots[4], 4);
        assert!(
            snapshot.current_stretches[1].as_ref().expect("stretch")
                <= snapshot.stretch_overestimates[1].as_ref().expect("upper")
        );

        let mut forged = trace;
        forged.events[0].after.reference_tree_support[1]
            .as_mut()
            .expect("support")
            .from = 4;
        assert_eq!(
            check_dynamic_low_stretch_forest_trace(
                &input(),
                &[DynamicLowStretchForestOperation::VertexSplit {
                    vertex: 1,
                    new_vertex: 4,
                    moved_edges: vec![1],
                }],
                &forged
            ),
            Err(DynamicLowStretchForestError::TraceVerification)
        );
    }

    #[test]
    fn atomic_split_preserves_reference_support_for_a_moved_tree_incidence() {
        let incidence = DynamicLowStretchForestIncidence {
            edge: 1,
            endpoint: DynamicLowStretchForestIncidenceEndpoint::Tail,
        };
        let batch = DynamicLowStretchForestStageBatch {
            outer_stage: 1,
            updates: vec![DynamicLowStretchForestStageUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                new_side_incidences: vec![incidence],
                encoded_side: DynamicLowStretchForestEncodedSide::New,
                encoded_incidences: vec![incidence],
            }],
        };
        let trace = trace_dynamic_low_stretch_forest_stages(&input(), &[batch]).expect("split");
        let snapshot = &trace.result.final_snapshot;
        assert_eq!(snapshot.edge_slots[1].as_ref().expect("dynamic").from, 4);
        assert_eq!(
            snapshot.reference_tree_support[1]
                .as_ref()
                .expect("support"),
            &edge(1, 1, 2, 1)
        );
    }

    #[test]
    fn initial_overestimates_are_positive_and_dominate_source_stretch() {
        let result = execute_dynamic_low_stretch_forest(&input(), &[]).expect("initial");
        for (actual, upper) in result
            .final_snapshot
            .current_stretches
            .iter()
            .zip(&result.final_snapshot.stretch_overestimates)
        {
            if let (Some(actual), Some(upper)) = (actual, upper) {
                assert!(upper >= actual);
                assert!(upper > &BigRational::zero());
            }
        }
    }

    #[test]
    fn every_nonempty_seed_subset_satisfies_all_four_hld_lsf_conditions() {
        let reference = build_reference_tree(&input()).expect("reference");
        verify_hld_lsf_source_contract(&reference).expect("conditions 2-4");
        check_hld_branch_free_tree(
            &reference.parent[..input().initial_node_count],
            &reference.auxiliary,
        )
        .expect("condition 1");
    }
}
