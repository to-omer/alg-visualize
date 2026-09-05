//! Atomic Shift/Rebuild epoch replacement for bounded dynamic tree chains.
//!
//! A completed fixed-epoch propagation exposes the current source graph and
//! sparse-core collection at every level. `Shift(i)` keeps levels through `i`,
//! advances only level `i`'s active branch modulo its collection size, and
//! replaces every deeper level with freshly initialized epochs. `Rebuild(i)`
//! replaces level `i` and its whole suffix, resetting every new active branch
//! to zero. Candidate suffixes are fully initialized and checked before
//! publication, so a failed transition leaves the prior snapshot intact.
//!
//! Callers may still provide an explicit replacement collection, or may ask
//! the checked MWU initializer to synthesize each suffix level recursively from
//! the graph projected by the preceding active branch.

use num_rational::BigRational;
use thiserror::Error;

use super::{
    DynamicActiveBranchProjectionInput, DynamicLevelEdge, DynamicLevelGraphSnapshot,
    DynamicLevelProjectionError, DynamicLowStretchForestEdge, DynamicMwuCollectionBridgeConfig,
    DynamicMwuCollectionBridgeError, DynamicMwuCollectionBridgeTraceResult,
    DynamicSparseCoreCollectionError, DynamicSparseCoreCollectionInput, DynamicSparseCoreSnapshot,
    DynamicTreeChainPropagationError, DynamicTreeChainPropagationInput,
    DynamicTreeChainPropagationTraceResult, ShiftedTreeChainEdge, ShiftedTreeChainGraph,
    check_dynamic_mwu_sparse_core_collection_trace, check_dynamic_tree_chain_propagation_trace,
    initialize_dynamic_level_projection, trace_dynamic_mwu_sparse_core_collection,
    trace_dynamic_sparse_core_collection_stages,
};

const CATALOG_ID: &str = "dynamic-tree-chain-epochs";

/// One materialized level in the current suffix-epoch layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochLevel {
    /// Monotone epoch identity; preserved levels retain their identity.
    pub epoch: u64,
    /// Current source graph for this level, normalized to local stage zero.
    pub source_graph: DynamicLevelGraphSnapshot,
    /// Current or freshly initialized sparse snapshots in branch order.
    pub branch_snapshots: Vec<DynamicSparseCoreSnapshot>,
    /// Branch whose sparse graph defines the next level.
    pub active_branch: usize,
}

/// Exact lifecycle counters for atomic epoch replacement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainEpochMetrics {
    /// Successful Shift calls.
    pub shifts: u64,
    /// Successful Rebuild calls.
    pub rebuilds: u64,
    /// Total suffix levels initialized after the base layout.
    pub reinitialized_levels: u64,
    /// Successful atomic state publications.
    pub state_transitions: u64,
}

/// Complete suffix-epoch state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochSnapshot {
    /// Nonempty levels in root-to-leaf order.
    pub levels: Vec<DynamicTreeChainEpochLevel>,
    /// Next never-published epoch identity.
    pub next_epoch: u64,
    /// Exact lifecycle counters.
    pub metrics: DynamicTreeChainEpochMetrics,
}

/// One source Definition 5.9/5.10 lifecycle request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicTreeChainEpochOperation {
    /// Advance one branch and replace only the deeper suffix.
    Shift {
        /// Shifted level.
        level: usize,
        /// Fresh levels `level + 1 .. depth` in order.
        replacement_suffix: Vec<DynamicActiveBranchProjectionInput>,
    },
    /// Replace the selected level and its complete suffix.
    Rebuild {
        /// First rebuilt level.
        level: usize,
        /// Fresh levels `level .. depth` in order, all with active branch zero.
        replacement_suffix: Vec<DynamicActiveBranchProjectionInput>,
    },
}

/// A Shift/Rebuild request whose fresh suffix was synthesized by source MWU.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochMwuPlan {
    /// Exact atomic epoch operation containing the converted collections.
    pub operation: DynamicTreeChainEpochOperation,
    /// Per-level stable-universe/MWU configurations in suffix order.
    pub configs: Vec<DynamicMwuCollectionBridgeConfig>,
    /// Checked MWU-to-dynamic initializer transcript for every fresh level.
    pub initializer_traces: Vec<DynamicMwuCollectionBridgeTraceResult>,
}

/// Exact successful epoch transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochTransitionResult {
    /// Atomically published suffix layout.
    pub final_snapshot: DynamicTreeChainEpochSnapshot,
}

/// One reversible successful epoch transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Shift/Rebuild request including replacement suffix inputs.
    pub operation: DynamicTreeChainEpochOperation,
    /// Published state before candidate construction.
    pub before: DynamicTreeChainEpochSnapshot,
    /// Published state after complete candidate validation.
    pub after: DynamicTreeChainEpochSnapshot,
}

/// Complete independently checkable epoch transition transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainEpochTraceResult {
    /// Single atomic publication boundary.
    pub event: DynamicTreeChainEpochTraceEvent,
    /// Exact fast result.
    pub result: DynamicTreeChainEpochTransitionResult,
}

/// Explicit bounded epoch-lifecycle failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeChainEpochError {
    /// Initial trace, level, suffix length, active branch, or graph is malformed.
    #[error("dynamic tree-chain epoch input is invalid")]
    InvalidInput,
    /// Fixed-epoch propagation transcript failed verification.
    #[error("dynamic tree-chain epoch propagation failed: {0}")]
    Propagation(#[from] DynamicTreeChainPropagationError),
    /// Sparse projection used to connect an epoch boundary failed.
    #[error("dynamic tree-chain epoch projection failed: {0}")]
    Projection(#[from] DynamicLevelProjectionError),
    /// Fresh sparse-core collection initialization failed.
    #[error("dynamic tree-chain epoch collection failed: {0}")]
    Collection(#[from] DynamicSparseCoreCollectionError),
    /// Source MWU could not synthesize a requested fresh collection.
    #[error("dynamic tree-chain epoch MWU initializer failed: {0}")]
    MwuBridge(#[from] DynamicMwuCollectionBridgeError),
    /// Checked epoch or metric arithmetic overflowed.
    #[error("dynamic tree-chain epoch arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied epoch transcript failed independent verification.
    #[error("dynamic tree-chain epoch trace verification failed")]
    TraceVerification,
}

/// Derives a normalized epoch layout from a checked propagation transcript.
///
/// # Errors
///
/// Rejects a forged propagation trace, inconsistent source rows, or overflow.
pub fn initialize_dynamic_tree_chain_epochs(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[super::DynamicCoreGraphStageBatch],
    trace: &DynamicTreeChainPropagationTraceResult,
) -> Result<DynamicTreeChainEpochSnapshot, DynamicTreeChainEpochError> {
    check_dynamic_tree_chain_propagation_trace(input, root_batches, trace)?;
    let root_source = current_root_source_graph(trace)?;
    let mut levels = Vec::with_capacity(trace.level_traces.len());
    for (index, level_trace) in trace.level_traces.iter().enumerate() {
        let source_graph = if index == 0 {
            root_source.clone()
        } else {
            normalize_graph(
                trace.level_traces[index - 1]
                    .result
                    .final_projection
                    .graph
                    .clone(),
            )
        };
        let branch_snapshots = level_trace.result.final_collection.branch_snapshots.clone();
        let active_branch = input.levels[index].active_branch;
        if active_branch >= branch_snapshots.len() {
            return Err(DynamicTreeChainEpochError::InvalidInput);
        }
        levels.push(DynamicTreeChainEpochLevel {
            epoch: 0,
            source_graph,
            branch_snapshots,
            active_branch,
        });
    }
    Ok(DynamicTreeChainEpochSnapshot {
        levels,
        next_epoch: 1,
        metrics: DynamicTreeChainEpochMetrics::default(),
    })
}

/// Preflights and atomically applies one Shift or Rebuild transition.
///
/// The input snapshot is borrowed and never mutated. Therefore every error is
/// an exact rollback to the supplied state.
///
/// # Errors
///
/// Rejects invalid levels, suffix lengths, graph handoffs, fresh collections,
/// branch resets, or checked overflow.
pub fn execute_dynamic_tree_chain_epoch_transition(
    initial: &DynamicTreeChainEpochSnapshot,
    operation: &DynamicTreeChainEpochOperation,
) -> Result<DynamicTreeChainEpochTransitionResult, DynamicTreeChainEpochError> {
    let final_snapshot = apply_transition(initial, operation)?;
    Ok(DynamicTreeChainEpochTransitionResult { final_snapshot })
}

/// Applies one epoch transition and records its atomic publication boundary.
///
/// # Errors
///
/// Returns the same preflight failures as
/// [`execute_dynamic_tree_chain_epoch_transition`].
pub fn trace_dynamic_tree_chain_epoch_transition(
    initial: &DynamicTreeChainEpochSnapshot,
    operation: &DynamicTreeChainEpochOperation,
) -> Result<DynamicTreeChainEpochTraceResult, DynamicTreeChainEpochError> {
    let result = execute_dynamic_tree_chain_epoch_transition(initial, operation)?;
    Ok(DynamicTreeChainEpochTraceResult {
        event: DynamicTreeChainEpochTraceEvent {
            catalog_id: CATALOG_ID,
            operation: operation.clone(),
            before: initial.clone(),
            after: result.final_snapshot.clone(),
        },
        result,
    })
}

/// Independently reconstructs one successful Shift/Rebuild publication.
///
/// # Errors
///
/// Rejects operation, before/after snapshot, epoch, suffix, branch, graph, or
/// metric drift.
pub fn check_dynamic_tree_chain_epoch_trace(
    initial: &DynamicTreeChainEpochSnapshot,
    operation: &DynamicTreeChainEpochOperation,
    trace: &DynamicTreeChainEpochTraceResult,
) -> Result<(), DynamicTreeChainEpochError> {
    if trace.event.catalog_id != CATALOG_ID
        || trace.event.operation != *operation
        || trace.event.before != *initial
    {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    let expected = audit_apply_transition(initial, operation)?;
    if trace.event.after != expected || trace.result.final_snapshot != expected {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    Ok(())
}

/// Synthesizes the deeper suffix of `Shift(level)` from checked MWU forests.
///
/// The first source graph is the sparse projection of the branch selected
/// after advancing `level`; every subsequent graph is the branch-zero
/// projection of the preceding freshly initialized collection.
///
/// # Errors
///
/// Rejects an invalid level/config count, terminal or disconnected source
/// graph, stable-universe mismatch, MWU/collection failure, or projection drift.
pub fn plan_dynamic_tree_chain_shift_from_mwu(
    initial: &DynamicTreeChainEpochSnapshot,
    level: usize,
    configs: &[DynamicMwuCollectionBridgeConfig],
) -> Result<DynamicTreeChainEpochMwuPlan, DynamicTreeChainEpochError> {
    validate_snapshot(initial)?;
    let suffix_start = level
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)?;
    if suffix_start > initial.levels.len() || configs.len() != initial.levels.len() - suffix_start {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    let current = initial
        .levels
        .get(level)
        .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
    if current.branch_snapshots.is_empty() {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    let selected = current
        .active_branch
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)?
        % current.branch_snapshots.len();
    let graph = normalized_projection_graph(&current.branch_snapshots[selected])?;
    let (replacement_suffix, initializer_traces) = synthesize_mwu_suffix(graph, configs)?;
    let plan = DynamicTreeChainEpochMwuPlan {
        operation: DynamicTreeChainEpochOperation::Shift {
            level,
            replacement_suffix,
        },
        configs: configs.to_vec(),
        initializer_traces,
    };
    check_dynamic_tree_chain_epoch_mwu_plan(initial, &plan)?;
    Ok(plan)
}

/// Synthesizes the complete suffix of `Rebuild(level)` from checked MWU forests.
///
/// # Errors
///
/// Rejects an invalid level/config count, terminal or disconnected source
/// graph, stable-universe mismatch, MWU/collection failure, or projection drift.
pub fn plan_dynamic_tree_chain_rebuild_from_mwu(
    initial: &DynamicTreeChainEpochSnapshot,
    level: usize,
    configs: &[DynamicMwuCollectionBridgeConfig],
) -> Result<DynamicTreeChainEpochMwuPlan, DynamicTreeChainEpochError> {
    validate_snapshot(initial)?;
    if level >= initial.levels.len() || configs.len() != initial.levels.len() - level {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    let graph = initial.levels[level].source_graph.clone();
    let (replacement_suffix, initializer_traces) = synthesize_mwu_suffix(graph, configs)?;
    let plan = DynamicTreeChainEpochMwuPlan {
        operation: DynamicTreeChainEpochOperation::Rebuild {
            level,
            replacement_suffix,
        },
        configs: configs.to_vec(),
        initializer_traces,
    };
    check_dynamic_tree_chain_epoch_mwu_plan(initial, &plan)?;
    Ok(plan)
}

/// Independently verifies every MWU initializer and its recursive graph handoff.
///
/// # Errors
///
/// Rejects operation/config/trace count drift, component transcript forgery,
/// converted collection replacement, active-branch drift, or an epoch suffix
/// that would fail the independent transition replay.
pub fn check_dynamic_tree_chain_epoch_mwu_plan(
    initial: &DynamicTreeChainEpochSnapshot,
    plan: &DynamicTreeChainEpochMwuPlan,
) -> Result<(), DynamicTreeChainEpochError> {
    audit_validate_snapshot(initial)?;
    let (mut graph, replacement) = match &plan.operation {
        DynamicTreeChainEpochOperation::Shift {
            level,
            replacement_suffix,
        } => {
            let start = level
                .checked_add(1)
                .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
            if start > initial.levels.len()
                || replacement_suffix.len() != initial.levels.len() - start
            {
                return Err(DynamicTreeChainEpochError::TraceVerification);
            }
            let current = initial
                .levels
                .get(*level)
                .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
            if current.branch_snapshots.is_empty() {
                return Err(DynamicTreeChainEpochError::TraceVerification);
            }
            let selected = current
                .active_branch
                .checked_add(1)
                .ok_or(DynamicTreeChainEpochError::TraceVerification)?
                % current.branch_snapshots.len();
            (
                audit_normalized_projection_graph(&current.branch_snapshots[selected])?,
                replacement_suffix,
            )
        }
        DynamicTreeChainEpochOperation::Rebuild {
            level,
            replacement_suffix,
        } => {
            if *level >= initial.levels.len()
                || replacement_suffix.len() != initial.levels.len() - *level
            {
                return Err(DynamicTreeChainEpochError::TraceVerification);
            }
            (
                initial.levels[*level].source_graph.clone(),
                replacement_suffix,
            )
        }
    };
    if plan.configs.len() != replacement.len() || plan.initializer_traces.len() != replacement.len()
    {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    for ((level, config), initializer) in replacement
        .iter()
        .zip(&plan.configs)
        .zip(&plan.initializer_traces)
    {
        if level.active_branch != 0
            || config.stable_edge_slots != graph.edge_slots.len()
            || level.collection != initializer.result.collection
        {
            return Err(DynamicTreeChainEpochError::TraceVerification);
        }
        let shifted = shifted_graph_from_level(&graph, true)?;
        check_dynamic_mwu_sparse_core_collection_trace(&shifted, *config, initializer)?;
        let selected = initializer
            .result
            .initialized
            .final_snapshot
            .branch_snapshots
            .first()
            .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
        graph = audit_normalized_projection_graph(selected)?;
    }
    let _ = audit_apply_transition(initial, &plan.operation)?;
    Ok(())
}

fn synthesize_mwu_suffix(
    mut graph: DynamicLevelGraphSnapshot,
    configs: &[DynamicMwuCollectionBridgeConfig],
) -> Result<
    (
        Vec<DynamicActiveBranchProjectionInput>,
        Vec<DynamicMwuCollectionBridgeTraceResult>,
    ),
    DynamicTreeChainEpochError,
> {
    let mut suffix = Vec::with_capacity(configs.len());
    let mut traces = Vec::with_capacity(configs.len());
    for &config in configs {
        if config.stable_edge_slots != graph.edge_slots.len() {
            return Err(DynamicTreeChainEpochError::InvalidInput);
        }
        let shifted = shifted_graph_from_level(&graph, false)?;
        let initializer = trace_dynamic_mwu_sparse_core_collection(&shifted, config)?;
        let selected = initializer
            .result
            .initialized
            .final_snapshot
            .branch_snapshots
            .first()
            .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
        graph = normalized_projection_graph(selected)?;
        suffix.push(DynamicActiveBranchProjectionInput {
            collection: initializer.result.collection.clone(),
            active_branch: 0,
        });
        traces.push(initializer);
    }
    Ok((suffix, traces))
}

fn shifted_graph_from_level(
    graph: &DynamicLevelGraphSnapshot,
    audit: bool,
) -> Result<ShiftedTreeChainGraph, DynamicTreeChainEpochError> {
    if graph.stage != 0 || graph.active_node_count == 0 {
        return Err(if audit {
            DynamicTreeChainEpochError::TraceVerification
        } else {
            DynamicTreeChainEpochError::InvalidInput
        });
    }
    let mut edges = Vec::new();
    for (slot, edge) in graph.edge_slots.iter().enumerate() {
        let Some(edge) = edge else {
            continue;
        };
        if edge.edge != slot
            || edge.from >= graph.active_node_count
            || edge.to >= graph.active_node_count
        {
            return Err(if audit {
                DynamicTreeChainEpochError::TraceVerification
            } else {
                DynamicTreeChainEpochError::InvalidInput
            });
        }
        edges.push(ShiftedTreeChainEdge {
            source_edge: slot,
            from: edge.from,
            to: edge.to,
            length: edge.length.clone(),
            gradient: edge.gradient.clone(),
        });
    }
    Ok(ShiftedTreeChainGraph {
        node_count: graph.active_node_count,
        edges,
    })
}

fn apply_transition(
    initial: &DynamicTreeChainEpochSnapshot,
    operation: &DynamicTreeChainEpochOperation,
) -> Result<DynamicTreeChainEpochSnapshot, DynamicTreeChainEpochError> {
    validate_snapshot(initial)?;
    let mut candidate = initial.clone();
    match operation {
        DynamicTreeChainEpochOperation::Shift {
            level,
            replacement_suffix,
        } => apply_shift(&mut candidate, *level, replacement_suffix)?,
        DynamicTreeChainEpochOperation::Rebuild {
            level,
            replacement_suffix,
        } => apply_rebuild(&mut candidate, *level, replacement_suffix)?,
    }
    validate_snapshot(&candidate)?;
    Ok(candidate)
}

fn apply_shift(
    candidate: &mut DynamicTreeChainEpochSnapshot,
    level: usize,
    replacement: &[DynamicActiveBranchProjectionInput],
) -> Result<(), DynamicTreeChainEpochError> {
    let suffix_start = level
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)?;
    if suffix_start > candidate.levels.len()
        || replacement.len() != candidate.levels.len() - suffix_start
    {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    let current = candidate
        .levels
        .get_mut(level)
        .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
    let branches = current.branch_snapshots.len();
    if branches == 0 {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    current.active_branch = current
        .active_branch
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)?
        % branches;
    let selected = current
        .branch_snapshots
        .get(current.active_branch)
        .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
    let expected = normalized_projection_graph(selected)?;
    let (suffix, next_epoch) =
        initialize_replacement_suffix(replacement, expected, candidate.next_epoch, true)?;
    candidate.levels.truncate(suffix_start);
    candidate.levels.extend(suffix);
    candidate.next_epoch = next_epoch;
    candidate.metrics.shifts = increment(candidate.metrics.shifts)?;
    candidate.metrics.reinitialized_levels =
        add_usize(candidate.metrics.reinitialized_levels, replacement.len())?;
    candidate.metrics.state_transitions = increment(candidate.metrics.state_transitions)?;
    Ok(())
}

fn apply_rebuild(
    candidate: &mut DynamicTreeChainEpochSnapshot,
    level: usize,
    replacement: &[DynamicActiveBranchProjectionInput],
) -> Result<(), DynamicTreeChainEpochError> {
    if level >= candidate.levels.len() || replacement.len() != candidate.levels.len() - level {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    let expected = candidate.levels[level].source_graph.clone();
    let (suffix, next_epoch) =
        initialize_replacement_suffix(replacement, expected, candidate.next_epoch, true)?;
    candidate.levels.truncate(level);
    candidate.levels.extend(suffix);
    candidate.next_epoch = next_epoch;
    candidate.metrics.rebuilds = increment(candidate.metrics.rebuilds)?;
    candidate.metrics.reinitialized_levels =
        add_usize(candidate.metrics.reinitialized_levels, replacement.len())?;
    candidate.metrics.state_transitions = increment(candidate.metrics.state_transitions)?;
    Ok(())
}

fn initialize_replacement_suffix(
    replacement: &[DynamicActiveBranchProjectionInput],
    mut expected: DynamicLevelGraphSnapshot,
    mut next_epoch: u64,
    require_zero_branch: bool,
) -> Result<(Vec<DynamicTreeChainEpochLevel>, u64), DynamicTreeChainEpochError> {
    let mut suffix = Vec::with_capacity(replacement.len());
    for level in replacement {
        if (require_zero_branch && level.active_branch != 0)
            || !collection_matches_graph(&level.collection, &expected)
        {
            return Err(DynamicTreeChainEpochError::InvalidInput);
        }
        let trace = trace_dynamic_sparse_core_collection_stages(&level.collection, &[])?;
        let branch_snapshots = trace.base_snapshot.branch_snapshots;
        let selected = branch_snapshots
            .get(level.active_branch)
            .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
        let source_graph = normalize_graph(expected);
        expected = normalized_projection_graph(selected)?;
        suffix.push(DynamicTreeChainEpochLevel {
            epoch: next_epoch,
            source_graph,
            branch_snapshots,
            active_branch: level.active_branch,
        });
        next_epoch = increment(next_epoch)?;
    }
    Ok((suffix, next_epoch))
}

fn validate_snapshot(
    snapshot: &DynamicTreeChainEpochSnapshot,
) -> Result<(), DynamicTreeChainEpochError> {
    if snapshot.levels.is_empty() || snapshot.next_epoch == 0 {
        return Err(DynamicTreeChainEpochError::InvalidInput);
    }
    for level in &snapshot.levels {
        if level.source_graph.stage != 0
            || level.branch_snapshots.is_empty()
            || level.active_branch >= level.branch_snapshots.len()
            || level.epoch >= snapshot.next_epoch
        {
            return Err(DynamicTreeChainEpochError::InvalidInput);
        }
    }
    Ok(())
}

fn current_root_source_graph(
    trace: &DynamicTreeChainPropagationTraceResult,
) -> Result<DynamicLevelGraphSnapshot, DynamicTreeChainEpochError> {
    let first_level = trace
        .level_traces
        .first()
        .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
    let first_branch = first_level
        .collection_trace
        .branch_traces
        .first()
        .ok_or(DynamicTreeChainEpochError::InvalidInput)?;
    let forest = &first_branch.core_trace.forest_trace.result.final_snapshot;
    let core = &first_branch.core_trace.result.final_snapshot;
    if forest.edge_slots.len() != core.source_gradients.len() {
        return Err(DynamicTreeChainEpochError::InvalidInput);
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
            _ => return Err(DynamicTreeChainEpochError::InvalidInput),
        }
    }
    Ok(DynamicLevelGraphSnapshot {
        active_node_count: forest.active_node_count,
        edge_slots,
        stage: 0,
    })
}

fn normalized_projection_graph(
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<DynamicLevelGraphSnapshot, DynamicTreeChainEpochError> {
    Ok(normalize_graph(
        initialize_dynamic_level_projection(snapshot)?.graph,
    ))
}

fn normalize_graph(mut graph: DynamicLevelGraphSnapshot) -> DynamicLevelGraphSnapshot {
    graph.stage = 0;
    graph
}

fn collection_matches_graph(
    collection: &DynamicSparseCoreCollectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> bool {
    !collection.branches.is_empty()
        && expected.stage == 0
        && collection.branches.iter().all(|branch| {
            let forest = &branch.core.forest;
            forest.initial_node_count == expected.active_node_count
                && forest.maximum_node_count >= expected.active_node_count
                && source_rows_match(
                    &forest.edge_slots,
                    &branch.core.initial_gradients,
                    &expected.edge_slots,
                )
        })
}

fn source_rows_match(
    forest: &[Option<DynamicLowStretchForestEdge>],
    gradients: &[Option<BigRational>],
    expected: &[Option<DynamicLevelEdge>],
) -> bool {
    forest.len() == expected.len()
        && gradients.len() == expected.len()
        && forest.iter().zip(gradients).zip(expected).all(
            |((forest_edge, gradient), expected_edge)| match (
                forest_edge.as_ref(),
                gradient.as_ref(),
                expected_edge.as_ref(),
            ) {
                (None, None, None) => true,
                (Some(forest), Some(gradient), Some(expected)) => {
                    forest.edge == expected.edge
                        && forest.from == expected.from
                        && forest.to == expected.to
                        && forest.length == expected.length
                        && gradient == &expected.gradient
                }
                _ => false,
            },
        )
}

fn audit_apply_transition(
    initial: &DynamicTreeChainEpochSnapshot,
    operation: &DynamicTreeChainEpochOperation,
) -> Result<DynamicTreeChainEpochSnapshot, DynamicTreeChainEpochError> {
    audit_validate_snapshot(initial)?;
    let mut expected = initial.clone();
    match operation {
        DynamicTreeChainEpochOperation::Shift {
            level,
            replacement_suffix,
        } => audit_shift(&mut expected, *level, replacement_suffix)?,
        DynamicTreeChainEpochOperation::Rebuild {
            level,
            replacement_suffix,
        } => audit_rebuild(&mut expected, *level, replacement_suffix)?,
    }
    audit_validate_snapshot(&expected)?;
    Ok(expected)
}

fn audit_shift(
    expected: &mut DynamicTreeChainEpochSnapshot,
    level: usize,
    replacement: &[DynamicActiveBranchProjectionInput],
) -> Result<(), DynamicTreeChainEpochError> {
    let start = level
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
    if start > expected.levels.len() || replacement.len() != expected.levels.len() - start {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    let current = expected
        .levels
        .get_mut(level)
        .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
    if current.branch_snapshots.is_empty() {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    current.active_branch = current
        .active_branch
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::TraceVerification)?
        % current.branch_snapshots.len();
    let mut graph =
        audit_normalized_projection_graph(&current.branch_snapshots[current.active_branch])?;
    let mut suffix = Vec::with_capacity(replacement.len());
    let mut epoch = expected.next_epoch;
    for level_input in replacement {
        if level_input.active_branch != 0
            || !audit_collection_matches_graph(&level_input.collection, &graph)
        {
            return Err(DynamicTreeChainEpochError::TraceVerification);
        }
        let trace = trace_dynamic_sparse_core_collection_stages(&level_input.collection, &[])?;
        let branches = trace.base_snapshot.branch_snapshots;
        let selected = branches
            .first()
            .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
        let source_graph = graph;
        graph = audit_normalized_projection_graph(selected)?;
        suffix.push(DynamicTreeChainEpochLevel {
            epoch,
            source_graph,
            branch_snapshots: branches,
            active_branch: 0,
        });
        epoch = audit_increment(epoch)?;
    }
    expected.levels.truncate(start);
    expected.levels.extend(suffix);
    expected.next_epoch = epoch;
    expected.metrics.shifts = audit_increment(expected.metrics.shifts)?;
    expected.metrics.reinitialized_levels =
        audit_add_usize(expected.metrics.reinitialized_levels, replacement.len())?;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    Ok(())
}

fn audit_rebuild(
    expected: &mut DynamicTreeChainEpochSnapshot,
    level: usize,
    replacement: &[DynamicActiveBranchProjectionInput],
) -> Result<(), DynamicTreeChainEpochError> {
    if level >= expected.levels.len() || replacement.len() != expected.levels.len() - level {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    let mut graph = expected.levels[level].source_graph.clone();
    let mut suffix = Vec::with_capacity(replacement.len());
    let mut epoch = expected.next_epoch;
    for level_input in replacement {
        if level_input.active_branch != 0
            || !audit_collection_matches_graph(&level_input.collection, &graph)
        {
            return Err(DynamicTreeChainEpochError::TraceVerification);
        }
        let trace = trace_dynamic_sparse_core_collection_stages(&level_input.collection, &[])?;
        let branches = trace.base_snapshot.branch_snapshots;
        let selected = branches
            .first()
            .ok_or(DynamicTreeChainEpochError::TraceVerification)?;
        let source_graph = graph;
        graph = audit_normalized_projection_graph(selected)?;
        suffix.push(DynamicTreeChainEpochLevel {
            epoch,
            source_graph,
            branch_snapshots: branches,
            active_branch: 0,
        });
        epoch = audit_increment(epoch)?;
    }
    expected.levels.truncate(level);
    expected.levels.extend(suffix);
    expected.next_epoch = epoch;
    expected.metrics.rebuilds = audit_increment(expected.metrics.rebuilds)?;
    expected.metrics.reinitialized_levels =
        audit_add_usize(expected.metrics.reinitialized_levels, replacement.len())?;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    Ok(())
}

fn audit_validate_snapshot(
    snapshot: &DynamicTreeChainEpochSnapshot,
) -> Result<(), DynamicTreeChainEpochError> {
    if snapshot.levels.is_empty() || snapshot.next_epoch == 0 {
        return Err(DynamicTreeChainEpochError::TraceVerification);
    }
    for level in &snapshot.levels {
        if level.source_graph.stage != 0
            || level.branch_snapshots.is_empty()
            || level.active_branch >= level.branch_snapshots.len()
            || level.epoch >= snapshot.next_epoch
        {
            return Err(DynamicTreeChainEpochError::TraceVerification);
        }
    }
    Ok(())
}

fn audit_normalized_projection_graph(
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<DynamicLevelGraphSnapshot, DynamicTreeChainEpochError> {
    let mut graph = initialize_dynamic_level_projection(snapshot)?.graph;
    graph.stage = 0;
    Ok(graph)
}

fn audit_collection_matches_graph(
    collection: &DynamicSparseCoreCollectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> bool {
    if collection.branches.is_empty() || expected.stage != 0 {
        return false;
    }
    for branch in &collection.branches {
        let forest = &branch.core.forest;
        if forest.initial_node_count != expected.active_node_count
            || forest.maximum_node_count < expected.active_node_count
            || forest.edge_slots.len() != expected.edge_slots.len()
            || branch.core.initial_gradients.len() != expected.edge_slots.len()
        {
            return false;
        }
        for index in 0..expected.edge_slots.len() {
            let matches = match (
                forest.edge_slots[index].as_ref(),
                branch.core.initial_gradients[index].as_ref(),
                expected.edge_slots[index].as_ref(),
            ) {
                (None, None, None) => true,
                (Some(forest), Some(gradient), Some(expected)) => {
                    forest.edge == expected.edge
                        && forest.from == expected.from
                        && forest.to == expected.to
                        && forest.length == expected.length
                        && gradient == &expected.gradient
                }
                _ => false,
            };
            if !matches {
                return false;
            }
        }
    }
    true
}

fn increment(value: u64) -> Result<u64, DynamicTreeChainEpochError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)
}

fn add_usize(value: u64, additional: usize) -> Result<u64, DynamicTreeChainEpochError> {
    value
        .checked_add(
            u64::try_from(additional)
                .map_err(|_| DynamicTreeChainEpochError::ArithmeticOverflow)?,
        )
        .ok_or(DynamicTreeChainEpochError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicTreeChainEpochError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainEpochError::TraceVerification)
}

fn audit_add_usize(value: u64, additional: usize) -> Result<u64, DynamicTreeChainEpochError> {
    value
        .checked_add(
            u64::try_from(additional).map_err(|_| DynamicTreeChainEpochError::TraceVerification)?,
        )
        .ok_or(DynamicTreeChainEpochError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;
    use crate::{
        DynamicCoreGraphInput, DynamicCoreGraphStageBatch, DynamicCoreGraphStageEdge,
        DynamicCoreGraphStageUpdate, DynamicLowStretchForestInput, DynamicSparseCoreInput,
        execute_dynamic_tree_chain_propagation, trace_dynamic_tree_chain_propagation,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn forest_edge(edge: &DynamicLevelEdge) -> DynamicLowStretchForestEdge {
        DynamicLowStretchForestEdge {
            edge: edge.edge,
            from: edge.from,
            to: edge.to,
            length: edge.length.clone(),
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

    fn root_branch(reference_tree_edges: Vec<usize>) -> DynamicSparseCoreInput {
        let edges = [(0, 0, 1, 1), (1, 1, 2, 1), (2, 1, 3, 1), (3, 2, 3, 2)];
        DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
                forest: DynamicLowStretchForestInput {
                    initial_node_count: 4,
                    maximum_node_count: 5,
                    edge_slots: edges
                        .iter()
                        .map(|(edge, from, to, length)| {
                            Some(DynamicLowStretchForestEdge {
                                edge: *edge,
                                from: *from,
                                to: *to,
                                length: rational(*length),
                            })
                        })
                        .chain(std::iter::once(None))
                        .collect(),
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

    fn collection_from_graph(
        graph: &DynamicLevelGraphSnapshot,
    ) -> DynamicSparseCoreCollectionInput {
        let reference_tree_edges = spanning_tree_edges(graph);
        let branch = || DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
                forest: DynamicLowStretchForestInput {
                    initial_node_count: graph.active_node_count,
                    maximum_node_count: 5,
                    edge_slots: graph
                        .edge_slots
                        .iter()
                        .map(|edge| edge.as_ref().map(forest_edge))
                        .collect(),
                    reference_tree_edges: reference_tree_edges.clone(),
                    reference_root: 0,
                    initial_root_seeds: vec![0],
                    initial_stretch_overestimates: None,
                },
                initial_gradients: graph
                    .edge_slots
                    .iter()
                    .map(|edge| edge.as_ref().map(|edge| edge.gradient.clone()))
                    .collect(),
            },
            branches: 2,
        };
        DynamicSparseCoreCollectionInput {
            branches: vec![branch(), branch()],
        }
    }

    fn spanning_tree_edges(graph: &DynamicLevelGraphSnapshot) -> Vec<usize> {
        let mut parent = (0..graph.active_node_count).collect::<Vec<_>>();
        let mut result = Vec::new();
        for edge in graph.edge_slots.iter().flatten() {
            let mut left = edge.from;
            while parent[left] != left {
                left = parent[left];
            }
            let mut right = edge.to;
            while parent[right] != right {
                right = parent[right];
            }
            if left != right {
                parent[right] = left;
                result.push(edge.edge);
            }
        }
        assert_eq!(result.len() + 1, graph.active_node_count);
        result
    }

    fn root_level() -> DynamicActiveBranchProjectionInput {
        DynamicActiveBranchProjectionInput {
            collection: DynamicSparseCoreCollectionInput {
                branches: vec![root_branch(vec![0, 1, 2]), root_branch(vec![0, 2, 3])],
            },
            active_branch: 1,
        }
    }

    fn empty_child_level() -> DynamicActiveBranchProjectionInput {
        let graph = DynamicLevelGraphSnapshot {
            active_node_count: 1,
            edge_slots: vec![None, None, None, None, None],
            stage: 0,
        };
        DynamicActiveBranchProjectionInput {
            collection: collection_from_graph(&graph),
            active_branch: 0,
        }
    }

    fn propagation_input() -> DynamicTreeChainPropagationInput {
        DynamicTreeChainPropagationInput {
            levels: vec![root_level(), empty_child_level()],
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

    fn epoch_state() -> DynamicTreeChainEpochSnapshot {
        let input = propagation_input();
        let batches = batches();
        let trace = trace_dynamic_tree_chain_propagation(&input, &batches).expect("propagation");
        initialize_dynamic_tree_chain_epochs(&input, &batches, &trace).expect("epochs")
    }

    fn shifted_child(state: &DynamicTreeChainEpochSnapshot) -> DynamicActiveBranchProjectionInput {
        let next = (state.levels[0].active_branch + 1) % state.levels[0].branch_snapshots.len();
        let graph = normalized_projection_graph(&state.levels[0].branch_snapshots[next])
            .expect("shift graph");
        DynamicActiveBranchProjectionInput {
            collection: collection_from_graph(&graph),
            active_branch: 0,
        }
    }

    fn mwu_config(slots: usize) -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: slots,
        }
    }

    #[test]
    fn shift_preserves_level_and_replaces_only_deeper_epoch() {
        let state = epoch_state();
        validate_snapshot(&state).expect("valid epoch state");
        let child = shifted_child(&state);
        let next = (state.levels[0].active_branch + 1) % state.levels[0].branch_snapshots.len();
        let expected = normalized_projection_graph(&state.levels[0].branch_snapshots[next])
            .expect("expected shift graph");
        assert!(collection_matches_graph(&child.collection, &expected));
        let operation = DynamicTreeChainEpochOperation::Shift {
            level: 0,
            replacement_suffix: vec![child],
        };
        let fast = execute_dynamic_tree_chain_epoch_transition(&state, &operation).expect("shift");
        let trace = trace_dynamic_tree_chain_epoch_transition(&state, &operation).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.final_snapshot.levels[0].epoch, state.levels[0].epoch);
        assert_eq!(fast.final_snapshot.levels[0].active_branch, 0);
        assert_eq!(fast.final_snapshot.levels[1].epoch, state.next_epoch);
        assert_eq!(fast.final_snapshot.levels[1].active_branch, 0);
        assert_eq!(fast.final_snapshot.metrics.shifts, 1);
        assert_eq!(fast.final_snapshot.metrics.reinitialized_levels, 1);
        check_dynamic_tree_chain_epoch_trace(&state, &operation, &trace).expect("check");
    }

    #[test]
    fn rebuild_replaces_selected_suffix_and_resets_branch_zero() {
        let state = epoch_state();
        validate_snapshot(&state).expect("valid epoch state");
        let replacement = DynamicActiveBranchProjectionInput {
            collection: collection_from_graph(&state.levels[1].source_graph),
            active_branch: 0,
        };
        assert!(collection_matches_graph(
            &replacement.collection,
            &state.levels[1].source_graph
        ));
        let operation = DynamicTreeChainEpochOperation::Rebuild {
            level: 1,
            replacement_suffix: vec![replacement],
        };
        let result =
            execute_dynamic_tree_chain_epoch_transition(&state, &operation).expect("rebuild");
        assert_eq!(result.final_snapshot.levels[0], state.levels[0]);
        assert_eq!(result.final_snapshot.levels[1].epoch, state.next_epoch);
        assert_eq!(result.final_snapshot.levels[1].active_branch, 0);
        assert_eq!(result.final_snapshot.metrics.rebuilds, 1);
    }

    #[test]
    fn mwu_plan_synthesizes_and_publishes_a_recursive_rebuild_suffix() {
        let state = epoch_state();
        let slots = state.levels[0].source_graph.edge_slots.len();
        let plan = plan_dynamic_tree_chain_rebuild_from_mwu(
            &state,
            0,
            &[mwu_config(slots), mwu_config(slots)],
        )
        .expect("MWU plan");
        check_dynamic_tree_chain_epoch_mwu_plan(&state, &plan).expect("plan check");
        assert_eq!(plan.initializer_traces.len(), 2);
        let result = execute_dynamic_tree_chain_epoch_transition(&state, &plan.operation)
            .expect("publish rebuild");
        assert_eq!(result.final_snapshot.levels.len(), 2);
        assert_eq!(result.final_snapshot.levels[0].epoch, state.next_epoch);
        assert_eq!(result.final_snapshot.levels[1].epoch, state.next_epoch + 1);
        assert!(
            result
                .final_snapshot
                .levels
                .iter()
                .all(|level| level.active_branch == 0)
        );
    }

    #[test]
    fn mwu_plan_checker_rejects_initializer_and_collection_tampering() {
        let state = epoch_state();
        let slots = state.levels[0].source_graph.edge_slots.len();
        let plan = plan_dynamic_tree_chain_rebuild_from_mwu(
            &state,
            0,
            &[mwu_config(slots), mwu_config(slots)],
        )
        .expect("MWU plan");

        let mut tampered = plan.clone();
        tampered.initializer_traces[0].mwu_trace.result.branches[0].candidate_index += 1;
        assert!(check_dynamic_tree_chain_epoch_mwu_plan(&state, &tampered).is_err());

        let mut tampered = plan;
        let DynamicTreeChainEpochOperation::Rebuild {
            replacement_suffix, ..
        } = &mut tampered.operation
        else {
            panic!("rebuild")
        };
        replacement_suffix[0].collection.branches[0]
            .core
            .forest
            .reference_root = 3;
        assert_eq!(
            check_dynamic_tree_chain_epoch_mwu_plan(&state, &tampered),
            Err(DynamicTreeChainEpochError::TraceVerification)
        );
    }

    #[test]
    fn mwu_plan_rejects_wrong_suffix_config_count_without_state_mutation() {
        let state = epoch_state();
        let before = state.clone();
        let slots = state.levels[0].source_graph.edge_slots.len();
        assert_eq!(
            plan_dynamic_tree_chain_rebuild_from_mwu(&state, 0, &[mwu_config(slots)]),
            Err(DynamicTreeChainEpochError::InvalidInput)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn invalid_suffix_is_rejected_without_mutating_published_state() {
        let state = epoch_state();
        let before = state.clone();
        let operation = DynamicTreeChainEpochOperation::Shift {
            level: 0,
            replacement_suffix: vec![empty_child_level()],
        };
        assert_eq!(
            execute_dynamic_tree_chain_epoch_transition(&state, &operation),
            Err(DynamicTreeChainEpochError::InvalidInput)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn checker_rejects_epoch_and_suffix_tampering() {
        let state = epoch_state();
        let operation = DynamicTreeChainEpochOperation::Shift {
            level: 0,
            replacement_suffix: vec![shifted_child(&state)],
        };
        let trace = trace_dynamic_tree_chain_epoch_transition(&state, &operation).expect("trace");
        let mut tampered = trace.clone();
        tampered.event.after.levels[1].epoch = 99;
        assert_eq!(
            check_dynamic_tree_chain_epoch_trace(&state, &operation, &tampered),
            Err(DynamicTreeChainEpochError::TraceVerification)
        );
        let mut tampered = trace;
        tampered.result.final_snapshot.levels[1]
            .source_graph
            .edge_slots
            .clear();
        assert_eq!(
            check_dynamic_tree_chain_epoch_trace(&state, &operation, &tampered),
            Err(DynamicTreeChainEpochError::TraceVerification)
        );
    }

    #[test]
    fn initialization_rejects_a_forged_propagation_trace() {
        let input = propagation_input();
        let batches = batches();
        let mut trace = trace_dynamic_tree_chain_propagation(&input, &batches).expect("trace");
        trace.result.terminal_batches.clear();
        assert!(matches!(
            initialize_dynamic_tree_chain_epochs(&input, &batches, &trace),
            Err(DynamicTreeChainEpochError::Propagation(
                DynamicTreeChainPropagationError::TraceVerification
            ))
        ));
        let _ = execute_dynamic_tree_chain_propagation(&input, &batches).expect("fast");
    }
}
