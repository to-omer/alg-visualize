//! Checked atomic projection from one active sparse-core branch to the next level.
//!
//! The source dynamic tree-chain passes one ordered sparse-core update batch from
//! the active branch at level `i` to graph `G_{i+1}`. This bounded primitive
//! materializes that boundary without flattening one parent stage into several
//! child stages. Stable edge slots are preserved across levels. Parent vertex
//! keys are mapped monotonically to dense child IDs, while a split's actual new
//! side and its encoded smaller side remain distinct.
//!
//! This module is deliberately only the checked boundary model. It does not yet
//! run the child low-stretch-forest/core/sparsifier collection, initialize its
//! static reference-tree support, or compose Shift/Rebuild suffix replacement.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    DYNAMIC_SPARSE_CORE_MAX_EDGES, DYNAMIC_SPARSE_CORE_MAX_NODES,
    DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS, DynamicCoreEdge, DynamicCoreEncodedSide,
    DynamicCoreIncidence, DynamicCoreIncidenceEndpoint, DynamicSparseCoreRefreshReason,
    DynamicSparseCoreSnapshot, DynamicSparseCoreUpdate,
};

const CATALOG_ID: &str = "dynamic-level-projection";

/// Maximum records in one atomic inter-level batch.
pub const DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES: usize = DYNAMIC_SPARSE_CORE_MAX_EDGES * 8;

/// One exact child-level edge row with a stable cross-level edge slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelEdge {
    /// Stable edge slot shared by every level in the bounded model.
    pub edge: usize,
    /// Dense child tail vertex.
    pub from: usize,
    /// Dense child head vertex.
    pub to: usize,
    /// Exact current edge length.
    pub length: BigRational,
    /// Exact current edge gradient.
    pub gradient: BigRational,
}

/// One parent-key to dense-child binding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DynamicLevelVertexBinding {
    /// Sparse-core vertex key at the parent level.
    pub parent_vertex: usize,
    /// Dense vertex ID in the child level epoch.
    pub child_vertex: usize,
}

/// Persistent vertex materialization map for one child-level epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelVertexMap {
    /// Bindings in strictly increasing parent-key order.
    pub parent_to_child: Vec<DynamicLevelVertexBinding>,
    /// Reverse map indexed by dense child vertex ID.
    pub child_to_parent: Vec<usize>,
}

/// Exact stable child graph at one outer source-stage boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelGraphSnapshot {
    /// Dense vertices are exactly `0..active_node_count`.
    pub active_node_count: usize,
    /// Stable edge slots; holes are never compacted.
    pub edge_slots: Vec<Option<DynamicLevelEdge>>,
    /// Completed outer source stage.
    pub stage: u64,
}

/// Complete checked projection state for one level epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelProjectionState {
    /// Parent-key to dense-child identity mapping.
    pub vertex_map: DynamicLevelVertexMap,
    /// Materialized child graph.
    pub graph: DynamicLevelGraphSnapshot,
}

/// Provenance retained for every projected vertex split.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicLevelSplitProvenance {
    /// Existing parent sparse-core vertex key.
    pub retained_parent_vertex: usize,
    /// Actual new parent sparse-core vertex key.
    pub new_parent_vertex: usize,
}

/// One ordered update inside an atomic child-level stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicLevelUpdate {
    /// Split one existing child vertex while preserving actual parent identity.
    VertexSplit {
        /// Existing dense child vertex that remains bound to the retained parent key.
        retained_vertex: usize,
        /// Next consecutive dense child vertex bound to the actual new parent key.
        new_vertex: usize,
        /// Exact incidences whose endpoint moves to `new_vertex`.
        new_side_incidences: Vec<DynamicCoreIncidence>,
        /// Canonical smaller side reported by the parent batch.
        encoded_side: DynamicCoreEncodedSide,
        /// Canonical strictly ordered incidences of the encoded smaller side.
        encoded_incidences: Vec<DynamicCoreIncidence>,
        /// Parent vertex-key provenance.
        provenance: DynamicLevelSplitProvenance,
    },
    /// Insert one inactive stable edge slot.
    EdgeInserted {
        /// Complete current child row.
        edge: DynamicLevelEdge,
        /// Direct source insertion or representative refresh.
        reason: DynamicSparseCoreRefreshReason,
    },
    /// Delete one active stable edge slot.
    EdgeDeleted {
        /// Complete child row at deletion time.
        edge: DynamicLevelEdge,
        /// Core deletion or representative refresh.
        reason: DynamicSparseCoreRefreshReason,
    },
    /// Explicitly reinsert one active row without changing topology.
    EdgeReinserted {
        /// Row immediately before the reinsertion record.
        before: DynamicLevelEdge,
        /// Current row after the reinsertion record.
        after: DynamicLevelEdge,
        /// Whether this record came from the stage-end re-embedded set.
        forced_by_reembedding: bool,
    },
    /// Replace attributes of one active row without resetting its insertion epoch.
    AttributesReplaced {
        /// Complete row before replacement.
        before: DynamicLevelEdge,
        /// Complete row after replacement.
        after: DynamicLevelEdge,
    },
}

/// One source stage projected as one atomic child batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelStageBatch {
    /// Parent/source stage completed by this batch.
    pub outer_stage: u64,
    /// Ordered topology, attribute, and reinsertion records.
    pub updates: Vec<DynamicLevelUpdate>,
}

/// Exact boundary work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicLevelProjectionMetrics {
    /// Sparse-core update records inspected.
    pub source_records: u64,
    /// Actual incidences moved to new child vertices.
    pub moved_incidences: u64,
    /// Incidences used by the canonical smaller-side encoding.
    pub encoded_incidences: u64,
    /// Stable edge insertions.
    pub edge_insertions: u64,
    /// Stable edge deletions.
    pub edge_deletions: u64,
    /// Explicit reinsertion records.
    pub edge_reinsertions: u64,
    /// Attribute-only replacements.
    pub attribute_replacements: u64,
}

/// Exact result of one atomic inter-level projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelProjectionResult {
    /// Atomic batch to pass to the next-level dynamic collection.
    pub batch: DynamicLevelStageBatch,
    /// Child graph and mapping after the whole batch.
    pub final_state: DynamicLevelProjectionState,
    /// Exact boundary counters.
    pub metrics: DynamicLevelProjectionMetrics,
}

/// One reversible inter-level boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelProjectionTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// State before the atomic parent batch.
    pub before: DynamicLevelProjectionState,
    /// Projected atomic child batch.
    pub batch: DynamicLevelStageBatch,
    /// State after the entire parent batch.
    pub after: DynamicLevelProjectionState,
    /// Exact boundary counters.
    pub metrics: DynamicLevelProjectionMetrics,
}

/// Complete independently checkable projection transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicLevelProjectionTraceResult {
    /// The single atomic boundary event.
    pub event: DynamicLevelProjectionTraceEvent,
    /// Exact fast result.
    pub result: DynamicLevelProjectionResult,
}

/// Explicit bounded projection failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicLevelProjectionError {
    /// Snapshot, mapping, or update input is malformed.
    #[error("dynamic level projection input is invalid")]
    InvalidInput,
    /// A stable identity, split, ordering, or before/after invariant failed.
    #[error("dynamic level projection invariant failed")]
    InvariantViolation,
    /// A concrete bounded admission limit was exceeded.
    #[error("dynamic level projection exceeds an admission limit")]
    AdmissionLimit,
    /// Checked counter arithmetic overflowed.
    #[error("dynamic level projection arithmetic overflow")]
    ArithmeticOverflow,
    /// The public transcript failed independent reconstruction.
    #[error("dynamic level projection trace verification failed")]
    TraceVerification,
}

/// Deterministically materialize a child epoch from one sparse-core snapshot.
///
/// # Errors
///
/// Returns an error when the sparse boundary, stable slots, attributes, or
/// vertex identities violate the bounded projection contract.
pub fn initialize_dynamic_level_projection(
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<DynamicLevelProjectionState, DynamicLevelProjectionError> {
    validate_sparse_boundary(snapshot)?;
    let vertex_map = initial_vertex_map(&snapshot.core_vertices)?;
    let graph = decode_sparse_graph(snapshot, &vertex_map)?;
    let state = DynamicLevelProjectionState { vertex_map, graph };
    verify_state(&state)?;
    Ok(state)
}

/// Project one active-branch sparse update batch without producing a transcript.
///
/// # Errors
///
/// Returns an error when the before/after boundary is not one atomic stage or
/// an ordered update cannot reproduce the supplied after snapshot.
pub fn execute_dynamic_level_projection(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
    after_sparse: &DynamicSparseCoreSnapshot,
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicLevelProjectionResult, DynamicLevelProjectionError> {
    project(initial, before_sparse, after_sparse, sparse_updates)
}

/// Project one active-branch sparse update batch with a reversible transcript.
///
/// # Errors
///
/// Returns an error under the same malformed-boundary, admission, arithmetic,
/// or transition failures as [`execute_dynamic_level_projection`].
pub fn trace_dynamic_level_projection(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
    after_sparse: &DynamicSparseCoreSnapshot,
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicLevelProjectionTraceResult, DynamicLevelProjectionError> {
    let result = project(initial, before_sparse, after_sparse, sparse_updates)?;
    let event = DynamicLevelProjectionTraceEvent {
        catalog_id: CATALOG_ID,
        before: initial.clone(),
        batch: result.batch.clone(),
        after: result.final_state.clone(),
        metrics: result.metrics,
    };
    Ok(DynamicLevelProjectionTraceResult { event, result })
}

/// Independently reconstruct and verify one inter-level projection transcript.
///
/// # Errors
///
/// Returns [`DynamicLevelProjectionError::TraceVerification`] when any claimed
/// batch, mapping, metric, or final graph differs from independent replay.
pub fn check_dynamic_level_projection_trace(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
    after_sparse: &DynamicSparseCoreSnapshot,
    sparse_updates: &[DynamicSparseCoreUpdate],
    trace: &DynamicLevelProjectionTraceResult,
) -> Result<(), DynamicLevelProjectionError> {
    if trace.event.catalog_id != CATALOG_ID || trace.event.before != *initial {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    audit_validate_sparse_boundary(before_sparse)?;
    audit_validate_sparse_boundary(after_sparse)?;
    audit_verify_initial(initial, before_sparse)?;
    let expected = audit_project(initial, before_sparse, after_sparse, sparse_updates)?;
    if trace.event.batch != expected.batch
        || trace.event.after != expected.final_state
        || trace.event.metrics != expected.metrics
        || trace.result != expected
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    Ok(())
}

fn project(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
    after_sparse: &DynamicSparseCoreSnapshot,
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicLevelProjectionResult, DynamicLevelProjectionError> {
    validate_sparse_boundary(before_sparse)?;
    validate_sparse_boundary(after_sparse)?;
    verify_state(initial)?;
    verify_state_matches_sparse(initial, before_sparse)?;
    validate_stage_boundary(before_sparse, after_sparse, sparse_updates.len())?;
    validate_forced_suffix(sparse_updates)?;

    let mut state = initial.clone();
    let mut projected = Vec::with_capacity(sparse_updates.len());
    let mut metrics = DynamicLevelProjectionMetrics::default();
    let mut forced_suffix = false;
    for update in sparse_updates {
        if matches!(update, DynamicSparseCoreUpdate::ForcedReinsert { .. }) {
            forced_suffix = true;
        } else if forced_suffix {
            return Err(DynamicLevelProjectionError::InvalidInput);
        }
        let next = apply_parent_update(&mut state, update)?;
        account_update(&mut metrics, &next)?;
        projected.push(next);
    }
    metrics.source_records = usize_to_u64(sparse_updates.len())?;
    state.graph.stage = after_sparse.stage;
    verify_state_matches_sparse(&state, after_sparse)?;
    let batch = DynamicLevelStageBatch {
        outer_stage: after_sparse.stage,
        updates: projected,
    };
    Ok(DynamicLevelProjectionResult {
        batch,
        final_state: state,
        metrics,
    })
}

fn apply_parent_update(
    state: &mut DynamicLevelProjectionState,
    update: &DynamicSparseCoreUpdate,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    match update {
        DynamicSparseCoreUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => apply_parent_split(
            state,
            *retained_vertex,
            *new_vertex,
            new_side_incidences,
            *encoded_side,
            encoded_incidences,
        ),
        DynamicSparseCoreUpdate::EdgeInserted { edge, reason } => {
            let row = map_parent_edge(edge, &state.vertex_map)?;
            let slot = state
                .graph
                .edge_slots
                .get_mut(row.edge)
                .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
            if slot.is_some() {
                return Err(DynamicLevelProjectionError::InvariantViolation);
            }
            *slot = Some(row.clone());
            Ok(DynamicLevelUpdate::EdgeInserted {
                edge: row,
                reason: *reason,
            })
        }
        DynamicSparseCoreUpdate::EdgeDeleted { edge, reason } => {
            let row = map_parent_edge(edge, &state.vertex_map)?;
            let slot = state
                .graph
                .edge_slots
                .get_mut(row.edge)
                .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
            if slot.as_ref() != Some(&row) {
                return Err(DynamicLevelProjectionError::InvariantViolation);
            }
            *slot = None;
            Ok(DynamicLevelUpdate::EdgeDeleted {
                edge: row,
                reason: *reason,
            })
        }
        DynamicSparseCoreUpdate::EdgeReinserted { before, after } => {
            let before = map_parent_edge(before, &state.vertex_map)?;
            let after = map_parent_edge(after, &state.vertex_map)?;
            replace_active_row(&mut state.graph, &before, &after)?;
            Ok(DynamicLevelUpdate::EdgeReinserted {
                before,
                after,
                forced_by_reembedding: false,
            })
        }
        DynamicSparseCoreUpdate::GradientReplaced {
            edge,
            before,
            after,
        } => apply_gradient_replacement(state, *edge, before, after),
        DynamicSparseCoreUpdate::LengthReplaced {
            edge,
            before,
            after,
        } => apply_length_replacement(state, *edge, before, after),
        DynamicSparseCoreUpdate::ForcedReinsert { edge } => {
            let row = state
                .graph
                .edge_slots
                .get(*edge)
                .and_then(Option::as_ref)
                .ok_or(DynamicLevelProjectionError::InvariantViolation)?
                .clone();
            Ok(DynamicLevelUpdate::EdgeReinserted {
                before: row.clone(),
                after: row,
                forced_by_reembedding: true,
            })
        }
    }
}

fn apply_gradient_replacement(
    state: &mut DynamicLevelProjectionState,
    edge: usize,
    before: &BigRational,
    after: &BigRational,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let row = state
        .graph
        .edge_slots
        .get_mut(edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    if &row.gradient != before {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    let old = row.clone();
    row.gradient = after.clone();
    Ok(DynamicLevelUpdate::AttributesReplaced {
        before: old,
        after: row.clone(),
    })
}

fn apply_length_replacement(
    state: &mut DynamicLevelProjectionState,
    edge: usize,
    before: &BigRational,
    after: &BigRational,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let row = state
        .graph
        .edge_slots
        .get_mut(edge)
        .and_then(Option::as_mut)
        .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    if &row.length != before {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    let old = row.clone();
    row.length = after.clone();
    Ok(DynamicLevelUpdate::AttributesReplaced {
        before: old,
        after: row.clone(),
    })
}

fn apply_parent_split(
    state: &mut DynamicLevelProjectionState,
    retained_parent: usize,
    new_parent: usize,
    new_side: &[DynamicCoreIncidence],
    encoded_side: DynamicCoreEncodedSide,
    encoded: &[DynamicCoreIncidence],
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let retained_child = child_for_parent(&state.vertex_map, retained_parent)
        .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    if child_for_parent(&state.vertex_map, new_parent).is_some() {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    let new_child = state.graph.active_node_count;
    validate_split_encoding(
        &state.graph.edge_slots,
        retained_child,
        new_side,
        encoded_side,
        encoded,
    )?;
    apply_split_incidences(
        &mut state.graph.edge_slots,
        retained_child,
        new_child,
        new_side,
    )?;
    insert_vertex_binding(&mut state.vertex_map, new_parent, new_child)?;
    state.graph.active_node_count = state
        .graph
        .active_node_count
        .checked_add(1)
        .ok_or(DynamicLevelProjectionError::ArithmeticOverflow)?;
    Ok(DynamicLevelUpdate::VertexSplit {
        retained_vertex: retained_child,
        new_vertex: new_child,
        new_side_incidences: new_side.to_vec(),
        encoded_side,
        encoded_incidences: encoded.to_vec(),
        provenance: DynamicLevelSplitProvenance {
            retained_parent_vertex: retained_parent,
            new_parent_vertex: new_parent,
        },
    })
}

fn validate_split_encoding(
    edges: &[Option<DynamicLevelEdge>],
    retained_vertex: usize,
    new_side: &[DynamicCoreIncidence],
    encoded_side: DynamicCoreEncodedSide,
    encoded: &[DynamicCoreIncidence],
) -> Result<(), DynamicLevelProjectionError> {
    if !strictly_increasing(new_side) || !strictly_increasing(encoded) {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    let incident = incidences_at(edges, retained_vertex);
    if new_side
        .iter()
        .any(|incidence| incident.binary_search(incidence).is_err())
    {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    let retained_side = incident
        .iter()
        .copied()
        .filter(|incidence| new_side.binary_search(incidence).is_err())
        .collect::<Vec<_>>();
    let (expected_side, expected_encoding) = if new_side.len() <= retained_side.len() {
        (DynamicCoreEncodedSide::New, new_side)
    } else {
        (DynamicCoreEncodedSide::Retained, retained_side.as_slice())
    };
    if encoded_side != expected_side || encoded != expected_encoding {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    Ok(())
}

fn apply_split_incidences(
    edges: &mut [Option<DynamicLevelEdge>],
    retained_vertex: usize,
    new_vertex: usize,
    incidences: &[DynamicCoreIncidence],
) -> Result<(), DynamicLevelProjectionError> {
    for incidence in incidences {
        let row = edges
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
            .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
        let endpoint = match incidence.endpoint {
            DynamicCoreIncidenceEndpoint::Tail => &mut row.from,
            DynamicCoreIncidenceEndpoint::Head => &mut row.to,
        };
        if *endpoint != retained_vertex {
            return Err(DynamicLevelProjectionError::InvariantViolation);
        }
        *endpoint = new_vertex;
    }
    Ok(())
}

fn replace_active_row(
    graph: &mut DynamicLevelGraphSnapshot,
    before: &DynamicLevelEdge,
    after: &DynamicLevelEdge,
) -> Result<(), DynamicLevelProjectionError> {
    if before.edge != after.edge || before.from != after.from || before.to != after.to {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    let slot = graph
        .edge_slots
        .get_mut(before.edge)
        .ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    if slot.as_ref() != Some(before) {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    *slot = Some(after.clone());
    Ok(())
}

fn account_update(
    metrics: &mut DynamicLevelProjectionMetrics,
    update: &DynamicLevelUpdate,
) -> Result<(), DynamicLevelProjectionError> {
    match update {
        DynamicLevelUpdate::VertexSplit {
            new_side_incidences,
            encoded_incidences,
            ..
        } => {
            metrics.moved_incidences =
                add_usize(metrics.moved_incidences, new_side_incidences.len())?;
            metrics.encoded_incidences =
                add_usize(metrics.encoded_incidences, encoded_incidences.len())?;
        }
        DynamicLevelUpdate::EdgeInserted { .. } => {
            metrics.edge_insertions = increment(metrics.edge_insertions)?;
        }
        DynamicLevelUpdate::EdgeDeleted { .. } => {
            metrics.edge_deletions = increment(metrics.edge_deletions)?;
        }
        DynamicLevelUpdate::EdgeReinserted { .. } => {
            metrics.edge_reinsertions = increment(metrics.edge_reinsertions)?;
        }
        DynamicLevelUpdate::AttributesReplaced { .. } => {
            metrics.attribute_replacements = increment(metrics.attribute_replacements)?;
        }
    }
    Ok(())
}

fn initial_vertex_map(
    parent_vertices: &[usize],
) -> Result<DynamicLevelVertexMap, DynamicLevelProjectionError> {
    if parent_vertices.is_empty()
        || parent_vertices.len() > DYNAMIC_SPARSE_CORE_MAX_NODES
        || parent_vertices.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    let parent_to_child = parent_vertices
        .iter()
        .copied()
        .enumerate()
        .map(|(child_vertex, parent_vertex)| DynamicLevelVertexBinding {
            parent_vertex,
            child_vertex,
        })
        .collect();
    Ok(DynamicLevelVertexMap {
        parent_to_child,
        child_to_parent: parent_vertices.to_vec(),
    })
}

fn insert_vertex_binding(
    map: &mut DynamicLevelVertexMap,
    parent_vertex: usize,
    child_vertex: usize,
) -> Result<(), DynamicLevelProjectionError> {
    if child_vertex != map.child_to_parent.len()
        || child_for_parent(map, parent_vertex).is_some()
        || map.parent_to_child.len() >= DYNAMIC_SPARSE_CORE_MAX_NODES
    {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    map.child_to_parent.push(parent_vertex);
    map.parent_to_child.push(DynamicLevelVertexBinding {
        parent_vertex,
        child_vertex,
    });
    map.parent_to_child.sort_unstable();
    Ok(())
}

fn child_for_parent(map: &DynamicLevelVertexMap, parent: usize) -> Option<usize> {
    map.parent_to_child
        .binary_search_by_key(&parent, |binding| binding.parent_vertex)
        .ok()
        .map(|index| map.parent_to_child[index].child_vertex)
}

fn decode_sparse_graph(
    snapshot: &DynamicSparseCoreSnapshot,
    map: &DynamicLevelVertexMap,
) -> Result<DynamicLevelGraphSnapshot, DynamicLevelProjectionError> {
    let mut edge_slots = Vec::with_capacity(snapshot.spanner_edge_slots.len());
    for row in &snapshot.spanner_edge_slots {
        edge_slots.push(
            row.as_ref()
                .map(|edge| map_parent_edge(edge, map))
                .transpose()?,
        );
    }
    Ok(DynamicLevelGraphSnapshot {
        active_node_count: map.child_to_parent.len(),
        edge_slots,
        stage: snapshot.stage,
    })
}

fn map_parent_edge(
    edge: &DynamicCoreEdge,
    map: &DynamicLevelVertexMap,
) -> Result<DynamicLevelEdge, DynamicLevelProjectionError> {
    let from =
        child_for_parent(map, edge.from).ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    let to =
        child_for_parent(map, edge.to).ok_or(DynamicLevelProjectionError::InvariantViolation)?;
    if edge.edge >= DYNAMIC_SPARSE_CORE_MAX_EDGES
        || edge.length <= BigRational::zero()
        || rational_too_wide(&edge.length)
        || rational_too_wide(&edge.gradient)
    {
        return Err(DynamicLevelProjectionError::AdmissionLimit);
    }
    Ok(DynamicLevelEdge {
        edge: edge.edge,
        from,
        to,
        length: edge.length.clone(),
        gradient: edge.gradient.clone(),
    })
}

fn verify_state_matches_sparse(
    state: &DynamicLevelProjectionState,
    sparse: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicLevelProjectionError> {
    let parents = state
        .vertex_map
        .parent_to_child
        .iter()
        .map(|binding| binding.parent_vertex)
        .collect::<Vec<_>>();
    if parents != sparse.core_vertices
        || state.graph != decode_sparse_graph(sparse, &state.vertex_map)?
    {
        return Err(DynamicLevelProjectionError::InvariantViolation);
    }
    Ok(())
}

fn verify_state(state: &DynamicLevelProjectionState) -> Result<(), DynamicLevelProjectionError> {
    let map = &state.vertex_map;
    if map.parent_to_child.is_empty()
        || map.parent_to_child.len() != map.child_to_parent.len()
        || map.parent_to_child.len() != state.graph.active_node_count
        || map.parent_to_child.len() > DYNAMIC_SPARSE_CORE_MAX_NODES
        || map
            .parent_to_child
            .windows(2)
            .any(|pair| pair[0].parent_vertex >= pair[1].parent_vertex)
        || state.graph.edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
    {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    let mut seen = vec![false; map.child_to_parent.len()];
    for binding in &map.parent_to_child {
        if binding.child_vertex >= seen.len()
            || seen[binding.child_vertex]
            || map.child_to_parent[binding.child_vertex] != binding.parent_vertex
        {
            return Err(DynamicLevelProjectionError::InvalidInput);
        }
        seen[binding.child_vertex] = true;
    }
    if seen.iter().any(|value| !value) {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    for (edge_id, row) in state.graph.edge_slots.iter().enumerate() {
        if let Some(row) = row
            && (row.edge != edge_id
                || row.from >= state.graph.active_node_count
                || row.to >= state.graph.active_node_count
                || row.length <= BigRational::zero()
                || rational_too_wide(&row.length)
                || rational_too_wide(&row.gradient))
        {
            return Err(DynamicLevelProjectionError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_sparse_boundary(
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicLevelProjectionError> {
    if snapshot.core_vertices.is_empty()
        || snapshot.core_vertices.len() > DYNAMIC_SPARSE_CORE_MAX_NODES
        || snapshot
            .core_vertices
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || snapshot.spanner_edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
        || snapshot.core_edge_slots.len() != snapshot.spanner_edge_slots.len()
        || snapshot.core_to_spanner.len() != snapshot.spanner_edge_slots.len()
        || snapshot.direct_edges.len() != snapshot.spanner_edge_slots.len()
    {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    for (edge_id, row) in snapshot.spanner_edge_slots.iter().enumerate() {
        if let Some(row) = row
            && (row.edge != edge_id
                || snapshot.core_edge_slots[edge_id].as_ref() != Some(row)
                || snapshot.core_vertices.binary_search(&row.from).is_err()
                || snapshot.core_vertices.binary_search(&row.to).is_err()
                || row.length <= BigRational::zero()
                || rational_too_wide(&row.length)
                || rational_too_wide(&row.gradient))
        {
            return Err(DynamicLevelProjectionError::InvalidInput);
        }
    }
    Ok(())
}

fn validate_stage_boundary(
    before: &DynamicSparseCoreSnapshot,
    after: &DynamicSparseCoreSnapshot,
    update_count: usize,
) -> Result<(), DynamicLevelProjectionError> {
    if update_count > DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES
        || before.complete
        || before.stage.checked_add(1) != Some(after.stage)
    {
        return Err(DynamicLevelProjectionError::InvalidInput);
    }
    Ok(())
}

fn validate_forced_suffix(
    updates: &[DynamicSparseCoreUpdate],
) -> Result<(), DynamicLevelProjectionError> {
    let mut forced = false;
    for update in updates {
        if matches!(update, DynamicSparseCoreUpdate::ForcedReinsert { .. }) {
            forced = true;
        } else if forced {
            return Err(DynamicLevelProjectionError::InvalidInput);
        }
    }
    Ok(())
}

fn incidences_at(edges: &[Option<DynamicLevelEdge>], vertex: usize) -> Vec<DynamicCoreIncidence> {
    let mut result = Vec::new();
    for row in edges.iter().flatten() {
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

fn strictly_increasing(values: &[DynamicCoreIncidence]) -> bool {
    !values.windows(2).any(|pair| pair[0] >= pair[1])
}

fn rational_too_wide(value: &BigRational) -> bool {
    bigint_bits(value.numer()) > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
        || bigint_bits(value.denom()) > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
}

fn bigint_bits(value: &BigInt) -> u64 {
    value.abs().to_biguint().map_or(0, |value| value.bits())
}

fn increment(value: u64) -> Result<u64, DynamicLevelProjectionError> {
    value
        .checked_add(1)
        .ok_or(DynamicLevelProjectionError::ArithmeticOverflow)
}

fn add_usize(value: u64, amount: usize) -> Result<u64, DynamicLevelProjectionError> {
    value
        .checked_add(usize_to_u64(amount)?)
        .ok_or(DynamicLevelProjectionError::ArithmeticOverflow)
}

fn usize_to_u64(value: usize) -> Result<u64, DynamicLevelProjectionError> {
    u64::try_from(value).map_err(|_| DynamicLevelProjectionError::ArithmeticOverflow)
}

// The audit path intentionally does not call the production projector or its
// transition helpers. It reconstructs the same public contract from the sparse
// before/after boundary and the claimed ordered source batch.

fn audit_validate_sparse_boundary(
    snapshot: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicLevelProjectionError> {
    if snapshot.core_vertices.is_empty()
        || snapshot.core_vertices.len() > DYNAMIC_SPARSE_CORE_MAX_NODES
        || snapshot
            .core_vertices
            .iter()
            .enumerate()
            .any(|(index, vertex)| index > 0 && snapshot.core_vertices[index - 1] >= *vertex)
        || snapshot.spanner_edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
        || snapshot.core_edge_slots.len() != snapshot.spanner_edge_slots.len()
        || snapshot.core_to_spanner.len() != snapshot.spanner_edge_slots.len()
        || snapshot.direct_edges.len() != snapshot.spanner_edge_slots.len()
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    for (slot, row) in snapshot.spanner_edge_slots.iter().enumerate() {
        if let Some(row) = row
            && (row.edge != slot
                || snapshot.core_edge_slots[slot].as_ref() != Some(row)
                || !snapshot.core_vertices.contains(&row.from)
                || !snapshot.core_vertices.contains(&row.to)
                || row.length <= BigRational::zero()
                || audit_rational_too_wide(&row.length)
                || audit_rational_too_wide(&row.gradient))
        {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
    }
    Ok(())
}

fn audit_verify_initial(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicLevelProjectionError> {
    let mut reverse = vec![None; initial.vertex_map.child_to_parent.len()];
    for binding in &initial.vertex_map.parent_to_child {
        if binding.child_vertex >= reverse.len()
            || reverse[binding.child_vertex].is_some()
            || initial.vertex_map.child_to_parent[binding.child_vertex] != binding.parent_vertex
        {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
        reverse[binding.child_vertex] = Some(binding.parent_vertex);
    }
    if reverse.iter().any(Option::is_none)
        || initial.graph.active_node_count != reverse.len()
        || initial.graph.stage != before_sparse.stage
        || initial.graph.edge_slots.len() != before_sparse.spanner_edge_slots.len()
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let mut parents = initial
        .vertex_map
        .parent_to_child
        .iter()
        .map(|binding| binding.parent_vertex)
        .collect::<Vec<_>>();
    parents.sort_unstable();
    if parents != before_sparse.core_vertices {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    for (slot, parent_row) in before_sparse.spanner_edge_slots.iter().enumerate() {
        let expected = parent_row
            .as_ref()
            .map(|row| audit_map_parent_edge(row, &initial.vertex_map))
            .transpose()?;
        if initial.graph.edge_slots[slot] != expected {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
    }
    Ok(())
}

fn audit_project(
    initial: &DynamicLevelProjectionState,
    before_sparse: &DynamicSparseCoreSnapshot,
    after_sparse: &DynamicSparseCoreSnapshot,
    sparse_updates: &[DynamicSparseCoreUpdate],
) -> Result<DynamicLevelProjectionResult, DynamicLevelProjectionError> {
    if before_sparse.stage.checked_add(1) != Some(after_sparse.stage)
        || sparse_updates.len() > DYNAMIC_LEVEL_PROJECTION_MAX_UPDATES
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let first_forced = sparse_updates
        .iter()
        .position(|update| matches!(update, DynamicSparseCoreUpdate::ForcedReinsert { .. }));
    if first_forced.is_some_and(|first| {
        sparse_updates[first..]
            .iter()
            .any(|update| !matches!(update, DynamicSparseCoreUpdate::ForcedReinsert { .. }))
    }) {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let mut state = initial.clone();
    let mut records = Vec::with_capacity(sparse_updates.len());
    let mut metrics = DynamicLevelProjectionMetrics::default();
    let mut in_forced_tail = false;
    for source in sparse_updates {
        let is_forced = matches!(source, DynamicSparseCoreUpdate::ForcedReinsert { .. });
        if is_forced {
            in_forced_tail = true;
        } else if in_forced_tail {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
        let record = audit_apply_parent_update(&mut state, source)?;
        audit_account_update(&mut metrics, &record)?;
        records.push(record);
    }
    metrics.source_records = u64::try_from(sparse_updates.len())
        .map_err(|_| DynamicLevelProjectionError::TraceVerification)?;
    state.graph.stage = after_sparse.stage;
    audit_verify_after(&state, after_sparse)?;
    Ok(DynamicLevelProjectionResult {
        batch: DynamicLevelStageBatch {
            outer_stage: after_sparse.stage,
            updates: records,
        },
        final_state: state,
        metrics,
    })
}

fn audit_apply_parent_update(
    state: &mut DynamicLevelProjectionState,
    source: &DynamicSparseCoreUpdate,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    match source {
        DynamicSparseCoreUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            new_side_incidences,
            encoded_side,
            encoded_incidences,
        } => audit_apply_parent_split(
            state,
            *retained_vertex,
            *new_vertex,
            new_side_incidences,
            *encoded_side,
            encoded_incidences,
        ),
        DynamicSparseCoreUpdate::EdgeInserted { edge, reason } => {
            let row = audit_map_parent_edge(edge, &state.vertex_map)?;
            let Some(slot) = state.graph.edge_slots.get_mut(row.edge) else {
                return Err(DynamicLevelProjectionError::TraceVerification);
            };
            if slot.is_some() {
                return Err(DynamicLevelProjectionError::TraceVerification);
            }
            *slot = Some(row.clone());
            Ok(DynamicLevelUpdate::EdgeInserted {
                edge: row,
                reason: *reason,
            })
        }
        DynamicSparseCoreUpdate::EdgeDeleted { edge, reason } => {
            let row = audit_map_parent_edge(edge, &state.vertex_map)?;
            let Some(slot) = state.graph.edge_slots.get_mut(row.edge) else {
                return Err(DynamicLevelProjectionError::TraceVerification);
            };
            if slot.as_ref() != Some(&row) {
                return Err(DynamicLevelProjectionError::TraceVerification);
            }
            *slot = None;
            Ok(DynamicLevelUpdate::EdgeDeleted {
                edge: row,
                reason: *reason,
            })
        }
        DynamicSparseCoreUpdate::EdgeReinserted { before, after } => {
            let old = audit_map_parent_edge(before, &state.vertex_map)?;
            let new = audit_map_parent_edge(after, &state.vertex_map)?;
            if old.edge != new.edge || old.from != new.from || old.to != new.to {
                return Err(DynamicLevelProjectionError::TraceVerification);
            }
            let Some(slot) = state.graph.edge_slots.get_mut(old.edge) else {
                return Err(DynamicLevelProjectionError::TraceVerification);
            };
            if slot.as_ref() != Some(&old) {
                return Err(DynamicLevelProjectionError::TraceVerification);
            }
            *slot = Some(new.clone());
            Ok(DynamicLevelUpdate::EdgeReinserted {
                before: old,
                after: new,
                forced_by_reembedding: false,
            })
        }
        DynamicSparseCoreUpdate::GradientReplaced {
            edge,
            before,
            after,
        } => audit_apply_gradient_replacement(state, *edge, before, after),
        DynamicSparseCoreUpdate::LengthReplaced {
            edge,
            before,
            after,
        } => audit_apply_length_replacement(state, *edge, before, after),
        DynamicSparseCoreUpdate::ForcedReinsert { edge } => {
            let Some(row) = state.graph.edge_slots.get(*edge).and_then(Option::as_ref) else {
                return Err(DynamicLevelProjectionError::TraceVerification);
            };
            Ok(DynamicLevelUpdate::EdgeReinserted {
                before: row.clone(),
                after: row.clone(),
                forced_by_reembedding: true,
            })
        }
    }
}

fn audit_apply_gradient_replacement(
    state: &mut DynamicLevelProjectionState,
    edge: usize,
    before: &BigRational,
    after: &BigRational,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let Some(row) = state
        .graph
        .edge_slots
        .get_mut(edge)
        .and_then(Option::as_mut)
    else {
        return Err(DynamicLevelProjectionError::TraceVerification);
    };
    if row.gradient != *before {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let old = row.clone();
    row.gradient = after.clone();
    Ok(DynamicLevelUpdate::AttributesReplaced {
        before: old,
        after: row.clone(),
    })
}

fn audit_apply_length_replacement(
    state: &mut DynamicLevelProjectionState,
    edge: usize,
    before: &BigRational,
    after: &BigRational,
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let Some(row) = state
        .graph
        .edge_slots
        .get_mut(edge)
        .and_then(Option::as_mut)
    else {
        return Err(DynamicLevelProjectionError::TraceVerification);
    };
    if row.length != *before {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let old = row.clone();
    row.length = after.clone();
    Ok(DynamicLevelUpdate::AttributesReplaced {
        before: old,
        after: row.clone(),
    })
}

fn audit_apply_parent_split(
    state: &mut DynamicLevelProjectionState,
    retained_parent: usize,
    new_parent: usize,
    new_side: &[DynamicCoreIncidence],
    encoded_side: DynamicCoreEncodedSide,
    encoded: &[DynamicCoreIncidence],
) -> Result<DynamicLevelUpdate, DynamicLevelProjectionError> {
    let retained_child = state
        .vertex_map
        .parent_to_child
        .iter()
        .find(|binding| binding.parent_vertex == retained_parent)
        .map(|binding| binding.child_vertex)
        .ok_or(DynamicLevelProjectionError::TraceVerification)?;
    if state
        .vertex_map
        .parent_to_child
        .iter()
        .any(|binding| binding.parent_vertex == new_parent)
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let new_child = state.graph.active_node_count;
    let all = audit_incidences_at(&state.graph.edge_slots, retained_child);
    if new_side.windows(2).any(|pair| pair[0] >= pair[1])
        || encoded.windows(2).any(|pair| pair[0] >= pair[1])
        || new_side.iter().any(|incidence| !all.contains(incidence))
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let stay = all
        .iter()
        .copied()
        .filter(|incidence| !new_side.contains(incidence))
        .collect::<Vec<_>>();
    let (side, encoding) = if new_side.len() <= stay.len() {
        (DynamicCoreEncodedSide::New, new_side.to_vec())
    } else {
        (DynamicCoreEncodedSide::Retained, stay)
    };
    if side != encoded_side || encoding != encoded {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    for incidence in new_side {
        let Some(row) = state
            .graph
            .edge_slots
            .get_mut(incidence.edge)
            .and_then(Option::as_mut)
        else {
            return Err(DynamicLevelProjectionError::TraceVerification);
        };
        let endpoint = if incidence.endpoint == DynamicCoreIncidenceEndpoint::Tail {
            &mut row.from
        } else {
            &mut row.to
        };
        if *endpoint != retained_child {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
        *endpoint = new_child;
    }
    state
        .vertex_map
        .parent_to_child
        .push(DynamicLevelVertexBinding {
            parent_vertex: new_parent,
            child_vertex: new_child,
        });
    state.vertex_map.parent_to_child.sort_unstable();
    state.vertex_map.child_to_parent.push(new_parent);
    state.graph.active_node_count = state
        .graph
        .active_node_count
        .checked_add(1)
        .ok_or(DynamicLevelProjectionError::TraceVerification)?;
    Ok(DynamicLevelUpdate::VertexSplit {
        retained_vertex: retained_child,
        new_vertex: new_child,
        new_side_incidences: new_side.to_vec(),
        encoded_side,
        encoded_incidences: encoded.to_vec(),
        provenance: DynamicLevelSplitProvenance {
            retained_parent_vertex: retained_parent,
            new_parent_vertex: new_parent,
        },
    })
}

fn audit_verify_after(
    state: &DynamicLevelProjectionState,
    after_sparse: &DynamicSparseCoreSnapshot,
) -> Result<(), DynamicLevelProjectionError> {
    if state.graph.stage != after_sparse.stage
        || state.vertex_map.parent_to_child.len() != after_sparse.core_vertices.len()
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    let parents = state
        .vertex_map
        .parent_to_child
        .iter()
        .map(|binding| binding.parent_vertex)
        .collect::<Vec<_>>();
    if parents != after_sparse.core_vertices
        || state.graph.edge_slots.len() != after_sparse.spanner_edge_slots.len()
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    for (slot, parent_row) in after_sparse.spanner_edge_slots.iter().enumerate() {
        let expected = parent_row
            .as_ref()
            .map(|row| audit_map_parent_edge(row, &state.vertex_map))
            .transpose()?;
        if state.graph.edge_slots[slot] != expected {
            return Err(DynamicLevelProjectionError::TraceVerification);
        }
    }
    Ok(())
}

fn audit_map_parent_edge(
    edge: &DynamicCoreEdge,
    map: &DynamicLevelVertexMap,
) -> Result<DynamicLevelEdge, DynamicLevelProjectionError> {
    let from = map
        .parent_to_child
        .iter()
        .find(|binding| binding.parent_vertex == edge.from)
        .map(|binding| binding.child_vertex)
        .ok_or(DynamicLevelProjectionError::TraceVerification)?;
    let to = map
        .parent_to_child
        .iter()
        .find(|binding| binding.parent_vertex == edge.to)
        .map(|binding| binding.child_vertex)
        .ok_or(DynamicLevelProjectionError::TraceVerification)?;
    if edge.edge >= DYNAMIC_SPARSE_CORE_MAX_EDGES
        || edge.length <= BigRational::zero()
        || audit_rational_too_wide(&edge.length)
        || audit_rational_too_wide(&edge.gradient)
    {
        return Err(DynamicLevelProjectionError::TraceVerification);
    }
    Ok(DynamicLevelEdge {
        edge: edge.edge,
        from,
        to,
        length: edge.length.clone(),
        gradient: edge.gradient.clone(),
    })
}

fn audit_incidences_at(
    rows: &[Option<DynamicLevelEdge>],
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

fn audit_account_update(
    metrics: &mut DynamicLevelProjectionMetrics,
    update: &DynamicLevelUpdate,
) -> Result<(), DynamicLevelProjectionError> {
    let add = |value: &mut u64, amount: u64| {
        *value = value
            .checked_add(amount)
            .ok_or(DynamicLevelProjectionError::TraceVerification)?;
        Ok::<(), DynamicLevelProjectionError>(())
    };
    match update {
        DynamicLevelUpdate::VertexSplit {
            new_side_incidences,
            encoded_incidences,
            ..
        } => {
            add(
                &mut metrics.moved_incidences,
                u64::try_from(new_side_incidences.len())
                    .map_err(|_| DynamicLevelProjectionError::TraceVerification)?,
            )?;
            add(
                &mut metrics.encoded_incidences,
                u64::try_from(encoded_incidences.len())
                    .map_err(|_| DynamicLevelProjectionError::TraceVerification)?,
            )?;
        }
        DynamicLevelUpdate::EdgeInserted { .. } => add(&mut metrics.edge_insertions, 1)?,
        DynamicLevelUpdate::EdgeDeleted { .. } => add(&mut metrics.edge_deletions, 1)?,
        DynamicLevelUpdate::EdgeReinserted { .. } => add(&mut metrics.edge_reinsertions, 1)?,
        DynamicLevelUpdate::AttributesReplaced { .. } => {
            add(&mut metrics.attribute_replacements, 1)?;
        }
    }
    Ok(())
}

fn audit_rational_too_wide(value: &BigRational) -> bool {
    let numerator = value
        .numer()
        .abs()
        .to_biguint()
        .map_or(0, |value| value.bits());
    let denominator = value
        .denom()
        .abs()
        .to_biguint()
        .map_or(0, |value| value.bits());
    numerator > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
        || denominator > DYNAMIC_SPARSE_CORE_MAX_RATIONAL_BITS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DynamicCoreGraphInput, DynamicCoreGraphOperation, DynamicLowStretchForestEdge,
        DynamicLowStretchForestInput, DynamicSparseCoreEventKind, DynamicSparseCoreInput,
        trace_dynamic_sparse_core,
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

    fn sparse_input() -> DynamicSparseCoreInput {
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

    #[test]
    fn direct_insert_and_forced_reinsert_remain_one_outer_stage() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let sparse = trace_dynamic_sparse_core(&sparse_input(), &operations).expect("sparse");
        let event = &sparse.events[0];
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &event.kind else {
            panic!("updated");
        };
        let initial = initialize_dynamic_level_projection(&event.before).expect("initial");
        let trace =
            trace_dynamic_level_projection(&initial, &event.before, &event.after, sparse_updates)
                .expect("projection");
        assert_eq!(trace.result.batch.outer_stage, 1);
        assert_eq!(trace.result.final_state.graph.stage, 1);
        assert!(trace.result.batch.updates.iter().any(|update| matches!(
            update,
            DynamicLevelUpdate::EdgeInserted { edge, .. } if edge.edge == 4
        )));
        assert!(trace.result.batch.updates.iter().any(|update| matches!(
            update,
            DynamicLevelUpdate::EdgeReinserted {
                before,
                forced_by_reembedding: true,
                ..
            } if before.edge == 4
        )));
        check_dynamic_level_projection_trace(
            &initial,
            &event.before,
            &event.after,
            sparse_updates,
            &trace,
        )
        .expect("check");
    }

    #[test]
    fn selected_reinsert_carries_exact_before_and_after_attributes() {
        let mut input = sparse_input();
        input.core.forest.initial_root_seeds = vec![0, 1, 2, 3];
        let operations = vec![DynamicCoreGraphOperation::Reinsert { edge: 3 }];
        let sparse = trace_dynamic_sparse_core(&input, &operations).expect("sparse");
        let event = &sparse.events[0];
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &event.kind else {
            panic!("updated");
        };
        let initial = initialize_dynamic_level_projection(&event.before).expect("initial");
        let result =
            execute_dynamic_level_projection(&initial, &event.before, &event.after, sparse_updates)
                .expect("projection");
        assert!(result.batch.updates.iter().any(|update| matches!(
            update,
            DynamicLevelUpdate::EdgeReinserted {
                before,
                after,
                forced_by_reembedding: false,
            } if before.edge == 3 && before.length >= after.length
        )));
    }

    fn manual_snapshot(
        vertices: Vec<usize>,
        rows: Vec<Option<DynamicCoreEdge>>,
        stage: u64,
    ) -> DynamicSparseCoreSnapshot {
        let width = rows.len();
        DynamicSparseCoreSnapshot {
            core_vertices: vertices,
            core_edge_slots: rows.clone(),
            spanner_edge_slots: rows,
            core_to_spanner: vec![Vec::new(); width],
            sparsify_certificates: Vec::new(),
            dynamic_spanner_buckets: Vec::new(),
            direct_edges: vec![false; width],
            gamma_length: 1,
            gamma_congestion: 1,
            last_reembedded: Vec::new(),
            stage,
            complete: false,
            metrics: crate::DynamicSparseCoreMetrics::default(),
        }
    }

    fn core_edge(edge: usize, from: usize, to: usize) -> DynamicCoreEdge {
        DynamicCoreEdge {
            edge,
            from,
            to,
            length: rational(1),
            gradient: rational(i64::try_from(edge).expect("test edge fits i64")),
        }
    }

    #[test]
    fn retained_encoded_side_does_not_swap_parent_identity() {
        let before = manual_snapshot(
            vec![10, 20, 30, 40, 50],
            vec![
                Some(core_edge(0, 10, 20)),
                Some(core_edge(1, 10, 30)),
                Some(core_edge(2, 10, 40)),
                Some(core_edge(3, 10, 50)),
            ],
            0,
        );
        let moved = vec![
            DynamicCoreIncidence {
                edge: 1,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
            DynamicCoreIncidence {
                edge: 2,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
            DynamicCoreIncidence {
                edge: 3,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            },
        ];
        let updates = vec![DynamicSparseCoreUpdate::VertexSplit {
            retained_vertex: 10,
            new_vertex: 99,
            new_side_incidences: moved.clone(),
            encoded_side: DynamicCoreEncodedSide::Retained,
            encoded_incidences: vec![DynamicCoreIncidence {
                edge: 0,
                endpoint: DynamicCoreIncidenceEndpoint::Tail,
            }],
        }];
        let mut after_rows = before.spanner_edge_slots.clone();
        for row in after_rows.iter_mut().take(4).skip(1) {
            row.as_mut().expect("row").from = 99;
        }
        let after = manual_snapshot(vec![10, 20, 30, 40, 50, 99], after_rows, 1);
        let initial = initialize_dynamic_level_projection(&before).expect("initial");
        let result = execute_dynamic_level_projection(&initial, &before, &after, &updates)
            .expect("projection");
        assert_eq!(
            child_for_parent(&result.final_state.vertex_map, 10),
            Some(0)
        );
        assert_eq!(
            child_for_parent(&result.final_state.vertex_map, 99),
            Some(5)
        );
        let DynamicLevelUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            encoded_side,
            ..
        } = &result.batch.updates[0]
        else {
            panic!("split");
        };
        assert_eq!((*retained_vertex, *new_vertex), (0, 5));
        assert_eq!(*encoded_side, DynamicCoreEncodedSide::Retained);
    }

    #[test]
    fn empty_split_and_single_self_loop_incidence_are_representable() {
        let before = manual_snapshot(vec![10], vec![Some(core_edge(0, 10, 10)), None], 0);
        let updates = vec![
            DynamicSparseCoreUpdate::VertexSplit {
                retained_vertex: 10,
                new_vertex: 11,
                new_side_incidences: Vec::new(),
                encoded_side: DynamicCoreEncodedSide::New,
                encoded_incidences: Vec::new(),
            },
            DynamicSparseCoreUpdate::VertexSplit {
                retained_vertex: 10,
                new_vertex: 12,
                new_side_incidences: vec![DynamicCoreIncidence {
                    edge: 0,
                    endpoint: DynamicCoreIncidenceEndpoint::Head,
                }],
                encoded_side: DynamicCoreEncodedSide::New,
                encoded_incidences: vec![DynamicCoreIncidence {
                    edge: 0,
                    endpoint: DynamicCoreIncidenceEndpoint::Head,
                }],
            },
        ];
        let after = manual_snapshot(vec![10, 11, 12], vec![Some(core_edge(0, 10, 12)), None], 1);
        let initial = initialize_dynamic_level_projection(&before).expect("initial");
        let result = execute_dynamic_level_projection(&initial, &before, &after, &updates)
            .expect("projection");
        assert_eq!(result.final_state.graph.active_node_count, 3);
        assert_eq!(
            result.final_state.graph.edge_slots[0]
                .as_ref()
                .map(|row| (row.from, row.to)),
            Some((0, 2))
        );
    }

    #[test]
    fn stable_holes_and_reverse_orientation_survive_projection() {
        let before = manual_snapshot(vec![5, 9], vec![None, None, Some(core_edge(2, 9, 5))], 4);
        let after = before.clone();
        let initial = initialize_dynamic_level_projection(&before).expect("initial");
        let result = execute_dynamic_level_projection(&initial, &before, &after, &[]);
        assert_eq!(result, Err(DynamicLevelProjectionError::InvalidInput));

        let mut after = after;
        after.stage = 5;
        let result =
            execute_dynamic_level_projection(&initial, &before, &after, &[]).expect("projection");
        assert!(result.final_state.graph.edge_slots[0].is_none());
        assert!(result.final_state.graph.edge_slots[1].is_none());
        assert_eq!(
            result.final_state.graph.edge_slots[2]
                .as_ref()
                .map(|row| (row.edge, row.from, row.to)),
            Some((2, 1, 0))
        );
    }

    #[test]
    fn checker_rejects_batch_mapping_and_forced_order_tampering() {
        let operations = vec![DynamicCoreGraphOperation::Insert {
            edge: edge(4, 2, 0, 3),
            gradient: rational(-4),
        }];
        let sparse = trace_dynamic_sparse_core(&sparse_input(), &operations).expect("sparse");
        let event = &sparse.events[0];
        let DynamicSparseCoreEventKind::Updated { sparse_updates, .. } = &event.kind else {
            panic!("updated");
        };
        let initial = initialize_dynamic_level_projection(&event.before).expect("initial");
        let trace =
            trace_dynamic_level_projection(&initial, &event.before, &event.after, sparse_updates)
                .expect("trace");

        let mut tampered = trace.clone();
        tampered.event.batch.updates.pop();
        assert_eq!(
            check_dynamic_level_projection_trace(
                &initial,
                &event.before,
                &event.after,
                sparse_updates,
                &tampered,
            ),
            Err(DynamicLevelProjectionError::TraceVerification)
        );

        let mut tampered = trace.clone();
        tampered.event.after.vertex_map.child_to_parent[0] = usize::MAX;
        assert_eq!(
            check_dynamic_level_projection_trace(
                &initial,
                &event.before,
                &event.after,
                sparse_updates,
                &tampered,
            ),
            Err(DynamicLevelProjectionError::TraceVerification)
        );

        let mut bad_order = sparse_updates.clone();
        let forced = bad_order.pop().expect("forced");
        bad_order.insert(0, forced);
        assert_eq!(
            execute_dynamic_level_projection(&initial, &event.before, &event.after, &bad_order),
            Err(DynamicLevelProjectionError::InvalidInput)
        );
    }
}
