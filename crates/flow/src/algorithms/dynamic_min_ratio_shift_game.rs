//! Post-execution isolation bridge from Algorithm 2 to Algorithm 3.
//!
//! Algorithm 2 never observes hidden witness weights. This module first checks
//! a completed topology-aware min-ratio-cycle transcript, then derives only its
//! public rebuild level and unsuccessful-query/shift count for each update
//! round. Caller-supplied hidden weights are passed to the independently
//! checked shift-and-rebuild game after execution. A final cross-contract audit
//! compares branch indices, passes, level shift/rebuild counts, and rounds.

use num_rational::BigRational;
use thiserror::Error;

use super::{
    DynamicMinRatioCycleConfig, DynamicMinRatioCycleError, DynamicMinRatioCycleEventKind,
    DynamicMinRatioCycleOperation, DynamicMinRatioCycleTraceResult,
    DynamicTreeChainEpochRuntimeState, ShiftRebuildGameConfig, ShiftRebuildGameError,
    ShiftRebuildGameTraceResult, ShiftRebuildRound, check_dynamic_min_ratio_cycle_trace,
    check_shift_rebuild_game_trace, materialize_dynamic_tree_chain_epoch_runtime,
    trace_shift_rebuild_game,
};

/// Derived public game rounds and independently checked Algorithm 3 transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicMinRatioShiftGameAudit {
    /// One round per topology-aware `Update`, in request order.
    pub rounds: Vec<ShiftRebuildRound>,
    /// Exact Algorithm 3 replay using caller-supplied hidden weights.
    pub game_trace: ShiftRebuildGameTraceResult,
}

/// Explicit isolation-bridge failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicMinRatioShiftGameError {
    /// Level count, uniform branch contract, initial branch, weight count, or event stream is invalid.
    #[error("dynamic min-ratio shift-game input is invalid")]
    InvalidInput,
    /// The checked Algorithm 2 transcript failed.
    #[error("dynamic min-ratio shift-game Algorithm 2 failed: {0}")]
    MinRatio(#[from] DynamicMinRatioCycleError),
    /// The independently checked Algorithm 3 transcript failed.
    #[error("dynamic min-ratio shift-game Algorithm 3 failed: {0}")]
    Game(#[from] ShiftRebuildGameError),
    /// Public rounds, counters, passes, or branch identities disagree.
    #[error("dynamic min-ratio shift-game cross-contract verification failed")]
    TraceVerification,
}

/// Builds the isolated Algorithm 3 replay after checking Algorithm 2.
///
/// Hidden weights are consumed only after the min-ratio transcript has already
/// fixed every rebuild and shift choice.
///
/// # Errors
///
/// Rejects either component transcript, invalid uniform game parameters,
/// hidden-weight cardinality, malformed public round extraction, or a
/// cross-contract mismatch.
pub fn trace_dynamic_min_ratio_shift_game_isolation(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
    min_ratio_trace: &DynamicMinRatioCycleTraceResult,
    hidden_weights: &[BigRational],
) -> Result<DynamicMinRatioShiftGameAudit, DynamicMinRatioShiftGameError> {
    check_dynamic_min_ratio_cycle_trace(initial_runtime, config, operations, min_ratio_trace)?;
    let game_config = game_config(initial_runtime, config)?;
    let rounds = derive_rounds(operations, min_ratio_trace, hidden_weights, false)?;
    let game_trace = trace_shift_rebuild_game(game_config, &rounds)?;
    let audit = DynamicMinRatioShiftGameAudit { rounds, game_trace };
    check_dynamic_min_ratio_shift_game_isolation(
        initial_runtime,
        config,
        operations,
        min_ratio_trace,
        hidden_weights,
        &audit,
    )?;
    Ok(audit)
}

/// Independently checks both algorithms and their public schedule alignment.
///
/// # Errors
///
/// Rejects component drift, hidden-weight leakage into round extraction,
/// rebuild/continuation mismatch, counter drift, pass drift, or final branch
/// disagreement.
pub fn check_dynamic_min_ratio_shift_game_isolation(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
    operations: &[DynamicMinRatioCycleOperation],
    min_ratio_trace: &DynamicMinRatioCycleTraceResult,
    hidden_weights: &[BigRational],
    audit: &DynamicMinRatioShiftGameAudit,
) -> Result<(), DynamicMinRatioShiftGameError> {
    check_dynamic_min_ratio_cycle_trace(initial_runtime, config, operations, min_ratio_trace)?;
    let game_config = game_config(initial_runtime, config)?;
    let expected_rounds = derive_rounds(operations, min_ratio_trace, hidden_weights, true)?;
    if audit.rounds != expected_rounds {
        return Err(DynamicMinRatioShiftGameError::TraceVerification);
    }
    check_shift_rebuild_game_trace(game_config, &expected_rounds, &audit.game_trace)?;
    cross_check(initial_runtime, min_ratio_trace, &audit.game_trace)
}

fn game_config(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    config: &DynamicMinRatioCycleConfig,
) -> Result<ShiftRebuildGameConfig, DynamicMinRatioShiftGameError> {
    let levels = initial_runtime.levels.len();
    let depth = levels
        .checked_sub(1)
        .ok_or(DynamicMinRatioShiftGameError::InvalidInput)?;
    let branches = config
        .level_configs
        .first()
        .ok_or(DynamicMinRatioShiftGameError::InvalidInput)?
        .branches;
    if branches == 0
        || config.level_configs.len() != levels
        || config
            .level_configs
            .iter()
            .any(|level| level.branches != branches)
        || initial_runtime
            .levels
            .iter()
            .any(|level| level.input.active_branch != 0)
    {
        return Err(DynamicMinRatioShiftGameError::InvalidInput);
    }
    Ok(ShiftRebuildGameConfig {
        depth,
        branches,
        psi: config.psi,
    })
}

fn derive_rounds(
    operations: &[DynamicMinRatioCycleOperation],
    trace: &DynamicMinRatioCycleTraceResult,
    hidden_weights: &[BigRational],
    audit: bool,
) -> Result<Vec<ShiftRebuildRound>, DynamicMinRatioShiftGameError> {
    let update_count = operations
        .iter()
        .filter(|operation| {
            matches!(
                operation,
                DynamicMinRatioCycleOperation::Update { .. }
                    | DynamicMinRatioCycleOperation::SourceProgressUpdate { .. }
            )
        })
        .count();
    if hidden_weights.len() != update_count {
        return Err(input_or_trace(audit));
    }
    let mut event_index = 0_usize;
    let mut weight_index = 0_usize;
    let mut rounds = Vec::with_capacity(update_count);
    for operation in operations {
        match operation {
            DynamicMinRatioCycleOperation::Update { .. }
            | DynamicMinRatioCycleOperation::SourceProgressUpdate { .. } => {
                rounds.push(derive_update_round(
                    operation,
                    trace,
                    &mut event_index,
                    &hidden_weights[weight_index],
                    audit,
                )?);
                weight_index += 1;
            }
            DynamicMinRatioCycleOperation::Query { .. } => {
                if !matches!(
                    event_kind(trace, event_index, audit)?,
                    DynamicMinRatioCycleEventKind::QueryReturned { .. }
                ) {
                    return Err(input_or_trace(audit));
                }
                event_index += 1;
            }
            DynamicMinRatioCycleOperation::Detect => {
                if !matches!(
                    event_kind(trace, event_index, audit)?,
                    DynamicMinRatioCycleEventKind::DetectReturned { .. }
                ) {
                    return Err(input_or_trace(audit));
                }
                event_index += 1;
            }
        }
    }
    if !matches!(
        event_kind(trace, event_index, audit)?,
        DynamicMinRatioCycleEventKind::Completed
    ) || event_index + 1 != trace.events.len()
    {
        return Err(input_or_trace(audit));
    }
    Ok(rounds)
}

fn derive_update_round(
    operation: &DynamicMinRatioCycleOperation,
    trace: &DynamicMinRatioCycleTraceResult,
    event_index: &mut usize,
    weight: &BigRational,
    audit: bool,
) -> Result<ShiftRebuildRound, DynamicMinRatioShiftGameError> {
    let has_topology_stage = match operation {
        DynamicMinRatioCycleOperation::Update { .. } => true,
        DynamicMinRatioCycleOperation::SourceProgressUpdate { updates } => !updates.is_empty(),
        DynamicMinRatioCycleOperation::Query { .. } | DynamicMinRatioCycleOperation::Detect => {
            return Err(input_or_trace(audit));
        }
    };
    let rebuild_level = if has_topology_stage {
        let first = event_kind(trace, *event_index, audit)?;
        if !matches!(
            first,
            DynamicMinRatioCycleEventKind::TopologyStageApplied { .. }
        ) {
            return Err(input_or_trace(audit));
        }
        *event_index += 1;
        match event_kind(trace, *event_index, audit)? {
            DynamicMinRatioCycleEventKind::PeriodicRebuilt { level, .. } => {
                *event_index += 1;
                Some(*level)
            }
            _ => None,
        }
    } else {
        None
    };
    let mut continuations = 0_u64;
    loop {
        let DynamicMinRatioCycleEventKind::CycleQueried { accepted, .. } =
            event_kind(trace, *event_index, audit)?
        else {
            return Err(input_or_trace(audit));
        };
        *event_index += 1;
        if *accepted {
            break;
        }
        if !matches!(
            event_kind(trace, *event_index, audit)?,
            DynamicMinRatioCycleEventKind::LevelShifted { .. }
        ) {
            return Err(input_or_trace(audit));
        }
        *event_index += 1;
        continuations = continuations
            .checked_add(1)
            .ok_or_else(|| input_or_trace(audit))?;
    }
    if !matches!(
        event_kind(trace, *event_index, audit)?,
        DynamicMinRatioCycleEventKind::FlowApplied { .. }
    ) {
        return Err(input_or_trace(audit));
    }
    *event_index += 1;
    Ok(ShiftRebuildRound {
        weight: weight.clone(),
        rebuild_level,
        continuations,
    })
}

fn event_kind(
    trace: &DynamicMinRatioCycleTraceResult,
    index: usize,
    audit: bool,
) -> Result<&DynamicMinRatioCycleEventKind, DynamicMinRatioShiftGameError> {
    trace
        .events
        .get(index)
        .map(|event| &event.kind)
        .ok_or_else(|| input_or_trace(audit))
}

fn cross_check(
    initial_runtime: &DynamicTreeChainEpochRuntimeState,
    min_ratio_trace: &DynamicMinRatioCycleTraceResult,
    game_trace: &ShiftRebuildGameTraceResult,
) -> Result<(), DynamicMinRatioShiftGameError> {
    let final_min_ratio = &min_ratio_trace.result.final_snapshot;
    let final_game = &game_trace.result.final_snapshot;
    let materialization = materialize_dynamic_tree_chain_epoch_runtime(&final_min_ratio.runtime)
        .map_err(DynamicMinRatioCycleError::from)?;
    let completed_updates = usize::try_from(final_min_ratio.stage)
        .map_err(|_| DynamicMinRatioShiftGameError::TraceVerification)?;
    if final_game.levels.len() != initial_runtime.levels.len()
        || final_min_ratio.passes.len() != final_game.levels.len()
        || final_min_ratio.metrics.level_shifts.len() != final_game.levels.len()
        || final_min_ratio.metrics.level_rebuilds.len() != final_game.levels.len()
        || final_game.completed_rounds != completed_updates
    {
        return Err(DynamicMinRatioShiftGameError::TraceVerification);
    }
    for (level, game) in final_game.levels.iter().enumerate() {
        let epoch = &materialization.epoch_snapshot.levels[level];
        if game.level != level
            || game.shift != epoch.active_branch
            || game.passes != final_min_ratio.passes[level]
            || game.shift_steps != final_min_ratio.metrics.level_shifts[level]
            || game.rebuild_steps != final_min_ratio.metrics.level_rebuilds[level]
        {
            return Err(DynamicMinRatioShiftGameError::TraceVerification);
        }
    }
    if game_trace.result.shifts != final_min_ratio.metrics.level_shifts
        || game_trace.result.rebuilds != final_min_ratio.metrics.level_rebuilds
    {
        return Err(DynamicMinRatioShiftGameError::TraceVerification);
    }
    Ok(())
}

fn input_or_trace(audit: bool) -> DynamicMinRatioShiftGameError {
    if audit {
        DynamicMinRatioShiftGameError::TraceVerification
    } else {
        DynamicMinRatioShiftGameError::InvalidInput
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::{
        DynamicActiveBranchProjectionInput, DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate,
        DynamicMwuCollectionBridgeConfig, DynamicTreeChainPropagationInput, ShiftedTreeChainEdge,
        ShiftedTreeChainGraph, initialize_dynamic_level_projection,
        initialize_dynamic_tree_chain_epoch_runtime, trace_dynamic_min_ratio_cycle,
        trace_dynamic_mwu_sparse_core_collection, trace_dynamic_tree_chain_propagation,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn bridge_config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: 5,
        }
    }

    fn graph() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                edge(0, 0, 1, 1, 2),
                edge(2, 1, 2, 1, 3),
                edge(3, 2, 3, 1, 5),
                edge(4, 0, 3, 2, 7),
            ],
        }
    }

    fn edge(
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

    fn runtime() -> DynamicTreeChainEpochRuntimeState {
        let root =
            trace_dynamic_mwu_sparse_core_collection(&graph(), bridge_config()).expect("root");
        let root_input = DynamicActiveBranchProjectionInput {
            collection: root.result.collection,
            active_branch: 0,
        };
        let child_graph = initialize_dynamic_level_projection(
            &root.result.initialized.final_snapshot.branch_snapshots[0],
        )
        .expect("projection")
        .graph;
        let child_shifted = ShiftedTreeChainGraph {
            node_count: child_graph.active_node_count,
            edges: child_graph
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
        };
        let child = trace_dynamic_mwu_sparse_core_collection(&child_shifted, bridge_config())
            .expect("child");
        let input = DynamicTreeChainPropagationInput {
            levels: vec![
                root_input,
                DynamicActiveBranchProjectionInput {
                    collection: child.result.collection,
                    active_branch: 0,
                },
            ],
        };
        let propagation = trace_dynamic_tree_chain_propagation(&input, &[]).expect("propagation");
        initialize_dynamic_tree_chain_epoch_runtime(&input, &[], &propagation).expect("runtime")
    }

    fn config() -> DynamicMinRatioCycleConfig {
        DynamicMinRatioCycleConfig {
            level_configs: vec![bridge_config(), bridge_config()],
            terminal_branches: 2,
            psi: 2,
            kappa_alpha: BigRational::new(BigInt::from(1), BigInt::from(1_000)),
            epsilon: BigRational::new(BigInt::from(1), BigInt::from(1_000)),
            rebuild_after_updates: vec![0, 0],
        }
    }

    fn operations() -> Vec<DynamicMinRatioCycleOperation> {
        vec![
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Insert {
                    edge: DynamicCoreGraphStageEdge {
                        edge: 1,
                        from: 0,
                        to: 2,
                        length: rational(1),
                        gradient: rational(-10),
                    },
                }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Query { edge: 1 },
        ]
    }

    #[test]
    fn hidden_weights_are_added_only_after_public_schedule_is_fixed() {
        let state = runtime();
        let config = config();
        let operations = operations();
        let min_ratio =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("min ratio");
        let audit = trace_dynamic_min_ratio_shift_game_isolation(
            &state,
            &config,
            &operations,
            &min_ratio,
            &[rational(3)],
        )
        .expect("audit");
        assert_eq!(audit.rounds[0].rebuild_level, Some(0));
        assert_eq!(audit.rounds[0].continuations, 0);
        assert_eq!(audit.game_trace.result.rebuilds, vec![1, 0]);
        check_dynamic_min_ratio_shift_game_isolation(
            &state,
            &config,
            &operations,
            &min_ratio,
            &[rational(3)],
            &audit,
        )
        .expect("check");
    }

    #[test]
    fn checker_rejects_weight_round_and_game_counter_tampering() {
        let state = runtime();
        let config = config();
        let operations = operations();
        let min_ratio =
            trace_dynamic_min_ratio_cycle(&state, &config, &operations).expect("min ratio");
        let source = trace_dynamic_min_ratio_shift_game_isolation(
            &state,
            &config,
            &operations,
            &min_ratio,
            &[rational(3)],
        )
        .expect("audit");

        assert!(
            check_dynamic_min_ratio_shift_game_isolation(
                &state,
                &config,
                &operations,
                &min_ratio,
                &[rational(4)],
                &source,
            )
            .is_err()
        );

        let mut round = source.clone();
        round.rounds[0].continuations += 1;
        assert_eq!(
            check_dynamic_min_ratio_shift_game_isolation(
                &state,
                &config,
                &operations,
                &min_ratio,
                &[rational(3)],
                &round,
            ),
            Err(DynamicMinRatioShiftGameError::TraceVerification)
        );

        let mut game = source;
        game.game_trace.result.final_snapshot.levels[0].rebuild_steps += 1;
        assert!(
            check_dynamic_min_ratio_shift_game_isolation(
                &state,
                &config,
                &operations,
                &min_ratio,
                &[rational(3)],
                &game,
            )
            .is_err()
        );
    }
}
