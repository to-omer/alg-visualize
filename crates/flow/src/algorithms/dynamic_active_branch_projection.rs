//! Checked one-level composition for dynamic tree-chain propagation.
//!
//! Every sparse-core branch at the current level consumes the same atomic
//! source stages. Exactly one configured active branch is projected to the
//! next level. Each projected parent stage remains one child core stage; the
//! inactive branches still advance and retain their checked snapshots. This
//! module composes existing bounded primitives and does not choose the active
//! branch, construct a deeper chain, or implement Shift/Rebuild suffix epochs.

use thiserror::Error;

use super::{
    DynamicCoreGraphStageBatch, DynamicLevelProjectionError, DynamicLevelProjectionMetrics,
    DynamicLevelProjectionState, DynamicLevelProjectionTraceResult, DynamicLevelStageAdapterError,
    DynamicLevelStageAdapterTraceResult, DynamicSparseCoreCollectionError,
    DynamicSparseCoreCollectionInput, DynamicSparseCoreCollectionSnapshot,
    DynamicSparseCoreCollectionStageEventKind, DynamicSparseCoreCollectionStageTraceResult,
    DynamicSparseCoreUpdate, check_dynamic_level_projection_trace,
    check_dynamic_level_stage_adapter_trace, check_dynamic_sparse_core_collection_stage_trace,
    initialize_dynamic_level_projection, trace_dynamic_level_projection,
    trace_dynamic_level_stage_adapter, trace_dynamic_sparse_core_collection_stages,
};

const CATALOG_ID: &str = "dynamic-active-branch-projection";

/// One current-level collection and its selected propagation branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicActiveBranchProjectionInput {
    /// All maintained sparse-core branches at this level.
    pub collection: DynamicSparseCoreCollectionInput,
    /// Canonical branch index propagated to the next level.
    pub active_branch: usize,
}

/// Exact work counters for active-branch propagation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicActiveBranchProjectionMetrics {
    /// Outer source stages propagated.
    pub outer_stages: u64,
    /// Active-branch sparse records inspected.
    pub propagated_records: u64,
    /// Actual split incidences moved at the child boundary.
    pub moved_incidences: u64,
    /// Smaller-side incidences retained for encoded update accounting.
    pub encoded_incidences: u64,
    /// Stable child edge insertions.
    pub edge_insertions: u64,
    /// Stable child edge deletions.
    pub edge_deletions: u64,
    /// Explicit and forced child edge reinsertions.
    pub edge_reinsertions: u64,
    /// Child attribute replacements.
    pub attribute_replacements: u64,
}

/// Exact one-level composition result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicActiveBranchProjectionResult {
    /// Terminal state of every current-level branch.
    pub final_collection: DynamicSparseCoreCollectionSnapshot,
    /// Terminal active-branch projection state.
    pub final_projection: DynamicLevelProjectionState,
    /// Atomic child batches in parent stage order.
    pub child_batches: Vec<DynamicCoreGraphStageBatch>,
    /// Exact propagation counters.
    pub metrics: DynamicActiveBranchProjectionMetrics,
}

/// One active-branch inter-level stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicActiveBranchProjectionTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Selected current-level branch.
    pub active_branch: usize,
    /// Independently checkable sparse-to-child projection.
    pub projection_trace: DynamicLevelProjectionTraceResult,
    /// Independently checkable conversion to the child core batch.
    pub adapter_trace: DynamicLevelStageAdapterTraceResult,
}

/// Complete current-level, projection, and adapter transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicActiveBranchProjectionTraceResult {
    /// Checked all-branch current-level execution.
    pub collection_trace: DynamicSparseCoreCollectionStageTraceResult,
    /// Projection state derived from the active branch before the first stage.
    pub base_projection: DynamicLevelProjectionState,
    /// One active-branch boundary per outer source stage.
    pub events: Vec<DynamicActiveBranchProjectionTraceEvent>,
    /// Exact terminal result.
    pub result: DynamicActiveBranchProjectionResult,
}

/// Explicit one-level composition failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicActiveBranchProjectionError {
    /// The collection or active branch is malformed.
    #[error("dynamic active-branch projection input is invalid")]
    InvalidInput,
    /// All-branch current-level execution failed.
    #[error("dynamic active-branch collection failed: {0}")]
    Collection(#[from] DynamicSparseCoreCollectionError),
    /// Sparse-to-child projection failed.
    #[error("dynamic active-branch level projection failed: {0}")]
    Projection(#[from] DynamicLevelProjectionError),
    /// Child core-stage adaptation failed.
    #[error("dynamic active-branch stage adapter failed: {0}")]
    Adapter(#[from] DynamicLevelStageAdapterError),
    /// Checked metric arithmetic overflowed.
    #[error("dynamic active-branch projection arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied composition transcript failed independent verification.
    #[error("dynamic active-branch projection trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    collection_trace: DynamicSparseCoreCollectionStageTraceResult,
    base_projection: DynamicLevelProjectionState,
    events: Vec<DynamicActiveBranchProjectionTraceEvent>,
    result: DynamicActiveBranchProjectionResult,
}

/// Advances every current-level branch and returns only the active child stream.
///
/// # Errors
///
/// Rejects an invalid branch, any component failure, projection drift, adapter
/// drift, or checked metric overflow.
pub fn execute_dynamic_active_branch_projection(
    input: &DynamicActiveBranchProjectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicActiveBranchProjectionResult, DynamicActiveBranchProjectionError> {
    compose(input, batches).map(|run| run.result)
}

/// Records the all-branch execution and every active inter-level boundary.
///
/// # Errors
///
/// Returns any input, component, projection, adapter, arithmetic, or replay
/// failure.
pub fn trace_dynamic_active_branch_projection(
    input: &DynamicActiveBranchProjectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicActiveBranchProjectionTraceResult, DynamicActiveBranchProjectionError> {
    let run = compose(input, batches)?;
    let trace = DynamicActiveBranchProjectionTraceResult {
        collection_trace: run.collection_trace,
        base_projection: run.base_projection,
        events: run.events,
        result: run.result,
    };
    check_dynamic_active_branch_projection_trace(input, batches, &trace)?;
    Ok(trace)
}

/// Independently verifies all branches and the selected propagation stream.
///
/// # Errors
///
/// Rejects branch-selection drift, component tampering, noncontiguous projection
/// state, child-batch changes, metric drift, or a forged terminal result.
pub fn check_dynamic_active_branch_projection_trace(
    input: &DynamicActiveBranchProjectionInput,
    batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicActiveBranchProjectionTraceResult,
) -> Result<(), DynamicActiveBranchProjectionError> {
    validate_input(input)?;
    check_dynamic_sparse_core_collection_stage_trace(
        &input.collection,
        batches,
        &trace.collection_trace,
    )?;
    if trace.events.len() != batches.len() {
        return Err(DynamicActiveBranchProjectionError::TraceVerification);
    }
    let active = input.active_branch;
    let base_sparse = trace
        .collection_trace
        .base_snapshot
        .branch_snapshots
        .get(active)
        .ok_or(DynamicActiveBranchProjectionError::TraceVerification)?;
    let mut projection = initialize_dynamic_level_projection(base_sparse)?;
    if trace.base_projection != projection {
        return Err(DynamicActiveBranchProjectionError::TraceVerification);
    }
    let mut child_batches = Vec::with_capacity(batches.len());
    let mut metrics = DynamicActiveBranchProjectionMetrics::default();
    for (index, event) in trace.events.iter().enumerate() {
        let (before_sparse, after_sparse, sparse_updates) =
            audit_active_stage(&trace.collection_trace, index, active, &batches[index])?;
        if event.catalog_id != CATALOG_ID || event.active_branch != active {
            return Err(DynamicActiveBranchProjectionError::TraceVerification);
        }
        check_dynamic_level_projection_trace(
            &projection,
            before_sparse,
            after_sparse,
            sparse_updates,
            &event.projection_trace,
        )?;
        check_dynamic_level_stage_adapter_trace(
            &event.projection_trace.result.batch,
            &event.adapter_trace,
        )?;
        let child_batch = event.adapter_trace.result.batch.clone();
        if child_batch.outer_stage != batches[index].outer_stage {
            return Err(DynamicActiveBranchProjectionError::TraceVerification);
        }
        audit_account_projection_metrics(&mut metrics, event.projection_trace.result.metrics)?;
        child_batches.push(child_batch);
        projection = event.projection_trace.result.final_state.clone();
    }
    let expected = DynamicActiveBranchProjectionResult {
        final_collection: trace.collection_trace.result.final_snapshot.clone(),
        final_projection: projection,
        child_batches,
        metrics,
    };
    if trace.result != expected {
        return Err(DynamicActiveBranchProjectionError::TraceVerification);
    }
    Ok(())
}

fn compose(
    input: &DynamicActiveBranchProjectionInput,
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<InternalRun, DynamicActiveBranchProjectionError> {
    validate_input(input)?;
    let collection_trace = trace_dynamic_sparse_core_collection_stages(&input.collection, batches)?;
    let active = input.active_branch;
    let base_sparse = collection_trace
        .base_snapshot
        .branch_snapshots
        .get(active)
        .ok_or(DynamicActiveBranchProjectionError::InvalidInput)?;
    let mut projection = initialize_dynamic_level_projection(base_sparse)?;
    let base_projection = projection.clone();
    let mut events = Vec::with_capacity(batches.len());
    let mut child_batches = Vec::with_capacity(batches.len());
    let mut metrics = DynamicActiveBranchProjectionMetrics::default();
    for (index, batch) in batches.iter().enumerate() {
        let (before_sparse, after_sparse, sparse_updates) =
            active_stage(&collection_trace, index, active, batch)?;
        let projection_trace = trace_dynamic_level_projection(
            &projection,
            before_sparse,
            after_sparse,
            sparse_updates,
        )?;
        let adapter_trace = trace_dynamic_level_stage_adapter(&projection_trace.result.batch)?;
        account_projection_metrics(&mut metrics, projection_trace.result.metrics)?;
        child_batches.push(adapter_trace.result.batch.clone());
        projection = projection_trace.result.final_state.clone();
        events.push(DynamicActiveBranchProjectionTraceEvent {
            catalog_id: CATALOG_ID,
            active_branch: active,
            projection_trace,
            adapter_trace,
        });
    }
    let result = DynamicActiveBranchProjectionResult {
        final_collection: collection_trace.result.final_snapshot.clone(),
        final_projection: projection,
        child_batches,
        metrics,
    };
    Ok(InternalRun {
        collection_trace,
        base_projection,
        events,
        result,
    })
}

fn validate_input(
    input: &DynamicActiveBranchProjectionInput,
) -> Result<(), DynamicActiveBranchProjectionError> {
    if input.collection.branches.is_empty()
        || input.active_branch >= input.collection.branches.len()
    {
        return Err(DynamicActiveBranchProjectionError::InvalidInput);
    }
    Ok(())
}

type ActiveStage<'a> = (
    &'a super::DynamicSparseCoreSnapshot,
    &'a super::DynamicSparseCoreSnapshot,
    &'a [DynamicSparseCoreUpdate],
);

fn active_stage<'a>(
    trace: &'a DynamicSparseCoreCollectionStageTraceResult,
    index: usize,
    active: usize,
    batch: &DynamicCoreGraphStageBatch,
) -> Result<ActiveStage<'a>, DynamicActiveBranchProjectionError> {
    extract_active_stage(trace, index, active, batch, false)
}

fn audit_active_stage<'a>(
    trace: &'a DynamicSparseCoreCollectionStageTraceResult,
    index: usize,
    active: usize,
    batch: &DynamicCoreGraphStageBatch,
) -> Result<ActiveStage<'a>, DynamicActiveBranchProjectionError> {
    extract_active_stage(trace, index, active, batch, true)
}

fn extract_active_stage<'a>(
    trace: &'a DynamicSparseCoreCollectionStageTraceResult,
    index: usize,
    active: usize,
    batch: &DynamicCoreGraphStageBatch,
    audit: bool,
) -> Result<ActiveStage<'a>, DynamicActiveBranchProjectionError> {
    let failure = || {
        if audit {
            DynamicActiveBranchProjectionError::TraceVerification
        } else {
            DynamicActiveBranchProjectionError::InvalidInput
        }
    };
    let event = trace.events.get(index).ok_or_else(&failure)?;
    let DynamicSparseCoreCollectionStageEventKind::Updated {
        batch: event_batch,
        branch_updates,
        ..
    } = &event.kind
    else {
        return Err(failure());
    };
    if event_batch != batch {
        return Err(failure());
    }
    let before = event
        .before
        .branch_snapshots
        .get(active)
        .ok_or_else(&failure)?;
    let after = event
        .after
        .branch_snapshots
        .get(active)
        .ok_or_else(&failure)?;
    let updates = branch_updates.get(active).ok_or_else(&failure)?;
    Ok((before, after, updates))
}

fn account_projection_metrics(
    target: &mut DynamicActiveBranchProjectionMetrics,
    source: DynamicLevelProjectionMetrics,
) -> Result<(), DynamicActiveBranchProjectionError> {
    target.outer_stages = increment(target.outer_stages)?;
    target.propagated_records = add(target.propagated_records, source.source_records)?;
    target.moved_incidences = add(target.moved_incidences, source.moved_incidences)?;
    target.encoded_incidences = add(target.encoded_incidences, source.encoded_incidences)?;
    target.edge_insertions = add(target.edge_insertions, source.edge_insertions)?;
    target.edge_deletions = add(target.edge_deletions, source.edge_deletions)?;
    target.edge_reinsertions = add(target.edge_reinsertions, source.edge_reinsertions)?;
    target.attribute_replacements =
        add(target.attribute_replacements, source.attribute_replacements)?;
    Ok(())
}

fn audit_account_projection_metrics(
    target: &mut DynamicActiveBranchProjectionMetrics,
    source: DynamicLevelProjectionMetrics,
) -> Result<(), DynamicActiveBranchProjectionError> {
    target.outer_stages = audit_increment(target.outer_stages)?;
    target.propagated_records = audit_add(target.propagated_records, source.source_records)?;
    target.moved_incidences = audit_add(target.moved_incidences, source.moved_incidences)?;
    target.encoded_incidences = audit_add(target.encoded_incidences, source.encoded_incidences)?;
    target.edge_insertions = audit_add(target.edge_insertions, source.edge_insertions)?;
    target.edge_deletions = audit_add(target.edge_deletions, source.edge_deletions)?;
    target.edge_reinsertions = audit_add(target.edge_reinsertions, source.edge_reinsertions)?;
    target.attribute_replacements =
        audit_add(target.attribute_replacements, source.attribute_replacements)?;
    Ok(())
}

fn increment(value: u64) -> Result<u64, DynamicActiveBranchProjectionError> {
    value
        .checked_add(1)
        .ok_or(DynamicActiveBranchProjectionError::ArithmeticOverflow)
}

fn add(left: u64, right: u64) -> Result<u64, DynamicActiveBranchProjectionError> {
    left.checked_add(right)
        .ok_or(DynamicActiveBranchProjectionError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicActiveBranchProjectionError> {
    value
        .checked_add(1)
        .ok_or(DynamicActiveBranchProjectionError::TraceVerification)
}

fn audit_add(left: u64, right: u64) -> Result<u64, DynamicActiveBranchProjectionError> {
    left.checked_add(right)
        .ok_or(DynamicActiveBranchProjectionError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;
    use crate::{
        DynamicCoreGraphInput, DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate,
        DynamicLowStretchForestEdge, DynamicLowStretchForestInput, DynamicSparseCoreInput,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn forest_edge(
        edge: usize,
        from: usize,
        to: usize,
        length: i64,
    ) -> DynamicLowStretchForestEdge {
        DynamicLowStretchForestEdge {
            edge,
            from,
            to,
            length: rational(length),
        }
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

    fn branch(reference_tree_edges: Vec<usize>) -> DynamicSparseCoreInput {
        DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
                forest: DynamicLowStretchForestInput {
                    initial_node_count: 4,
                    maximum_node_count: 5,
                    edge_slots: vec![
                        Some(forest_edge(0, 0, 1, 1)),
                        Some(forest_edge(1, 1, 2, 1)),
                        Some(forest_edge(2, 1, 3, 1)),
                        Some(forest_edge(3, 2, 3, 2)),
                        None,
                    ],
                    reference_tree_edges,
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
                ],
            },
            branches: 2,
        }
    }

    fn input(active_branch: usize) -> DynamicActiveBranchProjectionInput {
        DynamicActiveBranchProjectionInput {
            collection: DynamicSparseCoreCollectionInput {
                branches: vec![branch(vec![0, 1, 2]), branch(vec![0, 2, 3])],
            },
            active_branch,
        }
    }

    fn batches() -> Vec<DynamicCoreGraphStageBatch> {
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
    fn every_branch_advances_while_only_active_branch_is_projected() {
        let input = input(1);
        let batches = batches();
        let fast = execute_dynamic_active_branch_projection(&input, &batches).expect("fast");
        let trace = trace_dynamic_active_branch_projection(&input, &batches).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.final_collection.branch_snapshots.len(), 2);
        assert!(
            fast.final_collection
                .branch_snapshots
                .iter()
                .all(|branch| branch.stage == 1)
        );
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.events[0].active_branch, 1);
        assert_eq!(fast.child_batches.len(), 1);
        assert_eq!(fast.child_batches[0].outer_stage, 1);
        assert_eq!(fast.final_projection.graph.stage, 1);
        assert_eq!(fast.metrics.outer_stages, 1);
        assert!(fast.metrics.propagated_records >= 2);
        check_dynamic_active_branch_projection_trace(&input, &batches, &trace).expect("check");
    }

    #[test]
    fn different_active_branches_produce_their_checked_child_streams() {
        let left =
            trace_dynamic_active_branch_projection(&input(0), &batches()).expect("left trace");
        let right =
            trace_dynamic_active_branch_projection(&input(1), &batches()).expect("right trace");
        assert_ne!(
            left.result.final_collection.branch_snapshots[0].core_edge_slots,
            left.result.final_collection.branch_snapshots[1].core_edge_slots
        );
        assert_ne!(left.result.child_batches, right.result.child_batches);
        assert_eq!(left.events[0].active_branch, 0);
        assert_eq!(right.events[0].active_branch, 1);
    }

    #[test]
    fn composition_checker_rejects_branch_and_child_batch_tampering() {
        let input = input(1);
        let batches = batches();
        let trace = trace_dynamic_active_branch_projection(&input, &batches).expect("trace");

        let mut tampered = trace.clone();
        tampered.events[0].active_branch = 0;
        assert_eq!(
            check_dynamic_active_branch_projection_trace(&input, &batches, &tampered),
            Err(DynamicActiveBranchProjectionError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.events[0]
            .adapter_trace
            .result
            .batch
            .updates
            .clear();
        assert!(matches!(
            check_dynamic_active_branch_projection_trace(&input, &batches, &tampered),
            Err(DynamicActiveBranchProjectionError::Adapter(
                DynamicLevelStageAdapterError::TraceVerification
            ))
        ));
    }

    #[test]
    fn out_of_range_active_branch_fails_before_collection_execution() {
        assert_eq!(
            execute_dynamic_active_branch_projection(&input(2), &[]),
            Err(DynamicActiveBranchProjectionError::InvalidInput)
        );
    }
}
