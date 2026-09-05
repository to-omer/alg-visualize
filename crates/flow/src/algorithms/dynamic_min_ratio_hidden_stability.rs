//! Post-execution Definition 4.4 audit for topology-aware Algorithm 2.
//!
//! The observable runner never receives hidden circulation or width values.
//! This boundary first checks the completed dynamic min-ratio transcript and
//! the independently produced hidden-witness transcript. It then aligns every
//! root topology stage by stable edge slot, endpoint, length, gradient, and
//! insertion epoch. Only a root `Insert` or `Reinsert` may reset a continuing
//! hidden edge epoch; the first observed stage explicitly starts every active
//! edge because no earlier witness history is supplied.

use std::collections::BTreeSet;

use num_rational::BigRational;
use num_traits::Zero;
use thiserror::Error;

use super::{
    DynamicCoreGraphStageUpdate, DynamicHiddenStabilityCertificate,
    DynamicHiddenStabilityStageCertificate, DynamicMinRatioCycleConfig, DynamicMinRatioCycleError,
    DynamicMinRatioCycleEventKind, DynamicMinRatioCycleOperation, DynamicMinRatioCycleTraceResult,
    DynamicTreeChainEpochRuntimeState, HiddenStableWitnessConfig, HiddenStableWitnessError,
    HiddenStableWitnessEventKind, HiddenStableWitnessStage, HiddenStableWitnessTraceResult,
    check_dynamic_min_ratio_cycle_trace, check_hidden_stable_witness_trace,
};

/// Read-only inputs for one topology-aware post-execution isolation audit.
pub struct DynamicMinRatioHiddenStabilityAudit<'a> {
    /// Initial witness-free topology runtime.
    pub initial_runtime: &'a DynamicTreeChainEpochRuntimeState,
    /// Observable Algorithm 2 configuration.
    pub dynamic_config: &'a DynamicMinRatioCycleConfig,
    /// Public requests supplied to the witness-free runner.
    pub operations: &'a [DynamicMinRatioCycleOperation],
    /// Completed observable transcript.
    pub observable_trace: &'a DynamicMinRatioCycleTraceResult,
    /// Hidden verifier configuration.
    pub witness_config: &'a HiddenStableWitnessConfig,
    /// Hidden stages unavailable to the observable runner.
    pub witness_stages: &'a [HiddenStableWitnessStage],
    /// Independently checked hidden-witness transcript.
    pub witness_trace: &'a HiddenStableWitnessTraceResult,
    /// Exact approximation factor relating `alpha` to `kappa_alpha`.
    pub kappa: &'a BigRational,
}

/// Explicit failure of the topology-aware hidden-stability boundary.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicMinRatioHiddenStabilityError {
    /// `kappa`, root universe, or component cardinality is malformed.
    #[error("topology-aware hidden-stability audit input is invalid")]
    InvalidInput,
    /// Observable Algorithm 2 transcript is invalid.
    #[error(transparent)]
    Observable(#[from] DynamicMinRatioCycleError),
    /// Hidden witness transcript is invalid.
    #[error(transparent)]
    Hidden(#[from] HiddenStableWitnessError),
    /// Stage, topology, attribute, insertion epoch, threshold, or ratio alignment failed.
    #[error("topology-aware hidden-stability isolation mismatch")]
    IsolationMismatch,
}

struct ObservableStage<'a> {
    stage: u64,
    graph: &'a super::DynamicLevelGraphSnapshot,
    explicit_insertions: BTreeSet<usize>,
    cycle_ratio: BigRational,
}

/// Certifies Definition 4.4 without exposing a witness to Algorithm 2.
///
/// Both component transcripts are checked before any cross-contract data is
/// read. The returned certificate contains ratios only; hidden circulation and
/// width coordinates remain confined to the supplied audit input.
///
/// # Errors
///
/// Rejects either component transcript, a nonpositive `kappa`, a universe or
/// threshold mismatch, stage drift, stable-slot drift, topology/attribute
/// drift, or an insertion-epoch reset not justified by public root updates.
pub fn check_dynamic_min_ratio_hidden_stability_isolation(
    audit: &DynamicMinRatioHiddenStabilityAudit<'_>,
) -> Result<DynamicHiddenStabilityCertificate, DynamicMinRatioHiddenStabilityError> {
    if audit.kappa <= &BigRational::zero() {
        return Err(DynamicMinRatioHiddenStabilityError::InvalidInput);
    }
    check_dynamic_min_ratio_cycle_trace(
        audit.initial_runtime,
        audit.dynamic_config,
        audit.operations,
        audit.observable_trace,
    )?;
    check_hidden_stable_witness_trace(
        audit.witness_config,
        audit.witness_stages,
        audit.witness_trace,
    )?;

    let root_config = audit
        .dynamic_config
        .level_configs
        .first()
        .ok_or(DynamicMinRatioHiddenStabilityError::InvalidInput)?;
    let runtime_root = audit
        .initial_runtime
        .levels
        .first()
        .and_then(|level| level.input.collection.branches.first())
        .ok_or(DynamicMinRatioHiddenStabilityError::InvalidInput)?;
    if root_config.maximum_node_count != runtime_root.core.forest.maximum_node_count
        || root_config.stable_edge_slots != runtime_root.core.forest.edge_slots.len()
        || audit.witness_config.node_count != root_config.maximum_node_count
        || audit.kappa * &audit.witness_config.alpha != audit.dynamic_config.kappa_alpha
    {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }

    let observable = extract_observable_stages(audit.operations, audit.observable_trace)?;
    let hidden_ratios = extract_hidden_ratios(audit.witness_stages.len(), audit.witness_trace)?;
    if observable.len() != audit.witness_stages.len() || observable.len() != hidden_ratios.len() {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }

    let mut stages = Vec::with_capacity(observable.len());
    for (index, ((observable, witness), hidden_ratio)) in observable
        .iter()
        .zip(audit.witness_stages)
        .zip(hidden_ratios)
        .enumerate()
    {
        align_stage(index, observable, witness)?;
        stages.push(DynamicHiddenStabilityStageCertificate {
            update_stage: observable.stage,
            hidden_ratio,
            observable_cycle_ratio: observable.cycle_ratio.clone(),
        });
    }
    Ok(DynamicHiddenStabilityCertificate {
        kappa: audit.kappa.clone(),
        kappa_alpha: audit.dynamic_config.kappa_alpha.clone(),
        stages,
    })
}

fn extract_observable_stages<'a>(
    operations: &[DynamicMinRatioCycleOperation],
    trace: &'a DynamicMinRatioCycleTraceResult,
) -> Result<Vec<ObservableStage<'a>>, DynamicMinRatioHiddenStabilityError> {
    let update_batches = operations
        .iter()
        .filter_map(|operation| match operation {
            DynamicMinRatioCycleOperation::Update { updates, .. } => Some(updates),
            DynamicMinRatioCycleOperation::SourceProgressUpdate { updates }
                if !updates.is_empty() =>
            {
                Some(updates)
            }
            DynamicMinRatioCycleOperation::SourceProgressUpdate { .. }
            | DynamicMinRatioCycleOperation::Query { .. }
            | DynamicMinRatioCycleOperation::Detect => None,
        })
        .collect::<Vec<_>>();
    let mut topology = Vec::with_capacity(update_batches.len());
    for event in &trace.events {
        let DynamicMinRatioCycleEventKind::TopologyStageApplied {
            stage,
            runtime_trace,
            ..
        } = &event.kind
        else {
            continue;
        };
        let level = runtime_trace
            .result
            .final_materialization
            .epoch_snapshot
            .levels
            .first()
            .ok_or(DynamicMinRatioHiddenStabilityError::IsolationMismatch)?;
        topology.push((*stage, &level.source_graph));
    }
    let ratios = trace
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                DynamicMinRatioCycleEventKind::FlowApplied { .. }
            )
        })
        .map(|event| {
            event
                .after
                .last_candidate
                .as_ref()
                .map(|candidate| candidate.ratio.clone())
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(DynamicMinRatioHiddenStabilityError::IsolationMismatch)?;
    if topology.len() != update_batches.len() || ratios.len() != update_batches.len() {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }
    topology
        .into_iter()
        .zip(update_batches)
        .zip(ratios)
        .enumerate()
        .map(|(index, (((stage, graph), updates), cycle_ratio))| {
            let expected_stage = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or(DynamicMinRatioHiddenStabilityError::IsolationMismatch)?;
            if stage != expected_stage {
                return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
            }
            Ok(ObservableStage {
                stage,
                graph,
                explicit_insertions: explicit_insertions(updates),
                cycle_ratio,
            })
        })
        .collect()
}

fn explicit_insertions(updates: &[DynamicCoreGraphStageUpdate]) -> BTreeSet<usize> {
    updates
        .iter()
        .filter_map(|update| match update {
            DynamicCoreGraphStageUpdate::Insert { edge } => Some(edge.edge),
            DynamicCoreGraphStageUpdate::Reinsert { after, .. } => Some(after.edge),
            DynamicCoreGraphStageUpdate::Delete { .. }
            | DynamicCoreGraphStageUpdate::ReplaceAttributes { .. }
            | DynamicCoreGraphStageUpdate::VertexSplit { .. } => None,
        })
        .collect()
}

fn align_stage(
    index: usize,
    observable: &ObservableStage<'_>,
    witness: &HiddenStableWitnessStage,
) -> Result<(), DynamicMinRatioHiddenStabilityError> {
    let active_count = observable
        .graph
        .edge_slots
        .iter()
        .filter(|row| row.is_some())
        .count();
    if witness.edges.len() != active_count {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }
    let mut witness_rows = witness.edges.iter();
    for (slot, row) in observable.graph.edge_slots.iter().enumerate() {
        let Some(row) = row else {
            continue;
        };
        let hidden = witness_rows
            .next()
            .ok_or(DynamicMinRatioHiddenStabilityError::IsolationMismatch)?;
        let explicitly_inserted = index == 0 || observable.explicit_insertions.contains(&slot);
        if row.edge != slot
            || hidden.edge != slot
            || hidden.from != row.from
            || hidden.to != row.to
            || hidden.length != row.length
            || hidden.gradient != row.gradient
            || hidden.explicitly_inserted != explicitly_inserted
        {
            return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
        }
    }
    if witness_rows.next().is_some() {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }
    Ok(())
}

fn extract_hidden_ratios(
    stage_count: usize,
    trace: &HiddenStableWitnessTraceResult,
) -> Result<Vec<BigRational>, DynamicMinRatioHiddenStabilityError> {
    let ratios = trace
        .events
        .iter()
        .take(stage_count)
        .map(|event| match &event.kind {
            HiddenStableWitnessEventKind::StageVerified { certificate } => {
                Ok(certificate.ratio.clone())
            }
            HiddenStableWitnessEventKind::Completed => {
                Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ratios.len() != stage_count {
        return Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch);
    }
    Ok(ratios)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::{
        DynamicActiveBranchProjectionInput, DynamicCoreEncodedSide, DynamicCoreGraphStageEdge,
        DynamicCoreIncidence, DynamicCoreIncidenceEndpoint, DynamicMwuCollectionBridgeConfig,
        DynamicTreeChainPropagationInput, HiddenStableEdgeWitness, ShiftedTreeChainEdge,
        ShiftedTreeChainGraph, initialize_dynamic_level_projection,
        initialize_dynamic_tree_chain_epoch_runtime, trace_dynamic_min_ratio_cycle,
        trace_dynamic_mwu_sparse_core_collection, trace_dynamic_tree_chain_propagation,
        trace_hidden_stable_witness,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn ratio(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
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

    fn root_graph() -> ShiftedTreeChainGraph {
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

    fn bridge_config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: 5,
        }
    }

    fn shifted(graph: &super::super::DynamicLevelGraphSnapshot) -> ShiftedTreeChainGraph {
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

    fn runtime() -> DynamicTreeChainEpochRuntimeState {
        let root =
            trace_dynamic_mwu_sparse_core_collection(&root_graph(), bridge_config()).expect("root");
        let root_input = DynamicActiveBranchProjectionInput {
            collection: root.result.collection,
            active_branch: 0,
        };
        let child_graph = initialize_dynamic_level_projection(
            &root.result.initialized.final_snapshot.branch_snapshots[0],
        )
        .expect("projection")
        .graph;
        let child =
            trace_dynamic_mwu_sparse_core_collection(&shifted(&child_graph), bridge_config())
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
            kappa_alpha: ratio(1, 1_000),
            epsilon: ratio(1, 1_000),
            rebuild_after_updates: vec![1_000; 2],
        }
    }

    fn inserted_row() -> DynamicCoreGraphStageEdge {
        DynamicCoreGraphStageEdge {
            edge: 1,
            from: 0,
            to: 2,
            length: rational(1),
            gradient: rational(-10),
        }
    }

    fn operations() -> Vec<DynamicMinRatioCycleOperation> {
        let inserted = inserted_row();
        let moved = DynamicCoreGraphStageEdge {
            from: 4,
            ..inserted.clone()
        };
        let reinserted = DynamicCoreGraphStageEdge {
            edge: 0,
            from: 0,
            to: 1,
            length: rational(1),
            gradient: rational(2),
        };
        vec![
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Insert {
                    edge: inserted.clone(),
                }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::VertexSplit {
                    retained_vertex: 0,
                    new_vertex: 4,
                    new_side_incidences: vec![DynamicCoreIncidence {
                        edge: 1,
                        endpoint: DynamicCoreIncidenceEndpoint::Tail,
                    }],
                    encoded_side: DynamicCoreEncodedSide::New,
                    encoded_incidences: vec![DynamicCoreIncidence {
                        edge: 1,
                        endpoint: DynamicCoreIncidenceEndpoint::Tail,
                    }],
                }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Delete { edge: moved }],
                eta: rational(1),
            },
            DynamicMinRatioCycleOperation::Update {
                updates: vec![DynamicCoreGraphStageUpdate::Reinsert {
                    before: reinserted.clone(),
                    after: reinserted,
                }],
                eta: rational(1),
            },
        ]
    }

    fn hidden_row(
        edge: usize,
        endpoints: (usize, usize),
        attributes: [i64; 4],
        explicitly_inserted: bool,
    ) -> HiddenStableEdgeWitness {
        let (from, to) = endpoints;
        let [length, gradient, circulation, width] = attributes;
        HiddenStableEdgeWitness {
            edge,
            from,
            to,
            explicitly_inserted,
            length: rational(length),
            gradient: rational(gradient),
            circulation: rational(circulation),
            width: rational(width),
        }
    }

    fn original_rows(explicit_zero: bool) -> Vec<HiddenStableEdgeWitness> {
        vec![
            hidden_row(0, (0, 1), [1, 2, -1, 1], explicit_zero),
            hidden_row(2, (1, 2), [1, 3, -1, 1], false),
            hidden_row(3, (2, 3), [1, 5, -1, 1], false),
            hidden_row(4, (0, 3), [2, 7, 1, 2], false),
        ]
    }

    fn stages() -> Vec<HiddenStableWitnessStage> {
        let mut first = original_rows(true);
        first.insert(1, hidden_row(1, (0, 2), [1, -10, 0, 1], true));
        for row in &mut first {
            row.explicitly_inserted = true;
        }
        let mut split = original_rows(false);
        split.insert(1, hidden_row(1, (4, 2), [1, -10, 0, 1], false));
        let deleted = original_rows(false);
        let reinserted = original_rows(true)
            .into_iter()
            .enumerate()
            .map(|(index, mut row)| {
                row.explicitly_inserted = index == 0;
                row
            })
            .collect();
        vec![
            HiddenStableWitnessStage { edges: first },
            HiddenStableWitnessStage { edges: split },
            HiddenStableWitnessStage { edges: deleted },
            HiddenStableWitnessStage { edges: reinserted },
        ]
    }

    fn witness_config() -> HiddenStableWitnessConfig {
        HiddenStableWitnessConfig {
            node_count: 5,
            alpha: ratio(1, 1_000),
            scalar_lower: ratio(1, 1_000),
            scalar_upper: rational(10),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn audit<'a>(
        state: &'a DynamicTreeChainEpochRuntimeState,
        dynamic_config: &'a DynamicMinRatioCycleConfig,
        operations: &'a [DynamicMinRatioCycleOperation],
        observable: &'a DynamicMinRatioCycleTraceResult,
        witness_config: &'a HiddenStableWitnessConfig,
        stages: &'a [HiddenStableWitnessStage],
        witness: &'a HiddenStableWitnessTraceResult,
        kappa: &'a BigRational,
    ) -> DynamicMinRatioHiddenStabilityAudit<'a> {
        DynamicMinRatioHiddenStabilityAudit {
            initial_runtime: state,
            dynamic_config,
            operations,
            observable_trace: observable,
            witness_config,
            witness_stages: stages,
            witness_trace: witness,
            kappa,
        }
    }

    #[test]
    fn certifies_insert_split_delete_and_reinsert_without_witness_leakage() {
        let state = runtime();
        let dynamic_config = config();
        let operations = operations();
        let observable = trace_dynamic_min_ratio_cycle(&state, &dynamic_config, &operations)
            .expect("observable");
        let stages = stages();
        let witness_config = witness_config();
        let witness =
            trace_hidden_stable_witness(&witness_config, &stages).expect("hidden witness");
        let kappa = rational(1);
        let certificate = check_dynamic_min_ratio_hidden_stability_isolation(&audit(
            &state,
            &dynamic_config,
            &operations,
            &observable,
            &witness_config,
            &stages,
            &witness,
            &kappa,
        ))
        .expect("isolation");
        assert_eq!(certificate.stages.len(), 4);
        assert_eq!(certificate.stages[0].hidden_ratio, ratio(-1, 2));
        assert_eq!(certificate.stages[2].hidden_ratio, ratio(-3, 5));
        assert!(
            certificate
                .stages
                .iter()
                .all(|stage| stage.observable_cycle_ratio > dynamic_config.kappa_alpha)
        );
    }

    #[test]
    fn rejects_unjustified_epoch_reset_endpoint_drift_and_deleted_slot_reappearance() {
        let state = runtime();
        let dynamic_config = config();
        let operations = operations();
        let observable = trace_dynamic_min_ratio_cycle(&state, &dynamic_config, &operations)
            .expect("observable");
        let witness_config = witness_config();
        let kappa = rational(1);

        let mut epoch = stages();
        epoch[1].edges[0].explicitly_inserted = true;
        let epoch_trace =
            trace_hidden_stable_witness(&witness_config, &epoch).expect("component accepts reset");
        assert_eq!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &observable,
                &witness_config,
                &epoch,
                &epoch_trace,
                &kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
        );

        let mut endpoint = stages();
        endpoint[1].edges[1].from = 0;
        let endpoint_trace = trace_hidden_stable_witness(&witness_config, &endpoint)
            .expect("zero-coordinate endpoint remains a witness");
        assert_eq!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &observable,
                &witness_config,
                &endpoint,
                &endpoint_trace,
                &kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
        );

        let mut hole = stages();
        hole[2]
            .edges
            .insert(1, hidden_row(1, (4, 2), [1, -10, 0, 1], true));
        let hole_trace = trace_hidden_stable_witness(&witness_config, &hole)
            .expect("extra zero row is feasible");
        assert_eq!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &observable,
                &witness_config,
                &hole,
                &hole_trace,
                &kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
        );
    }

    #[test]
    fn rejects_component_threshold_and_universe_tampering() {
        let state = runtime();
        let dynamic_config = config();
        let operations = operations();
        let observable = trace_dynamic_min_ratio_cycle(&state, &dynamic_config, &operations)
            .expect("observable");
        let stages = stages();
        let witness_config = witness_config();
        let witness =
            trace_hidden_stable_witness(&witness_config, &stages).expect("hidden witness");
        let kappa = rational(1);

        let mut component = observable.clone();
        component.result.final_snapshot.metrics.updates += 1;
        assert!(matches!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &component,
                &witness_config,
                &stages,
                &witness,
                &kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::Observable(_))
        ));

        let wrong_kappa = rational(2);
        assert_eq!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &observable,
                &witness_config,
                &stages,
                &witness,
                &wrong_kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
        );

        let mut universe = witness_config.clone();
        universe.node_count = 6;
        let universe_trace =
            trace_hidden_stable_witness(&universe, &stages).expect("unused node is not needed");
        assert_eq!(
            check_dynamic_min_ratio_hidden_stability_isolation(&audit(
                &state,
                &dynamic_config,
                &operations,
                &observable,
                &universe,
                &stages,
                &universe_trace,
                &kappa,
            )),
            Err(DynamicMinRatioHiddenStabilityError::IsolationMismatch)
        );
    }
}
