//! Checked bounded multi-level propagation for a dynamic shifted tree chain.
//!
//! Each configured level advances all of its sparse-core branches and projects
//! only its selected active branch. The emitted atomic child batches become the
//! exact input stream of the next level. A level is admitted only when every
//! branch's initial source graph equals the previous active branch's projected
//! base graph, including stable holes, dense vertex IDs, lengths, and gradients.
//!
//! This realizes fixed-epoch all-level propagation. It deliberately does not
//! choose branches, build reference trees, or implement Shift/Rebuild suffix
//! replacement and rollback; those epoch operations remain a separate layer.

use thiserror::Error;

use super::{
    DynamicActiveBranchProjectionError, DynamicActiveBranchProjectionInput,
    DynamicActiveBranchProjectionResult, DynamicActiveBranchProjectionTraceResult,
    DynamicCoreGraphStageBatch, DynamicLevelGraphSnapshot, DynamicLowStretchForestEdge,
    check_dynamic_active_branch_projection_trace, trace_dynamic_active_branch_projection,
};

const MAX_LEVELS: usize = 8;

/// Fixed-epoch collection and active-branch configuration for every level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainPropagationInput {
    /// Nonempty levels in root-to-leaf order.
    pub levels: Vec<DynamicActiveBranchProjectionInput>,
}

/// Exact multi-level propagation counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainPropagationMetrics {
    /// Levels advanced.
    pub levels: u64,
    /// Sum of outer-stage applications across levels.
    pub level_stage_applications: u64,
    /// Sum of active sparse records propagated across levels.
    pub propagated_records: u64,
    /// Atomic records emitted beyond the final configured level.
    pub terminal_records: u64,
}

/// Exact terminal state of a fixed-epoch propagation run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainPropagationResult {
    /// One checked result per configured level.
    pub level_results: Vec<DynamicActiveBranchProjectionResult>,
    /// Atomic stream emitted by the final configured level.
    pub terminal_batches: Vec<DynamicCoreGraphStageBatch>,
    /// Exact multi-level counters.
    pub metrics: DynamicTreeChainPropagationMetrics,
}

/// Complete independently checkable multi-level transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainPropagationTraceResult {
    /// One all-branch and active-projection transcript per level.
    pub level_traces: Vec<DynamicActiveBranchProjectionTraceResult>,
    /// Exact terminal result.
    pub result: DynamicTreeChainPropagationResult,
}

/// Explicit bounded multi-level propagation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeChainPropagationError {
    /// Level count or an inter-level initial graph boundary is malformed.
    #[error("dynamic tree-chain propagation input is invalid")]
    InvalidInput,
    /// One checked level failed.
    #[error("dynamic tree-chain propagation level {level} failed: {error}")]
    Level {
        /// Root-based level index.
        level: usize,
        /// Component error.
        #[source]
        error: DynamicActiveBranchProjectionError,
    },
    /// Checked metric arithmetic overflowed.
    #[error("dynamic tree-chain propagation arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied multi-level transcript failed independent verification.
    #[error("dynamic tree-chain propagation trace verification failed")]
    TraceVerification,
}

/// Propagates root stages through every configured fixed-epoch level.
///
/// # Errors
///
/// Rejects invalid level boundaries, component failures, or checked overflow.
pub fn execute_dynamic_tree_chain_propagation(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicTreeChainPropagationResult, DynamicTreeChainPropagationError> {
    compose(input, root_batches).map(|trace| trace.result)
}

/// Propagates and records every fixed-epoch level boundary.
///
/// # Errors
///
/// Returns any level-boundary, component, arithmetic, or replay failure.
pub fn trace_dynamic_tree_chain_propagation(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicTreeChainPropagationTraceResult, DynamicTreeChainPropagationError> {
    let trace = compose(input, root_batches)?;
    check_dynamic_tree_chain_propagation_trace(input, root_batches, &trace)?;
    Ok(trace)
}

/// Independently verifies every level and each inter-level stream handoff.
///
/// # Errors
///
/// Rejects a level trace, initial graph boundary, child-stream order, terminal
/// result, or metric that differs from exact replay.
pub fn check_dynamic_tree_chain_propagation_trace(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[DynamicCoreGraphStageBatch],
    trace: &DynamicTreeChainPropagationTraceResult,
) -> Result<(), DynamicTreeChainPropagationError> {
    validate_input(input)?;
    if trace.level_traces.len() != input.levels.len() {
        return Err(DynamicTreeChainPropagationError::TraceVerification);
    }
    let mut batches = root_batches.to_vec();
    let mut previous_graph: Option<&DynamicLevelGraphSnapshot> = None;
    let mut level_results = Vec::with_capacity(input.levels.len());
    let mut metrics = DynamicTreeChainPropagationMetrics::default();
    for (level, (level_input, level_trace)) in
        input.levels.iter().zip(&trace.level_traces).enumerate()
    {
        if let Some(graph) = previous_graph {
            audit_validate_level_initial_graph(level_input, graph)?;
        }
        check_dynamic_active_branch_projection_trace(level_input, &batches, level_trace)
            .map_err(|error| DynamicTreeChainPropagationError::Level { level, error })?;
        audit_account_level(&mut metrics, &level_trace.result)?;
        batches.clone_from(&level_trace.result.child_batches);
        previous_graph = Some(&level_trace.base_projection.graph);
        level_results.push(level_trace.result.clone());
    }
    metrics.terminal_records = audit_count_records(&batches)?;
    let expected = DynamicTreeChainPropagationResult {
        level_results,
        terminal_batches: batches,
        metrics,
    };
    if trace.result != expected {
        return Err(DynamicTreeChainPropagationError::TraceVerification);
    }
    Ok(())
}

fn compose(
    input: &DynamicTreeChainPropagationInput,
    root_batches: &[DynamicCoreGraphStageBatch],
) -> Result<DynamicTreeChainPropagationTraceResult, DynamicTreeChainPropagationError> {
    validate_input(input)?;
    let mut batches = root_batches.to_vec();
    let mut previous_graph: Option<DynamicLevelGraphSnapshot> = None;
    let mut level_traces = Vec::with_capacity(input.levels.len());
    let mut level_results = Vec::with_capacity(input.levels.len());
    let mut metrics = DynamicTreeChainPropagationMetrics::default();
    for (level, level_input) in input.levels.iter().enumerate() {
        if let Some(graph) = &previous_graph {
            validate_level_initial_graph(level_input, graph)?;
        }
        let level_trace = trace_dynamic_active_branch_projection(level_input, &batches)
            .map_err(|error| DynamicTreeChainPropagationError::Level { level, error })?;
        account_level(&mut metrics, &level_trace.result)?;
        batches.clone_from(&level_trace.result.child_batches);
        previous_graph = Some(level_trace.base_projection.graph.clone());
        level_results.push(level_trace.result.clone());
        level_traces.push(level_trace);
    }
    metrics.terminal_records = count_records(&batches)?;
    Ok(DynamicTreeChainPropagationTraceResult {
        level_traces,
        result: DynamicTreeChainPropagationResult {
            level_results,
            terminal_batches: batches,
            metrics,
        },
    })
}

fn validate_input(
    input: &DynamicTreeChainPropagationInput,
) -> Result<(), DynamicTreeChainPropagationError> {
    if input.levels.is_empty() || input.levels.len() > MAX_LEVELS {
        return Err(DynamicTreeChainPropagationError::InvalidInput);
    }
    Ok(())
}

fn validate_level_initial_graph(
    level: &DynamicActiveBranchProjectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> Result<(), DynamicTreeChainPropagationError> {
    if level_matches_graph(level, expected) {
        Ok(())
    } else {
        Err(DynamicTreeChainPropagationError::InvalidInput)
    }
}

fn audit_validate_level_initial_graph(
    level: &DynamicActiveBranchProjectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> Result<(), DynamicTreeChainPropagationError> {
    if audit_level_matches_graph(level, expected) {
        Ok(())
    } else {
        Err(DynamicTreeChainPropagationError::TraceVerification)
    }
}

fn level_matches_graph(
    level: &DynamicActiveBranchProjectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> bool {
    expected.stage == 0
        && !level.collection.branches.is_empty()
        && level.collection.branches.iter().all(|branch| {
            let forest = &branch.core.forest;
            forest.initial_node_count == expected.active_node_count
                && forest.maximum_node_count >= expected.active_node_count
                && source_slots_match(
                    &forest.edge_slots,
                    &branch.core.initial_gradients,
                    &expected.edge_slots,
                )
        })
}

fn audit_level_matches_graph(
    level: &DynamicActiveBranchProjectionInput,
    expected: &DynamicLevelGraphSnapshot,
) -> bool {
    if expected.stage != 0 || level.collection.branches.is_empty() {
        return false;
    }
    for branch in &level.collection.branches {
        let forest = &branch.core.forest;
        if forest.initial_node_count != expected.active_node_count
            || forest.maximum_node_count < expected.active_node_count
            || !audit_source_slots_match(
                &forest.edge_slots,
                &branch.core.initial_gradients,
                &expected.edge_slots,
            )
        {
            return false;
        }
    }
    true
}

fn source_slots_match(
    forest: &[Option<DynamicLowStretchForestEdge>],
    gradients: &[Option<num_rational::BigRational>],
    expected: &[Option<super::DynamicLevelEdge>],
) -> bool {
    forest.len() == expected.len()
        && gradients.len() == expected.len()
        && forest.iter().zip(gradients).zip(expected).all(
            |((forest_edge, gradient), expected_edge)| {
                source_slot_matches(
                    forest_edge.as_ref(),
                    gradient.as_ref(),
                    expected_edge.as_ref(),
                )
            },
        )
}

fn source_slot_matches(
    forest: Option<&DynamicLowStretchForestEdge>,
    gradient: Option<&num_rational::BigRational>,
    expected: Option<&super::DynamicLevelEdge>,
) -> bool {
    match (forest, gradient, expected) {
        (None, None, None) => true,
        (Some(forest), Some(gradient), Some(expected)) => {
            forest.edge == expected.edge
                && forest.from == expected.from
                && forest.to == expected.to
                && forest.length == expected.length
                && gradient == &expected.gradient
        }
        _ => false,
    }
}

fn audit_source_slots_match(
    forest: &[Option<DynamicLowStretchForestEdge>],
    gradients: &[Option<num_rational::BigRational>],
    expected: &[Option<super::DynamicLevelEdge>],
) -> bool {
    if forest.len() != expected.len() || gradients.len() != expected.len() {
        return false;
    }
    forest.iter().zip(gradients).zip(expected).all(
        |((forest_edge, gradient), expected_edge)| match (forest_edge, gradient, expected_edge) {
            (None, None, None) => true,
            (Some(forest), Some(gradient), Some(expected)) => {
                forest.edge == expected.edge
                    && forest.from == expected.from
                    && forest.to == expected.to
                    && forest.length == expected.length
                    && *gradient == expected.gradient
            }
            _ => false,
        },
    )
}

fn account_level(
    metrics: &mut DynamicTreeChainPropagationMetrics,
    result: &DynamicActiveBranchProjectionResult,
) -> Result<(), DynamicTreeChainPropagationError> {
    metrics.levels = increment(metrics.levels)?;
    metrics.level_stage_applications = add(
        metrics.level_stage_applications,
        result.metrics.outer_stages,
    )?;
    metrics.propagated_records = add(
        metrics.propagated_records,
        result.metrics.propagated_records,
    )?;
    Ok(())
}

fn audit_account_level(
    metrics: &mut DynamicTreeChainPropagationMetrics,
    result: &DynamicActiveBranchProjectionResult,
) -> Result<(), DynamicTreeChainPropagationError> {
    metrics.levels = audit_increment(metrics.levels)?;
    metrics.level_stage_applications = audit_add(
        metrics.level_stage_applications,
        result.metrics.outer_stages,
    )?;
    metrics.propagated_records = audit_add(
        metrics.propagated_records,
        result.metrics.propagated_records,
    )?;
    Ok(())
}

fn count_records(
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<u64, DynamicTreeChainPropagationError> {
    let total = batches.iter().try_fold(0usize, |total, batch| {
        total.checked_add(batch.updates.len())
    });
    u64::try_from(total.ok_or(DynamicTreeChainPropagationError::ArithmeticOverflow)?)
        .map_err(|_| DynamicTreeChainPropagationError::ArithmeticOverflow)
}

fn audit_count_records(
    batches: &[DynamicCoreGraphStageBatch],
) -> Result<u64, DynamicTreeChainPropagationError> {
    let mut total = 0u64;
    for batch in batches {
        total = total
            .checked_add(
                u64::try_from(batch.updates.len())
                    .map_err(|_| DynamicTreeChainPropagationError::TraceVerification)?,
            )
            .ok_or(DynamicTreeChainPropagationError::TraceVerification)?;
    }
    Ok(total)
}

fn increment(value: u64) -> Result<u64, DynamicTreeChainPropagationError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainPropagationError::ArithmeticOverflow)
}

fn add(left: u64, right: u64) -> Result<u64, DynamicTreeChainPropagationError> {
    left.checked_add(right)
        .ok_or(DynamicTreeChainPropagationError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicTreeChainPropagationError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainPropagationError::TraceVerification)
}

fn audit_add(left: u64, right: u64) -> Result<u64, DynamicTreeChainPropagationError> {
    left.checked_add(right)
        .ok_or(DynamicTreeChainPropagationError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use super::*;
    use crate::{
        DynamicCoreGraphInput, DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate,
        DynamicLowStretchForestInput, DynamicSparseCoreCollectionInput, DynamicSparseCoreInput,
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

    fn root_branch(reference_tree_edges: Vec<usize>) -> DynamicSparseCoreInput {
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

    fn root_level() -> DynamicActiveBranchProjectionInput {
        DynamicActiveBranchProjectionInput {
            collection: DynamicSparseCoreCollectionInput {
                branches: vec![root_branch(vec![0, 1, 2]), root_branch(vec![0, 2, 3])],
            },
            active_branch: 1,
        }
    }

    fn child_branch() -> DynamicSparseCoreInput {
        DynamicSparseCoreInput {
            core: DynamicCoreGraphInput {
                forest: DynamicLowStretchForestInput {
                    initial_node_count: 1,
                    maximum_node_count: 5,
                    edge_slots: vec![None, None, None, None, None],
                    reference_tree_edges: vec![],
                    reference_root: 0,
                    initial_root_seeds: vec![0],
                    initial_stretch_overestimates: None,
                },
                initial_gradients: vec![None, None, None, None, None],
            },
            branches: 2,
        }
    }

    fn child_level() -> DynamicActiveBranchProjectionInput {
        DynamicActiveBranchProjectionInput {
            collection: DynamicSparseCoreCollectionInput {
                branches: vec![child_branch(), child_branch()],
            },
            active_branch: 0,
        }
    }

    fn root_batches() -> Vec<DynamicCoreGraphStageBatch> {
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

    fn input() -> DynamicTreeChainPropagationInput {
        DynamicTreeChainPropagationInput {
            levels: vec![root_level(), child_level()],
        }
    }

    #[test]
    fn two_levels_advance_all_branches_and_preserve_one_stage_per_level() {
        let input = input();
        let batches = root_batches();
        let fast = execute_dynamic_tree_chain_propagation(&input, &batches).expect("fast");
        let trace = trace_dynamic_tree_chain_propagation(&input, &batches).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(fast.level_results.len(), 2);
        assert_eq!(fast.metrics.levels, 2);
        assert_eq!(fast.metrics.level_stage_applications, 2);
        for level in &fast.level_results {
            assert!(
                level
                    .final_collection
                    .branch_snapshots
                    .iter()
                    .all(|branch| branch.stage == 1)
            );
            assert_eq!(level.child_batches.len(), 1);
            assert_eq!(level.child_batches[0].outer_stage, 1);
        }
        assert_eq!(fast.terminal_batches.len(), 1);
        check_dynamic_tree_chain_propagation_trace(&input, &batches, &trace).expect("check");
    }

    #[test]
    fn mismatched_child_base_graph_fails_closed() {
        let mut input = input();
        input.levels[1].collection.branches[0]
            .core
            .forest
            .initial_node_count = 2;
        assert_eq!(
            execute_dynamic_tree_chain_propagation(&input, &root_batches()),
            Err(DynamicTreeChainPropagationError::InvalidInput)
        );
    }

    #[test]
    fn checker_rejects_intermediate_and_terminal_stream_tampering() {
        let input = input();
        let batches = root_batches();
        let trace = trace_dynamic_tree_chain_propagation(&input, &batches).expect("trace");

        let mut tampered = trace.clone();
        tampered.level_traces[1].events[0]
            .adapter_trace
            .result
            .batch
            .updates
            .clear();
        assert!(matches!(
            check_dynamic_tree_chain_propagation_trace(&input, &batches, &tampered),
            Err(DynamicTreeChainPropagationError::Level { level: 1, .. })
        ));

        let mut tampered = trace;
        tampered.result.terminal_batches.clear();
        assert_eq!(
            check_dynamic_tree_chain_propagation_trace(&input, &batches, &tampered),
            Err(DynamicTreeChainPropagationError::TraceVerification)
        );
    }
}
