//! Bounded explicit realization of the CKLPPS decremental-spanner reduction.
//!
//! The source reduction maintains level graphs `H_0..H_L`, partial embeddings
//! `Pi_0..Pi_L`, and touched sets `S_0..S_L`.  This module keeps those exact
//! abstract states on the repository's small graph band.  Expensive delegated
//! subroutines are replaced by exhaustive deterministic routines, but the
//! stage-dependent rebuild level, projection graph, preimage selection,
//! composed embedding, and reported re-embedded set `D` are retained.
//!
//! The runtime is intentionally decremental.  A caller that also receives
//! insertions must retain new edges directly until it starts a fresh epoch, as
//! required by the Dynamic Sparse Core composition.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use super::deterministic_spanner_sparsify::{
    DeterministicSpannerArc, DeterministicSpannerInputEdge,
    DeterministicSpannerSparsifyCertificate, DeterministicSpannerSparsifyError,
    bounded_deterministic_sparsify, check_bounded_deterministic_sparsify_certificate,
};

/// Maximum levels for the explicit source schedule.
pub const BOUNDED_DYNAMIC_SPANNER_MAX_LEVELS: usize = 4;
/// Maximum decremental micro-stages retained in one trace.
pub const BOUNDED_DYNAMIC_SPANNER_MAX_STAGES: usize = 256;

/// Stable endpoint moved by one source vertex split.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BoundedDynamicSpannerEndpoint {
    /// Move the stored tail from the retained vertex to the new vertex.
    Tail,
    /// Move the stored head from the retained vertex to the new vertex.
    Head,
}

/// One current stable graph row. Deleted rows retain their latest endpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerEdgeState {
    /// Current stored tail.
    pub from: usize,
    /// Current stored head.
    pub to: usize,
    /// Whether the decremental edge is still present.
    pub active: bool,
}

/// One single-update source stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundedDynamicSpannerUpdate {
    /// Delete one active stable edge.
    Delete {
        /// Stable edge ID.
        edge: usize,
    },
    /// Split a vertex and move the listed active incidences.
    VertexSplit {
        /// Existing vertex retained by the split.
        retained_vertex: usize,
        /// New vertex created by the split.
        new_vertex: usize,
        /// Strictly sorted stable edge/endpoint moves.
        moved: Vec<(usize, BoundedDynamicSpannerEndpoint)>,
    },
}

/// One projected auxiliary edge and its exact host embedding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerProjectedEdge {
    /// Dense edge ID in the auxiliary graph `J`.
    pub projected_edge: usize,
    /// Stable source edge represented by this projected edge.
    pub source_edge: usize,
    /// Projected tail in `S_(j-1)`.
    pub from: usize,
    /// Projected head in `S_(j-1)`.
    pub to: usize,
    /// Current path from the source tail to `from` in the old embedding graph.
    pub source_to_from: Vec<DeterministicSpannerArc>,
    /// Current path from `to` to the source head in the old embedding graph.
    pub to_to_source: Vec<DeterministicSpannerArc>,
    /// Source `Pi_(J -> H_<j union E_affected)` path from `from` to `to`.
    pub projected_to_host: Vec<DeterministicSpannerArc>,
}

/// Complete certificate for one nontrivial level rebuild.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerProjectionCertificate {
    /// Source micro-stage.
    pub stage: usize,
    /// Rebuilt level `j`.
    pub level: usize,
    /// `S_(j-1)` immediately before the suffix reset.
    pub touched_vertices: Vec<usize>,
    /// Stable active edges whose old embedding graph met the touched set.
    pub affected_edges: Vec<usize>,
    /// Non-loop auxiliary graph edges in stable source order.
    pub projected_edges: Vec<BoundedDynamicSpannerProjectedEdge>,
    /// Exact static deterministic source certificate for auxiliary `J`.
    pub sparsify: Option<DeterministicSpannerSparsifyCertificate>,
    /// Preimages of selected auxiliary edges inserted into `H_j`.
    pub selected_source_edges: Vec<usize>,
    /// Image of selected auxiliary edges in the host graph.
    pub reembedded_edges: Vec<usize>,
}

/// One source level `H_j`, `Pi_j`, and `S_j`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerLevelSnapshot {
    /// Stable source edges whose preimages form `H_j`.
    pub selected_edges: Vec<usize>,
    /// Partial map `Pi_j`, indexed by stable source edge.
    pub embeddings: Vec<Option<Vec<DeterministicSpannerArc>>>,
    /// Source touched set `S_j`.
    pub touched_vertices: Vec<usize>,
    /// Global micro-stage at which this level was rebuilt.
    pub last_rebuilt_stage: usize,
    /// Static certificate used for this level's auxiliary graph, when nonempty.
    pub sparsify_certificate: Option<DeterministicSpannerSparsifyCertificate>,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BoundedDynamicSpannerMetrics {
    /// Applied decremental micro-stages.
    pub source_stages: u64,
    /// Whole-epoch restarts at the padded source period.
    pub restarts: u64,
    /// Nonzero level rebuilds.
    pub level_rebuilds: u64,
    /// Active source edges classified as affected.
    pub affected_edges: u64,
    /// Non-loop auxiliary edges materialized.
    pub projected_edges: u64,
    /// Selected auxiliary preimages inserted into level graphs.
    pub selected_preimages: u64,
    /// Host edges reported through `D`.
    pub reembedded_edges: u64,
    /// Explicit embedding arcs inspected or published.
    pub embedding_arcs: u64,
}

/// Full level schedule and implicit union spanner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerSnapshot {
    /// Stable vertex universe, including isolated split targets.
    pub vertex_count: usize,
    /// Current stable source rows; deleted rows retain endpoints.
    pub edge_rows: Vec<BoundedDynamicSpannerEdgeState>,
    /// Source levels `0..L`.
    pub levels: Vec<BoundedDynamicSpannerLevelSnapshot>,
    /// Sorted union of active `H_j` edges.
    pub spanner_edges: Vec<usize>,
    /// Implicit highest-level embedding `Pi_<=L`.
    pub graph_to_spanner: Vec<Vec<DeterministicSpannerArc>>,
    /// Current source low-re-embedding set `D`.
    pub last_reembedded: Vec<usize>,
    /// Most recent nonzero-level projection certificate.
    pub last_projection: Option<BoundedDynamicSpannerProjectionCertificate>,
    /// Total processed micro-stages across restarts.
    pub total_stage: usize,
    /// Whole-period restart epoch.
    pub epoch: usize,
    /// Current source stage inside the epoch.
    pub stage_in_epoch: usize,
    /// Padded power-of-two period used by the exact level schedule.
    pub period: usize,
    /// Exact cumulative work counters.
    pub metrics: BoundedDynamicSpannerMetrics,
}

/// Reversible state after every single source update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedDynamicSpannerTrace {
    /// Checked source initialization.
    pub initial: BoundedDynamicSpannerSnapshot,
    /// Applied source updates.
    pub updates: Vec<BoundedDynamicSpannerUpdate>,
    /// Reversible snapshot after every update.
    pub snapshots: Vec<BoundedDynamicSpannerSnapshot>,
    /// Final snapshot, duplicated for constant-time consumers.
    pub final_snapshot: BoundedDynamicSpannerSnapshot,
}

/// Closed failure contract for the bounded explicit schedule.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BoundedDynamicSpannerError {
    /// Input violates the closed graph or update contract.
    #[error("bounded dynamic spanner input is invalid")]
    InvalidInput,
    /// Explicit small-graph admission was exceeded.
    #[error("bounded dynamic spanner admission exceeded")]
    AdmissionLimit,
    /// A source state or certificate invariant failed.
    #[error("bounded dynamic spanner invariant failed")]
    InvariantViolation,
    /// Checked counter or size arithmetic overflowed.
    #[error("bounded dynamic spanner arithmetic overflow")]
    ArithmeticOverflow,
    /// One source edge could not be projected onto the current touched set.
    #[error("bounded dynamic spanner projection failed for stable edge {0}")]
    ProjectionEdge(usize),
}

/// Builds the source initialization and applies each decremental micro-stage.
///
/// # Errors
///
/// Returns a typed error when the bounded admission is exceeded, an update is
/// malformed, checked arithmetic overflows, or a source invariant cannot be
/// certified.
pub fn trace_bounded_dynamic_spanner(
    vertex_count: usize,
    edge_rows: Vec<BoundedDynamicSpannerEdgeState>,
    updates: Vec<BoundedDynamicSpannerUpdate>,
) -> Result<BoundedDynamicSpannerTrace, BoundedDynamicSpannerError> {
    validate_initial(vertex_count, &edge_rows, updates.len())?;
    let initial = initialize_snapshot(vertex_count, edge_rows)?;
    let mut current = initial.clone();
    let mut snapshots = Vec::with_capacity(updates.len());
    for update in &updates {
        current = apply_update(&current, update)?;
        snapshots.push(current.clone());
    }
    Ok(BoundedDynamicSpannerTrace {
        initial,
        updates,
        snapshots,
        final_snapshot: current,
    })
}

fn validate_initial(
    vertex_count: usize,
    edge_rows: &[BoundedDynamicSpannerEdgeState],
    stage_count: usize,
) -> Result<(), BoundedDynamicSpannerError> {
    if !(2..=8).contains(&vertex_count)
        || edge_rows.len() > 12
        || stage_count > BOUNDED_DYNAMIC_SPANNER_MAX_STAGES
    {
        return Err(BoundedDynamicSpannerError::AdmissionLimit);
    }
    if edge_rows.iter().any(|edge| {
        edge.from >= vertex_count || edge.to >= vertex_count || edge.from == edge.to || !edge.active
    }) {
        return Err(BoundedDynamicSpannerError::InvalidInput);
    }
    Ok(())
}

fn initialize_snapshot(
    vertex_count: usize,
    edge_rows: Vec<BoundedDynamicSpannerEdgeState>,
) -> Result<BoundedDynamicSpannerSnapshot, BoundedDynamicSpannerError> {
    let period = vertex_count.next_power_of_two();
    let depth = period.trailing_zeros() as usize;
    if depth + 1 > BOUNDED_DYNAMIC_SPANNER_MAX_LEVELS {
        return Err(BoundedDynamicSpannerError::AdmissionLimit);
    }
    let initial_edges = active_input_edges(&edge_rows);
    let sparse = bounded_deterministic_sparsify(vertex_count, edge_rows.len(), &initial_edges)
        .map_err(map_static_error)?;
    let mut embeddings = vec![None; edge_rows.len()];
    for edge in &initial_edges {
        embeddings[edge.edge] = Some(sparse.embedding[edge.edge].clone());
    }
    let mut levels = Vec::with_capacity(depth + 1);
    levels.push(BoundedDynamicSpannerLevelSnapshot {
        selected_edges: sparse.selected_edges.clone(),
        embeddings,
        touched_vertices: Vec::new(),
        last_rebuilt_stage: 0,
        sparsify_certificate: Some(sparse.certificate),
    });
    for _ in 1..=depth {
        levels.push(empty_level(edge_rows.len(), 0));
    }
    let mut snapshot = BoundedDynamicSpannerSnapshot {
        vertex_count,
        edge_rows,
        levels,
        spanner_edges: Vec::new(),
        graph_to_spanner: vec![Vec::new(); initial_edges.len()],
        last_reembedded: Vec::new(),
        last_projection: None,
        total_stage: 0,
        epoch: 0,
        stage_in_epoch: 0,
        period,
        metrics: BoundedDynamicSpannerMetrics {
            selected_preimages: u64::try_from(sparse.selected_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
            embedding_arcs: count_arcs(&sparse.embedding)?,
            ..BoundedDynamicSpannerMetrics::default()
        },
    };
    refresh_implicit_state(&mut snapshot)?;
    Ok(snapshot)
}

fn empty_level(edge_count: usize, stage: usize) -> BoundedDynamicSpannerLevelSnapshot {
    BoundedDynamicSpannerLevelSnapshot {
        selected_edges: Vec::new(),
        embeddings: vec![None; edge_count],
        touched_vertices: Vec::new(),
        last_rebuilt_stage: stage,
        sparsify_certificate: None,
    }
}

fn active_input_edges(
    rows: &[BoundedDynamicSpannerEdgeState],
) -> Vec<DeterministicSpannerInputEdge> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.active)
        .map(|(edge, row)| DeterministicSpannerInputEdge {
            edge,
            from: row.from,
            to: row.to,
        })
        .collect()
}

fn map_static_error(error: DeterministicSpannerSparsifyError) -> BoundedDynamicSpannerError {
    match error {
        DeterministicSpannerSparsifyError::ArithmeticOverflow => {
            BoundedDynamicSpannerError::ArithmeticOverflow
        }
        DeterministicSpannerSparsifyError::AdmissionLimit => {
            BoundedDynamicSpannerError::AdmissionLimit
        }
        DeterministicSpannerSparsifyError::InvalidInput => BoundedDynamicSpannerError::InvalidInput,
        DeterministicSpannerSparsifyError::InvariantViolation => {
            BoundedDynamicSpannerError::InvariantViolation
        }
    }
}

fn apply_update(
    before: &BoundedDynamicSpannerSnapshot,
    update: &BoundedDynamicSpannerUpdate,
) -> Result<BoundedDynamicSpannerSnapshot, BoundedDynamicSpannerError> {
    let mut next = before.clone();
    let touched = apply_graph_update(&mut next, update)?;
    next.total_stage = next
        .total_stage
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    next.stage_in_epoch = next
        .stage_in_epoch
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    next.metrics.source_stages = next
        .metrics
        .source_stages
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;

    for level in &mut next.levels {
        let mut set = level
            .touched_vertices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        set.extend(touched.iter().copied());
        level.touched_vertices = set.into_iter().collect();
        level
            .selected_edges
            .retain(|&edge| next.edge_rows[edge].active);
    }

    if next.stage_in_epoch == next.period {
        restart_epoch(&mut next)?;
    } else {
        let level = rebuild_level(next.stage_in_epoch, next.period, next.levels.len())?;
        rebuild_level_suffix(&mut next, level)?;
    }
    refresh_implicit_state(&mut next)?;
    check_snapshot_invariants(&next)?;
    Ok(next)
}

fn apply_graph_update(
    snapshot: &mut BoundedDynamicSpannerSnapshot,
    update: &BoundedDynamicSpannerUpdate,
) -> Result<Vec<usize>, BoundedDynamicSpannerError> {
    match update {
        BoundedDynamicSpannerUpdate::Delete { edge } => {
            let row = snapshot
                .edge_rows
                .get_mut(*edge)
                .ok_or(BoundedDynamicSpannerError::InvalidInput)?;
            if !row.active {
                return Err(BoundedDynamicSpannerError::InvalidInput);
            }
            row.active = false;
            Ok(vec![row.from, row.to])
        }
        BoundedDynamicSpannerUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            moved,
        } => {
            if *retained_vertex >= snapshot.vertex_count
                || *new_vertex >= snapshot.vertex_count
                || retained_vertex == new_vertex
                || moved.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(BoundedDynamicSpannerError::InvalidInput);
            }
            for &(edge, endpoint) in moved {
                let row = snapshot
                    .edge_rows
                    .get_mut(edge)
                    .ok_or(BoundedDynamicSpannerError::InvalidInput)?;
                if !row.active {
                    return Err(BoundedDynamicSpannerError::InvalidInput);
                }
                match endpoint {
                    BoundedDynamicSpannerEndpoint::Tail if row.from == *retained_vertex => {
                        row.from = *new_vertex;
                    }
                    BoundedDynamicSpannerEndpoint::Head if row.to == *retained_vertex => {
                        row.to = *new_vertex;
                    }
                    _ => return Err(BoundedDynamicSpannerError::InvalidInput),
                }
                if row.from == row.to {
                    return Err(BoundedDynamicSpannerError::InvalidInput);
                }
            }
            Ok(vec![*retained_vertex, *new_vertex])
        }
    }
}

fn rebuild_level(
    stage_in_epoch: usize,
    period: usize,
    level_count: usize,
) -> Result<usize, BoundedDynamicSpannerError> {
    for level in 0..level_count {
        let divisor = period >> level;
        if divisor > 0 && stage_in_epoch.is_multiple_of(divisor) {
            return Ok(level);
        }
    }
    Err(BoundedDynamicSpannerError::InvariantViolation)
}

fn restart_epoch(
    snapshot: &mut BoundedDynamicSpannerSnapshot,
) -> Result<(), BoundedDynamicSpannerError> {
    let input = active_input_edges(&snapshot.edge_rows);
    let sparse =
        bounded_deterministic_sparsify(snapshot.vertex_count, snapshot.edge_rows.len(), &input)
            .map_err(map_static_error)?;
    let mut embeddings = vec![None; snapshot.edge_rows.len()];
    for edge in &input {
        embeddings[edge.edge] = Some(sparse.embedding[edge.edge].clone());
    }
    snapshot.levels[0] = BoundedDynamicSpannerLevelSnapshot {
        selected_edges: sparse.selected_edges.clone(),
        embeddings,
        touched_vertices: Vec::new(),
        last_rebuilt_stage: snapshot.total_stage,
        sparsify_certificate: Some(sparse.certificate),
    };
    for level in snapshot.levels.iter_mut().skip(1) {
        *level = empty_level(snapshot.edge_rows.len(), snapshot.total_stage);
    }
    snapshot.last_reembedded.clone_from(&sparse.selected_edges);
    snapshot.last_projection = None;
    snapshot.epoch = snapshot
        .epoch
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.stage_in_epoch = 0;
    snapshot.metrics.restarts = snapshot
        .metrics
        .restarts
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.selected_preimages = snapshot
        .metrics
        .selected_preimages
        .checked_add(
            u64::try_from(sparse.selected_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.reembedded_edges = snapshot
        .metrics
        .reembedded_edges
        .checked_add(
            u64::try_from(snapshot.last_reembedded.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.embedding_arcs = snapshot
        .metrics
        .embedding_arcs
        .checked_add(count_arcs(&sparse.embedding)?)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    Ok(())
}

fn rebuild_level_suffix(
    snapshot: &mut BoundedDynamicSpannerSnapshot,
    level: usize,
) -> Result<(), BoundedDynamicSpannerError> {
    if level == 0 || level >= snapshot.levels.len() {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let touched = snapshot.levels[level - 1].touched_vertices.clone();
    for current in snapshot.levels.iter_mut().skip(level) {
        *current = empty_level(snapshot.edge_rows.len(), snapshot.total_stage);
    }
    let mut certificate = build_projection(snapshot, level, &touched)?;
    let mut selected_edges = certificate.selected_source_edges.clone();
    selected_edges.sort_unstable();
    selected_edges.dedup();
    let embeddings = materialize_level_embeddings(snapshot, level, &certificate)?;
    snapshot.levels[level] = BoundedDynamicSpannerLevelSnapshot {
        selected_edges,
        embeddings,
        touched_vertices: Vec::new(),
        last_rebuilt_stage: snapshot.total_stage,
        sparsify_certificate: certificate.sparsify.clone(),
    };
    certificate.reembedded_edges.sort_unstable();
    certificate.reembedded_edges.dedup();
    snapshot
        .last_reembedded
        .clone_from(&certificate.reembedded_edges);
    snapshot.last_projection = Some(certificate.clone());
    snapshot.metrics.level_rebuilds = snapshot
        .metrics
        .level_rebuilds
        .checked_add(1)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.affected_edges = snapshot
        .metrics
        .affected_edges
        .checked_add(
            u64::try_from(certificate.affected_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.projected_edges = snapshot
        .metrics
        .projected_edges
        .checked_add(
            u64::try_from(certificate.projected_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.selected_preimages = snapshot
        .metrics
        .selected_preimages
        .checked_add(
            u64::try_from(certificate.selected_source_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    snapshot.metrics.reembedded_edges = snapshot
        .metrics
        .reembedded_edges
        .checked_add(
            u64::try_from(certificate.reembedded_edges.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    Ok(())
}

enum SourceEdgeProjection {
    Unaffected,
    Contracted,
    Projected(BoundedDynamicSpannerProjectedEdge),
}

fn build_projection(
    snapshot: &BoundedDynamicSpannerSnapshot,
    level: usize,
    touched_vertices: &[usize],
) -> Result<BoundedDynamicSpannerProjectionCertificate, BoundedDynamicSpannerError> {
    let touched = touched_vertices.iter().copied().collect::<BTreeSet<_>>();
    if touched.is_empty() {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let mut affected_edges = Vec::new();
    let mut projected_edges = Vec::new();
    for edge in 0..snapshot.edge_rows.len() {
        match project_source_edge(snapshot, level, &touched, edge, projected_edges.len())? {
            SourceEdgeProjection::Unaffected => {}
            SourceEdgeProjection::Contracted => affected_edges.push(edge),
            SourceEdgeProjection::Projected(projected) => {
                affected_edges.push(edge);
                projected_edges.push(projected);
            }
        }
    }
    let (sparsify, selected_source_edges, reembedded_edges) =
        sparsify_projected_edges(snapshot.vertex_count, &projected_edges)?;
    Ok(BoundedDynamicSpannerProjectionCertificate {
        stage: snapshot.total_stage,
        level,
        touched_vertices: touched_vertices.to_vec(),
        affected_edges,
        projected_edges,
        sparsify,
        selected_source_edges,
        reembedded_edges,
    })
}

fn project_source_edge(
    snapshot: &BoundedDynamicSpannerSnapshot,
    level: usize,
    touched: &BTreeSet<usize>,
    edge: usize,
    projected_edge: usize,
) -> Result<SourceEdgeProjection, BoundedDynamicSpannerError> {
    let row = snapshot.edge_rows[edge];
    if !row.active {
        return Ok(SourceEdgeProjection::Unaffected);
    }
    let projection_error = |_| BoundedDynamicSpannerError::ProjectionEdge(edge);
    let old_path = highest_embedding(snapshot, edge, level).map_err(projection_error)?;
    let path_vertices =
        embedding_graph_vertices(old_path, &snapshot.edge_rows).map_err(projection_error)?;
    if path_vertices.is_disjoint(touched) {
        return Ok(SourceEdgeProjection::Unaffected);
    }
    let allowed = old_path.iter().map(|arc| arc.edge).collect::<BTreeSet<_>>();
    let from = closest_touched_vertex(row.from, &allowed, touched, &snapshot.edge_rows)
        .map_err(projection_error)?;
    let to = closest_touched_vertex(row.to, &allowed, touched, &snapshot.edge_rows)
        .map_err(projection_error)?;
    let source_to_from = shortest_path_on_edges(
        snapshot.vertex_count,
        row.from,
        from,
        &allowed,
        &snapshot.edge_rows,
    )
    .map_err(projection_error)?;
    let to_to_source = shortest_path_on_edges(
        snapshot.vertex_count,
        to,
        row.to,
        &allowed,
        &snapshot.edge_rows,
    )
    .map_err(projection_error)?;
    if from == to {
        return Ok(SourceEdgeProjection::Contracted);
    }
    let mut projected_to_host = reverse_path(&source_to_from);
    projected_to_host.push(DeterministicSpannerArc { edge, direction: 1 });
    projected_to_host.extend(reverse_path(&to_to_source));
    projected_to_host = loop_erase_stable_path(
        snapshot.vertex_count,
        from,
        to,
        &projected_to_host,
        &snapshot.edge_rows,
    )
    .map_err(projection_error)?;
    Ok(SourceEdgeProjection::Projected(
        BoundedDynamicSpannerProjectedEdge {
            projected_edge,
            source_edge: edge,
            from,
            to,
            source_to_from,
            to_to_source,
            projected_to_host,
        },
    ))
}

type ProjectedSparsify = (
    Option<DeterministicSpannerSparsifyCertificate>,
    Vec<usize>,
    Vec<usize>,
);

fn sparsify_projected_edges(
    vertex_count: usize,
    projected_edges: &[BoundedDynamicSpannerProjectedEdge],
) -> Result<ProjectedSparsify, BoundedDynamicSpannerError> {
    if projected_edges.is_empty() {
        return Ok((None, Vec::new(), Vec::new()));
    }
    let input = projected_edges
        .iter()
        .map(|edge| DeterministicSpannerInputEdge {
            edge: edge.projected_edge,
            from: edge.from,
            to: edge.to,
        })
        .collect::<Vec<_>>();
    let result = bounded_deterministic_sparsify(vertex_count, input.len(), &input)
        .map_err(map_static_error)?;
    let selected_source_edges = result
        .selected_edges
        .iter()
        .map(|&projected| projected_edges[projected].source_edge)
        .collect::<Vec<_>>();
    let reembedded_edges = result
        .selected_edges
        .iter()
        .flat_map(|&projected| {
            projected_edges[projected]
                .projected_to_host
                .iter()
                .map(|arc| arc.edge)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok((
        Some(result.certificate),
        selected_source_edges,
        reembedded_edges,
    ))
}

fn materialize_level_embeddings(
    snapshot: &BoundedDynamicSpannerSnapshot,
    level: usize,
    certificate: &BoundedDynamicSpannerProjectionCertificate,
) -> Result<Vec<Option<Vec<DeterministicSpannerArc>>>, BoundedDynamicSpannerError> {
    let mut embeddings = vec![None; snapshot.edge_rows.len()];
    let projected_by_source = certificate
        .projected_edges
        .iter()
        .map(|edge| (edge.source_edge, edge))
        .collect::<BTreeMap<_, _>>();
    for &edge in &certificate.affected_edges {
        let row = snapshot.edge_rows[edge];
        let old_path = highest_embedding(snapshot, edge, level)?;
        let path = if let Some(projected) = projected_by_source.get(&edge) {
            let sparsify = certificate
                .sparsify
                .as_ref()
                .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
            let auxiliary_path = sparsify
                .graph_to_sparsifier
                .get(projected.projected_edge)
                .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
            let mut middle = Vec::new();
            for arc in auxiliary_path {
                let selected = certificate
                    .projected_edges
                    .get(arc.edge)
                    .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
                if arc.direction == 1 {
                    middle.extend_from_slice(&selected.projected_to_host);
                } else if arc.direction == -1 {
                    middle.extend(reverse_path(&selected.projected_to_host));
                } else {
                    return Err(BoundedDynamicSpannerError::InvariantViolation);
                }
            }
            let middle = loop_erase_stable_path(
                snapshot.vertex_count,
                projected.from,
                projected.to,
                &middle,
                &snapshot.edge_rows,
            )?;
            let mut path = projected.source_to_from.clone();
            path.extend(middle);
            path.extend_from_slice(&projected.to_to_source);
            loop_erase_stable_path(
                snapshot.vertex_count,
                row.from,
                row.to,
                &path,
                &snapshot.edge_rows,
            )?
        } else {
            let allowed = old_path.iter().map(|arc| arc.edge).collect::<BTreeSet<_>>();
            let touched = certificate
                .touched_vertices
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let projected_vertex =
                closest_touched_vertex(row.from, &allowed, &touched, &snapshot.edge_rows)?;
            let mut path = shortest_path_on_edges(
                snapshot.vertex_count,
                row.from,
                projected_vertex,
                &allowed,
                &snapshot.edge_rows,
            )?;
            path.extend(shortest_path_on_edges(
                snapshot.vertex_count,
                projected_vertex,
                row.to,
                &allowed,
                &snapshot.edge_rows,
            )?);
            loop_erase_stable_path(
                snapshot.vertex_count,
                row.from,
                row.to,
                &path,
                &snapshot.edge_rows,
            )?
        };
        embeddings[edge] = Some(path);
    }
    Ok(embeddings)
}

fn highest_embedding(
    snapshot: &BoundedDynamicSpannerSnapshot,
    edge: usize,
    exclusive_level: usize,
) -> Result<&[DeterministicSpannerArc], BoundedDynamicSpannerError> {
    snapshot.levels[..exclusive_level]
        .iter()
        .rev()
        .find_map(|level| level.embeddings[edge].as_deref())
        .ok_or(BoundedDynamicSpannerError::InvariantViolation)
}

fn embedding_graph_vertices(
    path: &[DeterministicSpannerArc],
    rows: &[BoundedDynamicSpannerEdgeState],
) -> Result<BTreeSet<usize>, BoundedDynamicSpannerError> {
    let mut vertices = BTreeSet::new();
    for arc in path {
        let row = rows
            .get(arc.edge)
            .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        if arc.direction != 1 && arc.direction != -1 {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        vertices.insert(row.from);
        vertices.insert(row.to);
    }
    Ok(vertices)
}

fn closest_touched_vertex(
    start: usize,
    allowed: &BTreeSet<usize>,
    touched: &BTreeSet<usize>,
    rows: &[BoundedDynamicSpannerEdgeState],
) -> Result<usize, BoundedDynamicSpannerError> {
    if touched.contains(&start) {
        return Ok(start);
    }
    let vertex_count = rows
        .iter()
        .flat_map(|row| [row.from, row.to])
        .max()
        .map_or(start + 1, |vertex| vertex + 1);
    let mut distance = vec![usize::MAX; vertex_count];
    let mut queue = VecDeque::from([start]);
    distance[start] = 0;
    while let Some(vertex) = queue.pop_front() {
        for (edge, row) in rows.iter().enumerate() {
            if !row.active || !allowed.contains(&edge) {
                continue;
            }
            let next = if row.from == vertex {
                Some(row.to)
            } else if row.to == vertex {
                Some(row.from)
            } else {
                None
            };
            if let Some(next) = next
                && distance[next] == usize::MAX
            {
                distance[next] = distance[vertex]
                    .checked_add(1)
                    .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
                queue.push_back(next);
            }
        }
    }
    touched
        .iter()
        .copied()
        .filter(|&vertex| vertex < distance.len() && distance[vertex] != usize::MAX)
        .min_by_key(|&vertex| (distance[vertex], vertex))
        .ok_or(BoundedDynamicSpannerError::InvariantViolation)
}

fn shortest_path_on_edges(
    vertex_count: usize,
    source: usize,
    sink: usize,
    allowed: &BTreeSet<usize>,
    rows: &[BoundedDynamicSpannerEdgeState],
) -> Result<Vec<DeterministicSpannerArc>, BoundedDynamicSpannerError> {
    if source == sink {
        return Ok(Vec::new());
    }
    let mut adjacency = vec![Vec::<(usize, usize, i8)>::new(); vertex_count];
    for (edge, row) in rows.iter().enumerate() {
        if !row.active || !allowed.contains(&edge) {
            continue;
        }
        adjacency[row.from].push((edge, row.to, 1));
        adjacency[row.to].push((edge, row.from, -1));
    }
    for list in &mut adjacency {
        list.sort_unstable();
    }
    let mut previous = vec![None::<(usize, DeterministicSpannerArc)>; vertex_count];
    let mut queue = VecDeque::from([source]);
    previous[source] = Some((
        source,
        DeterministicSpannerArc {
            edge: usize::MAX,
            direction: 1,
        },
    ));
    while let Some(vertex) = queue.pop_front() {
        if vertex == sink {
            break;
        }
        for &(edge, next, direction) in &adjacency[vertex] {
            if previous[next].is_none() {
                previous[next] = Some((vertex, DeterministicSpannerArc { edge, direction }));
                queue.push_back(next);
            }
        }
    }
    if previous[sink].is_none() {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let mut path = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let (parent, arc) =
            previous[cursor].ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        path.push(arc);
        cursor = parent;
    }
    path.reverse();
    Ok(path)
}

fn reverse_path(path: &[DeterministicSpannerArc]) -> Vec<DeterministicSpannerArc> {
    path.iter()
        .rev()
        .map(|arc| DeterministicSpannerArc {
            edge: arc.edge,
            direction: -arc.direction,
        })
        .collect()
}

fn loop_erase_stable_path(
    vertex_count: usize,
    source: usize,
    sink: usize,
    walk: &[DeterministicSpannerArc],
    rows: &[BoundedDynamicSpannerEdgeState],
) -> Result<Vec<DeterministicSpannerArc>, BoundedDynamicSpannerError> {
    if source >= vertex_count || sink >= vertex_count {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let mut vertices = vec![source];
    let mut path = Vec::new();
    for &arc in walk {
        let row = rows
            .get(arc.edge)
            .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        if !row.active {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        let current = *vertices
            .last()
            .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        let next = match arc.direction {
            1 if row.from == current => row.to,
            -1 if row.to == current => row.from,
            _ => return Err(BoundedDynamicSpannerError::InvariantViolation),
        };
        if let Some(position) = vertices.iter().position(|&vertex| vertex == next) {
            vertices.truncate(position + 1);
            path.truncate(position);
        } else {
            vertices.push(next);
            path.push(arc);
        }
    }
    if vertices.last().copied() != Some(sink) {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    Ok(path)
}

fn refresh_implicit_state(
    snapshot: &mut BoundedDynamicSpannerSnapshot,
) -> Result<(), BoundedDynamicSpannerError> {
    snapshot.spanner_edges = snapshot
        .levels
        .iter()
        .flat_map(|level| level.selected_edges.iter().copied())
        .filter(|&edge| snapshot.edge_rows[edge].active)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let support = snapshot
        .spanner_edges
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    snapshot.graph_to_spanner = vec![Vec::new(); snapshot.edge_rows.len()];
    for edge in 0..snapshot.edge_rows.len() {
        if !snapshot.edge_rows[edge].active {
            continue;
        }
        let path = snapshot
            .levels
            .iter()
            .rev()
            .find_map(|level| level.embeddings[edge].as_ref())
            .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        let row = snapshot.edge_rows[edge];
        let checked = loop_erase_stable_path(
            snapshot.vertex_count,
            row.from,
            row.to,
            path,
            &snapshot.edge_rows,
        )?;
        if checked != *path || path.iter().any(|arc| !support.contains(&arc.edge)) {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        snapshot.graph_to_spanner[edge].clone_from(path);
    }
    snapshot.metrics.embedding_arcs = snapshot
        .metrics
        .embedding_arcs
        .checked_add(count_arcs(&snapshot.graph_to_spanner)?)
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)?;
    Ok(())
}

fn count_arcs(paths: &[Vec<DeterministicSpannerArc>]) -> Result<u64, BoundedDynamicSpannerError> {
    paths.iter().try_fold(0_u64, |sum, path| {
        sum.checked_add(
            u64::try_from(path.len())
                .map_err(|_| BoundedDynamicSpannerError::ArithmeticOverflow)?,
        )
        .ok_or(BoundedDynamicSpannerError::ArithmeticOverflow)
    })
}

fn check_snapshot_invariants(
    snapshot: &BoundedDynamicSpannerSnapshot,
) -> Result<(), BoundedDynamicSpannerError> {
    if snapshot.vertex_count < 2
        || snapshot.vertex_count > 8
        || snapshot.period != snapshot.vertex_count.next_power_of_two()
        || snapshot.levels.len() != snapshot.period.trailing_zeros() as usize + 1
        || snapshot.stage_in_epoch >= snapshot.period
        || snapshot.graph_to_spanner.len() != snapshot.edge_rows.len()
        || snapshot.levels.iter().any(|level| {
            level.embeddings.len() != snapshot.edge_rows.len()
                || !is_strictly_sorted(&level.selected_edges)
                || !is_strictly_sorted(&level.touched_vertices)
        })
        || !is_strictly_sorted(&snapshot.spanner_edges)
        || !is_strictly_sorted(&snapshot.last_reembedded)
    {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let selected = snapshot
        .levels
        .iter()
        .flat_map(|level| level.selected_edges.iter().copied())
        .filter(|&edge| snapshot.edge_rows.get(edge).is_some_and(|row| row.active))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if selected != snapshot.spanner_edges {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let support = selected.iter().copied().collect::<BTreeSet<_>>();
    for (edge, row) in snapshot.edge_rows.iter().enumerate() {
        if row.from >= snapshot.vertex_count
            || row.to >= snapshot.vertex_count
            || row.from == row.to
        {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        if !row.active {
            if !snapshot.graph_to_spanner[edge].is_empty() {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
            continue;
        }
        let path = &snapshot.graph_to_spanner[edge];
        if path.iter().any(|arc| !support.contains(&arc.edge))
            || loop_erase_stable_path(
                snapshot.vertex_count,
                row.from,
                row.to,
                path,
                &snapshot.edge_rows,
            )? != *path
        {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
    }
    for (index, level) in snapshot.levels.iter().enumerate() {
        if index == 0 && level.sparsify_certificate.is_none() {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        if let Some(certificate) = &level.sparsify_certificate
            && index == 0
        {
            let input = active_input_edges(&snapshot.edge_rows)
                .into_iter()
                .filter(|edge| level.embeddings[edge.edge].is_some())
                .collect::<Vec<_>>();
            if level.last_rebuilt_stage == snapshot.total_stage {
                check_bounded_deterministic_sparsify_certificate(
                    snapshot.vertex_count,
                    snapshot.edge_rows.len(),
                    &input,
                    certificate,
                )
                .map_err(map_static_error)?;
            }
        }
    }
    if let Some(projection) = &snapshot.last_projection {
        check_projection_certificate(snapshot, projection)?;
    }
    Ok(())
}

fn check_projection_certificate(
    snapshot: &BoundedDynamicSpannerSnapshot,
    certificate: &BoundedDynamicSpannerProjectionCertificate,
) -> Result<(), BoundedDynamicSpannerError> {
    if certificate.stage != snapshot.total_stage
        || certificate.level == 0
        || certificate.level >= snapshot.levels.len()
        || !is_strictly_sorted(&certificate.touched_vertices)
        || !is_strictly_sorted(&certificate.affected_edges)
        || !is_strictly_sorted(&certificate.selected_source_edges)
        || !is_strictly_sorted(&certificate.reembedded_edges)
        || certificate
            .projected_edges
            .iter()
            .enumerate()
            .any(|(index, edge)| edge.projected_edge != index)
    {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let projected_input = certificate
        .projected_edges
        .iter()
        .map(|edge| DeterministicSpannerInputEdge {
            edge: edge.projected_edge,
            from: edge.from,
            to: edge.to,
        })
        .collect::<Vec<_>>();
    match (&certificate.sparsify, projected_input.is_empty()) {
        (None, true) => {
            if !certificate.selected_source_edges.is_empty()
                || !certificate.reembedded_edges.is_empty()
            {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
        }
        (Some(sparsify), false) => {
            let result = check_bounded_deterministic_sparsify_certificate(
                snapshot.vertex_count,
                projected_input.len(),
                &projected_input,
                sparsify,
            )
            .map_err(map_static_error)?;
            let mut selected = result
                .selected_edges
                .iter()
                .map(|&projected| certificate.projected_edges[projected].source_edge)
                .collect::<Vec<_>>();
            selected.sort_unstable();
            selected.dedup();
            let image = result
                .selected_edges
                .iter()
                .flat_map(|&projected| {
                    certificate.projected_edges[projected]
                        .projected_to_host
                        .iter()
                        .map(|arc| arc.edge)
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if selected != certificate.selected_source_edges
                || image != certificate.reembedded_edges
            {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
        }
        _ => return Err(BoundedDynamicSpannerError::InvariantViolation),
    }
    for projected in &certificate.projected_edges {
        let row = snapshot
            .edge_rows
            .get(projected.source_edge)
            .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
        if !row.active
            || projected.from == projected.to
            || loop_erase_stable_path(
                snapshot.vertex_count,
                projected.from,
                projected.to,
                &projected.projected_to_host,
                &snapshot.edge_rows,
            )? != projected.projected_to_host
        {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
    }
    Ok(())
}

/// Audits level timing, graph mutations, projection certificates, final paths,
/// and the source low-re-embedding property without invoking the trace runner.
///
/// # Errors
///
/// Returns a typed invariant error when any recorded transition, projection,
/// embedding, counter, or low-re-embedding certificate is inconsistent.
pub fn check_bounded_dynamic_spanner_trace(
    trace: &BoundedDynamicSpannerTrace,
) -> Result<(), BoundedDynamicSpannerError> {
    if trace.updates.len() != trace.snapshots.len()
        || trace.final_snapshot != *trace.snapshots.last().unwrap_or(&trace.initial)
        || trace.initial.total_stage != 0
        || trace.initial.epoch != 0
        || trace.initial.stage_in_epoch != 0
    {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    check_snapshot_invariants(&trace.initial)?;
    let mut before = &trace.initial;
    for (index, (update, after)) in trace.updates.iter().zip(&trace.snapshots).enumerate() {
        check_row_transition(before, after, update)?;
        if after.total_stage != index + 1
            || after.metrics.source_stages != before.metrics.source_stages + 1
        {
            return Err(BoundedDynamicSpannerError::InvariantViolation);
        }
        let expected_restart = before.stage_in_epoch + 1 == before.period;
        if expected_restart {
            if after.epoch != before.epoch + 1
                || after.stage_in_epoch != 0
                || after.last_projection.is_some()
                || after.levels.iter().skip(1).any(|level| {
                    !level.selected_edges.is_empty()
                        || level.embeddings.iter().any(Option::is_some)
                        || !level.touched_vertices.is_empty()
                })
            {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
        } else {
            let expected_level = rebuild_level(
                before.stage_in_epoch + 1,
                before.period,
                before.levels.len(),
            )?;
            let projection = after
                .last_projection
                .as_ref()
                .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
            if projection.level != expected_level
                || after.epoch != before.epoch
                || after.stage_in_epoch != before.stage_in_epoch + 1
            {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
            check_projection_against_previous(after, projection)?;
        }
        check_low_reembedding(before, after)?;
        check_snapshot_invariants(after)?;
        before = after;
    }
    Ok(())
}

fn check_projection_against_previous(
    snapshot: &BoundedDynamicSpannerSnapshot,
    certificate: &BoundedDynamicSpannerProjectionCertificate,
) -> Result<(), BoundedDynamicSpannerError> {
    let level = certificate.level;
    if level == 0
        || level >= snapshot.levels.len()
        || snapshot.levels[level - 1].touched_vertices != certificate.touched_vertices
    {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let touched = certificate
        .touched_vertices
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut affected = Vec::new();
    let mut projected = Vec::new();
    for edge in 0..snapshot.edge_rows.len() {
        let row = snapshot.edge_rows[edge];
        if !row.active {
            continue;
        }
        let old_path = highest_embedding(snapshot, edge, level)?;
        if embedding_graph_vertices(old_path, &snapshot.edge_rows)?.is_disjoint(&touched) {
            continue;
        }
        affected.push(edge);
        let allowed = old_path.iter().map(|arc| arc.edge).collect::<BTreeSet<_>>();
        let from = closest_touched_vertex(row.from, &allowed, &touched, &snapshot.edge_rows)?;
        let to = closest_touched_vertex(row.to, &allowed, &touched, &snapshot.edge_rows)?;
        let source_to_from = shortest_path_on_edges(
            snapshot.vertex_count,
            row.from,
            from,
            &allowed,
            &snapshot.edge_rows,
        )?;
        let to_to_source = shortest_path_on_edges(
            snapshot.vertex_count,
            to,
            row.to,
            &allowed,
            &snapshot.edge_rows,
        )?;
        if from == to {
            continue;
        }
        let mut projected_to_host = reverse_path(&source_to_from);
        projected_to_host.push(DeterministicSpannerArc { edge, direction: 1 });
        projected_to_host.extend(reverse_path(&to_to_source));
        let projected_to_host = loop_erase_stable_path(
            snapshot.vertex_count,
            from,
            to,
            &projected_to_host,
            &snapshot.edge_rows,
        )?;
        projected.push(BoundedDynamicSpannerProjectedEdge {
            projected_edge: projected.len(),
            source_edge: edge,
            from,
            to,
            source_to_from,
            to_to_source,
            projected_to_host,
        });
    }
    if affected != certificate.affected_edges || projected != certificate.projected_edges {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    Ok(())
}

fn check_row_transition(
    before: &BoundedDynamicSpannerSnapshot,
    after: &BoundedDynamicSpannerSnapshot,
    update: &BoundedDynamicSpannerUpdate,
) -> Result<(), BoundedDynamicSpannerError> {
    if before.vertex_count != after.vertex_count
        || before.edge_rows.len() != after.edge_rows.len()
        || before.period != after.period
    {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    let mut expected = before.edge_rows.clone();
    match update {
        BoundedDynamicSpannerUpdate::Delete { edge } => {
            let row = expected
                .get_mut(*edge)
                .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
            if !row.active {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
            row.active = false;
        }
        BoundedDynamicSpannerUpdate::VertexSplit {
            retained_vertex,
            new_vertex,
            moved,
        } => {
            for &(edge, endpoint) in moved {
                let row = expected
                    .get_mut(edge)
                    .ok_or(BoundedDynamicSpannerError::InvariantViolation)?;
                match endpoint {
                    BoundedDynamicSpannerEndpoint::Tail if row.from == *retained_vertex => {
                        row.from = *new_vertex;
                    }
                    BoundedDynamicSpannerEndpoint::Head if row.to == *retained_vertex => {
                        row.to = *new_vertex;
                    }
                    _ => return Err(BoundedDynamicSpannerError::InvariantViolation),
                }
            }
        }
    }
    if expected != after.edge_rows {
        return Err(BoundedDynamicSpannerError::InvariantViolation);
    }
    Ok(())
}

fn check_low_reembedding(
    before: &BoundedDynamicSpannerSnapshot,
    after: &BoundedDynamicSpannerSnapshot,
) -> Result<(), BoundedDynamicSpannerError> {
    let reported = after
        .last_reembedded
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for edge in 0..after.edge_rows.len() {
        if !after.edge_rows[edge].active {
            continue;
        }
        let old = before.graph_to_spanner[edge]
            .iter()
            .map(|arc| arc.edge)
            .collect::<BTreeSet<_>>();
        for new_edge in after.graph_to_spanner[edge]
            .iter()
            .map(|arc| arc.edge)
            .collect::<BTreeSet<_>>()
            .difference(&old)
        {
            if !reported.contains(new_edge) {
                return Err(BoundedDynamicSpannerError::InvariantViolation);
            }
        }
    }
    Ok(())
}

fn is_strictly_sorted(values: &[usize]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(from: usize, to: usize) -> BoundedDynamicSpannerEdgeState {
        BoundedDynamicSpannerEdgeState {
            from,
            to,
            active: true,
        }
    }

    #[test]
    fn source_level_schedule_projects_touched_paths_and_certifies_low_reembedding() {
        let rows = vec![
            row(0, 1),
            row(1, 2),
            row(2, 3),
            row(3, 0),
            row(0, 2),
            row(1, 3),
        ];
        let updates = vec![
            BoundedDynamicSpannerUpdate::Delete { edge: 4 },
            BoundedDynamicSpannerUpdate::VertexSplit {
                retained_vertex: 1,
                new_vertex: 4,
                moved: vec![(0, BoundedDynamicSpannerEndpoint::Head)],
            },
            BoundedDynamicSpannerUpdate::Delete { edge: 5 },
        ];
        let trace = trace_bounded_dynamic_spanner(8, rows, updates).expect("dynamic spanner");
        assert_eq!(
            trace.snapshots[0].last_projection.as_ref().unwrap().level,
            3
        );
        assert_eq!(
            trace.snapshots[1].last_projection.as_ref().unwrap().level,
            2
        );
        assert_eq!(
            trace.snapshots[2].last_projection.as_ref().unwrap().level,
            3
        );
        assert!(trace.snapshots.iter().all(|snapshot| {
            snapshot
                .edge_rows
                .iter()
                .enumerate()
                .all(|(edge, row)| !row.active || !snapshot.graph_to_spanner[edge].is_empty())
        }));
        check_bounded_dynamic_spanner_trace(&trace).expect("audit");

        let mut forged = trace.clone();
        forged.snapshots[1].last_reembedded.clear();
        forged.final_snapshot = forged.snapshots.last().unwrap().clone();
        assert_eq!(
            check_bounded_dynamic_spanner_trace(&forged),
            Err(BoundedDynamicSpannerError::InvariantViolation)
        );
    }

    #[test]
    fn period_boundary_restarts_level_zero_and_clears_the_suffix() {
        let rows = vec![
            row(0, 1),
            row(1, 2),
            row(2, 3),
            row(3, 4),
            row(4, 5),
            row(5, 6),
            row(6, 7),
            row(7, 0),
            row(0, 2),
            row(2, 4),
            row(4, 6),
            row(6, 0),
        ];
        let updates = (0..8)
            .map(|edge| BoundedDynamicSpannerUpdate::Delete { edge: edge + 4 })
            .collect::<Vec<_>>();
        let trace = trace_bounded_dynamic_spanner(8, rows, updates).expect("restart");
        let final_snapshot = &trace.final_snapshot;
        assert_eq!(final_snapshot.epoch, 1);
        assert_eq!(final_snapshot.stage_in_epoch, 0);
        assert_eq!(final_snapshot.metrics.restarts, 1);
        assert!(final_snapshot.last_projection.is_none());
        assert!(
            final_snapshot
                .levels
                .iter()
                .skip(1)
                .all(|level| level.selected_edges.is_empty())
        );
        check_bounded_dynamic_spanner_trace(&trace).expect("audit");
    }
}
