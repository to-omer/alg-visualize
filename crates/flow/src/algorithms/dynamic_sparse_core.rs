//! Exact bounded dynamic sparsified-core transition primitive.
//!
//! This module composes the checked dynamic core graph with the update order in
//! Dynamic Sparsified Core Graphs: core deletions and vertex splits are applied
//! to the maintained subgraph, core insertions are inserted directly, and the
//! exact re-embedded set is reported as forced reinsertion updates. Initial
//! core edges are partitioned into factor-two length buckets. Every bucket runs
//! the three deterministic source tasks: expander decomposition plus witness
//! construction, witness-to-core embedding, and core-to-witness embedding.
//! Active edges inserted or reinserted later are retained directly, while
//! omitted self-loops use an empty path.
//!
//! Each initial length bucket also carries the bounded CKLPPS schedule with
//! `H_0..H_L`, `Pi_0..Pi_L`, touched sets, projected auxiliary multigraphs,
//! selected preimages, composed embeddings, and the exact re-embedded source
//! set `D`. Later insertions remain direct, as required by the source update
//! order. Delegated expander/path subroutines are exhaustive and exact on the
//! small graph band; the project does not claim the paper's asymptotic
//! expander constant, amortized low-recourse bound, or almost-linear runtime.

use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use num_rational::BigRational;
use thiserror::Error;

use super::bounded_dynamic_spanner::{
    BoundedDynamicSpannerEdgeState, BoundedDynamicSpannerEndpoint, BoundedDynamicSpannerTrace,
    BoundedDynamicSpannerUpdate, check_bounded_dynamic_spanner_trace,
    trace_bounded_dynamic_spanner,
};
use super::deterministic_spanner_sparsify::DeterministicSpannerSparsifyCertificate;
use super::{
    DYNAMIC_CORE_MAX_EDGES, DYNAMIC_CORE_MAX_NODES, DYNAMIC_CORE_MAX_OPERATIONS,
    DYNAMIC_CORE_MAX_RATIONAL_BITS, DynamicCoreEdge, DynamicCoreEncodedSide, DynamicCoreGraphError,
    DynamicCoreGraphEventKind, DynamicCoreGraphInput, DynamicCoreGraphOperation,
    DynamicCoreGraphSnapshot, DynamicCoreGraphStageBatch, DynamicCoreGraphStageEventKind,
    DynamicCoreGraphStageTraceResult, DynamicCoreGraphTraceResult, DynamicCoreIncidence,
    DynamicCoreIncidenceEndpoint, DynamicCoreUpdate, check_dynamic_core_graph_stage_trace,
    check_dynamic_core_graph_trace, trace_dynamic_core_graph, trace_dynamic_core_graph_stages,
};

/// Maximum stable vertices.
pub const DYNAMIC_SPARSE_CORE_MAX_NODES: usize = DYNAMIC_CORE_MAX_NODES;
/// Maximum stable core/spanner edge slots.
pub const DYNAMIC_SPARSE_CORE_MAX_EDGES: usize = DYNAMIC_CORE_MAX_EDGES;
/// Maximum source topology operations.
pub const DYNAMIC_SPARSE_CORE_MAX_OPERATIONS: usize = DYNAMIC_CORE_MAX_OPERATIONS;
/// Maximum branch/reduction factor used in Definition 5.7 bounds.
pub const DYNAMIC_SPARSE_CORE_MAX_BRANCHES: usize = 8;
/// Maximum reversible boundaries, including completion.
pub const DYNAMIC_SPARSE_CORE_MAX_TRACE_EVENTS: usize = DYNAMIC_SPARSE_CORE_MAX_OPERATIONS + 1;
/// Maximum exact numerator or denominator width inherited from the core.
pub const DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS: u64 = DYNAMIC_CORE_MAX_RATIONAL_BITS;

const CATALOG_ID: &str = "dynamic-sparsified-core";

/// Input core graph and bounded Definition 5.7 reduction factor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreInput {
    /// Source/LSF/core input.
    pub core: DynamicCoreGraphInput,
    /// Positive reduction factor `k` used by the edge/congestion bounds.
    pub branches: usize,
}

/// One signed stable spanner edge in an explicit embedding path.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DynamicSparseCoreEmbeddingArc {
    /// Stable core/spanner edge ID.
    pub edge: usize,
    /// `1` follows the stored orientation and `-1` opposes it.
    pub direction: i8,
}

/// Why a maintained spanner edge was structurally inserted or deleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreRefreshReason {
    /// A newly inserted core edge is retained directly, as in the source update order.
    DirectInsertion,
    /// The corresponding core edge was deleted.
    CoreDeletion,
    /// The bounded deterministic source sparsifier changed at stage end.
    SparsifyRefresh,
}

/// One ordered update to the maintained sparsified core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreUpdate {
    /// A core vertex split is forwarded using the current spanner incidences.
    VertexSplit {
        /// Existing spanner vertex.
        retained_vertex: usize,
        /// Newly materialized spanner vertex.
        new_vertex: usize,
        /// Actual spanner incidences moved to the new vertex.
        new_side_incidences: Vec<DynamicCoreIncidence>,
        /// Canonical smaller side of the spanner split encoding.
        encoded_side: DynamicCoreEncodedSide,
        /// Strictly ordered smaller-side incidences.
        encoded_incidences: Vec<DynamicCoreIncidence>,
    },
    /// One spanner-subgraph edge was inserted.
    EdgeInserted {
        /// Complete row, identical to the current core row.
        edge: DynamicCoreEdge,
        /// Source-direct or deterministic-sparsifier refresh insertion.
        reason: DynamicSparseCoreRefreshReason,
    },
    /// One spanner-subgraph edge was deleted.
    EdgeDeleted {
        /// Complete row at deletion time.
        edge: DynamicCoreEdge,
        /// Core deletion or deterministic-sparsifier refresh deletion.
        reason: DynamicSparseCoreRefreshReason,
    },
    /// A selected active edge was explicitly reinserted without topology change.
    EdgeReinserted {
        /// Selected sparse-core row before reinsertion.
        before: DynamicCoreEdge,
        /// Current selected sparse-core row after reinsertion.
        after: DynamicCoreEdge,
    },
    /// A selected edge's core gradient changed after a source vertex split.
    GradientReplaced {
        /// Stable edge ID.
        edge: usize,
        /// Gradient before replacement.
        before: BigRational,
        /// Gradient after replacement.
        after: BigRational,
    },
    /// A selected edge's exact core length changed in place.
    LengthReplaced {
        /// Stable edge ID.
        edge: usize,
        /// Length before replacement.
        before: BigRational,
        /// Length after replacement.
        after: BigRational,
    },
    /// A current spanner edge gained at least one embedding preimage.
    ForcedReinsert {
        /// Stable edge ID in the exact re-embedded set.
        edge: usize,
    },
}

/// Exact dynamic sparse-core work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicSparseCoreMetrics {
    /// Source/core stages consumed.
    pub source_updates: u64,
    /// Core updates inspected.
    pub core_updates: u64,
    /// Spanner vertex splits emitted.
    pub vertex_splits: u64,
    /// Actual spanner incidences moved by splits.
    pub endpoint_moves: u64,
    /// Spanner edge insertions.
    pub edge_insertions: u64,
    /// Spanner edge deletions.
    pub edge_deletions: u64,
    /// Selected active-edge reinsertions emitted.
    pub edge_reinsertions: u64,
    /// Selected-edge gradient replacements.
    pub gradient_replacements: u64,
    /// Selected-edge length replacements.
    pub length_replacements: u64,
    /// Deterministic-sparsifier refresh insertions plus deletions.
    pub sparsify_refreshes: u64,
    /// Forced reinsertion records.
    pub forced_reinsertions: u64,
    /// Active core embedding checks.
    pub embedding_checks: u64,
    /// Reversible public transitions.
    pub state_transitions: u64,
}

/// One fixed factor-two length bucket and its decremental CKLPPS schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreSpannerBucket {
    /// Stable core edge ID for every dense schedule row.
    pub stable_edges: Vec<usize>,
    /// Exact `H_0..H_L`, `Pi_0..Pi_L`, touched-set, projection, and `D` trace.
    pub trace: BoundedDynamicSpannerTrace,
}

/// Complete core, sparsifier, embedding, and recourse state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreSnapshot {
    /// Current contracted vertices.
    pub core_vertices: Vec<usize>,
    /// Current exact core edge slots.
    pub core_edge_slots: Vec<Option<DynamicCoreEdge>>,
    /// Current exact sparsified-core subgraph edge slots.
    pub spanner_edge_slots: Vec<Option<DynamicCoreEdge>>,
    /// Stable embedding path for every active core edge slot.
    pub core_to_spanner: Vec<Vec<DynamicSparseCoreEmbeddingArc>>,
    /// Exact three-task source certificates, one per nonempty length bucket.
    pub sparsify_certificates: Vec<DeterministicSpannerSparsifyCertificate>,
    /// Fixed initial length buckets with source-faithful decremental schedules.
    pub dynamic_spanner_buckets: Vec<DynamicSparseCoreSpannerBucket>,
    /// Whether an active edge was directly inserted after initialization.
    pub direct_edges: Vec<bool>,
    /// Minimal positive Definition 5.7 embedding/sparsity parameter.
    pub gamma_length: usize,
    /// Minimal positive Definition 5.7 congestion parameter.
    pub gamma_congestion: usize,
    /// Exact minimal re-embedded set from the most recent stage.
    pub last_reembedded: Vec<usize>,
    /// Completed source stages.
    pub stage: u64,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: DynamicSparseCoreMetrics,
}

/// Meaning of one reversible sparse-core boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreEventKind {
    /// One checked core event and its induced sparse-core update batch.
    Updated {
        /// Core event identity.
        core_event: Box<DynamicCoreGraphEventKind>,
        /// Ordered sparse-core updates, ending in forced reinsertions.
        sparse_updates: Vec<DynamicSparseCoreUpdate>,
    },
    /// Every supplied source operation completed.
    Completed,
}

/// One fully reversible sparse-core boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source/core/sparsifier transition.
    pub kind: DynamicSparseCoreEventKind,
    /// State before the transition.
    pub before: DynamicSparseCoreSnapshot,
    /// State after the transition.
    pub after: DynamicSparseCoreSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreResult {
    /// Terminal sparse-core state.
    pub final_snapshot: DynamicSparseCoreSnapshot,
}

/// Complete component and sparse-core transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreTraceResult {
    /// Independently checkable dynamic-core component transcript.
    pub core_trace: DynamicCoreGraphTraceResult,
    /// Initial sparse-core state.
    pub base_snapshot: DynamicSparseCoreSnapshot,
    /// One event per source operation followed by completion.
    pub events: Vec<DynamicSparseCoreTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicSparseCoreResult,
}

/// Meaning of one atomic sparse-core stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreStageEventKind {
    /// One checked atomic core stage and its induced sparse update batch.
    Updated {
        /// Atomic core event identity.
        core_event: Box<DynamicCoreGraphStageEventKind>,
        /// Ordered sparse updates, ending in forced reinsertions.
        sparse_updates: Vec<DynamicSparseCoreUpdate>,
    },
    /// Every supplied source stage completed.
    Completed,
}

/// One reversible atomic sparse-core boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreStageTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Atomic core/sparse transition.
    pub kind: DynamicSparseCoreStageEventKind,
    /// Sparse-core state before the whole batch.
    pub before: DynamicSparseCoreSnapshot,
    /// Sparse-core state after the whole batch.
    pub after: DynamicSparseCoreSnapshot,
}

/// Complete atomic core and sparse-core transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreStageTraceResult {
    /// Independently checkable atomic core component transcript.
    pub core_trace: DynamicCoreGraphStageTraceResult,
    /// Initial sparse-core state.
    pub base_snapshot: DynamicSparseCoreSnapshot,
    /// One event per source stage followed by completion.
    pub events: Vec<DynamicSparseCoreStageTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicSparseCoreResult,
}

/// Explicit bounded dynamic sparse-core failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicSparseCoreError {
    /// Branch/reduction config or operation input is malformed.
    #[error("dynamic sparsified core input is invalid")]
    InvalidInput,
    /// The composed core-graph primitive failed.
    #[error("dynamic sparsified core component failed: {0}")]
    Core(#[from] DynamicCoreGraphError),
    /// A Definition 5.7, subgraph, embedding, or update invariant failed.
    #[error("dynamic sparsified core invariant failed")]
    InvariantViolation,
    /// Checked work arithmetic overflowed.
    #[error("dynamic sparsified core arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact independent replay.
    #[error("dynamic sparsified core trace verification failed")]
    TraceVerification,
}

struct SparseBuild {
    spanner_edge_slots: Vec<Option<DynamicCoreEdge>>,
    embedding: Vec<Vec<DynamicSparseCoreEmbeddingArc>>,
    sparsify_certificates: Vec<DeterministicSpannerSparsifyCertificate>,
    gamma_length: usize,
    gamma_congestion: usize,
}

struct SparseAccumulator {
    selected: BTreeSet<usize>,
    embedding: Vec<Vec<DynamicSparseCoreEmbeddingArc>>,
    sparsify_certificates: Vec<DeterministicSpannerSparsifyCertificate>,
    scheduled_edges: BTreeSet<usize>,
}

struct InternalRun {
    core_trace: DynamicCoreGraphTraceResult,
    base_snapshot: DynamicSparseCoreSnapshot,
    events: Vec<DynamicSparseCoreTraceEvent>,
    result: DynamicSparseCoreResult,
}

/// Executes the bounded dynamic sparsified-core transition system.
///
/// # Errors
///
/// Rejects invalid/out-of-band input, component failure, or any violated
/// subgraph, embedding, re-embedding, and exact-work invariant.
pub fn execute_dynamic_sparse_core(
    input: &DynamicSparseCoreInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicSparseCoreResult, DynamicSparseCoreError> {
    run_internal(input, operations, false).map(|run| run.result)
}

/// Records every source topology and completion boundary.
///
/// # Errors
///
/// Returns any execution or independent replay-checker failure.
pub fn trace_dynamic_sparse_core(
    input: &DynamicSparseCoreInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicSparseCoreTraceResult, DynamicSparseCoreError> {
    let run = run_internal(input, operations, true)?;
    let trace = DynamicSparseCoreTraceResult {
        core_trace: run.core_trace,
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_sparse_core_trace(input, operations, &trace)?;
    Ok(trace)
}

/// Independently verifies the core component and sparse-core transcript.
///
/// # Errors
///
/// Rejects invalid input, component drift, noncanonical update order, a
/// nonminimal re-embedded set, or any final Definition 5.7 mismatch.
pub fn check_dynamic_sparse_core_trace(
    input: &DynamicSparseCoreInput,
    operations: &[DynamicCoreGraphOperation],
    trace: &DynamicSparseCoreTraceResult,
) -> Result<(), DynamicSparseCoreError> {
    validate_config(input, operations)?;
    check_dynamic_core_graph_trace(&input.core, operations, &trace.core_trace)?;
    let mut direct_edges = vec![false; input.core.forest.edge_slots.len()];
    let mut dynamic_spanner_buckets = initialize_dynamic_spanner_buckets(
        &trace.core_trace.base_snapshot.edge_slots,
        &direct_edges,
        input.core.forest.maximum_node_count,
    )?;
    let mut snapshot = build_snapshot(
        &trace.core_trace.base_snapshot,
        &direct_edges,
        &dynamic_spanner_buckets,
        input.branches,
        DynamicSparseCoreMetrics::default(),
        Vec::new(),
        false,
    )?;
    snapshot.metrics.embedding_checks = active_core_count(&snapshot)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != operations
                .len()
                .checked_add(1)
                .ok_or(DynamicSparseCoreError::TraceVerification)?
    {
        return Err(DynamicSparseCoreError::TraceVerification);
    }

    for (index, event) in trace.events.iter().take(operations.len()).enumerate() {
        let core_event = &trace.core_trace.events[index];
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicSparseCoreError::TraceVerification);
        }
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &core_event.kind else {
            return Err(DynamicSparseCoreError::TraceVerification);
        };
        let next_dynamic_spanner_buckets =
            advance_dynamic_spanner_buckets(&dynamic_spanner_buckets, core_updates)?;
        let expected_updates = audit_sparse_updates(
            input.branches,
            &snapshot,
            core_updates,
            &core_event.after,
            &mut direct_edges,
            &next_dynamic_spanner_buckets,
        )?;
        let reembedded = exact_reembedded_set(
            &snapshot,
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
        )?;
        let mut metrics = audit_metrics(snapshot.metrics, core_updates, &expected_updates)?;
        metrics.source_updates = audit_increment(metrics.source_updates)?;
        metrics.state_transitions = audit_increment(metrics.state_transitions)?;
        let mut expected_after = build_snapshot(
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
            metrics,
            reembedded,
            false,
        )?;
        expected_after.metrics.embedding_checks = expected_after
            .metrics
            .embedding_checks
            .checked_add(active_core_count(&expected_after)?)
            .ok_or(DynamicSparseCoreError::TraceVerification)?;
        if event.kind
            != (DynamicSparseCoreEventKind::Updated {
                core_event: Box::new(core_event.kind.clone()),
                sparse_updates: expected_updates,
            })
            || event.after != expected_after
        {
            return Err(DynamicSparseCoreError::TraceVerification);
        }
        snapshot = expected_after;
        dynamic_spanner_buckets = next_dynamic_spanner_buckets;
    }
    audit_completion(&snapshot, trace)
}

/// Executes atomic dynamic-core batches with one sparse refresh per outer stage.
///
/// # Errors
///
/// Rejects invalid configuration, component failure, or any violated sparse
/// subgraph, embedding, re-embedding, ordering, and exact-work invariant.
pub fn execute_dynamic_sparse_core_stages(
    input: &DynamicSparseCoreInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicSparseCoreResult, DynamicSparseCoreError> {
    run_stage_internal(input, batches, false).map(|run| run.result)
}

/// Executes atomic dynamic-core batches with reversible sparse boundaries.
///
/// # Errors
///
/// Returns any component execution or independent sparse replay failure.
pub fn trace_dynamic_sparse_core_stages(
    input: &DynamicSparseCoreInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicSparseCoreStageTraceResult, DynamicSparseCoreError> {
    let run = run_stage_internal(input, batches, true)?;
    let trace = DynamicSparseCoreStageTraceResult {
        core_trace: run.core_trace,
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_sparse_core_stage_trace(input, batches, &trace)?;
    Ok(trace)
}

/// Independently verifies atomic core and sparse-core stage transcripts.
///
/// # Errors
///
/// Rejects invalid input, component drift, noncanonical update order, a
/// nonminimal re-embedded set, or any final sparse definition mismatch.
pub fn check_dynamic_sparse_core_stage_trace(
    input: &DynamicSparseCoreInput,
    batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicSparseCoreStageTraceResult,
) -> Result<(), DynamicSparseCoreError> {
    validate_stage_config(input, batches)?;
    check_dynamic_core_graph_stage_trace(&input.core, batches, &trace.core_trace)?;
    let mut direct_edges = vec![false; input.core.forest.edge_slots.len()];
    let mut dynamic_spanner_buckets = initialize_dynamic_spanner_buckets(
        &trace.core_trace.base_snapshot.edge_slots,
        &direct_edges,
        input.core.forest.maximum_node_count,
    )?;
    let mut snapshot = build_snapshot(
        &trace.core_trace.base_snapshot,
        &direct_edges,
        &dynamic_spanner_buckets,
        input.branches,
        DynamicSparseCoreMetrics::default(),
        Vec::new(),
        false,
    )?;
    snapshot.metrics.embedding_checks = active_core_count(&snapshot)?;
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != batches
                .len()
                .checked_add(1)
                .ok_or(DynamicSparseCoreError::TraceVerification)?
    {
        return Err(DynamicSparseCoreError::TraceVerification);
    }
    for (index, event) in trace.events.iter().take(batches.len()).enumerate() {
        let core_event = &trace.core_trace.events[index];
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicSparseCoreError::TraceVerification);
        }
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } = &core_event.kind else {
            return Err(DynamicSparseCoreError::TraceVerification);
        };
        let next_dynamic_spanner_buckets =
            advance_dynamic_spanner_buckets(&dynamic_spanner_buckets, core_updates)?;
        let expected_updates = audit_sparse_updates(
            input.branches,
            &snapshot,
            core_updates,
            &core_event.after,
            &mut direct_edges,
            &next_dynamic_spanner_buckets,
        )?;
        let reembedded = exact_reembedded_set(
            &snapshot,
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
        )?;
        let mut metrics = audit_metrics(snapshot.metrics, core_updates, &expected_updates)?;
        metrics.source_updates = audit_increment(metrics.source_updates)?;
        metrics.state_transitions = audit_increment(metrics.state_transitions)?;
        let mut expected = build_snapshot(
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
            metrics,
            reembedded,
            false,
        )?;
        expected.metrics.embedding_checks = expected
            .metrics
            .embedding_checks
            .checked_add(active_core_count(&expected)?)
            .ok_or(DynamicSparseCoreError::TraceVerification)?;
        let kind = DynamicSparseCoreStageEventKind::Updated {
            core_event: Box::new(core_event.kind.clone()),
            sparse_updates: expected_updates,
        };
        if event.kind != kind || event.after != expected {
            return Err(DynamicSparseCoreError::TraceVerification);
        }
        snapshot = expected;
        dynamic_spanner_buckets = next_dynamic_spanner_buckets;
    }
    audit_stage_completion(&snapshot, trace)
}

struct InternalStageRun {
    core_trace: DynamicCoreGraphStageTraceResult,
    base_snapshot: DynamicSparseCoreSnapshot,
    events: Vec<DynamicSparseCoreStageTraceEvent>,
    result: DynamicSparseCoreResult,
}

fn run_stage_internal(
    input: &DynamicSparseCoreInput,
    batches: &[DynamicCoreGraphStageBatch],
    record: bool,
) -> Result<InternalStageRun, DynamicSparseCoreError> {
    validate_stage_config(input, batches)?;
    let core_trace = trace_dynamic_core_graph_stages(&input.core, batches)?;
    let mut direct_edges = vec![false; input.core.forest.edge_slots.len()];
    let mut dynamic_spanner_buckets = initialize_dynamic_spanner_buckets(
        &core_trace.base_snapshot.edge_slots,
        &direct_edges,
        input.core.forest.maximum_node_count,
    )?;
    let mut snapshot = build_snapshot(
        &core_trace.base_snapshot,
        &direct_edges,
        &dynamic_spanner_buckets,
        input.branches,
        DynamicSparseCoreMetrics::default(),
        Vec::new(),
        false,
    )?;
    snapshot.metrics.embedding_checks = active_core_count(&snapshot)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { batches.len() + 1 } else { 0 });
    for core_event in core_trace.events.iter().take(batches.len()) {
        let before = snapshot.clone();
        let DynamicCoreGraphStageEventKind::Updated { core_updates, .. } = &core_event.kind else {
            return Err(DynamicSparseCoreError::InvariantViolation);
        };
        let next_dynamic_spanner_buckets =
            advance_dynamic_spanner_buckets(&dynamic_spanner_buckets, core_updates)?;
        let sparse_updates = derive_sparse_updates(
            input.branches,
            &snapshot,
            core_updates,
            &core_event.after,
            &mut direct_edges,
            &next_dynamic_spanner_buckets,
        )?;
        let reembedded = exact_reembedded_set(
            &snapshot,
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
        )?;
        let mut metrics = apply_metrics(snapshot.metrics, core_updates, &sparse_updates)?;
        metrics.source_updates = increment(metrics.source_updates)?;
        metrics.state_transitions = increment(metrics.state_transitions)?;
        snapshot = build_snapshot(
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
            metrics,
            reembedded,
            false,
        )?;
        snapshot.metrics.embedding_checks = snapshot
            .metrics
            .embedding_checks
            .checked_add(active_core_count(&snapshot)?)
            .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?;
        if record {
            events.push(DynamicSparseCoreStageTraceEvent {
                catalog_id: CATALOG_ID,
                kind: DynamicSparseCoreStageEventKind::Updated {
                    core_event: Box::new(core_event.kind.clone()),
                    sparse_updates,
                },
                before,
                after: snapshot.clone(),
            });
        }
        dynamic_spanner_buckets = next_dynamic_spanner_buckets;
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicSparseCoreStageTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicSparseCoreStageEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalStageRun {
        core_trace,
        base_snapshot,
        events,
        result: DynamicSparseCoreResult {
            final_snapshot: snapshot,
        },
    })
}

fn validate_stage_config(
    input: &DynamicSparseCoreInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<(), DynamicSparseCoreError> {
    if input.branches == 0
        || input.branches > DYNAMIC_SPARSE_CORE_MAX_BRANCHES
        || input.core.forest.maximum_node_count > DYNAMIC_SPARSE_CORE_MAX_NODES
        || input.core.forest.edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
        || batches.len() > DYNAMIC_SPARSE_CORE_MAX_OPERATIONS
    {
        return Err(DynamicSparseCoreError::InvalidInput);
    }
    Ok(())
}

fn audit_stage_completion(
    snapshot: &DynamicSparseCoreSnapshot,
    trace: &DynamicSparseCoreStageTraceResult,
) -> Result<(), DynamicSparseCoreError> {
    let completion = trace
        .events
        .last()
        .ok_or(DynamicSparseCoreError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.complete = true;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != DynamicSparseCoreStageEventKind::Completed
        || completion.before != *snapshot
        || completion.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicSparseCoreError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    input: &DynamicSparseCoreInput,
    operations: &[DynamicCoreGraphOperation],
    record: bool,
) -> Result<InternalRun, DynamicSparseCoreError> {
    validate_config(input, operations)?;
    let core_trace = trace_dynamic_core_graph(&input.core, operations)?;
    let mut direct_edges = vec![false; input.core.forest.edge_slots.len()];
    let mut dynamic_spanner_buckets = initialize_dynamic_spanner_buckets(
        &core_trace.base_snapshot.edge_slots,
        &direct_edges,
        input.core.forest.maximum_node_count,
    )?;
    let mut snapshot = build_snapshot(
        &core_trace.base_snapshot,
        &direct_edges,
        &dynamic_spanner_buckets,
        input.branches,
        DynamicSparseCoreMetrics::default(),
        Vec::new(),
        false,
    )?;
    snapshot.metrics.embedding_checks = active_core_count(&snapshot)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { operations.len() + 1 } else { 0 });

    for core_event in core_trace.events.iter().take(operations.len()) {
        let before = snapshot.clone();
        let DynamicCoreGraphEventKind::Updated { core_updates, .. } = &core_event.kind else {
            return Err(DynamicSparseCoreError::InvariantViolation);
        };
        let next_dynamic_spanner_buckets =
            advance_dynamic_spanner_buckets(&dynamic_spanner_buckets, core_updates)?;
        let sparse_updates = derive_sparse_updates(
            input.branches,
            &snapshot,
            core_updates,
            &core_event.after,
            &mut direct_edges,
            &next_dynamic_spanner_buckets,
        )?;
        let reembedded = exact_reembedded_set(
            &snapshot,
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
        )?;
        let mut metrics = apply_metrics(snapshot.metrics, core_updates, &sparse_updates)?;
        metrics.source_updates = increment(metrics.source_updates)?;
        metrics.state_transitions = increment(metrics.state_transitions)?;
        snapshot = build_snapshot(
            &core_event.after,
            &direct_edges,
            &next_dynamic_spanner_buckets,
            input.branches,
            metrics,
            reembedded,
            false,
        )?;
        snapshot.metrics.embedding_checks = snapshot
            .metrics
            .embedding_checks
            .checked_add(active_core_count(&snapshot)?)
            .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?;
        if record {
            events.push(DynamicSparseCoreTraceEvent {
                catalog_id: CATALOG_ID,
                kind: DynamicSparseCoreEventKind::Updated {
                    core_event: Box::new(core_event.kind.clone()),
                    sparse_updates,
                },
                before,
                after: snapshot.clone(),
            });
        }
        dynamic_spanner_buckets = next_dynamic_spanner_buckets;
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicSparseCoreTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicSparseCoreEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalRun {
        core_trace,
        base_snapshot,
        events,
        result: DynamicSparseCoreResult {
            final_snapshot: snapshot,
        },
    })
}

fn validate_config(
    input: &DynamicSparseCoreInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<(), DynamicSparseCoreError> {
    if input.branches == 0
        || input.branches > DYNAMIC_SPARSE_CORE_MAX_BRANCHES
        || input.core.forest.maximum_node_count > DYNAMIC_SPARSE_CORE_MAX_NODES
        || input.core.forest.edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
        || operations.len() > DYNAMIC_SPARSE_CORE_MAX_OPERATIONS
    {
        return Err(DynamicSparseCoreError::InvalidInput);
    }
    Ok(())
}

fn build_snapshot(
    core: &DynamicCoreGraphSnapshot,
    direct_edges: &[bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
    branches: usize,
    metrics: DynamicSparseCoreMetrics,
    last_reembedded: Vec<usize>,
    complete: bool,
) -> Result<DynamicSparseCoreSnapshot, DynamicSparseCoreError> {
    let sparse = build_sparse(
        &core.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        branches,
    )?;
    let snapshot = DynamicSparseCoreSnapshot {
        core_vertices: core.core_vertices.clone(),
        core_edge_slots: core.edge_slots.clone(),
        spanner_edge_slots: sparse.spanner_edge_slots,
        core_to_spanner: sparse.embedding,
        sparsify_certificates: sparse.sparsify_certificates,
        dynamic_spanner_buckets: dynamic_spanner_buckets.to_vec(),
        direct_edges: direct_edges.to_vec(),
        gamma_length: sparse.gamma_length,
        gamma_congestion: sparse.gamma_congestion,
        last_reembedded,
        stage: core.stage,
        complete,
        metrics,
    };
    verify_snapshot(branches, &snapshot)?;
    Ok(snapshot)
}

fn build_sparse(
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
    branches: usize,
) -> Result<SparseBuild, DynamicSparseCoreError> {
    if direct_edges.len() != core_slots.len() {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let mut accumulator = SparseAccumulator {
        selected: BTreeSet::new(),
        embedding: vec![Vec::new(); core_slots.len()],
        sparsify_certificates: Vec::new(),
        scheduled_edges: BTreeSet::new(),
    };
    for bucket in dynamic_spanner_buckets {
        absorb_dynamic_bucket(&mut accumulator, bucket, core_slots, direct_edges)?;
    }
    add_direct_edges(&mut accumulator, core_slots, direct_edges);
    finish_sparse_build(core_slots, branches, accumulator)
}

fn absorb_dynamic_bucket(
    accumulator: &mut SparseAccumulator,
    bucket: &DynamicSparseCoreSpannerBucket,
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
) -> Result<(), DynamicSparseCoreError> {
    check_bounded_dynamic_spanner_trace(&bucket.trace)
        .map_err(|_| DynamicSparseCoreError::InvariantViolation)?;
    if bucket.stable_edges.len() != bucket.trace.initial.edge_rows.len()
        || !is_strictly_sorted_usize(&bucket.stable_edges)
        || bucket
            .stable_edges
            .iter()
            .any(|&edge| edge >= core_slots.len() || !accumulator.scheduled_edges.insert(edge))
    {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let state = &bucket.trace.final_snapshot;
    for &local_edge in &state.spanner_edges {
        let stable_edge = bucket.stable_edges[local_edge];
        if core_slots[stable_edge].is_some() && !direct_edges[stable_edge] {
            accumulator.selected.insert(stable_edge);
        }
    }
    for (local_edge, &stable_edge) in bucket.stable_edges.iter().enumerate() {
        absorb_bucket_embedding(
            accumulator,
            bucket,
            core_slots,
            direct_edges,
            local_edge,
            stable_edge,
        )?;
    }
    accumulator.sparsify_certificates.extend(
        state
            .levels
            .iter()
            .filter_map(|level| level.sparsify_certificate.clone()),
    );
    Ok(())
}

fn absorb_bucket_embedding(
    accumulator: &mut SparseAccumulator,
    bucket: &DynamicSparseCoreSpannerBucket,
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
    local_edge: usize,
    stable_edge: usize,
) -> Result<(), DynamicSparseCoreError> {
    let state = &bucket.trace.final_snapshot;
    match (&core_slots[stable_edge], state.edge_rows[local_edge].active) {
        (Some(row), true) if !direct_edges[stable_edge] => {
            if row.from != state.edge_rows[local_edge].from
                || row.to != state.edge_rows[local_edge].to
            {
                return Err(DynamicSparseCoreError::InvariantViolation);
            }
            accumulator.embedding[stable_edge] = state.graph_to_spanner[local_edge]
                .iter()
                .map(|arc| DynamicSparseCoreEmbeddingArc {
                    edge: bucket.stable_edges[arc.edge],
                    direction: arc.direction,
                })
                .collect();
        }
        (None, false) => {}
        (Some(_), _) if direct_edges[stable_edge] => {}
        _ => return Err(DynamicSparseCoreError::InvariantViolation),
    }
    Ok(())
}

fn add_direct_edges(
    accumulator: &mut SparseAccumulator,
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
) {
    for (edge, direct) in direct_edges.iter().enumerate() {
        if *direct
            && core_slots[edge]
                .as_ref()
                .is_some_and(|row| row.from != row.to)
        {
            accumulator.selected.insert(edge);
            accumulator.embedding[edge] =
                vec![DynamicSparseCoreEmbeddingArc { edge, direction: 1 }];
        }
    }
}

fn finish_sparse_build(
    core_slots: &[Option<DynamicCoreEdge>],
    branches: usize,
    accumulator: SparseAccumulator,
) -> Result<SparseBuild, DynamicSparseCoreError> {
    let mut spanner_edge_slots = vec![None; core_slots.len()];
    for edge in accumulator.selected {
        spanner_edge_slots[edge] = Some(
            core_slots[edge]
                .as_ref()
                .ok_or(DynamicSparseCoreError::InvariantViolation)?
                .clone(),
        );
    }
    let mut congestion = vec![0_usize; core_slots.len()];
    for path in &accumulator.embedding {
        for arc in path {
            congestion[arc.edge] = congestion[arc.edge]
                .checked_add(1)
                .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?;
        }
    }
    let maximum_congestion = congestion.into_iter().max().unwrap_or(0);
    let maximum_path_length = accumulator
        .embedding
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let core_count = core_slots.iter().flatten().count();
    let spanner_count = spanner_edge_slots.iter().flatten().count();
    let sparsity_gamma = if core_count == 0 {
        1
    } else {
        ceil_div(
            branches
                .checked_mul(spanner_count)
                .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?,
            core_count,
        )?
        .max(1)
    };
    let gamma_length = sparsity_gamma.max(maximum_path_length).max(1);
    let gamma_congestion = ceil_div(maximum_congestion, branches)?.max(1);
    Ok(SparseBuild {
        spanner_edge_slots,
        embedding: accumulator.embedding,
        sparsify_certificates: accumulator.sparsify_certificates,
        gamma_length,
        gamma_congestion,
    })
}

fn is_strictly_sorted_usize(values: &[usize]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn factor_two_length_buckets(
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
) -> Vec<Vec<usize>> {
    let mut edges = core_slots
        .iter()
        .flatten()
        .filter(|edge| edge.from != edge.to && !direct_edges[edge.edge])
        .map(|edge| (edge.length.clone(), edge.edge))
        .collect::<Vec<_>>();
    edges.sort();
    let mut buckets = Vec::new();
    let mut cursor = 0;
    while cursor < edges.len() {
        let upper = &edges[cursor].0 * BigInt::from(2_u8);
        let mut end = cursor + 1;
        while end < edges.len() && edges[end].0 <= upper {
            end += 1;
        }
        let mut bucket = edges[cursor..end]
            .iter()
            .map(|(_, edge)| *edge)
            .collect::<Vec<_>>();
        bucket.sort_unstable();
        buckets.push(bucket);
        cursor = end;
    }
    buckets
}

fn initialize_dynamic_spanner_buckets(
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
    vertex_count: usize,
) -> Result<Vec<DynamicSparseCoreSpannerBucket>, DynamicSparseCoreError> {
    factor_two_length_buckets(core_slots, direct_edges)
        .into_iter()
        .map(|stable_edges| {
            let rows = stable_edges
                .iter()
                .map(|&edge| {
                    let row = core_slots[edge]
                        .as_ref()
                        .ok_or(DynamicSparseCoreError::InvariantViolation)?;
                    Ok(BoundedDynamicSpannerEdgeState {
                        from: row.from,
                        to: row.to,
                        active: true,
                    })
                })
                .collect::<Result<Vec<_>, DynamicSparseCoreError>>()?;
            let trace = trace_bounded_dynamic_spanner(vertex_count, rows, Vec::new())
                .map_err(|_| DynamicSparseCoreError::InvariantViolation)?;
            Ok(DynamicSparseCoreSpannerBucket {
                stable_edges,
                trace,
            })
        })
        .collect()
}

fn advance_dynamic_spanner_buckets(
    buckets: &[DynamicSparseCoreSpannerBucket],
    core_updates: &[DynamicCoreUpdate],
) -> Result<Vec<DynamicSparseCoreSpannerBucket>, DynamicSparseCoreError> {
    buckets
        .iter()
        .map(|bucket| {
            let stable_to_local = bucket
                .stable_edges
                .iter()
                .enumerate()
                .map(|(local, &stable)| (stable, local))
                .collect::<BTreeMap<_, _>>();
            let mut updates = bucket.trace.updates.clone();
            let mut active = bucket.trace.final_snapshot.edge_rows.clone();
            for core_update in core_updates {
                match core_update {
                    DynamicCoreUpdate::EdgeDeleted { edge } => {
                        let Some(&local) = stable_to_local.get(&edge.edge) else {
                            continue;
                        };
                        if active[local].active {
                            updates.push(BoundedDynamicSpannerUpdate::Delete { edge: local });
                            active[local].active = false;
                        }
                    }
                    DynamicCoreUpdate::VertexSplit {
                        retained_vertex,
                        new_vertex,
                        new_side_incidences,
                        ..
                    } => {
                        let mut moved = Vec::new();
                        for incidence in new_side_incidences {
                            let Some(&local) = stable_to_local.get(&incidence.edge) else {
                                continue;
                            };
                            if !active[local].active {
                                continue;
                            }
                            let endpoint = match incidence.endpoint {
                                DynamicCoreIncidenceEndpoint::Tail => {
                                    BoundedDynamicSpannerEndpoint::Tail
                                }
                                DynamicCoreIncidenceEndpoint::Head => {
                                    BoundedDynamicSpannerEndpoint::Head
                                }
                            };
                            moved.push((local, endpoint));
                        }
                        moved.sort_unstable();
                        moved.dedup();
                        if !moved.is_empty() {
                            for &(local, endpoint) in &moved {
                                match endpoint {
                                    BoundedDynamicSpannerEndpoint::Tail => {
                                        if active[local].from != *retained_vertex {
                                            return Err(DynamicSparseCoreError::InvariantViolation);
                                        }
                                        active[local].from = *new_vertex;
                                    }
                                    BoundedDynamicSpannerEndpoint::Head => {
                                        if active[local].to != *retained_vertex {
                                            return Err(DynamicSparseCoreError::InvariantViolation);
                                        }
                                        active[local].to = *new_vertex;
                                    }
                                }
                            }
                            updates.push(BoundedDynamicSpannerUpdate::VertexSplit {
                                retained_vertex: *retained_vertex,
                                new_vertex: *new_vertex,
                                moved,
                            });
                        }
                    }
                    DynamicCoreUpdate::EdgeReinserted { after, .. } => {
                        let Some(&local) = stable_to_local.get(&after.edge) else {
                            continue;
                        };
                        if active[local].active {
                            updates.push(BoundedDynamicSpannerUpdate::Delete { edge: local });
                            active[local].active = false;
                        }
                    }
                    DynamicCoreUpdate::EdgeInserted { .. }
                    | DynamicCoreUpdate::GradientReplaced { .. }
                    | DynamicCoreUpdate::LengthReplaced { .. } => {}
                }
            }
            let trace = trace_bounded_dynamic_spanner(
                bucket.trace.initial.vertex_count,
                bucket.trace.initial.edge_rows.clone(),
                updates,
            )
            .map_err(|_| DynamicSparseCoreError::InvariantViolation)?;
            Ok(DynamicSparseCoreSpannerBucket {
                stable_edges: bucket.stable_edges.clone(),
                trace,
            })
        })
        .collect()
}

fn derive_sparse_updates(
    branches: usize,
    before: &DynamicSparseCoreSnapshot,
    core_updates: &[DynamicCoreUpdate],
    core_after: &DynamicCoreGraphSnapshot,
    direct_edges: &mut [bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
) -> Result<Vec<DynamicSparseCoreUpdate>, DynamicSparseCoreError> {
    let mut working = before.spanner_edge_slots.clone();
    let mut updates = Vec::new();
    for core_update in core_updates {
        apply_core_update_to_spanner(core_update, &mut working, direct_edges, &mut updates)?;
    }
    promote_unscheduled_nonloops(core_after, dynamic_spanner_buckets, direct_edges)?;
    refresh_sparsifier(
        branches,
        &core_after.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        &mut working,
        &mut updates,
    )?;
    let after = build_sparse(
        &core_after.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        branches,
    )?;
    if working != after.spanner_edge_slots {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let forced = reembedded_from_embeddings(
        &before.core_to_spanner,
        &after.embedding,
        &after.spanner_edge_slots,
    )
    .into_iter()
    .collect::<BTreeSet<_>>();
    updates.extend(
        forced
            .into_iter()
            .map(|edge| DynamicSparseCoreUpdate::ForcedReinsert { edge }),
    );
    Ok(updates)
}

fn audit_sparse_updates(
    branches: usize,
    before: &DynamicSparseCoreSnapshot,
    core_updates: &[DynamicCoreUpdate],
    core_after: &DynamicCoreGraphSnapshot,
    direct_edges: &mut [bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
) -> Result<Vec<DynamicSparseCoreUpdate>, DynamicSparseCoreError> {
    let mut rows = before.spanner_edge_slots.clone();
    let mut expected = Vec::new();
    for update in core_updates {
        audit_core_update_to_spanner(update, &mut rows, direct_edges, &mut expected)?;
    }
    promote_unscheduled_nonloops(core_after, dynamic_spanner_buckets, direct_edges)?;
    audit_refresh_sparsifier(
        branches,
        &core_after.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        &mut rows,
        &mut expected,
    )?;
    let after = build_sparse(
        &core_after.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        branches,
    )?;
    if rows != after.spanner_edge_slots {
        return Err(DynamicSparseCoreError::TraceVerification);
    }
    let forced = audit_reembedded_set(
        &before.core_to_spanner,
        &after.embedding,
        &after.spanner_edge_slots,
    )
    .into_iter()
    .collect::<BTreeSet<_>>();
    expected.extend(
        forced
            .into_iter()
            .map(|edge| DynamicSparseCoreUpdate::ForcedReinsert { edge }),
    );
    Ok(expected)
}

fn promote_unscheduled_nonloops(
    core: &DynamicCoreGraphSnapshot,
    buckets: &[DynamicSparseCoreSpannerBucket],
    direct_edges: &mut [bool],
) -> Result<(), DynamicSparseCoreError> {
    let scheduled = buckets
        .iter()
        .flat_map(|bucket| {
            bucket
                .stable_edges
                .iter()
                .enumerate()
                .filter(|(local, _)| bucket.trace.final_snapshot.edge_rows[*local].active)
                .map(|(_, &stable)| stable)
        })
        .collect::<BTreeSet<_>>();
    for edge in core.edge_slots.iter().flatten() {
        if edge.from != edge.to && !scheduled.contains(&edge.edge) {
            *direct_edges
                .get_mut(edge.edge)
                .ok_or(DynamicSparseCoreError::InvariantViolation)? = true;
        }
    }
    Ok(())
}

fn apply_core_update_to_spanner(
    core_update: &DynamicCoreUpdate,
    working: &mut [Option<DynamicCoreEdge>],
    direct_edges: &mut [bool],
    updates: &mut Vec<DynamicSparseCoreUpdate>,
) -> Result<(), DynamicSparseCoreError> {
    match core_update {
        DynamicCoreUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            ..
        } => {
            let moved = new_side_incidences
                .iter()
                .copied()
                .filter(|incidence| working[incidence.edge].is_some())
                .collect::<Vec<_>>();
            let update = make_spanner_split(*retained_vertex, *new_vertex, moved, working)?;
            apply_spanner_split(working, &update)?;
            updates.push(update);
        }
        DynamicCoreUpdate::EdgeInserted { edge } => {
            direct_edges[edge.edge] = true;
            if edge.from != edge.to {
                working[edge.edge] = Some(edge.clone());
                updates.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: edge.clone(),
                    reason: DynamicSparseCoreRefreshReason::DirectInsertion,
                });
            }
        }
        DynamicCoreUpdate::EdgeDeleted { edge } => {
            direct_edges[edge.edge] = false;
            if let Some(selected) = working[edge.edge].take() {
                if selected != *edge {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                updates.push(DynamicSparseCoreUpdate::EdgeDeleted {
                    edge: selected,
                    reason: DynamicSparseCoreRefreshReason::CoreDeletion,
                });
            }
        }
        DynamicCoreUpdate::EdgeReinserted { before, after } => {
            if before.edge != after.edge {
                return Err(DynamicSparseCoreError::InvariantViolation);
            }
            direct_edges[before.edge] = true;
            if let Some(selected) = &mut working[before.edge] {
                if selected != before {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                *selected = after.clone();
                updates.push(DynamicSparseCoreUpdate::EdgeReinserted {
                    before: before.clone(),
                    after: after.clone(),
                });
            } else if after.from != after.to {
                working[after.edge] = Some(after.clone());
                updates.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: after.clone(),
                    reason: DynamicSparseCoreRefreshReason::DirectInsertion,
                });
            }
        }
        DynamicCoreUpdate::GradientReplaced {
            edge,
            before,
            after,
        } => {
            if let Some(selected) = &mut working[*edge] {
                if selected.gradient != *before {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                selected.gradient = after.clone();
                updates.push(DynamicSparseCoreUpdate::GradientReplaced {
                    edge: *edge,
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }
        DynamicCoreUpdate::LengthReplaced {
            edge,
            before,
            after,
        } => {
            if let Some(selected) = &mut working[*edge] {
                if selected.length != *before {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                selected.length = after.clone();
                updates.push(DynamicSparseCoreUpdate::LengthReplaced {
                    edge: *edge,
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }
    }
    Ok(())
}

fn audit_core_update_to_spanner(
    core_update: &DynamicCoreUpdate,
    rows: &mut [Option<DynamicCoreEdge>],
    direct_edges: &mut [bool],
    expected: &mut Vec<DynamicSparseCoreUpdate>,
) -> Result<(), DynamicSparseCoreError> {
    match core_update {
        DynamicCoreUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            ..
        } => {
            let moved: Vec<_> = new_side_incidences
                .iter()
                .copied()
                .filter(|incidence| rows[incidence.edge].is_some())
                .collect();
            let split = audit_make_spanner_split(*retained_vertex, *new_vertex, moved, rows)?;
            audit_apply_spanner_split(rows, &split)?;
            expected.push(split);
        }
        DynamicCoreUpdate::EdgeInserted { edge } => {
            direct_edges[edge.edge] = true;
            if edge.from != edge.to {
                if rows[edge.edge].is_some() {
                    return Err(DynamicSparseCoreError::TraceVerification);
                }
                rows[edge.edge] = Some(edge.clone());
                expected.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: edge.clone(),
                    reason: DynamicSparseCoreRefreshReason::DirectInsertion,
                });
            }
        }
        DynamicCoreUpdate::EdgeDeleted { edge } => {
            direct_edges[edge.edge] = false;
            if let Some(selected) = rows[edge.edge].take() {
                if selected != *edge {
                    return Err(DynamicSparseCoreError::TraceVerification);
                }
                expected.push(DynamicSparseCoreUpdate::EdgeDeleted {
                    edge: selected,
                    reason: DynamicSparseCoreRefreshReason::CoreDeletion,
                });
            }
        }
        DynamicCoreUpdate::EdgeReinserted { before, after } => {
            if before.edge != after.edge {
                return Err(DynamicSparseCoreError::TraceVerification);
            }
            direct_edges[before.edge] = true;
            if let Some(selected) = &mut rows[before.edge] {
                if selected != before {
                    return Err(DynamicSparseCoreError::TraceVerification);
                }
                *selected = after.clone();
                expected.push(DynamicSparseCoreUpdate::EdgeReinserted {
                    before: before.clone(),
                    after: after.clone(),
                });
            } else if after.from != after.to {
                rows[after.edge] = Some(after.clone());
                expected.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: after.clone(),
                    reason: DynamicSparseCoreRefreshReason::DirectInsertion,
                });
            }
        }
        DynamicCoreUpdate::GradientReplaced {
            edge,
            before,
            after,
        } => {
            if let Some(row) = &mut rows[*edge] {
                if row.gradient != *before {
                    return Err(DynamicSparseCoreError::TraceVerification);
                }
                row.gradient = after.clone();
                expected.push(DynamicSparseCoreUpdate::GradientReplaced {
                    edge: *edge,
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }
        DynamicCoreUpdate::LengthReplaced {
            edge,
            before,
            after,
        } => {
            if let Some(selected) = &mut rows[*edge] {
                if selected.length != *before {
                    return Err(DynamicSparseCoreError::TraceVerification);
                }
                selected.length = after.clone();
                expected.push(DynamicSparseCoreUpdate::LengthReplaced {
                    edge: *edge,
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }
    }
    Ok(())
}

fn refresh_sparsifier(
    branches: usize,
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
    working: &mut [Option<DynamicCoreEdge>],
    updates: &mut Vec<DynamicSparseCoreUpdate>,
) -> Result<(), DynamicSparseCoreError> {
    let desired = build_sparse(core_slots, direct_edges, dynamic_spanner_buckets, branches)?
        .spanner_edge_slots;
    for edge in 0..working.len() {
        if working[edge].is_some() && desired[edge].is_none() {
            let row = working[edge]
                .take()
                .ok_or(DynamicSparseCoreError::InvariantViolation)?;
            updates.push(DynamicSparseCoreUpdate::EdgeDeleted {
                edge: row,
                reason: DynamicSparseCoreRefreshReason::SparsifyRefresh,
            });
        }
    }
    for edge in 0..working.len() {
        match (&working[edge], &desired[edge]) {
            (None, Some(row)) => {
                working[edge] = Some(row.clone());
                updates.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: row.clone(),
                    reason: DynamicSparseCoreRefreshReason::SparsifyRefresh,
                });
            }
            (Some(left), Some(right)) if left != right => {
                return Err(DynamicSparseCoreError::InvariantViolation);
            }
            _ => {}
        }
    }
    Ok(())
}

fn audit_refresh_sparsifier(
    branches: usize,
    core_slots: &[Option<DynamicCoreEdge>],
    direct_edges: &[bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
    rows: &mut [Option<DynamicCoreEdge>],
    expected: &mut Vec<DynamicSparseCoreUpdate>,
) -> Result<(), DynamicSparseCoreError> {
    let desired = build_sparse(core_slots, direct_edges, dynamic_spanner_buckets, branches)?
        .spanner_edge_slots;
    for edge in 0..rows.len() {
        if rows[edge].is_some() && desired[edge].is_none() {
            let row = rows[edge]
                .take()
                .ok_or(DynamicSparseCoreError::TraceVerification)?;
            expected.push(DynamicSparseCoreUpdate::EdgeDeleted {
                edge: row,
                reason: DynamicSparseCoreRefreshReason::SparsifyRefresh,
            });
        }
    }
    for edge in 0..rows.len() {
        match (&rows[edge], &desired[edge]) {
            (None, Some(row)) => {
                rows[edge] = Some(row.clone());
                expected.push(DynamicSparseCoreUpdate::EdgeInserted {
                    edge: row.clone(),
                    reason: DynamicSparseCoreRefreshReason::SparsifyRefresh,
                });
            }
            (Some(left), Some(right)) if left != right => {
                return Err(DynamicSparseCoreError::TraceVerification);
            }
            _ => {}
        }
    }
    Ok(())
}

fn make_spanner_split(
    retained_vertex: usize,
    new_vertex: usize,
    mut moved: Vec<DynamicCoreIncidence>,
    working: &[Option<DynamicCoreEdge>],
) -> Result<DynamicSparseCoreUpdate, DynamicSparseCoreError> {
    moved.sort_unstable();
    moved.dedup();
    let incident = spanner_incidences_at(working, retained_vertex);
    if moved.iter().any(|incidence| !incident.contains(incidence)) {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let retained: Vec<_> = incident
        .iter()
        .copied()
        .filter(|incidence| !moved.contains(incidence))
        .collect();
    let (encoded_side, encoded_incidences) = if moved.len() <= retained.len() {
        (DynamicCoreEncodedSide::New, moved.clone())
    } else {
        (DynamicCoreEncodedSide::Retained, retained)
    };
    Ok(DynamicSparseCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences: moved,
        encoded_side,
        encoded_incidences,
    })
}

fn audit_make_spanner_split(
    retained_vertex: usize,
    new_vertex: usize,
    mut moved: Vec<DynamicCoreIncidence>,
    rows: &[Option<DynamicCoreEdge>],
) -> Result<DynamicSparseCoreUpdate, DynamicSparseCoreError> {
    moved.sort_unstable();
    moved.dedup();
    let all = audit_spanner_incidences_at(rows, retained_vertex);
    if moved
        .iter()
        .any(|incidence| all.binary_search(incidence).is_err())
    {
        return Err(DynamicSparseCoreError::TraceVerification);
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
    Ok(DynamicSparseCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences: moved,
        encoded_side: side,
        encoded_incidences: encoding,
    })
}

fn apply_spanner_split(
    working: &mut [Option<DynamicCoreEdge>],
    update: &DynamicSparseCoreUpdate,
) -> Result<(), DynamicSparseCoreError> {
    let DynamicSparseCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences,
        ..
    } = update
    else {
        return Err(DynamicSparseCoreError::InvariantViolation);
    };
    for incidence in new_side_incidences {
        let row = working[incidence.edge]
            .as_mut()
            .ok_or(DynamicSparseCoreError::InvariantViolation)?;
        let endpoint = match incidence.endpoint {
            DynamicCoreIncidenceEndpoint::Tail => &mut row.from,
            DynamicCoreIncidenceEndpoint::Head => &mut row.to,
        };
        if *endpoint != *retained_vertex {
            return Err(DynamicSparseCoreError::InvariantViolation);
        }
        *endpoint = *new_vertex;
    }
    Ok(())
}

fn audit_apply_spanner_split(
    rows: &mut [Option<DynamicCoreEdge>],
    update: &DynamicSparseCoreUpdate,
) -> Result<(), DynamicSparseCoreError> {
    let DynamicSparseCoreUpdate::VertexSplit {
        retained_vertex,
        new_vertex,
        new_side_incidences,
        ..
    } = update
    else {
        return Err(DynamicSparseCoreError::TraceVerification);
    };
    for incidence in new_side_incidences {
        let row = rows[incidence.edge]
            .as_mut()
            .ok_or(DynamicSparseCoreError::TraceVerification)?;
        let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
            &mut row.from
        } else {
            &mut row.to
        };
        if *endpoint != *retained_vertex {
            return Err(DynamicSparseCoreError::TraceVerification);
        }
        *endpoint = *new_vertex;
    }
    Ok(())
}

fn exact_reembedded_set(
    before: &DynamicSparseCoreSnapshot,
    core_after: &DynamicCoreGraphSnapshot,
    direct_edges: &[bool],
    dynamic_spanner_buckets: &[DynamicSparseCoreSpannerBucket],
    branches: usize,
) -> Result<Vec<usize>, DynamicSparseCoreError> {
    let after = build_sparse(
        &core_after.edge_slots,
        direct_edges,
        dynamic_spanner_buckets,
        branches,
    )?;
    Ok(reembedded_from_embeddings(
        &before.core_to_spanner,
        &after.embedding,
        &after.spanner_edge_slots,
    ))
}

fn reembedded_from_embeddings(
    before: &[Vec<DynamicSparseCoreEmbeddingArc>],
    after: &[Vec<DynamicSparseCoreEmbeddingArc>],
    after_spanner: &[Option<DynamicCoreEdge>],
) -> Vec<usize> {
    let mut result = Vec::new();
    for (spanner_edge, row) in after_spanner.iter().enumerate() {
        if row.is_none() {
            continue;
        }
        let gained = after.iter().enumerate().any(|(core_edge, path)| {
            path.iter().any(|arc| arc.edge == spanner_edge)
                && before
                    .get(core_edge)
                    .is_none_or(|old| old.iter().all(|arc| arc.edge != spanner_edge))
        });
        if gained {
            result.push(spanner_edge);
        }
    }
    result
}

fn audit_reembedded_set(
    before: &[Vec<DynamicSparseCoreEmbeddingArc>],
    after: &[Vec<DynamicSparseCoreEmbeddingArc>],
    current_spanner: &[Option<DynamicCoreEdge>],
) -> Vec<usize> {
    let mut result = Vec::new();
    for (edge, row) in current_spanner.iter().enumerate() {
        if row.is_none() {
            continue;
        }
        let mut newly_used = false;
        for (core_edge, path) in after.iter().enumerate() {
            let now_uses = path.iter().any(|arc| arc.edge == edge);
            let previously_used = before
                .get(core_edge)
                .is_some_and(|path| path.iter().any(|arc| arc.edge == edge));
            newly_used |= now_uses && !previously_used;
        }
        if newly_used {
            result.push(edge);
        }
    }
    result
}

fn verify_snapshot(
    branches: usize,
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicSparseCoreError> {
    if snapshot.core_edge_slots.len() != snapshot.spanner_edge_slots.len()
        || snapshot.core_edge_slots.len() != snapshot.core_to_spanner.len()
        || snapshot.core_edge_slots.len() != snapshot.direct_edges.len()
        || snapshot.gamma_length == 0
        || snapshot.gamma_congestion == 0
        || snapshot
            .last_reembedded
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    verify_sparsify_reconstruction(branches, snapshot)?;
    let core_count = snapshot.core_edge_slots.iter().flatten().count();
    let spanner_count = snapshot.spanner_edge_slots.iter().flatten().count();
    if spanner_count
        .checked_mul(branches)
        .zip(core_count.checked_mul(snapshot.gamma_length))
        .is_none_or(|(left, right)| left > right)
    {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let mut congestion = vec![0_usize; snapshot.spanner_edge_slots.len()];
    for (edge_id, core_edge) in snapshot.core_edge_slots.iter().enumerate() {
        match core_edge {
            None if snapshot.core_to_spanner[edge_id].is_empty()
                && snapshot.spanner_edge_slots[edge_id].is_none()
                && !snapshot.direct_edges[edge_id] => {}
            Some(core_edge) => {
                if let Some(spanner_edge) = &snapshot.spanner_edge_slots[edge_id]
                    && spanner_edge != core_edge
                {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                let path = &snapshot.core_to_spanner[edge_id];
                if path.len() > snapshot.gamma_length {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
                let mut cursor = core_edge.from;
                for arc in path {
                    let row = snapshot
                        .spanner_edge_slots
                        .get(arc.edge)
                        .and_then(Option::as_ref)
                        .ok_or(DynamicSparseCoreError::InvariantViolation)?;
                    let next = match arc.direction {
                        1 if row.from == cursor => row.to,
                        -1 if row.to == cursor => row.from,
                        _ => return Err(DynamicSparseCoreError::InvariantViolation),
                    };
                    let shorter = core_edge.length.clone().min(row.length.clone());
                    let longer = core_edge.length.clone().max(row.length.clone());
                    if longer > shorter * BigInt::from(2_u8) {
                        return Err(DynamicSparseCoreError::InvariantViolation);
                    }
                    congestion[arc.edge] = congestion[arc.edge]
                        .checked_add(1)
                        .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?;
                    cursor = next;
                }
                if cursor != core_edge.to {
                    return Err(DynamicSparseCoreError::InvariantViolation);
                }
            }
            None => return Err(DynamicSparseCoreError::InvariantViolation),
        }
    }
    let bound = branches
        .checked_mul(snapshot.gamma_congestion)
        .ok_or(DynamicSparseCoreError::ArithmeticOverflow)?;
    if congestion.into_iter().any(|value| value > bound)
        || snapshot.last_reembedded.iter().any(|&edge| {
            snapshot
                .spanner_edge_slots
                .get(edge)
                .is_none_or(Option::is_none)
        })
    {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    Ok(())
}

fn verify_sparsify_reconstruction(
    branches: usize,
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicSparseCoreError> {
    let expected = build_sparse(
        &snapshot.core_edge_slots,
        &snapshot.direct_edges,
        &snapshot.dynamic_spanner_buckets,
        branches,
    )?;
    if snapshot.spanner_edge_slots != expected.spanner_edge_slots
        || snapshot.core_to_spanner != expected.embedding
        || snapshot.sparsify_certificates != expected.sparsify_certificates
        || snapshot.gamma_length != expected.gamma_length
        || snapshot.gamma_congestion != expected.gamma_congestion
    {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    let mut covered = BTreeSet::new();
    for bucket in &snapshot.dynamic_spanner_buckets {
        check_bounded_dynamic_spanner_trace(&bucket.trace)
            .map_err(|_| DynamicSparseCoreError::InvariantViolation)?;
        if bucket.stable_edges.len() != bucket.trace.initial.edge_rows.len()
            || !is_strictly_sorted_usize(&bucket.stable_edges)
            || bucket
                .stable_edges
                .iter()
                .any(|&edge| edge >= snapshot.core_edge_slots.len() || !covered.insert(edge))
        {
            return Err(DynamicSparseCoreError::InvariantViolation);
        }
        let mut active_lengths = bucket
            .stable_edges
            .iter()
            .enumerate()
            .filter_map(|(local, &stable)| {
                if bucket.trace.final_snapshot.edge_rows[local].active
                    && !snapshot.direct_edges[stable]
                {
                    snapshot.core_edge_slots[stable]
                        .as_ref()
                        .map(|row| row.length.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        active_lengths.sort();
        if let (Some(shortest), Some(longest)) = (active_lengths.first(), active_lengths.last())
            && longest > &(shortest * BigInt::from(2_u8))
        {
            return Err(DynamicSparseCoreError::InvariantViolation);
        }
    }
    Ok(())
}

fn spanner_incidences_at(
    rows: &[Option<DynamicCoreEdge>],
    vertex: usize,
) -> Vec<DynamicCoreIncidence> {
    let mut result = Vec::new();
    for row in rows.iter().flatten() {
        if row.from == vertex {
            result.push(DynamicCoreIncidence {
                edge: row.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        }
        if row.to == vertex {
            result.push(DynamicCoreIncidence {
                edge: row.edge,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn audit_spanner_incidences_at(
    rows: &[Option<DynamicCoreEdge>],
    vertex: usize,
) -> Vec<DynamicCoreIncidence> {
    let mut result = Vec::new();
    for (edge, row) in rows.iter().enumerate() {
        let Some(row) = row else { continue };
        if row.from == vertex {
            result.push(DynamicCoreIncidence {
                edge,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            });
        }
        if row.to == vertex {
            result.push(DynamicCoreIncidence {
                edge,
                endpoint: DynamicCoreIncidenceEndpoint::Head,
            });
        }
    }
    result.sort_unstable();
    result
}

fn apply_metrics(
    before: DynamicSparseCoreMetrics,
    core_updates: &[DynamicCoreUpdate],
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicSparseCoreMetrics, DynamicSparseCoreError> {
    let mut metrics = before;
    metrics.core_updates = add_usize(metrics.core_updates, core_updates.len())?;
    for update in sparse_updates {
        match update {
            DynamicSparseCoreUpdate::VertexSplit {
                new_side_incidences,
                ..
            } => {
                metrics.vertex_splits = increment(metrics.vertex_splits)?;
                metrics.endpoint_moves =
                    add_usize(metrics.endpoint_moves, new_side_incidences.len())?;
            }
            DynamicSparseCoreUpdate::EdgeInserted { reason, .. } => {
                metrics.edge_insertions = increment(metrics.edge_insertions)?;
                if *reason == DynamicSparseCoreRefreshReason::SparsifyRefresh {
                    metrics.sparsify_refreshes = increment(metrics.sparsify_refreshes)?;
                }
            }
            DynamicSparseCoreUpdate::EdgeDeleted { reason, .. } => {
                metrics.edge_deletions = increment(metrics.edge_deletions)?;
                if *reason == DynamicSparseCoreRefreshReason::SparsifyRefresh {
                    metrics.sparsify_refreshes = increment(metrics.sparsify_refreshes)?;
                }
            }
            DynamicSparseCoreUpdate::EdgeReinserted { .. } => {
                metrics.edge_reinsertions = increment(metrics.edge_reinsertions)?;
            }
            DynamicSparseCoreUpdate::GradientReplaced { .. } => {
                metrics.gradient_replacements = increment(metrics.gradient_replacements)?;
            }
            DynamicSparseCoreUpdate::LengthReplaced { .. } => {
                metrics.length_replacements = increment(metrics.length_replacements)?;
            }
            DynamicSparseCoreUpdate::ForcedReinsert { .. } => {
                metrics.forced_reinsertions = increment(metrics.forced_reinsertions)?;
            }
        }
    }
    Ok(metrics)
}

fn audit_metrics(
    before: DynamicSparseCoreMetrics,
    core_updates: &[DynamicCoreUpdate],
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicSparseCoreMetrics, DynamicSparseCoreError> {
    let mut metrics = before;
    metrics.core_updates = audit_add_usize(metrics.core_updates, core_updates.len())?;
    for update in sparse_updates {
        match update {
            DynamicSparseCoreUpdate::VertexSplit {
                new_side_incidences,
                ..
            } => {
                metrics.vertex_splits = audit_increment(metrics.vertex_splits)?;
                metrics.endpoint_moves =
                    audit_add_usize(metrics.endpoint_moves, new_side_incidences.len())?;
            }
            DynamicSparseCoreUpdate::EdgeInserted { reason, .. } => {
                metrics.edge_insertions = audit_increment(metrics.edge_insertions)?;
                if *reason == DynamicSparseCoreRefreshReason::SparsifyRefresh {
                    metrics.sparsify_refreshes = audit_increment(metrics.sparsify_refreshes)?;
                }
            }
            DynamicSparseCoreUpdate::EdgeDeleted { reason, .. } => {
                metrics.edge_deletions = audit_increment(metrics.edge_deletions)?;
                if *reason == DynamicSparseCoreRefreshReason::SparsifyRefresh {
                    metrics.sparsify_refreshes = audit_increment(metrics.sparsify_refreshes)?;
                }
            }
            DynamicSparseCoreUpdate::EdgeReinserted { .. } => {
                metrics.edge_reinsertions = audit_increment(metrics.edge_reinsertions)?;
            }
            DynamicSparseCoreUpdate::GradientReplaced { .. } => {
                metrics.gradient_replacements = audit_increment(metrics.gradient_replacements)?;
            }
            DynamicSparseCoreUpdate::LengthReplaced { .. } => {
                metrics.length_replacements = audit_increment(metrics.length_replacements)?;
            }
            DynamicSparseCoreUpdate::ForcedReinsert { .. } => {
                metrics.forced_reinsertions = audit_increment(metrics.forced_reinsertions)?;
            }
        }
    }
    Ok(metrics)
}

fn audit_completion(
    snapshot: &DynamicSparseCoreSnapshot,
    trace: &DynamicSparseCoreTraceResult,
) -> Result<(), DynamicSparseCoreError> {
    let completion = trace
        .events
        .last()
        .ok_or(DynamicSparseCoreError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.complete = true;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != DynamicSparseCoreEventKind::Completed
        || completion.before != *snapshot
        || completion.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicSparseCoreError::TraceVerification);
    }
    Ok(())
}

fn active_core_count(snapshot: &DynamicSparseCoreSnapshot) -> Result<u64, DynamicSparseCoreError> {
    u64::try_from(snapshot.core_edge_slots.iter().flatten().count())
        .map_err(|_| DynamicSparseCoreError::ArithmeticOverflow)
}

fn ceil_div(numerator: usize, denominator: usize) -> Result<usize, DynamicSparseCoreError> {
    if denominator == 0 {
        return Err(DynamicSparseCoreError::InvariantViolation);
    }
    numerator
        .checked_add(denominator - 1)
        .and_then(|value| value.checked_div(denominator))
        .ok_or(DynamicSparseCoreError::ArithmeticOverflow)
}

fn increment(value: u64) -> Result<u64, DynamicSparseCoreError> {
    value
        .checked_add(1)
        .ok_or(DynamicSparseCoreError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicSparseCoreError> {
    value
        .checked_add(1)
        .ok_or(DynamicSparseCoreError::TraceVerification)
}

fn add_usize(value: u64, additional: usize) -> Result<u64, DynamicSparseCoreError> {
    value
        .checked_add(
            u64::try_from(additional).map_err(|_| DynamicSparseCoreError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicSparseCoreError::ArithmeticOverflow)
}

fn audit_add_usize(value: u64, additional: usize) -> Result<u64, DynamicSparseCoreError> {
    value
        .checked_add(
            u64::try_from(additional).map_err(|_| DynamicSparseCoreError::TraceVerification)?,
        )
        .ok_or(DynamicSparseCoreError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_rational::BigRational;

    use super::*;
    use crate::{
        DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate, DynamicLowStretchForestEdge,
        DynamicLowStretchForestInput,
    };

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

    fn input() -> DynamicSparseCoreInput {
        DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
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
            },
            branches: 2,
        }
    }

    fn parallel_input() -> DynamicSparseCoreInput {
        let mut parallel = input();
        parallel.core.forest.initial_root_seeds = vec![0, 1, 2, 3];
        parallel.core.forest.edge_slots[5] = Some(edge(5, 3, 2, 3));
        parallel
    }

    #[test]
    fn initially_contracted_self_loops_have_empty_embeddings() {
        let snapshot = execute_dynamic_sparse_core(&input(), &[])
            .expect("sparse")
            .final_snapshot;
        assert!(snapshot.spanner_edge_slots.iter().all(Option::is_none));
        assert!(snapshot.core_to_spanner.iter().all(Vec::is_empty));
        assert_eq!(snapshot.gamma_length, 1);
        assert_eq!(snapshot.gamma_congestion, 1);
    }

    #[test]
    fn parallel_reverse_edges_use_a_certified_source_embedding() {
        let snapshot = execute_dynamic_sparse_core(&parallel_input(), &[])
            .expect("sparse")
            .final_snapshot;
        assert!(snapshot.spanner_edge_slots[3].is_some());
        assert!(snapshot.spanner_edge_slots[5].is_none());
        assert_eq!(
            snapshot.core_to_spanner[5],
            vec![DynamicSparseCoreEmbeddingArc {
                edge: 3,
                direction: -1,
            }]
        );
        verify_snapshot(2, &snapshot).expect("definition 5.7 witness");
    }

    #[test]
    fn deleting_a_selected_edge_refreshes_the_source_sparsifier() {
        let operations = vec![DynamicCoreGraphOperation::Delete { edge: 3 }];
        let trace = trace_dynamic_sparse_core(&parallel_input(), &operations).expect("trace");
        let after = &trace.events[0].after;
        let bucket_index = after
            .dynamic_spanner_buckets
            .iter()
            .position(|bucket| bucket.stable_edges.contains(&3))
            .expect("deleted edge bucket");
        let dynamic = &after.dynamic_spanner_buckets[bucket_index].trace;
        assert_eq!(dynamic.updates.len(), 1);
        assert!(dynamic.final_snapshot.last_projection.is_some());
        check_bounded_dynamic_spanner_trace(dynamic).expect("dynamic source schedule");
        assert!(after.spanner_edge_slots[3].is_none());
        assert!(after.spanner_edge_slots[5].is_some());
        assert_eq!(after.last_reembedded, vec![5]);
        assert_eq!(
            after.core_to_spanner[5],
            vec![DynamicSparseCoreEmbeddingArc {
                edge: 5,
                direction: 1,
            }]
        );
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("update");
        };
        assert!(sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::EdgeInserted {
                edge,
                reason: DynamicSparseCoreRefreshReason::SparsifyRefresh
            } if edge.edge == 5
        )));
        assert!(matches!(
            sparse_updates.last(),
            Some(DynamicSparseCoreUpdate::ForcedReinsert { edge: 5 })
        ));
        check_dynamic_sparse_core_trace(&parallel_input(), &operations, &trace).expect("check");

        let mut forged = trace.clone();
        forged.events[0].after.dynamic_spanner_buckets[bucket_index]
            .trace
            .snapshots[0]
            .last_reembedded
            .clear();
        assert_eq!(
            check_dynamic_sparse_core_trace(&parallel_input(), &operations, &forged),
            Err(DynamicSparseCoreError::TraceVerification)
        );
    }

    #[test]
    fn direct_insertion_is_retained_and_reported_as_reembedded() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        let after = &trace.events[0].after;
        assert!(after.direct_edges[4]);
        assert!(after.spanner_edge_slots[4].is_some());
        assert_eq!(after.last_reembedded, vec![0, 4]);
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("update");
        };
        assert!(sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::EdgeInserted {
                edge,
                reason: DynamicSparseCoreRefreshReason::DirectInsertion
            } if edge.edge == 4
        )));
        assert!(matches!(
            sparse_updates.last(),
            Some(DynamicSparseCoreUpdate::ForcedReinsert { edge: 4 })
        ));
    }

    #[test]
    fn selected_edge_reinsertion_updates_attributes_without_topology_churn() {
        let operations = vec![DynamicCoreGraphOperation::Reinsert { edge: 3 }];
        let trace = trace_dynamic_sparse_core(&parallel_input(), &operations).expect("trace");
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("update");
        };
        let DynamicSparseCoreUpdate::EdgeReinserted { before, after } = sparse_updates
            .iter()
            .find(|update| matches!(update, DynamicSparseCoreUpdate::EdgeReinserted { .. }))
            .expect("reinsertion")
        else {
            panic!("reinsertion");
        };
        assert_eq!(before.edge, 3);
        assert_eq!(after.edge, 3);
        assert_eq!(after.length, rational(2));
        assert!(before.length >= after.length);
        assert!(trace.result.final_snapshot.direct_edges[3]);
        assert!(trace.result.final_snapshot.spanner_edge_slots[3].is_some());
        assert!(!sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::EdgeDeleted { edge, .. } if edge.edge == 3
        )));
        assert_eq!(trace.result.final_snapshot.metrics.edge_reinsertions, 1);
        check_dynamic_sparse_core_trace(&parallel_input(), &operations, &trace).expect("check");
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
    fn atomic_sparse_stage_refreshes_once_after_insert_then_reinsert() {
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
        let fast = execute_dynamic_sparse_core_stages(&input(), &batches).expect("fast");
        let trace = trace_dynamic_sparse_core_stages(&input(), &batches).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 2);
        assert_eq!(fast.final_snapshot.stage, 1);
        assert!(fast.final_snapshot.direct_edges[4]);
        let DynamicSparseCoreStageEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        let insert = sparse_updates
            .iter()
            .position(|update| matches!(update, DynamicSparseCoreUpdate::EdgeInserted { edge, .. } if edge.edge == 4))
            .expect("insert");
        let reinsert = sparse_updates
            .iter()
            .position(|update| matches!(update, DynamicSparseCoreUpdate::EdgeReinserted { after, .. } if after.edge == 4))
            .expect("reinsert");
        assert!(insert < reinsert);
        check_dynamic_sparse_core_stage_trace(&input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_sparse_stage_propagates_selected_length_replacement() {
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: stage_edge(3, 2, 3, 2, 7),
                after: stage_edge(3, 2, 3, 4, 7),
            }],
        }];
        let trace = trace_dynamic_sparse_core_stages(&parallel_input(), &batches).expect("trace");
        let DynamicSparseCoreStageEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        assert!(sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::LengthReplaced { edge: 3, before, after }
                if before < after
        )));
        assert_eq!(trace.result.final_snapshot.metrics.length_replacements, 1);
        check_dynamic_sparse_core_stage_trace(&parallel_input(), &batches, &trace).expect("check");
    }

    #[test]
    fn atomic_sparse_checker_rejects_batch_and_component_tampering() {
        let batches = vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: stage_edge(3, 2, 3, 2, 7),
                after: stage_edge(3, 2, 3, 4, 7),
            }],
        }];
        let trace = trace_dynamic_sparse_core_stages(&parallel_input(), &batches).expect("trace");
        let mut tampered = trace.clone();
        let DynamicSparseCoreStageEventKind::Updated { sparse_updates, .. } =
            &mut tampered.events[0].kind
        else {
            panic!("stage");
        };
        sparse_updates.clear();
        assert_eq!(
            check_dynamic_sparse_core_stage_trace(&parallel_input(), &batches, &tampered),
            Err(DynamicSparseCoreError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.core_trace.events[0].after.edge_slots[3]
            .as_mut()
            .expect("edge")
            .length += rational(1);
        assert!(matches!(
            check_dynamic_sparse_core_stage_trace(&parallel_input(), &batches, &tampered),
            Err(DynamicSparseCoreError::Core(
                DynamicCoreGraphError::TraceVerification
            ))
        ));
    }

    #[test]
    fn deletion_forwards_vertex_splits_and_removes_selected_core_edge() {
        let operations = vec![
            DynamicCoreGraphOperation::Insert {
                edge: edge(4, 2, 0, 3),
                gradient: rational(-4),
            },
            DynamicCoreGraphOperation::Delete { edge: 4 },
        ];
        let trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &trace.events[1].kind
        else {
            panic!("update");
        };
        assert!(sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::EdgeDeleted {
                edge,
                reason: DynamicSparseCoreRefreshReason::CoreDeletion
            } if edge.edge == 4
        )));
        assert!(!trace.result.final_snapshot.direct_edges[4]);
        assert!(trace.result.final_snapshot.spanner_edge_slots[4].is_none());
    }

    #[test]
    fn vertex_split_forwards_selected_incidences_and_gradient_change() {
        let operations = vec![DynamicCoreGraphOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![5],
        }];
        let trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &trace.events[0].kind
        else {
            panic!("update");
        };
        assert!(sparse_updates.iter().any(|update| matches!(
            update,
            DynamicSparseCoreUpdate::VertexSplit { new_vertex: 4, .. }
        )));
        let selected = trace.result.final_snapshot.spanner_edge_slots[5]
            .as_ref()
            .expect("selected edge");
        assert_eq!((selected.from, selected.to), (4, 1));
        assert_eq!(selected.gradient, rational(11));
    }

    #[test]
    fn vertex_split_propagates_a_moved_tree_source_row_with_static_support() {
        let operations = vec![DynamicCoreGraphOperation::VertexSplit {
            vertex: 1,
            new_vertex: 4,
            moved_edges: vec![1],
        }];
        let trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        let forest = &trace.core_trace.forest_trace.result.final_snapshot;
        assert_eq!(forest.edge_slots[1].as_ref().expect("dynamic").from, 4);
        assert_eq!(
            forest.reference_tree_support[1].as_ref().expect("support"),
            &edge(1, 1, 2, 1)
        );
        assert!(matches!(
            &trace.events[0].kind,
            DynamicSparseCoreEventKind::Updated { .. }
        ));
        check_dynamic_sparse_core_trace(&input(), &operations, &trace).expect("check");
    }

    #[test]
    fn fast_trace_checker_match_and_reject_reembedded_tampering() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let fast = execute_dynamic_sparse_core(&input(), &operations).expect("fast");
        let mut trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        assert_eq!(fast, trace.result);
        check_dynamic_sparse_core_trace(&input(), &operations, &trace).expect("check");
        trace.events[0].after.last_reembedded.clear();
        assert_eq!(
            check_dynamic_sparse_core_trace(&input(), &operations, &trace),
            Err(DynamicSparseCoreError::TraceVerification)
        );
    }

    #[test]
    fn checker_rejects_embedding_metric_and_component_tampering() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let mut trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        trace.events[0].after.core_to_spanner[4][0].direction = -1;
        assert_eq!(
            check_dynamic_sparse_core_trace(&input(), &operations, &trace),
            Err(DynamicSparseCoreError::TraceVerification)
        );

        let mut trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        trace.events[0].after.metrics.forced_reinsertions = 0;
        assert_eq!(
            check_dynamic_sparse_core_trace(&input(), &operations, &trace),
            Err(DynamicSparseCoreError::TraceVerification)
        );

        let certificate_operations = vec![DynamicCoreGraphOperation::Delete { edge: 3 }];
        let mut trace = trace_dynamic_sparse_core(&parallel_input(), &certificate_operations)
            .expect("certificate trace");
        trace.events[0].after.sparsify_certificates[0].phi += rational(1);
        assert_eq!(
            check_dynamic_sparse_core_trace(&parallel_input(), &certificate_operations, &trace),
            Err(DynamicSparseCoreError::TraceVerification)
        );

        let mut trace = trace_dynamic_sparse_core(&input(), &operations).expect("trace");
        trace.core_trace.events[0].after.core_vertices.pop();
        assert!(matches!(
            check_dynamic_sparse_core_trace(&input(), &operations, &trace),
            Err(DynamicSparseCoreError::Core(
                DynamicCoreGraphError::TraceVerification
            ))
        ));
    }

    #[test]
    fn invalid_branch_band_fails_closed() {
        let mut zero = input();
        zero.branches = 0;
        assert_eq!(
            execute_dynamic_sparse_core(&zero, &[]),
            Err(DynamicSparseCoreError::InvalidInput)
        );
        let mut large = input();
        large.branches = DYNAMIC_SPARSE_CORE_MAX_BRANCHES + 1;
        assert_eq!(
            execute_dynamic_sparse_core(&large, &[]),
            Err(DynamicSparseCoreError::InvalidInput)
        );
    }
}
