//! Checked conversion from an inter-level projection batch to a core stage.
//!
//! The projection boundary retains parent-key provenance and sparse refresh
//! reasons. The next level's dynamic core consumes dense vertices, stable edge
//! slots, exact attributes, and one atomic outer-stage batch. This adapter
//! drops only that non-executable provenance while preserving update order,
//! actual split sides, encoded split sides, full before/after rows, and the
//! outer stage. It never flattens one parent batch into several child stages.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES, DYNAMIC_SPARSE_CORE_MAX_EDGES,
    DYNAMIC_SPARSE_CORE_MAX_NODES, DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS,
    DynamicCoreGraphStageBatch, DynamicCoreGraphStageEdge, DynamicCoreGraphStageUpdate,
    DynamicCoreIncidence, DynamicLevelEdge, DynamicLevelStageBatch, DynamicLevelUpdate,
};

const CATALOG_ID: &str = "dynamic-level-stage-adapter";

/// Exact conversion counters for one atomic level boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicLevelStageAdapterMetrics {
    /// Source records converted.
    pub source_records: u64,
    /// Vertex-split records preserving actual and encoded sides.
    pub vertex_splits: u64,
    /// Edge insertions.
    pub edge_insertions: u64,
    /// Edge deletions.
    pub edge_deletions: u64,
    /// Explicit or forced edge reinsertions.
    pub edge_reinsertions: u64,
    /// Attribute-only replacements.
    pub attribute_replacements: u64,
}

/// Exact adapted child-stage batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelStageAdapterResult {
    /// Batch accepted by the next-level dynamic core collection.
    pub batch: DynamicCoreGraphStageBatch,
    /// Exact conversion counters.
    pub metrics: DynamicLevelStageAdapterMetrics,
}

/// One reversible adapter boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelStageAdapterTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Projection batch before executable-provenance erasure.
    pub source: DynamicLevelStageBatch,
    /// Adapted child batch and counters.
    pub result: DynamicLevelStageAdapterResult,
}

/// Complete adapter transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelStageAdapterTraceResult {
    /// Single atomic conversion boundary.
    pub event: DynamicLevelStageAdapterTraceEvent,
    /// Exact fast result.
    pub result: DynamicLevelStageAdapterResult,
}

/// Explicit bounded adapter failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicLevelStageAdapterError {
    /// Stage, edge, split, ordering, or attribute input is malformed.
    #[error("dynamic level stage adapter input is invalid")]
    InvalidInput,
    /// Checked metric arithmetic overflowed.
    #[error("dynamic level stage adapter arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact independent conversion.
    #[error("dynamic level stage adapter trace verification failed")]
    TraceVerification,
}

/// Converts one projected batch to one next-level atomic core batch.
///
/// # Errors
///
/// Rejects malformed stages, rows, split incidences, or checked overflow.
pub fn adapt_dynamic_level_stage_batch(
    source: &DynamicLevelStageBatch,
) -> Result<DynamicLevelStageAdapterResult, DynamicLevelStageAdapterError> {
    validate_source(source)?;
    let mut updates = Vec::with_capacity(source.updates.len());
    let mut metrics = DynamicLevelStageAdapterMetrics::default();
    for update in &source.updates {
        let converted = convert_update(update);
        account_update(&mut metrics, &converted)?;
        updates.push(converted);
    }
    metrics.source_records = usize_to_u64(source.updates.len())?;
    Ok(DynamicLevelStageAdapterResult {
        batch: DynamicCoreGraphStageBatch {
            outer_stage: source.outer_stage,
            updates,
        },
        metrics,
    })
}

/// Converts one projected batch and records the exact atomic boundary.
///
/// # Errors
///
/// Returns the same input or arithmetic failures as
/// [`adapt_dynamic_level_stage_batch`].
pub fn trace_dynamic_level_stage_adapter(
    source: &DynamicLevelStageBatch,
) -> Result<DynamicLevelStageAdapterTraceResult, DynamicLevelStageAdapterError> {
    let result = adapt_dynamic_level_stage_batch(source)?;
    Ok(DynamicLevelStageAdapterTraceResult {
        event: DynamicLevelStageAdapterTraceEvent {
            catalog_id: CATALOG_ID,
            source: source.clone(),
            result: result.clone(),
        },
        result,
    })
}

/// Independently verifies one adapter transcript.
///
/// # Errors
///
/// Returns [`DynamicLevelStageAdapterError::TraceVerification`] for any source,
/// batch, order, row, split, or metric drift.
pub fn check_dynamic_level_stage_adapter_trace(
    source: &DynamicLevelStageBatch,
    trace: &DynamicLevelStageAdapterTraceResult,
) -> Result<(), DynamicLevelStageAdapterError> {
    if trace.event.catalog_id != CATALOG_ID || trace.event.source != *source {
        return Err(DynamicLevelStageAdapterError::TraceVerification);
    }
    let expected = audit_convert(source)?;
    if trace.event.result != expected || trace.result != expected {
        return Err(DynamicLevelStageAdapterError::TraceVerification);
    }
    Ok(())
}

fn validate_source(source: &DynamicLevelStageBatch) -> Result<(), DynamicLevelStageAdapterError> {
    if source.outer_stage == 0 || source.updates.len() > DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES {
        return Err(DynamicLevelStageAdapterError::InvalidInput);
    }
    for update in &source.updates {
        validate_update(update)?;
    }
    Ok(())
}

fn validate_update(update: &DynamicLevelUpdate) -> Result<(), DynamicLevelStageAdapterError> {
    match update {
        DynamicLevelUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_incidences,
            provenance,
            ..
        } => {
            if *retained_vertex >= DYNAMIC_SPARSE_CORE_MAX_NODES
                || *new_vertex >= DYNAMIC_SPARSE_CORE_MAX_NODES
                || retained_vertex == new_vertex
                || provenance.retained_parent_vertex == provenance.new_parent_vertex
                || !strictly_increasing(new_side_incidences)
                || !strictly_increasing(encoded_incidences)
                || new_side_incidences
                    .iter()
                    .chain(encoded_incidences)
                    .any(|incidence| incidence.edge >= DYNAMIC_SPARSE_CORE_MAX_EDGES)
            {
                return Err(DynamicLevelStageAdapterError::InvalidInput);
            }
        }
        DynamicLevelUpdate::EdgeInserted { edge, .. }
        | DynamicLevelUpdate::EdgeDeleted { edge, .. } => validate_edge(edge)?,
        DynamicLevelUpdate::EdgeReinserted { before, after, .. }
        | DynamicLevelUpdate::AttributesReplaced { before, after } => {
            validate_edge(before)?;
            validate_edge(after)?;
            if before.edge != after.edge || before.from != after.from || before.to != after.to {
                return Err(DynamicLevelStageAdapterError::InvalidInput);
            }
        }
    }
    Ok(())
}

fn validate_edge(edge: &DynamicLevelEdge) -> Result<(), DynamicLevelStageAdapterError> {
    if edge.edge >= DYNAMIC_SPARSE_CORE_MAX_EDGES
        || edge.from >= DYNAMIC_SPARSE_CORE_MAX_NODES
        || edge.to >= DYNAMIC_SPARSE_CORE_MAX_NODES
        || edge.length <= BigRational::zero()
        || rational_too_wide(&edge.length)
        || rational_too_wide(&edge.gradient)
    {
        return Err(DynamicLevelStageAdapterError::InvalidInput);
    }
    Ok(())
}

fn convert_update(update: &DynamicLevelUpdate) -> DynamicCoreGraphStageUpdate {
    match update {
        DynamicLevelUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
            ..
        } => DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex: *retained_vertex,
            new_vertex: *new_vertex,
            new_side_incidences: new_side_incidences.clone(),
            encoded_side: *encoded_side,
            encoded_incidences: encoded_incidences.clone(),
        },
        DynamicLevelUpdate::EdgeInserted { edge, .. } => DynamicCoreGraphStageUpdate::Insert {
            edge: convert_edge(edge),
        },
        DynamicLevelUpdate::EdgeDeleted { edge, .. } => DynamicCoreGraphStageUpdate::Delete {
            edge: convert_edge(edge),
        },
        DynamicLevelUpdate::EdgeReinserted { before, after, .. } => {
            DynamicCoreGraphStageUpdate::Reinsert {
                before: convert_edge(before),
                after: convert_edge(after),
            }
        }
        DynamicLevelUpdate::AttributesReplaced { before, after } => {
            DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: convert_edge(before),
                after: convert_edge(after),
            }
        }
    }
}

fn convert_edge(edge: &DynamicLevelEdge) -> DynamicCoreGraphStageEdge {
    DynamicCoreGraphStageEdge {
        edge: edge.edge,
        from: edge.from,
        to: edge.to,
        length: edge.length.clone(),
        gradient: edge.gradient.clone(),
    }
}

fn account_update(
    metrics: &mut DynamicLevelStageAdapterMetrics,
    update: &DynamicCoreGraphStageUpdate,
) -> Result<(), DynamicLevelStageAdapterError> {
    let counter = match update {
        DynamicCoreGraphStageUpdate::VertexSplit { .. } => &mut metrics.vertex_splits,
        DynamicCoreGraphStageUpdate::Insert { .. } => &mut metrics.edge_insertions,
        DynamicCoreGraphStageUpdate::Delete { .. } => &mut metrics.edge_deletions,
        DynamicCoreGraphStageUpdate::Reinsert { .. } => &mut metrics.edge_reinsertions,
        DynamicCoreGraphStageUpdate::ReplaceAttributes { .. } => {
            &mut metrics.attribute_replacements
        }
    };
    *counter = increment(*counter)?;
    Ok(())
}

fn audit_convert(
    source: &DynamicLevelStageBatch,
) -> Result<DynamicLevelStageAdapterResult, DynamicLevelStageAdapterError> {
    if source.outer_stage == 0 || source.updates.len() > DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES {
        return Err(DynamicLevelStageAdapterError::TraceVerification);
    }
    let mut updates = Vec::with_capacity(source.updates.len());
    let mut metrics = DynamicLevelStageAdapterMetrics::default();
    for update in &source.updates {
        audit_validate_update(update)?;
        let converted = match update {
            DynamicLevelUpdate::VertexSplit {
                retained_vertex,
                new_vertex,
                new_side_incidences,
                encoded_side,
                encoded_incidences,
                ..
            } => DynamicCoreGraphStageUpdate::VertexSplit {
                retained_vertex: *retained_vertex,
                new_vertex: *new_vertex,
                new_side_incidences: new_side_incidences.clone(),
                encoded_side: *encoded_side,
                encoded_incidences: encoded_incidences.clone(),
            },
            DynamicLevelUpdate::EdgeInserted { edge, .. } => DynamicCoreGraphStageUpdate::Insert {
                edge: audit_convert_edge(edge),
            },
            DynamicLevelUpdate::EdgeDeleted { edge, .. } => DynamicCoreGraphStageUpdate::Delete {
                edge: audit_convert_edge(edge),
            },
            DynamicLevelUpdate::EdgeReinserted { before, after, .. } => {
                DynamicCoreGraphStageUpdate::Reinsert {
                    before: audit_convert_edge(before),
                    after: audit_convert_edge(after),
                }
            }
            DynamicLevelUpdate::AttributesReplaced { before, after } => {
                DynamicCoreGraphStageUpdate::ReplaceAttributes {
                    before: audit_convert_edge(before),
                    after: audit_convert_edge(after),
                }
            }
        };
        audit_account_update(&mut metrics, &converted)?;
        updates.push(converted);
    }
    metrics.source_records = u64::try_from(source.updates.len())
        .map_err(|_| DynamicLevelStageAdapterError::TraceVerification)?;
    Ok(DynamicLevelStageAdapterResult {
        batch: DynamicCoreGraphStageBatch {
            outer_stage: source.outer_stage,
            updates,
        },
        metrics,
    })
}

fn audit_validate_update(update: &DynamicLevelUpdate) -> Result<(), DynamicLevelStageAdapterError> {
    let valid_edge = |edge: &DynamicLevelEdge| {
        edge.edge < DYNAMIC_SPARSE_CORE_MAX_EDGES
            && edge.from < DYNAMIC_SPARSE_CORE_MAX_NODES
            && edge.to < DYNAMIC_SPARSE_CORE_MAX_NODES
            && edge.length > BigRational::zero()
            && !audit_rational_too_wide(&edge.length)
            && !audit_rational_too_wide(&edge.gradient)
    };
    let valid = match update {
        DynamicLevelUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_incidences,
            provenance,
            ..
        } => {
            *retained_vertex < DYNAMIC_SPARSE_CORE_MAX_NODES
                && *new_vertex < DYNAMIC_SPARSE_CORE_MAX_NODES
                && retained_vertex != new_vertex
                && provenance.retained_parent_vertex != provenance.new_parent_vertex
                && audit_strictly_increasing(new_side_incidences)
                && audit_strictly_increasing(encoded_incidences)
                && new_side_incidences
                    .iter()
                    .chain(encoded_incidences)
                    .all(|incidence| incidence.edge < DYNAMIC_SPARSE_CORE_MAX_EDGES)
        }
        DynamicLevelUpdate::EdgeInserted { edge, .. }
        | DynamicLevelUpdate::EdgeDeleted { edge, .. } => valid_edge(edge),
        DynamicLevelUpdate::EdgeReinserted { before, after, .. }
        | DynamicLevelUpdate::AttributesReplaced { before, after } => {
            valid_edge(before)
                && valid_edge(after)
                && before.edge == after.edge
                && before.from == after.from
                && before.to == after.to
        }
    };
    if !valid {
        return Err(DynamicLevelStageAdapterError::TraceVerification);
    }
    Ok(())
}

fn audit_convert_edge(edge: &DynamicLevelEdge) -> DynamicCoreGraphStageEdge {
    DynamicCoreGraphStageEdge {
        edge: edge.edge,
        from: edge.from,
        to: edge.to,
        length: edge.length.clone(),
        gradient: edge.gradient.clone(),
    }
}

fn audit_account_update(
    metrics: &mut DynamicLevelStageAdapterMetrics,
    update: &DynamicCoreGraphStageUpdate,
) -> Result<(), DynamicLevelStageAdapterError> {
    let counter = match update {
        DynamicCoreGraphStageUpdate::VertexSplit { .. } => &mut metrics.vertex_splits,
        DynamicCoreGraphStageUpdate::Insert { .. } => &mut metrics.edge_insertions,
        DynamicCoreGraphStageUpdate::Delete { .. } => &mut metrics.edge_deletions,
        DynamicCoreGraphStageUpdate::Reinsert { .. } => &mut metrics.edge_reinsertions,
        DynamicCoreGraphStageUpdate::ReplaceAttributes { .. } => {
            &mut metrics.attribute_replacements
        }
    };
    *counter = counter
        .checked_add(1)
        .ok_or(DynamicLevelStageAdapterError::TraceVerification)?;
    Ok(())
}

fn strictly_increasing(values: &[DynamicCoreIncidence]) -> bool {
    !values.windows(2).any(|pair| pair[0] >= pair[1])
}

fn audit_strictly_increasing(values: &[DynamicCoreIncidence]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn rational_too_wide(value: &BigRational) -> bool {
    bigint_bits(value.numer()) > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
        || bigint_bits(value.denom()) > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
}

fn audit_rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
}

fn bigint_bits(value: &BigInt) -> u64 {
    value.abs().to_biguint().map_or(0, |value| value.bits())
}

fn increment(value: u64) -> Result<u64, DynamicLevelStageAdapterError> {
    value
        .checked_add(1)
        .ok_or(DynamicLevelStageAdapterError::ArithmeticOverflow)
}

fn usize_to_u64(value: usize) -> Result<u64, DynamicLevelStageAdapterError> {
    u64::try_from(value).map_err(|_| DynamicLevelStageAdapterError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::{
        DynamicCoreEncodedSide, DynamicCoreIncidenceEndpoint, DynamicLevelSplitProvenance,
        DynamicSparseCoreRefreshReason,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn edge(edge: usize, from: usize, to: usize, length: i64, gradient: i64) -> DynamicLevelEdge {
        DynamicLevelEdge {
            edge,
            from,
            to,
            length: rational(length),
            gradient: rational(gradient),
        }
    }

    fn source() -> DynamicLevelStageBatch {
        let before_reinsert = edge(3, 1, 0, 5, 7);
        let before_attributes = edge(4, 0, 1, 2, 3);
        DynamicLevelStageBatch {
            outer_stage: 7,
            updates: vec![
                DynamicLevelUpdate::VertexSplit {
                    retained_vertex: 1,
                    new_vertex: 2,
                    new_side_incidences: vec![DynamicCoreIncidence {
                        edge: 0,
                        endpoint: DynamicCoreIncidenceEndpoint::Tail,
                    }],
                    encoded_side: DynamicCoreEncodedSide::Retained,
                    encoded_incidences: vec![DynamicCoreIncidence {
                        edge: 1,
                        endpoint: DynamicCoreIncidenceEndpoint::Head,
                    }],
                    provenance: DynamicLevelSplitProvenance {
                        retained_parent_vertex: 9,
                        new_parent_vertex: 42,
                    },
                },
                DynamicLevelUpdate::EdgeInserted {
                    edge: edge(2, 2, 0, 3, -1),
                    reason: DynamicSparseCoreRefreshReason::DirectInsertion,
                },
                DynamicLevelUpdate::EdgeDeleted {
                    edge: edge(1, 0, 1, 1, 4),
                    reason: DynamicSparseCoreRefreshReason::SparsifyRefresh,
                },
                DynamicLevelUpdate::EdgeReinserted {
                    before: before_reinsert.clone(),
                    after: edge(3, 1, 0, 4, 6),
                    forced_by_reembedding: true,
                },
                DynamicLevelUpdate::AttributesReplaced {
                    before: before_attributes.clone(),
                    after: edge(4, 0, 1, 6, 8),
                },
            ],
        }
    }

    #[test]
    fn adapter_preserves_atomic_order_rows_and_actual_split_identity() {
        let source = source();
        let result = adapt_dynamic_level_stage_batch(&source).expect("adapt");
        assert_eq!(result.batch.outer_stage, 7);
        assert_eq!(result.batch.updates.len(), source.updates.len());
        let DynamicCoreGraphStageUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } = &result.batch.updates[0]
        else {
            panic!("split");
        };
        assert_eq!((*retained_vertex, *new_vertex), (1, 2));
        assert_eq!(new_side_incidences.len(), 1);
        assert_eq!(*encoded_side, DynamicCoreEncodedSide::Retained);
        assert_eq!(encoded_incidences.len(), 1);
        assert!(matches!(
            result.batch.updates[3],
            DynamicCoreGraphStageUpdate::Reinsert { .. }
        ));
        assert_eq!(result.metrics.source_records, 5);
        assert_eq!(result.metrics.edge_reinsertions, 1);
    }

    #[test]
    fn adapter_trace_checker_rejects_output_and_source_tampering() {
        let source = source();
        let trace = trace_dynamic_level_stage_adapter(&source).expect("trace");
        check_dynamic_level_stage_adapter_trace(&source, &trace).expect("check");

        let mut tampered = trace.clone();
        tampered.result.batch.updates.swap(1, 2);
        assert_eq!(
            check_dynamic_level_stage_adapter_trace(&source, &tampered),
            Err(DynamicLevelStageAdapterError::TraceVerification)
        );

        let mut tampered = trace;
        tampered.event.source.outer_stage = 8;
        assert_eq!(
            check_dynamic_level_stage_adapter_trace(&source, &tampered),
            Err(DynamicLevelStageAdapterError::TraceVerification)
        );
    }

    #[test]
    fn adapter_rejects_endpoint_changing_reinsertion() {
        let source = DynamicLevelStageBatch {
            outer_stage: 1,
            updates: vec![DynamicLevelUpdate::EdgeReinserted {
                before: edge(0, 0, 1, 1, 2),
                after: edge(0, 1, 0, 1, 2),
                forced_by_reembedding: false,
            }],
        };
        assert_eq!(
            adapt_dynamic_level_stage_batch(&source),
            Err(DynamicLevelStageAdapterError::InvalidInput)
        );
    }
}
