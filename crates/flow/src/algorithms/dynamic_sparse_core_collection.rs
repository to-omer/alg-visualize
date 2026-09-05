//! Exact bounded collection of dynamic sparsified-core branches.
//!
//! The Dynamic Sparse Core lemma maintains one low-stretch forest/core/spanner
//! instance for every branch `j` and applies the same source update batch to
//! all of them. This module makes that collection boundary explicit: branch
//! reference trees and root seeds may differ, while the source graph, stable
//! slots, gradients, reduction parameter, and operation sequence are shared.
//!
//! Every branch is checked by the independent dynamic sparse-core checker.
//! The collection checker then reconstructs operation alignment, ordered
//! branch batches, re-embedded sets, metrics, and completion without invoking
//! the collection runner. This is the per-level collection primitive needed
//! before active-branch updates can be propagated through every tree-chain
//! level. It does not claim the paper's expander-spanner or recourse bounds.

use thiserror::Error;

use super::{
    DYNAMIC_SPARSE_CORE_MAX_BRANCHES, DYNAMIC_SPARSE_CORE_MAX_OPERATIONS,
    DynamicCoreGraphEventKind, DynamicCoreGraphOperation, DynamicCoreGraphStageBatch,
    DynamicCoreGraphStageEventKind, DynamicSparseCoreError, DynamicSparseCoreEventKind,
    DynamicSparseCoreInput, DynamicSparseCoreSnapshot, DynamicSparseCoreStageEventKind,
    DynamicSparseCoreStageTraceResult, DynamicSparseCoreTraceResult, DynamicSparseCoreUpdate,
    check_dynamic_sparse_core_stage_trace, check_dynamic_sparse_core_trace,
    execute_dynamic_sparse_core, execute_dynamic_sparse_core_stages, trace_dynamic_sparse_core,
    trace_dynamic_sparse_core_stages,
};

/// Maximum maintained sparse-core branches at one tree-chain level.
pub const DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES: usize = DYNAMIC_SPARSE_CORE_MAX_BRANCHES;
/// Maximum shared source operations.
pub const DYNAMIC_SPARSE_CORE_COLLECTION_MAX_OPERATIONS: usize = DYNAMIC_SPARSE_CORE_MAX_OPERATIONS;
/// Maximum public collection boundaries, including completion.
pub const DYNAMIC_SPARSE_CORE_COLLECTION_MAX_TRACE_EVENTS: usize =
    DYNAMIC_SPARSE_CORE_COLLECTION_MAX_OPERATIONS + 1;

const CATALOG_ID: &str = "dynamic-sparsified-core-collection";

/// Branch-specific LSF/core inputs sharing one source graph and update stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionInput {
    /// Nonempty branch list in canonical source order `j=0..k-1`.
    pub branches: Vec<DynamicSparseCoreInput>,
}

/// Exact collection-level work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionMetrics {
    /// Shared source stages completed.
    pub source_stages: u64,
    /// Branch applications, exactly `branches * source_stages`.
    pub branch_applications: u64,
    /// Ordered sparse-core update records emitted by all branches.
    pub sparse_updates: u64,
    /// Forced reinsertion records emitted by all branches.
    pub reembedded_edges: u64,
    /// Public reversible collection transitions.
    pub state_transitions: u64,
}

/// Complete state of all sparse-core branches at one shared source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionSnapshot {
    /// Exact branch snapshots in canonical branch order.
    pub branch_snapshots: Vec<DynamicSparseCoreSnapshot>,
    /// Completed shared source stages.
    pub stage: u64,
    /// Whether collection completion was emitted.
    pub complete: bool,
    /// Exact collection-level work counters.
    pub metrics: DynamicSparseCoreCollectionMetrics,
}

/// Meaning of one collection boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreCollectionEventKind {
    /// The same source operation was applied to every branch.
    Updated {
        /// Shared source operation.
        operation: Box<DynamicCoreGraphOperation>,
        /// Ordered sparse-core update batch for each branch.
        branch_updates: Vec<Vec<DynamicSparseCoreUpdate>>,
        /// Exact branch-local re-embedded sets after this source stage.
        reembedded: Vec<Vec<usize>>,
    },
    /// Every supplied source operation completed on every branch.
    Completed,
}

/// One reversible collection transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source-level collection meaning.
    pub kind: DynamicSparseCoreCollectionEventKind,
    /// State before the boundary.
    pub before: DynamicSparseCoreCollectionSnapshot,
    /// State after the boundary.
    pub after: DynamicSparseCoreCollectionSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionResult {
    /// Terminal checked branch collection.
    pub final_snapshot: DynamicSparseCoreCollectionSnapshot,
}

/// Complete component and collection transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionTraceResult {
    /// Independently checkable branch transcripts.
    pub branch_traces: Vec<DynamicSparseCoreTraceResult>,
    /// Initial shared collection state.
    pub base_snapshot: DynamicSparseCoreCollectionSnapshot,
    /// One event per source stage, followed by completion.
    pub events: Vec<DynamicSparseCoreCollectionTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicSparseCoreCollectionResult,
}

/// Meaning of one atomic collection stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicSparseCoreCollectionStageEventKind {
    /// The same ordered source batch was applied atomically to every branch.
    Updated {
        /// Shared atomic source batch.
        batch: DynamicCoreGraphStageBatch,
        /// Ordered sparse-core update batch for each branch.
        branch_updates: Vec<Vec<DynamicSparseCoreUpdate>>,
        /// Exact branch-local re-embedded sets after this outer stage.
        reembedded: Vec<Vec<usize>>,
    },
    /// Every supplied outer source stage completed on every branch.
    Completed,
}

/// One reversible atomic collection transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionStageTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Atomic collection-stage meaning.
    pub kind: DynamicSparseCoreCollectionStageEventKind,
    /// State before the whole source batch.
    pub before: DynamicSparseCoreCollectionSnapshot,
    /// State after the whole source batch.
    pub after: DynamicSparseCoreCollectionSnapshot,
}

/// Complete atomic component and collection transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicSparseCoreCollectionStageTraceResult {
    /// Independently checkable atomic branch transcripts.
    pub branch_traces: Vec<DynamicSparseCoreStageTraceResult>,
    /// Initial shared collection state.
    pub base_snapshot: DynamicSparseCoreCollectionSnapshot,
    /// One event per outer source stage, followed by completion.
    pub events: Vec<DynamicSparseCoreCollectionStageTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicSparseCoreCollectionResult,
}

/// Explicit bounded collection failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicSparseCoreCollectionError {
    /// Branches do not share one source graph/reduction contract.
    #[error("dynamic sparse-core collection input is invalid")]
    InvalidInput,
    /// The collection exceeds its explicit branch or operation band.
    #[error("dynamic sparse-core collection exceeds its admission band")]
    AdmissionLimit,
    /// One checked sparse-core branch failed.
    #[error("dynamic sparse-core collection branch {branch} failed: {error}")]
    Branch {
        /// Canonical branch index.
        branch: usize,
        /// Component failure.
        #[source]
        error: DynamicSparseCoreError,
    },
    /// Checked metric arithmetic overflowed.
    #[error("dynamic sparse-core collection arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied collection transcript is not the exact component replay.
    #[error("dynamic sparse-core collection trace verification failed")]
    TraceVerification,
}

/// Executes every bounded sparse-core branch without retaining collection events.
///
/// # Errors
///
/// Rejects mismatched branches, out-of-band requests, component failure, or
/// checked metric overflow.
pub fn execute_dynamic_sparse_core_collection(
    input: &DynamicSparseCoreCollectionInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicSparseCoreCollectionResult, DynamicSparseCoreCollectionError> {
    validate_input(input, operations)?;
    let mut branch_snapshots = Vec::with_capacity(input.branches.len());
    for (branch, branch_input) in input.branches.iter().enumerate() {
        let result = execute_dynamic_sparse_core(branch_input, operations)
            .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?;
        branch_snapshots.push(result.final_snapshot);
    }
    let metrics = aggregate_final_metrics(&branch_snapshots, operations.len())?;
    Ok(DynamicSparseCoreCollectionResult {
        final_snapshot: DynamicSparseCoreCollectionSnapshot {
            branch_snapshots,
            stage: to_u64(operations.len())?,
            complete: true,
            metrics,
        },
    })
}

/// Records the source-aligned update batch from every sparse-core branch.
///
/// # Errors
///
/// Returns any input, component, arithmetic, or independent replay failure.
pub fn trace_dynamic_sparse_core_collection(
    input: &DynamicSparseCoreCollectionInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<DynamicSparseCoreCollectionTraceResult, DynamicSparseCoreCollectionError> {
    validate_input(input, operations)?;
    let mut branch_traces = Vec::with_capacity(input.branches.len());
    for (branch, branch_input) in input.branches.iter().enumerate() {
        branch_traces.push(
            trace_dynamic_sparse_core(branch_input, operations)
                .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?,
        );
    }
    let trace = assemble_trace(operations, branch_traces)?;
    check_dynamic_sparse_core_collection_trace(input, operations, &trace)?;
    Ok(trace)
}

/// Independently verifies all component traces and their collection alignment.
///
/// This checker never invokes either collection execution path. It delegates
/// only each branch's mathematical transcript to that branch's independent
/// checker, then reconstructs the collection state and counters itself.
///
/// # Errors
///
/// Rejects component drift, operation misalignment, reordered branch batches,
/// nonexact re-embedded sets, metric drift, or a forged completion boundary.
pub fn check_dynamic_sparse_core_collection_trace(
    input: &DynamicSparseCoreCollectionInput,
    operations: &[DynamicCoreGraphOperation],
    trace: &DynamicSparseCoreCollectionTraceResult,
) -> Result<(), DynamicSparseCoreCollectionError> {
    validate_input(input, operations)?;
    if trace.branch_traces.len() != input.branches.len()
        || trace.events.len()
            != operations
                .len()
                .checked_add(1)
                .ok_or(DynamicSparseCoreCollectionError::TraceVerification)?
    {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }
    for (branch, (branch_input, branch_trace)) in
        input.branches.iter().zip(&trace.branch_traces).enumerate()
    {
        check_dynamic_sparse_core_trace(branch_input, operations, branch_trace)
            .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?;
    }

    let mut snapshot = DynamicSparseCoreCollectionSnapshot {
        branch_snapshots: trace
            .branch_traces
            .iter()
            .map(|branch| branch.base_snapshot.clone())
            .collect(),
        stage: 0,
        complete: false,
        metrics: DynamicSparseCoreCollectionMetrics::default(),
    };
    if trace.base_snapshot != snapshot {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }

    for (index, operation) in operations.iter().enumerate() {
        let event = &trace.events[index];
        let (branch_updates, reembedded, branch_snapshots) =
            audit_component_stage(&trace.branch_traces, index, operation)?;
        let mut after = snapshot.clone();
        after.branch_snapshots = branch_snapshots;
        after.stage = audit_increment(after.stage)?;
        audit_add_stage_metrics(&mut after.metrics, &branch_updates, &reembedded)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        let expected_kind = DynamicSparseCoreCollectionEventKind::Updated {
            operation: Box::new(operation.clone()),
            branch_updates,
            reembedded,
        };
        if event.catalog_id != CATALOG_ID
            || event.kind != expected_kind
            || event.before != snapshot
            || event.after != after
        {
            return Err(DynamicSparseCoreCollectionError::TraceVerification);
        }
        snapshot = after;
    }
    audit_completion(&snapshot, trace)
}

/// Executes every sparse-core branch with one refresh per atomic outer stage.
///
/// # Errors
///
/// Rejects mismatched branches, out-of-band stages, component failure, or
/// checked metric overflow.
pub fn execute_dynamic_sparse_core_collection_stages(
    input: &DynamicSparseCoreCollectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicSparseCoreCollectionResult, DynamicSparseCoreCollectionError> {
    validate_stage_input(input, batches)?;
    let mut branch_snapshots = Vec::with_capacity(input.branches.len());
    for (branch, branch_input) in input.branches.iter().enumerate() {
        let result = execute_dynamic_sparse_core_stages(branch_input, batches)
            .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?;
        branch_snapshots.push(result.final_snapshot);
    }
    let metrics = aggregate_final_metrics(&branch_snapshots, batches.len())?;
    Ok(DynamicSparseCoreCollectionResult {
        final_snapshot: DynamicSparseCoreCollectionSnapshot {
            branch_snapshots,
            stage: to_u64(batches.len())?,
            complete: true,
            metrics,
        },
    })
}

/// Records source-stage-aligned atomic update batches from every branch.
///
/// # Errors
///
/// Returns any input, component, arithmetic, or independent replay failure.
pub fn trace_dynamic_sparse_core_collection_stages(
    input: &DynamicSparseCoreCollectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicSparseCoreCollectionStageTraceResult, DynamicSparseCoreCollectionError> {
    validate_stage_input(input, batches)?;
    let mut branch_traces = Vec::with_capacity(input.branches.len());
    for (branch, branch_input) in input.branches.iter().enumerate() {
        branch_traces.push(
            trace_dynamic_sparse_core_stages(branch_input, batches)
                .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?,
        );
    }
    let trace = assemble_stage_trace(batches, branch_traces)?;
    check_dynamic_sparse_core_collection_stage_trace(input, batches, &trace)?;
    Ok(trace)
}

/// Independently verifies every atomic branch and their collection alignment.
///
/// # Errors
///
/// Rejects component drift, batch misalignment, reordered branch updates,
/// nonexact re-embedded sets, metric drift, or forged completion.
pub fn check_dynamic_sparse_core_collection_stage_trace(
    input: &DynamicSparseCoreCollectionInput,
    batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicSparseCoreCollectionStageTraceResult,
) -> Result<(), DynamicSparseCoreCollectionError> {
    validate_stage_input(input, batches)?;
    if trace.branch_traces.len() != input.branches.len()
        || trace.events.len()
            != batches
                .len()
                .checked_add(1)
                .ok_or(DynamicSparseCoreCollectionError::TraceVerification)?
    {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }
    for (branch, (branch_input, branch_trace)) in
        input.branches.iter().zip(&trace.branch_traces).enumerate()
    {
        check_dynamic_sparse_core_stage_trace(branch_input, batches, branch_trace)
            .map_err(|error| DynamicSparseCoreCollectionError::Branch { branch, error })?;
    }

    let mut snapshot = initial_stage_collection_snapshot(&trace.branch_traces);
    if trace.base_snapshot != snapshot {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }
    for (index, batch) in batches.iter().enumerate() {
        let event = &trace.events[index];
        let (branch_updates, reembedded, branch_snapshots) =
            extract_stage_component_data(&trace.branch_traces, index, batch, true)?;
        let mut after = snapshot.clone();
        after.branch_snapshots = branch_snapshots;
        after.stage = audit_increment(after.stage)?;
        audit_add_stage_metrics(&mut after.metrics, &branch_updates, &reembedded)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        let expected_kind = DynamicSparseCoreCollectionStageEventKind::Updated {
            batch: batch.clone(),
            branch_updates,
            reembedded,
        };
        if event.catalog_id != CATALOG_ID
            || event.kind != expected_kind
            || event.before != snapshot
            || event.after != after
        {
            return Err(DynamicSparseCoreCollectionError::TraceVerification);
        }
        snapshot = after;
    }
    audit_stage_collection_completion(&snapshot, trace)
}

fn assemble_trace(
    operations: &[DynamicCoreGraphOperation],
    branch_traces: Vec<DynamicSparseCoreTraceResult>,
) -> Result<DynamicSparseCoreCollectionTraceResult, DynamicSparseCoreCollectionError> {
    let mut snapshot = DynamicSparseCoreCollectionSnapshot {
        branch_snapshots: branch_traces
            .iter()
            .map(|branch| branch.base_snapshot.clone())
            .collect(),
        stage: 0,
        complete: false,
        metrics: DynamicSparseCoreCollectionMetrics::default(),
    };
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(operations.len() + 1);
    for (index, operation) in operations.iter().enumerate() {
        let before = snapshot.clone();
        let (branch_updates, reembedded, branch_snapshots) =
            component_stage(&branch_traces, index, operation)?;
        snapshot.branch_snapshots = branch_snapshots;
        snapshot.stage = increment(snapshot.stage)?;
        add_stage_metrics(&mut snapshot.metrics, &branch_updates, &reembedded)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        events.push(DynamicSparseCoreCollectionTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicSparseCoreCollectionEventKind::Updated {
                operation: Box::new(operation.clone()),
                branch_updates,
                reembedded,
            },
            before,
            after: snapshot.clone(),
        });
    }
    let before = snapshot.clone();
    snapshot.branch_snapshots = branch_traces
        .iter()
        .map(|branch| branch.result.final_snapshot.clone())
        .collect();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    events.push(DynamicSparseCoreCollectionTraceEvent {
        catalog_id: CATALOG_ID,
        kind: DynamicSparseCoreCollectionEventKind::Completed,
        before,
        after: snapshot.clone(),
    });
    Ok(DynamicSparseCoreCollectionTraceResult {
        branch_traces,
        base_snapshot,
        events,
        result: DynamicSparseCoreCollectionResult {
            final_snapshot: snapshot,
        },
    })
}

fn assemble_stage_trace(
    batches: &[DynamicCoreGraphStageBatch],
    branch_traces: Vec<DynamicSparseCoreStageTraceResult>,
) -> Result<DynamicSparseCoreCollectionStageTraceResult, DynamicSparseCoreCollectionError> {
    let mut snapshot = initial_stage_collection_snapshot(&branch_traces);
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(batches.len() + 1);
    for (index, batch) in batches.iter().enumerate() {
        let before = snapshot.clone();
        let (branch_updates, reembedded, branch_snapshots) =
            extract_stage_component_data(&branch_traces, index, batch, false)?;
        snapshot.branch_snapshots = branch_snapshots;
        snapshot.stage = increment(snapshot.stage)?;
        add_stage_metrics(&mut snapshot.metrics, &branch_updates, &reembedded)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        events.push(DynamicSparseCoreCollectionStageTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicSparseCoreCollectionStageEventKind::Updated {
                batch: batch.clone(),
                branch_updates,
                reembedded,
            },
            before,
            after: snapshot.clone(),
        });
    }
    let before = snapshot.clone();
    snapshot.branch_snapshots = branch_traces
        .iter()
        .map(|branch| branch.result.final_snapshot.clone())
        .collect();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    events.push(DynamicSparseCoreCollectionStageTraceEvent {
        catalog_id: CATALOG_ID,
        kind: DynamicSparseCoreCollectionStageEventKind::Completed,
        before,
        after: snapshot.clone(),
    });
    Ok(DynamicSparseCoreCollectionStageTraceResult {
        branch_traces,
        base_snapshot,
        events,
        result: DynamicSparseCoreCollectionResult {
            final_snapshot: snapshot,
        },
    })
}

fn initial_stage_collection_snapshot(
    branches: &[DynamicSparseCoreStageTraceResult],
) -> DynamicSparseCoreCollectionSnapshot {
    DynamicSparseCoreCollectionSnapshot {
        branch_snapshots: branches
            .iter()
            .map(|branch| branch.base_snapshot.clone())
            .collect(),
        stage: 0,
        complete: false,
        metrics: DynamicSparseCoreCollectionMetrics::default(),
    }
}

fn component_stage(
    branches: &[DynamicSparseCoreTraceResult],
    index: usize,
    operation: &DynamicCoreGraphOperation,
) -> Result<StageData, DynamicSparseCoreCollectionError> {
    extract_component_stage(branches, index, operation, false)
}

fn audit_component_stage(
    branches: &[DynamicSparseCoreTraceResult],
    index: usize,
    operation: &DynamicCoreGraphOperation,
) -> Result<StageData, DynamicSparseCoreCollectionError> {
    extract_component_stage(branches, index, operation, true)
}

type StageData = (
    Vec<Vec<DynamicSparseCoreUpdate>>,
    Vec<Vec<usize>>,
    Vec<DynamicSparseCoreSnapshot>,
);

fn extract_component_stage(
    branches: &[DynamicSparseCoreTraceResult],
    index: usize,
    operation: &DynamicCoreGraphOperation,
    audit: bool,
) -> Result<StageData, DynamicSparseCoreCollectionError> {
    let failure = || {
        if audit {
            DynamicSparseCoreCollectionError::TraceVerification
        } else {
            DynamicSparseCoreCollectionError::InvalidInput
        }
    };
    let mut updates = Vec::with_capacity(branches.len());
    let mut reembedded = Vec::with_capacity(branches.len());
    let mut snapshots = Vec::with_capacity(branches.len());
    for branch in branches {
        let event = branch.events.get(index).ok_or_else(&failure)?;
        let DynamicSparseCoreEventKind::Updated {
            core_event,
            sparse_updates,
        } = &event.kind
        else {
            return Err(failure());
        };
        let DynamicCoreGraphEventKind::Updated {
            operation: branch_operation,
            ..
        } = core_event.as_ref()
        else {
            return Err(failure());
        };
        if branch_operation.as_ref() != operation {
            return Err(failure());
        }
        updates.push(sparse_updates.clone());
        reembedded.push(event.after.last_reembedded.clone());
        snapshots.push(event.after.clone());
    }
    Ok((updates, reembedded, snapshots))
}

fn extract_stage_component_data(
    branches: &[DynamicSparseCoreStageTraceResult],
    index: usize,
    batch: &DynamicCoreGraphStageBatch,
    audit: bool,
) -> Result<StageData, DynamicSparseCoreCollectionError> {
    let failure = || {
        if audit {
            DynamicSparseCoreCollectionError::TraceVerification
        } else {
            DynamicSparseCoreCollectionError::InvalidInput
        }
    };
    let mut updates = Vec::with_capacity(branches.len());
    let mut reembedded = Vec::with_capacity(branches.len());
    let mut snapshots = Vec::with_capacity(branches.len());
    for branch in branches {
        let event = branch.events.get(index).ok_or_else(&failure)?;
        let DynamicSparseCoreStageEventKind::Updated {
            core_event,
            sparse_updates,
        } = &event.kind
        else {
            return Err(failure());
        };
        let DynamicCoreGraphStageEventKind::Updated {
            batch: branch_batch,
            ..
        } = core_event.as_ref()
        else {
            return Err(failure());
        };
        if branch_batch != batch {
            return Err(failure());
        }
        updates.push(sparse_updates.clone());
        reembedded.push(event.after.last_reembedded.clone());
        snapshots.push(event.after.clone());
    }
    Ok((updates, reembedded, snapshots))
}

fn validate_input(
    input: &DynamicSparseCoreCollectionInput,
    operations: &[DynamicCoreGraphOperation],
) -> Result<(), DynamicSparseCoreCollectionError> {
    validate_shared_branches(input)?;
    if operations.len() > DYNAMIC_SPARSE_CORE_COLLECTION_MAX_OPERATIONS {
        return Err(DynamicSparseCoreCollectionError::AdmissionLimit);
    }
    Ok(())
}

fn validate_stage_input(
    input: &DynamicSparseCoreCollectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<(), DynamicSparseCoreCollectionError> {
    validate_shared_branches(input)?;
    if batches.len() > DYNAMIC_SPARSE_CORE_COLLECTION_MAX_OPERATIONS {
        return Err(DynamicSparseCoreCollectionError::AdmissionLimit);
    }
    Ok(())
}

fn validate_shared_branches(
    input: &DynamicSparseCoreCollectionInput,
) -> Result<(), DynamicSparseCoreCollectionError> {
    if input.branches.is_empty() {
        return Err(DynamicSparseCoreCollectionError::InvalidInput);
    }
    if input.branches.len() > DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES {
        return Err(DynamicSparseCoreCollectionError::AdmissionLimit);
    }
    let expected_branches = input.branches.len();
    let first = &input.branches[0];
    for branch in &input.branches {
        if branch.branches != expected_branches
            || branch.core.forest.initial_node_count != first.core.forest.initial_node_count
            || branch.core.forest.maximum_node_count != first.core.forest.maximum_node_count
            || branch.core.forest.edge_slots != first.core.forest.edge_slots
            || branch.core.initial_gradients != first.core.initial_gradients
        {
            return Err(DynamicSparseCoreCollectionError::InvalidInput);
        }
    }
    Ok(())
}

fn add_stage_metrics(
    metrics: &mut DynamicSparseCoreCollectionMetrics,
    branch_updates: &[Vec<DynamicSparseCoreUpdate>],
    reembedded: &[Vec<usize>],
) -> Result<(), DynamicSparseCoreCollectionError> {
    metrics.source_stages = increment(metrics.source_stages)?;
    metrics.branch_applications = add_usize(metrics.branch_applications, branch_updates.len())?;
    for updates in branch_updates {
        metrics.sparse_updates = add_usize(metrics.sparse_updates, updates.len())?;
    }
    for edges in reembedded {
        metrics.reembedded_edges = add_usize(metrics.reembedded_edges, edges.len())?;
    }
    Ok(())
}

fn audit_add_stage_metrics(
    metrics: &mut DynamicSparseCoreCollectionMetrics,
    branch_updates: &[Vec<DynamicSparseCoreUpdate>],
    reembedded: &[Vec<usize>],
) -> Result<(), DynamicSparseCoreCollectionError> {
    metrics.source_stages = audit_increment(metrics.source_stages)?;
    metrics.branch_applications =
        audit_add_usize(metrics.branch_applications, branch_updates.len())?;
    for updates in branch_updates {
        metrics.sparse_updates = audit_add_usize(metrics.sparse_updates, updates.len())?;
    }
    for edges in reembedded {
        metrics.reembedded_edges = audit_add_usize(metrics.reembedded_edges, edges.len())?;
    }
    Ok(())
}

fn aggregate_final_metrics(
    branches: &[DynamicSparseCoreSnapshot],
    stages: usize,
) -> Result<DynamicSparseCoreCollectionMetrics, DynamicSparseCoreCollectionError> {
    let mut metrics = DynamicSparseCoreCollectionMetrics {
        source_stages: to_u64(stages)?,
        branch_applications: to_u64(
            stages
                .checked_mul(branches.len())
                .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)?,
        )?,
        state_transitions: to_u64(
            stages
                .checked_add(1)
                .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)?,
        )?,
        ..DynamicSparseCoreCollectionMetrics::default()
    };
    for branch in branches {
        let branch_metrics = branch.metrics;
        let sparse_updates = branch_metrics
            .vertex_splits
            .checked_add(branch_metrics.edge_insertions)
            .and_then(|value| value.checked_add(branch_metrics.edge_deletions))
            .and_then(|value| value.checked_add(branch_metrics.edge_reinsertions))
            .and_then(|value| value.checked_add(branch_metrics.gradient_replacements))
            .and_then(|value| value.checked_add(branch_metrics.length_replacements))
            .and_then(|value| value.checked_add(branch_metrics.forced_reinsertions))
            .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)?;
        metrics.sparse_updates = metrics
            .sparse_updates
            .checked_add(sparse_updates)
            .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)?;
        metrics.reembedded_edges = metrics
            .reembedded_edges
            .checked_add(branch_metrics.forced_reinsertions)
            .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)?;
    }
    Ok(metrics)
}

fn audit_completion(
    snapshot: &DynamicSparseCoreCollectionSnapshot,
    trace: &DynamicSparseCoreCollectionTraceResult,
) -> Result<(), DynamicSparseCoreCollectionError> {
    let event = trace
        .events
        .last()
        .ok_or(DynamicSparseCoreCollectionError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.branch_snapshots = trace
        .branch_traces
        .iter()
        .map(|branch| branch.result.final_snapshot.clone())
        .collect();
    expected.complete = true;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    if event.catalog_id != CATALOG_ID
        || event.kind != DynamicSparseCoreCollectionEventKind::Completed
        || event.before != *snapshot
        || event.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }
    Ok(())
}

fn audit_stage_collection_completion(
    snapshot: &DynamicSparseCoreCollectionSnapshot,
    trace: &DynamicSparseCoreCollectionStageTraceResult,
) -> Result<(), DynamicSparseCoreCollectionError> {
    let event = trace
        .events
        .last()
        .ok_or(DynamicSparseCoreCollectionError::TraceVerification)?;
    let mut expected = snapshot.clone();
    expected.branch_snapshots = trace
        .branch_traces
        .iter()
        .map(|branch| branch.result.final_snapshot.clone())
        .collect();
    expected.complete = true;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    if event.catalog_id != CATALOG_ID
        || event.kind != DynamicSparseCoreCollectionStageEventKind::Completed
        || event.before != *snapshot
        || event.after != expected
        || trace.result.final_snapshot != expected
    {
        return Err(DynamicSparseCoreCollectionError::TraceVerification);
    }
    Ok(())
}

fn increment(value: u64) -> Result<u64, DynamicSparseCoreCollectionError> {
    value
        .checked_add(1)
        .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicSparseCoreCollectionError> {
    value
        .checked_add(1)
        .ok_or(DynamicSparseCoreCollectionError::TraceVerification)
}

fn add_usize(value: u64, additional: usize) -> Result<u64, DynamicSparseCoreCollectionError> {
    value
        .checked_add(to_u64(additional)?)
        .ok_or(DynamicSparseCoreCollectionError::ArithmeticOverflow)
}

fn audit_add_usize(value: u64, additional: usize) -> Result<u64, DynamicSparseCoreCollectionError> {
    value
        .checked_add(
            u64::try_from(additional)
                .map_err(|_| DynamicSparseCoreCollectionError::TraceVerification)?,
        )
        .ok_or(DynamicSparseCoreCollectionError::TraceVerification)
}

fn to_u64(value: usize) -> Result<u64, DynamicSparseCoreCollectionError> {
    u64::try_from(value).map_err(|_| DynamicSparseCoreCollectionError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;
    use crate::{
        DynamicCoreGraphInput, DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate,
        DynamicLowStretchForestEdge, DynamicLowStretchForestInput,
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

    fn branch(reference_tree_edges: Vec<usize>, root: usize) -> DynamicSparseCoreInput {
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
                    ],
                    reference_tree_edges,
                    reference_root: root,
                    initial_root_seeds: vec![root],
                    initial_stretch_overestimates: None,
                },
                initial_gradients: vec![
                    Some(rational(2)),
                    Some(rational(3)),
                    Some(rational(5)),
                    Some(rational(7)),
                    None,
                ],
            },
            branches: 2,
        }
    }

    fn input() -> DynamicSparseCoreCollectionInput {
        DynamicSparseCoreCollectionInput {
            branches: vec![branch(vec![0, 1, 2], 0), branch(vec![0, 2, 3], 0)],
        }
    }

    fn operations() -> Vec<DynamicCoreGraphOperation> {
        vec![
            DynamicCoreGraphOperation::Insert {
                edge: edge(4, 2, 0, 3),
                gradient: rational(-4),
            },
            DynamicCoreGraphOperation::Delete { edge: 4 },
        ]
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

    fn stage_batches() -> Vec<DynamicCoreGraphStageBatch> {
        let inserted = stage_edge(4, 2, 0, 3, -4);
        vec![DynamicCoreGraphStageBatch {
            outer_stage: 1,
            updates: vec![
                DynamicCoreGraphStageUpdate::Insert {
                    edge: inserted.clone(),
                },
                DynamicCoreGraphStageUpdate::Reinsert {
                    before: inserted,
                    after: stage_edge(4, 2, 0, 2, -3),
                },
            ],
        }]
    }

    #[test]
    fn one_source_stage_is_applied_to_every_branch_in_order() {
        let trace = trace_dynamic_sparse_core_collection(&input(), &operations()).expect("trace");
        assert_eq!(trace.events.len(), 3);
        let DynamicSparseCoreCollectionEventKind::Updated {
            operation,
            branch_updates,
            reembedded,
        } = &trace.events[0].kind
        else {
            panic!("update");
        };
        assert!(matches!(
            operation.as_ref(),
            DynamicCoreGraphOperation::Insert { edge, .. } if edge.edge == 4
        ));
        assert_eq!(branch_updates.len(), 2);
        assert_eq!(reembedded.len(), 2);
        assert!(branch_updates.iter().all(|updates| !updates.is_empty()));
        assert_eq!(trace.events[0].after.stage, 1);
        assert_eq!(trace.events[0].after.metrics.branch_applications, 2);
    }

    #[test]
    fn distinct_reference_trees_keep_distinct_checked_branch_states() {
        let trace = trace_dynamic_sparse_core_collection(&input(), &operations()).expect("trace");
        let branches = &trace.events[0].after.branch_snapshots;
        assert_ne!(branches[0].core_edge_slots, branches[1].core_edge_slots);
        assert_eq!(branches[0].stage, branches[1].stage);
        let left = branches[0].core_edge_slots[4].as_ref().expect("left");
        let right = branches[1].core_edge_slots[4].as_ref().expect("right");
        assert_eq!(
            (left.edge, left.from, left.to, &left.length),
            (right.edge, right.from, right.to, &right.length)
        );
        assert_ne!(left.gradient, right.gradient);
    }

    #[test]
    fn fast_trace_and_independent_collection_checker_match() {
        let input = input();
        let operations = operations();
        let fast = execute_dynamic_sparse_core_collection(&input, &operations).expect("fast");
        let trace = trace_dynamic_sparse_core_collection(&input, &operations).expect("trace");
        assert_eq!(fast, trace.result);
        check_dynamic_sparse_core_collection_trace(&input, &operations, &trace).expect("check");
    }

    #[test]
    fn atomic_collection_applies_insert_then_reinsert_once_per_branch() {
        let input = input();
        let batches = stage_batches();
        let fast =
            execute_dynamic_sparse_core_collection_stages(&input, &batches).expect("fast stage");
        let trace =
            trace_dynamic_sparse_core_collection_stages(&input, &batches).expect("trace stage");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 2);
        assert_eq!(fast.final_snapshot.stage, 1);
        assert_eq!(fast.final_snapshot.metrics.source_stages, 1);
        assert_eq!(fast.final_snapshot.metrics.branch_applications, 2);
        let DynamicSparseCoreCollectionStageEventKind::Updated {
            batch,
            branch_updates,
            reembedded,
        } = &trace.events[0].kind
        else {
            panic!("stage");
        };
        assert_eq!(batch, &batches[0]);
        assert_eq!(branch_updates.len(), 2);
        assert_eq!(reembedded.len(), 2);
        for updates in branch_updates {
            let insert = updates
                .iter()
                .position(|update| matches!(update, DynamicSparseCoreUpdate::EdgeInserted { edge, .. } if edge.edge == 4))
                .expect("insert");
            let reinsert = updates
                .iter()
                .position(|update| matches!(update, DynamicSparseCoreUpdate::EdgeReinserted { after, .. } if after.edge == 4))
                .expect("reinsert");
            assert!(insert < reinsert);
        }
        check_dynamic_sparse_core_collection_stage_trace(&input, &batches, &trace)
            .expect("check stage");
    }

    #[test]
    fn atomic_collection_keeps_branch_specific_reference_trees() {
        let trace = trace_dynamic_sparse_core_collection_stages(&input(), &stage_batches())
            .expect("trace stage");
        let branches = &trace.events[0].after.branch_snapshots;
        assert_ne!(branches[0].core_edge_slots, branches[1].core_edge_slots);
        assert_eq!(branches[0].stage, branches[1].stage);
    }

    #[test]
    fn atomic_collection_checker_rejects_batch_and_component_tampering() {
        let input = input();
        let batches = stage_batches();
        let trace =
            trace_dynamic_sparse_core_collection_stages(&input, &batches).expect("trace stage");

        let mut tampered = trace.clone();
        let DynamicSparseCoreCollectionStageEventKind::Updated { branch_updates, .. } =
            &mut tampered.events[0].kind
        else {
            panic!("stage");
        };
        branch_updates[0].clear();
        assert_eq!(
            check_dynamic_sparse_core_collection_stage_trace(&input, &batches, &tampered),
            Err(DynamicSparseCoreCollectionError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.branch_traces[0].events[0]
            .after
            .last_reembedded
            .clear();
        assert!(matches!(
            check_dynamic_sparse_core_collection_stage_trace(&input, &batches, &tampered),
            Err(DynamicSparseCoreCollectionError::Branch {
                branch: 0,
                error: DynamicSparseCoreError::TraceVerification
            })
        ));
    }

    #[test]
    fn checker_rejects_collection_and_component_tampering() {
        let input = input();
        let operations = operations();
        let mut trace = trace_dynamic_sparse_core_collection(&input, &operations).expect("trace");
        let DynamicSparseCoreCollectionEventKind::Updated { branch_updates, .. } =
            &mut trace.events[0].kind
        else {
            panic!("update");
        };
        branch_updates.swap(0, 1);
        assert_eq!(
            check_dynamic_sparse_core_collection_trace(&input, &operations, &trace),
            Err(DynamicSparseCoreCollectionError::TraceVerification)
        );

        let mut trace = trace_dynamic_sparse_core_collection(&input, &operations).expect("trace");
        trace.branch_traces[0].events[0]
            .after
            .last_reembedded
            .clear();
        assert!(matches!(
            check_dynamic_sparse_core_collection_trace(&input, &operations, &trace),
            Err(DynamicSparseCoreCollectionError::Branch {
                branch: 0,
                error: DynamicSparseCoreError::TraceVerification
            })
        ));
    }

    #[test]
    fn mismatched_source_graph_and_reduction_factor_fail_closed() {
        let mut graph_mismatch = input();
        graph_mismatch.branches[1].core.initial_gradients[3] = Some(rational(99));
        assert_eq!(
            execute_dynamic_sparse_core_collection(&graph_mismatch, &[]),
            Err(DynamicSparseCoreCollectionError::InvalidInput)
        );

        let mut count_mismatch = input();
        count_mismatch.branches[1].branches = 3;
        assert_eq!(
            execute_dynamic_sparse_core_collection(&count_mismatch, &[]),
            Err(DynamicSparseCoreCollectionError::InvalidInput)
        );
    }

    #[test]
    fn empty_and_overwide_collections_fail_before_components_run() {
        assert_eq!(
            execute_dynamic_sparse_core_collection(
                &DynamicSparseCoreCollectionInput { branches: vec![] },
                &[]
            ),
            Err(DynamicSparseCoreCollectionError::InvalidInput)
        );
        let repeated = branch(vec![0, 1, 2], 0);
        let too_many = DynamicSparseCoreCollectionInput {
            branches: vec![repeated; DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES + 1],
        };
        assert_eq!(
            execute_dynamic_sparse_core_collection(&too_many, &[]),
            Err(DynamicSparseCoreCollectionError::AdmissionLimit)
        );
    }
}
