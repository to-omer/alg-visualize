//! Continued atomic stages across dynamic tree-chain epoch boundaries.
//!
//! A fixed-epoch level runner starts at local stage zero. After Shift/Rebuild,
//! preserved prefix levels must retain their local histories while every fresh
//! suffix level starts a new local epoch. This module stores exactly that
//! boundary: each level owns its epoch-start collection input and the ordered
//! batches applied since that epoch began. A new root stage is replayed through
//! every current level; when it crosses into a newer child epoch, only the
//! batch's local `outer_stage` is renumbered. Its ordered topology/attribute
//! records remain byte-for-byte equal.
//!
//! Runtime transitions are copy-on-write and published only after every level,
//! active projection, adapter, and current graph handoff has passed its existing
//! independent checker. This is a bounded replay-based continuation layer, not
//! the source paper's incremental data-structure runtime or recourse bound.

use thiserror::Error;

use super::{
    DynamicActiveBranchProjectionError, DynamicActiveBranchProjectionInput,
    DynamicActiveBranchProjectionTraceEvent, DynamicCoreGraphStageBatch,
    DynamicCoreGraphStageUpdate, DynamicLevelEdge, DynamicLevelGraphSnapshot,
    DynamicLevelProjectionState, DynamicSparseCoreCollectionStageEventKind,
    DynamicSparseCoreCollectionStageTraceResult, DynamicTreeChainEpochError,
    DynamicTreeChainEpochMetrics, DynamicTreeChainEpochMwuPlan, DynamicTreeChainEpochOperation,
    DynamicTreeChainEpochSnapshot, DynamicTreeChainEpochTransitionResult,
    DynamicTreeChainPropagationError, DynamicTreeChainPropagationInput,
    DynamicTreeChainPropagationTraceResult, check_dynamic_level_projection_trace,
    check_dynamic_level_stage_adapter_trace, check_dynamic_sparse_core_collection_stage_trace,
    check_dynamic_tree_chain_epoch_mwu_plan, check_dynamic_tree_chain_propagation_trace,
    execute_dynamic_tree_chain_epoch_transition, initialize_dynamic_level_projection,
    initialize_dynamic_tree_chain_epochs, trace_dynamic_level_projection,
    trace_dynamic_level_stage_adapter, trace_dynamic_sparse_core_collection_stages,
};

const CATALOG_ID: &str = "dynamic-tree-chain-epoch-runtime";
const MAX_LEVELS: usize = 8;

/// One current level's epoch-start input and epoch-local source history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeLevel {
    /// Monotone epoch identity shared with the materialized epoch snapshot.
    pub epoch: u64,
    /// Collection and active branch at this epoch's local stage zero.
    pub input: DynamicActiveBranchProjectionInput,
    /// Exact consecutive local batches `outer_stage = 1..=len`.
    pub batches: Vec<DynamicCoreGraphStageBatch>,
    /// Fresh or persistent active-branch projection checkpoint.
    pub projection_base: DynamicLevelProjectionState,
    /// Number of collection batches already represented by `projection_base`.
    pub projection_start_stage: usize,
}

/// Runtime-only counters spanning multiple suffix epochs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeMetrics {
    /// Root stages atomically applied after runtime initialization.
    pub root_stages: u64,
    /// Shift/Rebuild plans atomically applied after initialization.
    pub epoch_transitions: u64,
    /// Successful public runtime state publications.
    pub state_transitions: u64,
}

/// Complete continued runtime state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeState {
    /// Nonempty root-to-leaf level sequence.
    pub levels: Vec<DynamicTreeChainEpochRuntimeLevel>,
    /// Next never-published epoch identity.
    pub next_epoch: u64,
    /// Shift/Rebuild counters reflected in materialized epoch state.
    pub epoch_metrics: DynamicTreeChainEpochMetrics,
    /// Cross-epoch continued-runtime counters.
    pub metrics: DynamicTreeChainEpochRuntimeMetrics,
}

/// One checked collection history and its possibly rebased active projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeLevelTrace {
    /// Checked full all-branch collection history for the current epoch.
    pub collection_trace: DynamicSparseCoreCollectionStageTraceResult,
    /// Projection checkpoint stored by the runtime level.
    pub base_projection: DynamicLevelProjectionState,
    /// First collection stage projected after the checkpoint.
    pub projection_start_stage: usize,
    /// Checked projection/adapter boundaries for the history suffix.
    pub events: Vec<DynamicActiveBranchProjectionTraceEvent>,
    /// Current active child projection.
    pub final_projection: DynamicLevelProjectionState,
    /// Child batches emitted since the projection checkpoint.
    pub child_batches: Vec<DynamicCoreGraphStageBatch>,
}

/// Independently checked materialization of all current level histories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeMaterialization {
    /// One checked all-branch/active-projection transcript per level.
    pub level_traces: Vec<DynamicTreeChainEpochRuntimeLevelTrace>,
    /// Current normalized epoch snapshot derived from those transcripts.
    pub epoch_snapshot: DynamicTreeChainEpochSnapshot,
}

/// One atomic continued-runtime request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicTreeChainEpochRuntimeOperation {
    /// Apply one new root source stage and propagate it across current epochs.
    ApplyRootStage {
        /// Ordered root records; the runtime assigns the next root-local stage.
        updates: Vec<DynamicCoreGraphStageUpdate>,
    },
    /// Apply a checked Shift/Rebuild plan and publish its fresh suffix inputs.
    ApplyEpochPlan {
        /// MWU-backed atomic suffix plan.
        plan: Box<DynamicTreeChainEpochMwuPlan>,
    },
}

/// Exact fast transition result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeResult {
    /// Atomically published continued state.
    pub final_state: DynamicTreeChainEpochRuntimeState,
    /// Checked materialization of `final_state`.
    pub final_materialization: DynamicTreeChainEpochRuntimeMaterialization,
    /// Batch emitted beyond the final level for a root stage, otherwise `None`.
    pub terminal_batch: Option<DynamicCoreGraphStageBatch>,
}

/// One fully reversible continued-runtime publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Exact requested transition.
    pub operation: DynamicTreeChainEpochRuntimeOperation,
    /// State before candidate construction.
    pub before: DynamicTreeChainEpochRuntimeState,
    /// Independently checkable current materialization before the request.
    pub before_materialization: DynamicTreeChainEpochRuntimeMaterialization,
    /// Atomically published state.
    pub after: DynamicTreeChainEpochRuntimeState,
    /// Independently checkable materialization after publication.
    pub after_materialization: DynamicTreeChainEpochRuntimeMaterialization,
    /// Final child batch for a stage request, otherwise `None`.
    pub terminal_batch: Option<DynamicCoreGraphStageBatch>,
}

/// Complete continued-runtime transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochRuntimeTraceResult {
    /// Single atomic publication boundary.
    pub event: DynamicTreeChainEpochRuntimeTraceEvent,
    /// Exact fast result.
    pub result: DynamicTreeChainEpochRuntimeResult,
}

/// Explicit bounded continued-runtime failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeChainEpochRuntimeError {
    /// Level history, local stage, operation, or handoff is malformed.
    #[error("dynamic tree-chain epoch runtime input is invalid")]
    InvalidInput,
    /// Initial fixed-epoch propagation failed verification.
    #[error("dynamic tree-chain epoch runtime propagation failed: {0}")]
    Propagation(#[from] DynamicTreeChainPropagationError),
    /// One level history or active projection failed.
    #[error("dynamic tree-chain epoch runtime level {level} failed: {error}")]
    Level {
        /// Root-based current level.
        level: usize,
        /// Checked component failure.
        #[source]
        error: DynamicActiveBranchProjectionError,
    },
    /// Shift/Rebuild epoch validation or publication failed.
    #[error("dynamic tree-chain epoch runtime lifecycle failed: {0}")]
    Epoch(#[from] DynamicTreeChainEpochError),
    /// Checked local stage or runtime counter overflowed.
    #[error("dynamic tree-chain epoch runtime arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied runtime transcript is not the exact independent replay.
    #[error("dynamic tree-chain epoch runtime trace verification failed")]
    TraceVerification,
}

/// Converts a checked fixed-epoch propagation into a continued runtime state.
///
/// # Errors
///
/// Rejects a forged propagation transcript, inconsistent child stream, invalid
/// local history, or disagreement with the existing epoch materializer.
pub fn initialize_dynamic_tree_chain_epoch_runtime(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicTreeChainPropagationTraceResult,
) -> Result<DynamicTreeChainEpochRuntimeState, DynamicTreeChainEpochRuntimeError> {
    check_dynamic_tree_chain_propagation_trace(input, root_batches, trace)?;
    let expected_epoch = initialize_dynamic_tree_chain_epochs(input, root_batches, trace)?;
    let mut batches = root_batches.to_vec();
    let mut levels = Vec::with_capacity(input.levels.len());
    for (index, level) in input.levels.iter().enumerate() {
        levels.push(DynamicTreeChainEpochRuntimeLevel {
            epoch: 0,
            input: level.clone(),
            batches: batches.clone(),
            projection_base: trace.level_traces[index].base_projection.clone(),
            projection_start_stage: 0,
        });
        batches.clone_from(&trace.level_traces[index].result.child_batches);
    }
    let state = DynamicTreeChainEpochRuntimeState {
        levels,
        next_epoch: 1,
        epoch_metrics: DynamicTreeChainEpochMetrics::default(),
        metrics: DynamicTreeChainEpochRuntimeMetrics::default(),
    };
    let materialized = materialize_state(&state)?;
    if materialized.epoch_snapshot != expected_epoch {
        return Err(DynamicTreeChainEpochRuntimeError::InvalidInput);
    }
    Ok(state)
}

/// Materializes and checks every current epoch-local level history.
///
/// # Errors
///
/// Rejects malformed state, component failure, or current graph handoff drift.
pub fn materialize_dynamic_tree_chain_epoch_runtime(
    state: &DynamicTreeChainEpochRuntimeState,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicTreeChainEpochRuntimeError> {
    materialize_state(state)
}

/// Independently checks a supplied current-epoch materialization.
///
/// This is the read-only verification boundary used by higher-level query and
/// flow-maintenance compositions. It replays every collection, projection,
/// adapter, local-stage, and inter-level handoff without publishing runtime
/// state.
///
/// # Errors
///
/// Rejects malformed runtime state or any supplied component transcript,
/// projection checkpoint, graph handoff, epoch snapshot, or metric drift.
pub fn check_dynamic_tree_chain_epoch_runtime_materialization(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<(), DynamicTreeChainEpochRuntimeError> {
    check_materialization(state, materialization)
}

/// Applies one root stage or checked epoch plan without retaining an event.
///
/// # Errors
///
/// Rejects any invalid component history, root stage, plan, suffix handoff,
/// arithmetic overflow, or final materialization drift. The borrowed state is
/// never mutated on failure.
pub fn execute_dynamic_tree_chain_epoch_runtime(
    initial: &DynamicTreeChainEpochRuntimeState,
    operation: &DynamicTreeChainEpochRuntimeOperation,
) -> Result<DynamicTreeChainEpochRuntimeResult, DynamicTreeChainEpochRuntimeError> {
    let before = materialize_state(initial)?;
    let (final_state, terminal_batch) = apply_operation(initial, &before, operation)?;
    let final_materialization = materialize_state(&final_state)?;
    Ok(DynamicTreeChainEpochRuntimeResult {
        final_state,
        final_materialization,
        terminal_batch,
    })
}

/// Applies one continued transition and records both checked materializations.
///
/// # Errors
///
/// Returns the same fail-closed errors as
/// [`execute_dynamic_tree_chain_epoch_runtime`].
pub fn trace_dynamic_tree_chain_epoch_runtime(
    initial: &DynamicTreeChainEpochRuntimeState,
    operation: &DynamicTreeChainEpochRuntimeOperation,
) -> Result<DynamicTreeChainEpochRuntimeTraceResult, DynamicTreeChainEpochRuntimeError> {
    let before_materialization = materialize_state(initial)?;
    let result = execute_dynamic_tree_chain_epoch_runtime(initial, operation)?;
    let trace = DynamicTreeChainEpochRuntimeTraceResult {
        event: DynamicTreeChainEpochRuntimeTraceEvent {
            catalog_id: CATALOG_ID,
            operation: operation.clone(),
            before: initial.clone(),
            before_materialization,
            after: result.final_state.clone(),
            after_materialization: result.final_materialization.clone(),
            terminal_batch: result.terminal_batch.clone(),
        },
        result,
    };
    check_dynamic_tree_chain_epoch_runtime_trace(initial, operation, &trace)?;
    Ok(trace)
}

/// Independently checks component transcripts, state mutation, and handoffs.
///
/// The checker never invokes the runtime execution path. It validates each
/// supplied materialization through the component checkers, reconstructs the
/// exact state/history mutation, and checks the resulting epoch snapshot.
///
/// # Errors
///
/// Rejects event identity, component transcript, local numbering, history,
/// handoff, epoch, metric, terminal batch, or result drift.
pub fn check_dynamic_tree_chain_epoch_runtime_trace(
    initial: &DynamicTreeChainEpochRuntimeState,
    operation: &DynamicTreeChainEpochRuntimeOperation,
    trace: &DynamicTreeChainEpochRuntimeTraceResult,
) -> Result<(), DynamicTreeChainEpochRuntimeError> {
    if trace.event.catalog_id != CATALOG_ID
        || trace.event.operation != *operation
        || trace.event.before != *initial
        || trace.result.final_state != trace.event.after
        || trace.result.final_materialization != trace.event.after_materialization
        || trace.result.terminal_batch != trace.event.terminal_batch
    {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    check_materialization(initial, &trace.event.before_materialization)?;
    let (expected_state, terminal_batch) = audit_apply_operation(
        initial,
        &trace.event.before_materialization,
        operation,
        &trace.event.after_materialization,
    )?;
    if trace.event.after != expected_state || trace.event.terminal_batch != terminal_batch {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    check_materialization(&expected_state, &trace.event.after_materialization)
}

fn apply_operation(
    initial: &DynamicTreeChainEpochRuntimeState,
    before: &DynamicTreeChainEpochRuntimeMaterialization,
    operation: &DynamicTreeChainEpochRuntimeOperation,
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        Option<DynamicCoreGraphStageBatch>,
    ),
    DynamicTreeChainEpochRuntimeError,
> {
    match operation {
        DynamicTreeChainEpochRuntimeOperation::ApplyRootStage { updates } => {
            apply_root_stage(initial, updates)
        }
        DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan { plan } => {
            apply_epoch_plan(initial, &before.epoch_snapshot, plan)
        }
    }
}

fn apply_root_stage(
    initial: &DynamicTreeChainEpochRuntimeState,
    updates: &[DynamicCoreGraphStageUpdate],
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        Option<DynamicCoreGraphStageBatch>,
    ),
    DynamicTreeChainEpochRuntimeError,
> {
    if updates.is_empty() {
        return Err(DynamicTreeChainEpochRuntimeError::InvalidInput);
    }
    let mut candidate = initial.clone();
    let root_stage = next_local_stage(&candidate.levels[0])?;
    let mut batch = DynamicCoreGraphStageBatch {
        outer_stage: root_stage,
        updates: updates.to_vec(),
    };
    for (index, level) in candidate.levels.iter_mut().enumerate() {
        batch.outer_stage = next_local_stage(level)?;
        level.batches.push(batch);
        let trace = materialize_level(level, index)?;
        batch = trace
            .child_batches
            .last()
            .cloned()
            .ok_or(DynamicTreeChainEpochRuntimeError::InvalidInput)?;
    }
    candidate.metrics.root_stages = increment(candidate.metrics.root_stages)?;
    candidate.metrics.state_transitions = increment(candidate.metrics.state_transitions)?;
    Ok((candidate, Some(batch)))
}

fn apply_epoch_plan(
    initial: &DynamicTreeChainEpochRuntimeState,
    before: &DynamicTreeChainEpochSnapshot,
    plan: &DynamicTreeChainEpochMwuPlan,
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        Option<DynamicCoreGraphStageBatch>,
    ),
    DynamicTreeChainEpochRuntimeError,
> {
    check_dynamic_tree_chain_epoch_mwu_plan(before, plan)?;
    let transition = execute_dynamic_tree_chain_epoch_transition(before, &plan.operation)?;
    let mut candidate = replace_runtime_suffix(initial, plan, &transition)?;
    candidate.metrics.epoch_transitions = increment(candidate.metrics.epoch_transitions)?;
    candidate.metrics.state_transitions = increment(candidate.metrics.state_transitions)?;
    Ok((candidate, None))
}

fn replace_runtime_suffix(
    initial: &DynamicTreeChainEpochRuntimeState,
    plan: &DynamicTreeChainEpochMwuPlan,
    transition: &DynamicTreeChainEpochTransitionResult,
) -> Result<DynamicTreeChainEpochRuntimeState, DynamicTreeChainEpochRuntimeError> {
    let mut candidate = initial.clone();
    let (start, replacement) = match &plan.operation {
        DynamicTreeChainEpochOperation::Shift {
            level,
            replacement_suffix,
        } => {
            let start = level
                .checked_add(1)
                .ok_or(DynamicTreeChainEpochRuntimeError::ArithmeticOverflow)?;
            let selected = transition
                .final_snapshot
                .levels
                .get(*level)
                .ok_or(DynamicTreeChainEpochRuntimeError::InvalidInput)?
                .active_branch;
            let shifted = candidate
                .levels
                .get_mut(*level)
                .ok_or(DynamicTreeChainEpochRuntimeError::InvalidInput)?;
            shifted.input.active_branch = selected;
            let selected_snapshot = transition.final_snapshot.levels[*level]
                .branch_snapshots
                .get(selected)
                .ok_or(DynamicTreeChainEpochRuntimeError::InvalidInput)?;
            shifted.projection_base = initialize_dynamic_level_projection(selected_snapshot)
                .map_err(DynamicActiveBranchProjectionError::from)
                .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
                    level: *level,
                    error,
                })?;
            shifted.projection_start_stage = shifted.batches.len();
            (start, replacement_suffix)
        }
        DynamicTreeChainEpochOperation::Rebuild {
            level,
            replacement_suffix,
        } => (*level, replacement_suffix),
    };
    if replacement.len() != transition.final_snapshot.levels.len() - start {
        return Err(DynamicTreeChainEpochRuntimeError::InvalidInput);
    }
    candidate.levels.truncate(start);
    for (offset, input) in replacement.iter().enumerate() {
        let materialized = &transition.final_snapshot.levels[start + offset];
        let epoch = materialized.epoch;
        let selected = materialized
            .branch_snapshots
            .get(input.active_branch)
            .ok_or(DynamicTreeChainEpochRuntimeError::InvalidInput)?;
        candidate.levels.push(DynamicTreeChainEpochRuntimeLevel {
            epoch,
            input: input.clone(),
            batches: Vec::new(),
            projection_base: initialize_dynamic_level_projection(selected)
                .map_err(DynamicActiveBranchProjectionError::from)
                .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
                    level: start + offset,
                    error,
                })?,
            projection_start_stage: 0,
        });
    }
    candidate.next_epoch = transition.final_snapshot.next_epoch;
    candidate.epoch_metrics = transition.final_snapshot.metrics;
    Ok(candidate)
}

fn materialize_state(
    state: &DynamicTreeChainEpochRuntimeState,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicTreeChainEpochRuntimeError> {
    validate_state_shape(state, false)?;
    let mut traces = Vec::with_capacity(state.levels.len());
    for (index, level) in state.levels.iter().enumerate() {
        traces.push(materialize_level(level, index)?);
    }
    assemble_materialization(state, traces, false)
}

fn materialize_level(
    level: &DynamicTreeChainEpochRuntimeLevel,
    level_index: usize,
) -> Result<DynamicTreeChainEpochRuntimeLevelTrace, DynamicTreeChainEpochRuntimeError> {
    let collection_trace =
        trace_dynamic_sparse_core_collection_stages(&level.input.collection, &level.batches)
            .map_err(DynamicActiveBranchProjectionError::from)
            .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
                level: level_index,
                error,
            })?;
    let mut projection = level.projection_base.clone();
    let mut events = Vec::with_capacity(level.batches.len() - level.projection_start_stage);
    let mut child_batches = Vec::with_capacity(events.capacity());
    for index in level.projection_start_stage..level.batches.len() {
        let (before, after, updates) =
            runtime_active_stage(&collection_trace, index, level.input.active_branch, false)?;
        let projection_trace = trace_dynamic_level_projection(&projection, before, after, updates)
            .map_err(DynamicActiveBranchProjectionError::from)
            .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
                level: level_index,
                error,
            })?;
        let adapter_trace = trace_dynamic_level_stage_adapter(&projection_trace.result.batch)
            .map_err(DynamicActiveBranchProjectionError::from)
            .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
                level: level_index,
                error,
            })?;
        child_batches.push(adapter_trace.result.batch.clone());
        projection = projection_trace.result.final_state.clone();
        events.push(DynamicActiveBranchProjectionTraceEvent {
            catalog_id: "dynamic-active-branch-projection",
            active_branch: level.input.active_branch,
            projection_trace,
            adapter_trace,
        });
    }
    Ok(DynamicTreeChainEpochRuntimeLevelTrace {
        collection_trace,
        base_projection: level.projection_base.clone(),
        projection_start_stage: level.projection_start_stage,
        events,
        final_projection: projection,
        child_batches,
    })
}

fn check_materialization(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<(), DynamicTreeChainEpochRuntimeError> {
    validate_state_shape(state, true)?;
    if materialization.level_traces.len() != state.levels.len() {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    for (index, (level, trace)) in state
        .levels
        .iter()
        .zip(&materialization.level_traces)
        .enumerate()
    {
        check_runtime_level_trace(level, index, trace)?;
    }
    let expected = assemble_materialization(state, materialization.level_traces.clone(), true)?;
    if &expected != materialization {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    Ok(())
}

fn check_runtime_level_trace(
    level: &DynamicTreeChainEpochRuntimeLevel,
    level_index: usize,
    trace: &DynamicTreeChainEpochRuntimeLevelTrace,
) -> Result<(), DynamicTreeChainEpochRuntimeError> {
    check_dynamic_sparse_core_collection_stage_trace(
        &level.input.collection,
        &level.batches,
        &trace.collection_trace,
    )
    .map_err(DynamicActiveBranchProjectionError::from)
    .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
        level: level_index,
        error,
    })?;
    if trace.projection_start_stage != level.projection_start_stage
        || trace.base_projection != level.projection_base
        || trace.events.len() != level.batches.len() - level.projection_start_stage
    {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    let checkpoint_sparse = if level.projection_start_stage == 0 {
        trace
            .collection_trace
            .base_snapshot
            .branch_snapshots
            .get(level.input.active_branch)
    } else {
        trace
            .collection_trace
            .events
            .get(level.projection_start_stage - 1)
            .and_then(|event| event.after.branch_snapshots.get(level.input.active_branch))
    }
    .ok_or(DynamicTreeChainEpochRuntimeError::TraceVerification)?;
    let fresh = initialize_dynamic_level_projection(checkpoint_sparse)
        .map_err(DynamicActiveBranchProjectionError::from)
        .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
            level: level_index,
            error,
        })?;
    if fresh != level.projection_base {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    let mut projection = level.projection_base.clone();
    let mut child_batches = Vec::with_capacity(trace.events.len());
    for (offset, event) in trace.events.iter().enumerate() {
        let index = level
            .projection_start_stage
            .checked_add(offset)
            .ok_or(DynamicTreeChainEpochRuntimeError::TraceVerification)?;
        if event.catalog_id != "dynamic-active-branch-projection"
            || event.active_branch != level.input.active_branch
        {
            return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
        }
        let (before, after, updates) = runtime_active_stage(
            &trace.collection_trace,
            index,
            level.input.active_branch,
            true,
        )?;
        check_dynamic_level_projection_trace(
            &projection,
            before,
            after,
            updates,
            &event.projection_trace,
        )
        .map_err(DynamicActiveBranchProjectionError::from)
        .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
            level: level_index,
            error,
        })?;
        check_dynamic_level_stage_adapter_trace(
            &event.projection_trace.result.batch,
            &event.adapter_trace,
        )
        .map_err(DynamicActiveBranchProjectionError::from)
        .map_err(|error| DynamicTreeChainEpochRuntimeError::Level {
            level: level_index,
            error,
        })?;
        let child = event.adapter_trace.result.batch.clone();
        if child.outer_stage != level.batches[index].outer_stage {
            return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
        }
        child_batches.push(child);
        projection = event.projection_trace.result.final_state.clone();
    }
    if trace.final_projection != projection || trace.child_batches != child_batches {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    Ok(())
}

type RuntimeActiveStage<'a> = (
    &'a super::DynamicSparseCoreSnapshot,
    &'a super::DynamicSparseCoreSnapshot,
    &'a [super::DynamicSparseCoreUpdate],
);

fn runtime_active_stage(
    trace: &DynamicSparseCoreCollectionStageTraceResult,
    index: usize,
    active: usize,
    audit: bool,
) -> Result<RuntimeActiveStage<'_>, DynamicTreeChainEpochRuntimeError> {
    let failure = || {
        if audit {
            DynamicTreeChainEpochRuntimeError::TraceVerification
        } else {
            DynamicTreeChainEpochRuntimeError::InvalidInput
        }
    };
    let event = trace.events.get(index).ok_or_else(&failure)?;
    let DynamicSparseCoreCollectionStageEventKind::Updated { branch_updates, .. } = &event.kind
    else {
        return Err(failure());
    };
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

fn assemble_materialization(
    state: &DynamicTreeChainEpochRuntimeState,
    traces: Vec<DynamicTreeChainEpochRuntimeLevelTrace>,
    audit: bool,
) -> Result<DynamicTreeChainEpochRuntimeMaterialization, DynamicTreeChainEpochRuntimeError> {
    let failure = || {
        if audit {
            DynamicTreeChainEpochRuntimeError::TraceVerification
        } else {
            DynamicTreeChainEpochRuntimeError::InvalidInput
        }
    };
    let mut levels = Vec::with_capacity(state.levels.len());
    let mut previous_projection: Option<DynamicLevelGraphSnapshot> = None;
    for (runtime, trace) in state.levels.iter().zip(&traces) {
        let source_graph = current_source_graph(trace, audit)?;
        if let Some(expected) = &previous_projection
            && normalize_graph(expected.clone()) != source_graph
        {
            return Err(failure());
        }
        levels.push(super::DynamicTreeChainEpochLevel {
            epoch: runtime.epoch,
            source_graph,
            branch_snapshots: trace
                .collection_trace
                .result
                .final_snapshot
                .branch_snapshots
                .clone(),
            active_branch: runtime.input.active_branch,
        });
        previous_projection = Some(trace.final_projection.graph.clone());
    }
    Ok(DynamicTreeChainEpochRuntimeMaterialization {
        level_traces: traces,
        epoch_snapshot: DynamicTreeChainEpochSnapshot {
            levels,
            next_epoch: state.next_epoch,
            metrics: state.epoch_metrics,
        },
    })
}

fn current_source_graph(
    trace: &DynamicTreeChainEpochRuntimeLevelTrace,
    audit: bool,
) -> Result<DynamicLevelGraphSnapshot, DynamicTreeChainEpochRuntimeError> {
    let failure = || {
        if audit {
            DynamicTreeChainEpochRuntimeError::TraceVerification
        } else {
            DynamicTreeChainEpochRuntimeError::InvalidInput
        }
    };
    let branch = trace
        .collection_trace
        .branch_traces
        .first()
        .ok_or_else(&failure)?;
    let forest = &branch.core_trace.forest_trace.result.final_snapshot;
    let core = &branch.core_trace.result.final_snapshot;
    if forest.edge_slots.len() != core.source_gradients.len() {
        return Err(failure());
    }
    let mut edge_slots = Vec::with_capacity(forest.edge_slots.len());
    for (edge, gradient) in forest.edge_slots.iter().zip(&core.source_gradients) {
        match (edge, gradient) {
            (Some(edge), Some(gradient)) => edge_slots.push(Some(DynamicLevelEdge {
                edge: edge.edge,
                from: edge.from,
                to: edge.to,
                length: edge.length.clone(),
                gradient: gradient.clone(),
            })),
            (None, None) => edge_slots.push(None),
            _ => return Err(failure()),
        }
    }
    Ok(DynamicLevelGraphSnapshot {
        active_node_count: forest.active_node_count,
        edge_slots,
        stage: 0,
    })
}

fn audit_apply_operation(
    initial: &DynamicTreeChainEpochRuntimeState,
    before: &DynamicTreeChainEpochRuntimeMaterialization,
    operation: &DynamicTreeChainEpochRuntimeOperation,
    after: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        Option<DynamicCoreGraphStageBatch>,
    ),
    DynamicTreeChainEpochRuntimeError,
> {
    match operation {
        DynamicTreeChainEpochRuntimeOperation::ApplyRootStage { updates } => {
            audit_apply_root_stage(initial, updates, after)
        }
        DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan { plan } => {
            check_dynamic_tree_chain_epoch_mwu_plan(&before.epoch_snapshot, plan)?;
            let transition = super::trace_dynamic_tree_chain_epoch_transition(
                &before.epoch_snapshot,
                &plan.operation,
            )?;
            super::check_dynamic_tree_chain_epoch_trace(
                &before.epoch_snapshot,
                &plan.operation,
                &transition,
            )?;
            let mut candidate = replace_runtime_suffix(initial, plan, &transition.result)?;
            candidate.metrics.epoch_transitions =
                audit_increment(candidate.metrics.epoch_transitions)?;
            candidate.metrics.state_transitions =
                audit_increment(candidate.metrics.state_transitions)?;
            Ok((candidate, None))
        }
    }
}

fn audit_apply_root_stage(
    initial: &DynamicTreeChainEpochRuntimeState,
    updates: &[DynamicCoreGraphStageUpdate],
    after: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<
    (
        DynamicTreeChainEpochRuntimeState,
        Option<DynamicCoreGraphStageBatch>,
    ),
    DynamicTreeChainEpochRuntimeError,
> {
    if updates.is_empty() || after.level_traces.len() != initial.levels.len() {
        return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
    }
    let mut candidate = initial.clone();
    let mut batch = DynamicCoreGraphStageBatch {
        outer_stage: audit_next_local_stage(&candidate.levels[0])?,
        updates: updates.to_vec(),
    };
    for (index, level) in candidate.levels.iter_mut().enumerate() {
        batch.outer_stage = audit_next_local_stage(level)?;
        level.batches.push(batch);
        let trace = &after.level_traces[index];
        if trace.events.len() != level.batches.len() - level.projection_start_stage {
            return Err(DynamicTreeChainEpochRuntimeError::TraceVerification);
        }
        batch = trace
            .child_batches
            .last()
            .cloned()
            .ok_or(DynamicTreeChainEpochRuntimeError::TraceVerification)?;
    }
    candidate.metrics.root_stages = audit_increment(candidate.metrics.root_stages)?;
    candidate.metrics.state_transitions = audit_increment(candidate.metrics.state_transitions)?;
    Ok((candidate, Some(batch)))
}

fn validate_state_shape(
    state: &DynamicTreeChainEpochRuntimeState,
    audit: bool,
) -> Result<(), DynamicTreeChainEpochRuntimeError> {
    let failure = || {
        if audit {
            DynamicTreeChainEpochRuntimeError::TraceVerification
        } else {
            DynamicTreeChainEpochRuntimeError::InvalidInput
        }
    };
    if state.levels.is_empty() || state.levels.len() > MAX_LEVELS || state.next_epoch == 0 {
        return Err(failure());
    }
    for level in &state.levels {
        if level.epoch >= state.next_epoch
            || level.input.collection.branches.is_empty()
            || level.input.active_branch >= level.input.collection.branches.len()
            || level.projection_start_stage > level.batches.len()
            || level.projection_base.graph.stage
                != u64::try_from(level.projection_start_stage).map_err(|_| failure())?
        {
            return Err(failure());
        }
        for (index, batch) in level.batches.iter().enumerate() {
            let expected = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(&failure)?;
            if batch.outer_stage != expected {
                return Err(failure());
            }
        }
    }
    Ok(())
}

fn next_local_stage(
    level: &DynamicTreeChainEpochRuntimeLevel,
) -> Result<u64, DynamicTreeChainEpochRuntimeError> {
    u64::try_from(level.batches.len())
        .map_err(|_| DynamicTreeChainEpochRuntimeError::ArithmeticOverflow)?
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochRuntimeError::ArithmeticOverflow)
}

fn audit_next_local_stage(
    level: &DynamicTreeChainEpochRuntimeLevel,
) -> Result<u64, DynamicTreeChainEpochRuntimeError> {
    u64::try_from(level.batches.len())
        .map_err(|_| DynamicTreeChainEpochRuntimeError::TraceVerification)?
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochRuntimeError::TraceVerification)
}

fn normalize_graph(mut graph: DynamicLevelGraphSnapshot) -> DynamicLevelGraphSnapshot {
    graph.stage = 0;
    graph
}

fn increment(value: u64) -> Result<u64, DynamicTreeChainEpochRuntimeError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochRuntimeError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicTreeChainEpochRuntimeError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochRuntimeError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;
    use crate::algorithms::{
        DynamicCoreGraphStageEdge, DynamicMwuCollectionBridgeConfig, ShiftedTreeChainEdge,
        ShiftedTreeChainGraph, initialize_dynamic_level_projection,
        plan_dynamic_tree_chain_rebuild_from_mwu, plan_dynamic_tree_chain_shift_from_mwu,
        trace_dynamic_mwu_sparse_core_collection, trace_dynamic_tree_chain_propagation,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn root_graph() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                shifted_edge(0, 0, 1, 1, 2),
                shifted_edge(1, 1, 2, 1, 3),
                shifted_edge(2, 2, 3, 1, 5),
                shifted_edge(3, 0, 3, 2, 7),
            ],
        }
    }

    fn shifted_edge(
        source_edge: usize,
        from: usize,
        to: usize,
        length: i64,
        gradient: i64,
    ) -> ShiftedTreeChainEdge {
        ShiftedTreeChainEdge {
            source_edge,
            from,
            to,
            length: rational(length),
            gradient: rational(gradient),
        }
    }

    fn config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: 5,
        }
    }

    fn shifted_from_level(graph: &DynamicLevelGraphSnapshot) -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: graph.active_node_count,
            edges: graph
                .edge_slots
                .iter()
                .flatten()
                .map(|edge| ShiftedTreeChainEdge {
                    source_edge: edge.edge,
                    from: edge.from,
                    to: edge.to,
                    length: edge.length.clone(),
                    gradient: edge.gradient.clone(),
                })
                .collect(),
        }
    }

    fn initial_runtime() -> DynamicTreeChainEpochRuntimeState {
        let root = trace_dynamic_mwu_sparse_core_collection(&root_graph(), config())
            .expect("root initializer");
        let root_input = DynamicActiveBranchProjectionInput {
            collection: root.result.collection,
            active_branch: 0,
        };
        let root_snapshot = &root.result.initialized.final_snapshot.branch_snapshots[0];
        let child_graph = initialize_dynamic_level_projection(root_snapshot)
            .expect("root projection")
            .graph;
        let child =
            trace_dynamic_mwu_sparse_core_collection(&shifted_from_level(&child_graph), config())
                .expect("child initializer");
        let input = DynamicTreeChainPropagationInput {
            levels: vec![
                root_input,
                DynamicActiveBranchProjectionInput {
                    collection: child.result.collection,
                    active_branch: 0,
                },
            ],
        };
        let trace = trace_dynamic_tree_chain_propagation(&input, &[]).expect("propagation");
        initialize_dynamic_tree_chain_epoch_runtime(&input, &[], &trace).expect("runtime")
    }

    fn stage_edge(edge: &DynamicLevelEdge) -> DynamicCoreGraphStageEdge {
        DynamicCoreGraphStageEdge {
            edge: edge.edge,
            from: edge.from,
            to: edge.to,
            length: edge.length.clone(),
            gradient: edge.gradient.clone(),
        }
    }

    fn insert_operation() -> DynamicTreeChainEpochRuntimeOperation {
        DynamicTreeChainEpochRuntimeOperation::ApplyRootStage {
            updates: vec![DynamicCoreGraphStageUpdate::Insert {
                edge: DynamicCoreGraphStageEdge {
                    edge: 4,
                    from: 0,
                    to: 2,
                    length: rational(3),
                    gradient: rational(-4),
                },
            }],
        }
    }

    fn gradient_operation(
        state: &DynamicTreeChainEpochRuntimeState,
        edge: usize,
    ) -> DynamicTreeChainEpochRuntimeOperation {
        let current = materialize_dynamic_tree_chain_epoch_runtime(state)
            .expect("materialize")
            .epoch_snapshot
            .levels[0]
            .source_graph
            .edge_slots[edge]
            .clone()
            .expect("edge");
        let before = stage_edge(&current);
        let mut after = before.clone();
        after.gradient += rational(1);
        DynamicTreeChainEpochRuntimeOperation::ApplyRootStage {
            updates: vec![DynamicCoreGraphStageUpdate::ReplaceAttributes { before, after }],
        }
    }

    #[test]
    fn stage_shift_stage_uses_preserved_and_fresh_local_histories() {
        let initial = initial_runtime();
        let first = trace_dynamic_tree_chain_epoch_runtime(&initial, &insert_operation())
            .expect("first stage");
        assert_eq!(first.result.final_state.levels[0].batches[0].outer_stage, 1);
        assert_eq!(first.result.final_state.levels[1].batches[0].outer_stage, 1);

        let current = &first.result.final_state;
        let epoch = &first.result.final_materialization.epoch_snapshot;
        let plan =
            plan_dynamic_tree_chain_shift_from_mwu(epoch, 0, &[config()]).expect("shift plan");
        let shift = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
            plan: Box::new(plan),
        };
        let shifted = trace_dynamic_tree_chain_epoch_runtime(current, &shift).expect("shift");
        assert_eq!(shifted.result.final_state.levels[0].batches.len(), 1);
        assert!(shifted.result.final_state.levels[1].batches.is_empty());
        assert_eq!(shifted.result.final_state.levels[1].epoch, epoch.next_epoch);

        let continued_operation = gradient_operation(&shifted.result.final_state, 0);
        let continued = trace_dynamic_tree_chain_epoch_runtime(
            &shifted.result.final_state,
            &continued_operation,
        )
        .expect("continued stage");
        assert_eq!(continued.result.final_state.levels[0].batches.len(), 2);
        assert_eq!(
            continued.result.final_state.levels[0].batches[1].outer_stage,
            2
        );
        assert_eq!(continued.result.final_state.levels[1].batches.len(), 1);
        assert_eq!(
            continued.result.final_state.levels[1].batches[0].outer_stage,
            1
        );
        check_dynamic_tree_chain_epoch_runtime_trace(
            &shifted.result.final_state,
            &continued_operation,
            &continued,
        )
        .expect("continued check");
    }

    #[test]
    fn rebuild_resets_all_histories_then_accepts_a_new_stage() {
        let initial = initial_runtime();
        let staged =
            execute_dynamic_tree_chain_epoch_runtime(&initial, &insert_operation()).expect("stage");
        let plan = plan_dynamic_tree_chain_rebuild_from_mwu(
            &staged.final_materialization.epoch_snapshot,
            0,
            &[config(), config()],
        )
        .expect("rebuild plan");
        let operation = DynamicTreeChainEpochRuntimeOperation::ApplyEpochPlan {
            plan: Box::new(plan),
        };
        let rebuilt = trace_dynamic_tree_chain_epoch_runtime(&staged.final_state, &operation)
            .expect("rebuild");
        assert!(
            rebuilt
                .result
                .final_state
                .levels
                .iter()
                .all(|level| level.batches.is_empty())
        );
        assert_eq!(rebuilt.result.final_state.epoch_metrics.rebuilds, 1);

        let next_operation = gradient_operation(&rebuilt.result.final_state, 1);
        let next =
            execute_dynamic_tree_chain_epoch_runtime(&rebuilt.result.final_state, &next_operation)
                .expect("post-rebuild stage");
        assert!(
            next.final_state
                .levels
                .iter()
                .all(|level| level.batches.len() == 1 && level.batches[0].outer_stage == 1)
        );
    }

    #[test]
    fn checker_rejects_child_local_stage_and_materialization_tampering() {
        let initial = initial_runtime();
        let operation = insert_operation();
        let trace = trace_dynamic_tree_chain_epoch_runtime(&initial, &operation).expect("trace");

        let mut tampered = trace.clone();
        tampered.event.after.levels[1].batches[0].outer_stage = 2;
        tampered.result.final_state = tampered.event.after.clone();
        assert_eq!(
            check_dynamic_tree_chain_epoch_runtime_trace(&initial, &operation, &tampered),
            Err(DynamicTreeChainEpochRuntimeError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.event.after_materialization.epoch_snapshot.levels[1]
            .source_graph
            .edge_slots
            .clear();
        tampered.result.final_materialization = tampered.event.after_materialization.clone();
        assert_eq!(
            check_dynamic_tree_chain_epoch_runtime_trace(&initial, &operation, &tampered),
            Err(DynamicTreeChainEpochRuntimeError::TraceVerification)
        );
    }

    #[test]
    fn invalid_stage_is_atomic_and_initialization_rejects_forged_propagation() {
        let state = initial_runtime();
        let before = state.clone();
        let invalid = DynamicTreeChainEpochRuntimeOperation::ApplyRootStage { updates: vec![] };
        assert_eq!(
            execute_dynamic_tree_chain_epoch_runtime(&state, &invalid),
            Err(DynamicTreeChainEpochRuntimeError::InvalidInput)
        );
        assert_eq!(state, before);

        let root = trace_dynamic_mwu_sparse_core_collection(&root_graph(), config())
            .expect("root initializer");
        let input = DynamicTreeChainPropagationInput {
            levels: vec![DynamicActiveBranchProjectionInput {
                collection: root.result.collection,
                active_branch: 0,
            }],
        };
        let mut propagation =
            trace_dynamic_tree_chain_propagation(&input, &[]).expect("propagation");
        propagation.result.level_results.clear();
        assert!(matches!(
            initialize_dynamic_tree_chain_epoch_runtime(&input, &[], &propagation),
            Err(DynamicTreeChainEpochRuntimeError::Propagation(
                DynamicTreeChainPropagationError::TraceVerification
            ))
        ));
    }
}
