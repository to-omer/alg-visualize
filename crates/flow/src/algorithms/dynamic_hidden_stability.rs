//! Isolation certificate between observable Algorithm 2 state and hidden witnesses.
//!
//! The dynamic runner's API contains no hidden circulation or width. This
//! module is a separate audit boundary: after an observable transcript exists,
//! it independently checks that transcript, independently checks a Definition
//! 4.4 hidden-witness transcript, aligns their stages, and proves that every
//! witness row has exactly the graph endpoint/length/gradient visible at that
//! stage. The decision procedure therefore cannot inspect hidden `c` or `w`.

use num_rational::BigRational;
use num_traits::Zero;
use thiserror::Error;

use super::{
    DynamicShiftedTreeChainConfig, DynamicShiftedTreeChainError, DynamicShiftedTreeChainEventKind,
    DynamicShiftedTreeChainOperation, DynamicShiftedTreeChainTraceResult,
    HiddenStableWitnessConfig, HiddenStableWitnessError, HiddenStableWitnessEventKind,
    HiddenStableWitnessStage, HiddenStableWitnessTraceResult, ShiftedTreeChainGraph,
    check_dynamic_shifted_tree_chain_trace, check_hidden_stable_witness_trace,
};

/// One aligned stage in the post-execution isolation certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicHiddenStabilityStageCertificate {
    /// One-based observable update stage.
    pub update_stage: u64,
    /// Exact hidden objective ratio, validated independently.
    pub hidden_ratio: BigRational,
    /// Exact positive ratio of the cycle selected without witness access.
    pub observable_cycle_ratio: BigRational,
}

/// Complete hidden/observable isolation certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicHiddenStabilityCertificate {
    /// Exact `kappa` used to relate `alpha` to the observable threshold.
    pub kappa: BigRational,
    /// Exact equality `kappa * alpha = kappa_alpha` checked by the audit.
    pub kappa_alpha: BigRational,
    /// Aligned stage certificates.
    pub stages: Vec<DynamicHiddenStabilityStageCertificate>,
}

/// Read-only inputs for one post-execution isolation audit.
pub struct DynamicHiddenStabilityAudit<'a> {
    /// Initial observable graph.
    pub graph: &'a ShiftedTreeChainGraph,
    /// Observable dynamic configuration.
    pub dynamic_config: &'a DynamicShiftedTreeChainConfig,
    /// Observable operations supplied to the witness-free runner.
    pub operations: &'a [DynamicShiftedTreeChainOperation],
    /// Completed observable transcript.
    pub observable_trace: &'a DynamicShiftedTreeChainTraceResult,
    /// Hidden verifier configuration.
    pub witness_config: &'a HiddenStableWitnessConfig,
    /// Hidden stages unavailable to the observable runner.
    pub witness_stages: &'a [HiddenStableWitnessStage],
    /// Independently checked hidden-witness transcript.
    pub witness_trace: &'a HiddenStableWitnessTraceResult,
    /// Exact approximation factor relating `alpha` to `kappa_alpha`.
    pub kappa: &'a BigRational,
}

/// Explicit failure of the post-execution isolation boundary.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicHiddenStabilityError {
    /// `kappa` is not a positive exact scalar.
    #[error("dynamic hidden-stability isolation input is invalid")]
    InvalidInput,
    /// Observable dynamic transcript is invalid.
    #[error(transparent)]
    Observable(#[from] DynamicShiftedTreeChainError),
    /// Hidden witness transcript is invalid.
    #[error(transparent)]
    Hidden(#[from] HiddenStableWitnessError),
    /// Stage count, threshold, or observable graph attributes do not align.
    #[error("dynamic hidden-stability isolation mismatch")]
    IsolationMismatch,
}

/// Certifies hidden stability without passing a witness to the dynamic runner.
///
/// # Errors
///
/// Rejects either invalid component trace, a nonpositive `kappa`, threshold
/// mismatch, stage mismatch, or any endpoint/length/gradient disagreement.
pub fn check_dynamic_hidden_stability_isolation(
    audit: &DynamicHiddenStabilityAudit<'_>,
) -> Result<DynamicHiddenStabilityCertificate, DynamicHiddenStabilityError> {
    let DynamicHiddenStabilityAudit {
        graph,
        dynamic_config,
        operations,
        observable_trace,
        witness_config,
        witness_stages,
        witness_trace,
        kappa,
    } = audit;
    let kappa = *kappa;
    if kappa <= &BigRational::zero() {
        return Err(DynamicHiddenStabilityError::InvalidInput);
    }
    check_dynamic_shifted_tree_chain_trace(graph, dynamic_config, operations, observable_trace)?;
    check_hidden_stable_witness_trace(witness_config, witness_stages, witness_trace)?;
    if witness_config.node_count != graph.node_count
        || kappa * &witness_config.alpha != dynamic_config.kappa_alpha
    {
        return Err(DynamicHiddenStabilityError::IsolationMismatch);
    }

    let update_count = operations
        .iter()
        .filter(|operation| matches!(operation, DynamicShiftedTreeChainOperation::Update { .. }))
        .count();
    if witness_stages.len() != update_count {
        return Err(DynamicHiddenStabilityError::IsolationMismatch);
    }
    align_observable_graphs(graph, operations, witness_stages)?;
    let hidden_ratios = extract_hidden_ratios(witness_stages.len(), witness_trace)?;
    let observable_ratios = extract_observable_ratios(update_count, observable_trace)?;
    let stages = hidden_ratios
        .into_iter()
        .zip(observable_ratios)
        .enumerate()
        .map(|(stage, (hidden_ratio, observable_cycle_ratio))| {
            Ok(DynamicHiddenStabilityStageCertificate {
                update_stage: u64::try_from(stage)
                    .ok()
                    .and_then(|stage| stage.checked_add(1))
                    .ok_or(DynamicHiddenStabilityError::IsolationMismatch)?,
                hidden_ratio,
                observable_cycle_ratio,
            })
        })
        .collect::<Result<Vec<_>, DynamicHiddenStabilityError>>()?;
    Ok(DynamicHiddenStabilityCertificate {
        kappa: kappa.clone(),
        kappa_alpha: dynamic_config.kappa_alpha.clone(),
        stages,
    })
}

fn align_observable_graphs(
    initial_graph: &ShiftedTreeChainGraph,
    operations: &[DynamicShiftedTreeChainOperation],
    witness_stages: &[HiddenStableWitnessStage],
) -> Result<(), DynamicHiddenStabilityError> {
    let mut graph = initial_graph.clone();
    let mut witness_index = 0_usize;
    for operation in operations {
        let DynamicShiftedTreeChainOperation::Update { coordinates, .. } = operation else {
            continue;
        };
        for coordinate in coordinates {
            let edge = graph
                .edges
                .get_mut(coordinate.edge)
                .ok_or(DynamicHiddenStabilityError::IsolationMismatch)?;
            edge.length.clone_from(&coordinate.length);
            edge.gradient.clone_from(&coordinate.gradient);
        }
        let witness = witness_stages
            .get(witness_index)
            .ok_or(DynamicHiddenStabilityError::IsolationMismatch)?;
        if witness.edges.len() != graph.edges.len() {
            return Err(DynamicHiddenStabilityError::IsolationMismatch);
        }
        for (index, (edge, hidden)) in graph.edges.iter().zip(&witness.edges).enumerate() {
            if hidden.edge != index
                || hidden.from != edge.from
                || hidden.to != edge.to
                || hidden.explicitly_inserted != (witness_index == 0)
                || hidden.length != edge.length
                || hidden.gradient != edge.gradient
            {
                return Err(DynamicHiddenStabilityError::IsolationMismatch);
            }
        }
        witness_index = witness_index
            .checked_add(1)
            .ok_or(DynamicHiddenStabilityError::IsolationMismatch)?;
    }
    if witness_index != witness_stages.len() {
        return Err(DynamicHiddenStabilityError::IsolationMismatch);
    }
    Ok(())
}

fn extract_hidden_ratios(
    stage_count: usize,
    trace: &HiddenStableWitnessTraceResult,
) -> Result<Vec<BigRational>, DynamicHiddenStabilityError> {
    let mut ratios = Vec::with_capacity(stage_count);
    for event in trace.events.iter().take(stage_count) {
        let HiddenStableWitnessEventKind::StageVerified { certificate } = &event.kind else {
            return Err(DynamicHiddenStabilityError::IsolationMismatch);
        };
        ratios.push(certificate.ratio.clone());
    }
    if ratios.len() != stage_count {
        return Err(DynamicHiddenStabilityError::IsolationMismatch);
    }
    Ok(ratios)
}

fn extract_observable_ratios(
    stage_count: usize,
    trace: &DynamicShiftedTreeChainTraceResult,
) -> Result<Vec<BigRational>, DynamicHiddenStabilityError> {
    let mut ratios = Vec::with_capacity(stage_count);
    for event in &trace.events {
        if !matches!(
            event.kind,
            DynamicShiftedTreeChainEventKind::FlowApplied { .. }
        ) {
            continue;
        }
        let candidate = event
            .after
            .last_candidate
            .as_ref()
            .ok_or(DynamicHiddenStabilityError::IsolationMismatch)?;
        ratios.push(candidate.ratio.clone());
    }
    if ratios.len() != stage_count {
        return Err(DynamicHiddenStabilityError::IsolationMismatch);
    }
    Ok(ratios)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::{
        DynamicShiftedTreeChainCoordinateUpdate, HiddenStableEdgeWitness, ShiftedTreeChainConfig,
        ShiftedTreeChainEdge, trace_dynamic_shifted_tree_chain, trace_hidden_stable_witness,
    };

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn graph() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 3,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1, 1),
                    gradient: rational(2, 1),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 1,
                    to: 2,
                    length: rational(2, 1),
                    gradient: rational(-1, 1),
                },
                ShiftedTreeChainEdge {
                    source_edge: 2,
                    from: 2,
                    to: 0,
                    length: rational(3, 1),
                    gradient: rational(4, 1),
                },
            ],
        }
    }

    fn dynamic_config() -> DynamicShiftedTreeChainConfig {
        DynamicShiftedTreeChainConfig {
            chain: ShiftedTreeChainConfig {
                depth: 2,
                branches: 2,
            },
            psi: 1,
            kappa_alpha: rational(1, 2),
            epsilon: rational(1, 3),
            rebuild_after_updates: vec![100, 100],
        }
    }

    fn operations() -> Vec<DynamicShiftedTreeChainOperation> {
        vec![DynamicShiftedTreeChainOperation::Update {
            coordinates: vec![DynamicShiftedTreeChainCoordinateUpdate {
                edge: 0,
                length: rational(2, 1),
                gradient: rational(2, 1),
            }],
            eta: rational(1, 1),
        }]
    }

    fn witness_stage() -> HiddenStableWitnessStage {
        let current = [(0, 1, 2, 2), (1, 2, 2, -1), (2, 0, 3, 4)];
        HiddenStableWitnessStage {
            edges: current
                .into_iter()
                .enumerate()
                .map(
                    |(edge, (from, to, length, gradient))| HiddenStableEdgeWitness {
                        edge,
                        from,
                        to,
                        explicitly_inserted: true,
                        length: rational(length, 1),
                        gradient: rational(gradient, 1),
                        circulation: rational(-1, 1),
                        width: rational(length, 1),
                    },
                )
                .collect(),
        }
    }

    fn witness_config() -> HiddenStableWitnessConfig {
        HiddenStableWitnessConfig {
            node_count: 3,
            alpha: rational(1, 2),
            scalar_lower: rational(1, 8),
            scalar_upper: rational(8, 1),
        }
    }

    #[test]
    fn certifies_stage_alignment_without_exposing_witness_to_runner() {
        let graph = graph();
        let dynamic_config = dynamic_config();
        let operations = operations();
        let observable =
            trace_dynamic_shifted_tree_chain(&graph, &dynamic_config, &operations).expect("trace");
        let stages = vec![witness_stage()];
        let witness_config = witness_config();
        let witness = trace_hidden_stable_witness(&witness_config, &stages).expect("witness trace");
        let kappa = rational(1, 1);
        let certificate = check_dynamic_hidden_stability_isolation(&DynamicHiddenStabilityAudit {
            graph: &graph,
            dynamic_config: &dynamic_config,
            operations: &operations,
            observable_trace: &observable,
            witness_config: &witness_config,
            witness_stages: &stages,
            witness_trace: &witness,
            kappa: &kappa,
        })
        .expect("isolation");
        assert_eq!(certificate.stages.len(), 1);
        assert_eq!(certificate.stages[0].hidden_ratio, rational(-5, 7));
        assert_eq!(certificate.stages[0].observable_cycle_ratio, rational(5, 7));
    }

    #[test]
    fn rejects_attribute_and_threshold_mismatch() {
        let graph = graph();
        let dynamic_config = dynamic_config();
        let operations = operations();
        let observable =
            trace_dynamic_shifted_tree_chain(&graph, &dynamic_config, &operations).expect("trace");
        let mut stages = vec![witness_stage()];
        stages[0].edges[0].length = rational(1, 1);
        stages[0].edges[0].width = rational(1, 1);
        let witness_config = witness_config();
        let witness = trace_hidden_stable_witness(&witness_config, &stages).expect("witness trace");
        let kappa = rational(1, 1);
        assert_eq!(
            check_dynamic_hidden_stability_isolation(&DynamicHiddenStabilityAudit {
                graph: &graph,
                dynamic_config: &dynamic_config,
                operations: &operations,
                observable_trace: &observable,
                witness_config: &witness_config,
                witness_stages: &stages,
                witness_trace: &witness,
                kappa: &kappa,
            },),
            Err(DynamicHiddenStabilityError::IsolationMismatch)
        );

        let stages = vec![witness_stage()];
        let witness = trace_hidden_stable_witness(&witness_config, &stages).expect("witness trace");
        let bad_kappa = rational(2, 1);
        assert_eq!(
            check_dynamic_hidden_stability_isolation(&DynamicHiddenStabilityAudit {
                graph: &graph,
                dynamic_config: &dynamic_config,
                operations: &operations,
                observable_trace: &observable,
                witness_config: &witness_config,
                witness_stages: &stages,
                witness_trace: &witness,
                kappa: &bad_kappa,
            },),
            Err(DynamicHiddenStabilityError::IsolationMismatch)
        );
    }

    #[test]
    fn rejects_invalid_component_transcripts_before_alignment() {
        let graph = graph();
        let dynamic_config = dynamic_config();
        let operations = operations();
        let mut observable =
            trace_dynamic_shifted_tree_chain(&graph, &dynamic_config, &operations).expect("trace");
        observable.events[0].after.stage = 99;
        let stages = vec![witness_stage()];
        let witness_config = witness_config();
        let witness = trace_hidden_stable_witness(&witness_config, &stages).expect("witness trace");
        let kappa = rational(1, 1);
        assert!(matches!(
            check_dynamic_hidden_stability_isolation(&DynamicHiddenStabilityAudit {
                graph: &graph,
                dynamic_config: &dynamic_config,
                operations: &operations,
                observable_trace: &observable,
                witness_config: &witness_config,
                witness_stages: &stages,
                witness_trace: &witness,
                kappa: &kappa,
            },),
            Err(DynamicHiddenStabilityError::Observable(_))
        ));
    }

    #[test]
    fn fixed_topology_cannot_reset_a_hidden_insertion_epoch() {
        let graph = graph();
        let dynamic_config = dynamic_config();
        let one_update = operations();
        let mut operations = one_update.clone();
        operations.extend(one_update);
        let observable =
            trace_dynamic_shifted_tree_chain(&graph, &dynamic_config, &operations).expect("trace");
        let first = witness_stage();
        let mut second = first.clone();
        for edge in &mut second.edges {
            edge.explicitly_inserted = true;
        }
        let stages = vec![first, second];
        let witness_config = witness_config();
        let witness =
            trace_hidden_stable_witness(&witness_config, &stages).expect("component accepts reset");
        let kappa = rational(1, 1);
        assert_eq!(
            check_dynamic_hidden_stability_isolation(&DynamicHiddenStabilityAudit {
                graph: &graph,
                dynamic_config: &dynamic_config,
                operations: &operations,
                observable_trace: &observable,
                witness_config: &witness_config,
                witness_stages: &stages,
                witness_trace: &witness,
                kappa: &kappa,
            }),
            Err(DynamicHiddenStabilityError::IsolationMismatch)
        );
    }
}
