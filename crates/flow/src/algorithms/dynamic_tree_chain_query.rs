//! Stable-slot `FindCycle` for a continued dynamic tree-chain epoch.
//!
//! The fixed-topology query uses dense edge positions. Dynamic levels instead
//! retain stable edge slots with holes, so this module evaluates the same two
//! source candidate classes directly on checked epoch materializations:
//! omitted core edges plus the reverse of their sparse embedding, and
//! fundamental cycles of terminal spanning trees. Every candidate is lifted
//! through the selected dynamic forests and scored on the current root graph.
//!
//! Connected terminal graphs with at least two vertices use the certified
//! exhaustive MWU tree collection. A one-vertex graph has the empty forest;
//! a disconnected terminal graph uses the canonical stable-edge spanning
//! forest of each component. These explicit base cases keep component-local
//! cycles executable without pretending that the MWU primitive accepts a
//! degenerate or disconnected graph.

use std::collections::{BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES, DYNAMIC_SPARSE_CORE_MAX_EDGES,
    DYNAMIC_SPARSE_CORE_MAX_NODES, DynamicLevelGraphSnapshot, DynamicSparseCoreSnapshot,
    DynamicTreeChainEpochRuntimeError, DynamicTreeChainEpochRuntimeMaterialization,
    DynamicTreeChainEpochRuntimeState, LowStretchForestMwuError, LowStretchForestMwuTraceResult,
    ShiftedTreeChainEdge, ShiftedTreeChainGraph,
    check_dynamic_tree_chain_epoch_runtime_materialization, check_low_stretch_forest_mwu_trace,
    materialize_dynamic_tree_chain_epoch_runtime, trace_low_stretch_forest_mwu_collection,
};

/// Maximum reversible work-item boundaries including completion.
pub const DYNAMIC_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS: usize = 256;
/// Maximum bit width of one exact lifted coefficient or score scalar.
pub const DYNAMIC_TREE_CHAIN_QUERY_MAX_SCALAR_BITS: u64 = 512;

const CATALOG_ID: &str = "dynamic-tree-chain-find-cycle";

/// One terminal spanning forest, encoded by stable edge slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainTerminalBranch {
    /// Strictly increasing active slots forming one tree per graph component.
    pub tree_edges: Vec<usize>,
}

/// Checked terminal graph and its deterministic tree collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainTerminalResult {
    /// Exact current graph emitted by the last dynamic level.
    pub graph: DynamicLevelGraphSnapshot,
    /// Branches in source MWU round order, or repeated empty trees at one vertex.
    pub branches: Vec<DynamicTreeChainTerminalBranch>,
}

/// Source certificate for terminal-tree initialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainTerminalTraceResult {
    /// Certified MWU transcript; absent for explicit degenerate/disconnected forests.
    pub mwu_trace: Option<LowStretchForestMwuTraceResult>,
    /// Exact converted terminal collection.
    pub result: DynamicTreeChainTerminalResult,
}

/// Stable identity of one source candidate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DynamicTreeChainCycleSource {
    /// Omitted core edge plus the reverse of its maintained embedding.
    FundamentalSpanner {
        /// Root-based dynamic level.
        level: usize,
        /// Stable core edge slot.
        core_edge: usize,
    },
    /// Terminal off-tree edge plus its tree path.
    TerminalTree {
        /// Terminal tree branch.
        branch: usize,
        /// Stable terminal edge slot.
        edge: usize,
    },
}

/// One exact circulation lifted to the current root stable-slot universe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCycleCandidate {
    /// Candidate origin before recursive lifting.
    pub source: DynamicTreeChainCycleSource,
    /// Integral coefficients indexed by stable root edge slot; holes are zero.
    pub coefficients: Vec<BigInt>,
    /// Exact non-positive root inner product.
    pub gradient: BigRational,
    /// Exact positive root weighted one-norm.
    pub weighted_length: BigRational,
    /// Exact absolute quality ratio.
    pub ratio: BigRational,
}

/// Exact bounded query work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainCycleQueryMetrics {
    /// Active intermediate core slots inspected.
    pub intermediate_edge_inspections: u64,
    /// Intermediate edges already retained by the sparse core.
    pub spanner_edges_skipped: u64,
    /// Sparse embedding arcs traversed.
    pub spanner_embedding_arcs: u64,
    /// Terminal tree/active-edge pairs inspected.
    pub terminal_edge_inspections: u64,
    /// Terminal pairs skipped because the edge belongs to the tree.
    pub tree_edges_skipped: u64,
    /// Candidate lifts that cancel to zero.
    pub zero_lifts_skipped: u64,
    /// Nonzero root circulations scored.
    pub candidates_evaluated: u64,
    /// Forest/tree path arcs traversed while lifting.
    pub path_arcs: u64,
    /// Public reversible boundaries.
    pub state_transitions: u64,
}

/// Query state at one work-item boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeChainCycleQuerySnapshot {
    /// Next stable work-item index.
    pub next_work_item: usize,
    /// Best stable candidate seen so far.
    pub best_candidate: Option<DynamicTreeChainCycleCandidate>,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: DynamicTreeChainCycleQueryMetrics,
}

/// Meaning of one query boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicTreeChainCycleQueryEventKind {
    /// An intermediate core edge is already a spanner edge.
    SpannerEdgeSkipped {
        /// Root-based dynamic level.
        level: usize,
        /// Stable retained core edge slot.
        core_edge: usize,
    },
    /// A terminal edge belongs to the selected tree.
    TreeEdgeSkipped {
        /// Terminal tree branch.
        branch: usize,
        /// Stable terminal edge slot.
        edge: usize,
    },
    /// Recursive lifting cancelled the source circulation.
    ZeroLiftSkipped {
        /// Candidate origin before cancellation.
        source: DynamicTreeChainCycleSource,
    },
    /// A nonzero root circulation was scored.
    CandidateEvaluated {
        /// Exact root circulation and score.
        candidate: Box<DynamicTreeChainCycleCandidate>,
        /// Whether the candidate strictly improved the stable incumbent.
        became_best: bool,
    },
    /// Every stable work item completed.
    Completed,
}

/// One reversible query boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCycleQueryTraceEvent {
    /// Stable primitive identity.
    pub catalog_id: &'static str,
    /// Source meaning of this boundary.
    pub kind: DynamicTreeChainCycleQueryEventKind,
    /// State before processing the item.
    pub before: DynamicTreeChainCycleQuerySnapshot,
    /// State after processing the item.
    pub after: DynamicTreeChainCycleQuerySnapshot,
}

/// Exact fast query result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCycleQueryResult {
    /// Best current candidate, or `None` if the graph contains no cycle.
    pub best_candidate: Option<DynamicTreeChainCycleCandidate>,
    /// Terminal query state.
    pub final_snapshot: DynamicTreeChainCycleQuerySnapshot,
}

/// Complete checked epoch, terminal initializer, and query transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeChainCycleQueryTraceResult {
    /// Independently checkable current runtime materialization.
    pub materialization: DynamicTreeChainEpochRuntimeMaterialization,
    /// Independently checkable terminal tree collection.
    pub terminal_trace: DynamicTreeChainTerminalTraceResult,
    /// Initial empty query state.
    pub base_snapshot: DynamicTreeChainCycleQuerySnapshot,
    /// One event per work item followed by completion.
    pub events: Vec<DynamicTreeChainCycleQueryTraceEvent>,
    /// Exact query result.
    pub result: DynamicTreeChainCycleQueryResult,
}

/// Explicit bounded dynamic-query failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeChainCycleQueryError {
    /// Runtime shape, terminal branch count, or stable graph is malformed.
    #[error("dynamic tree-chain query input is invalid")]
    InvalidInput,
    /// The request exceeds the explicit small-instance band.
    #[error("dynamic tree-chain query exceeds its admission band")]
    AdmissionLimit,
    /// Current epoch materialization failed.
    #[error("dynamic tree-chain query runtime failed: {0}")]
    Runtime(#[from] DynamicTreeChainEpochRuntimeError),
    /// Terminal MWU initialization failed.
    #[error("dynamic tree-chain query terminal MWU failed: {0}")]
    Mwu(#[from] LowStretchForestMwuError),
    /// A core circulation, embedding, forest lift, or root score is invalid.
    #[error("dynamic tree-chain query invariant failed")]
    InvariantViolation,
    /// Exact work arithmetic overflowed.
    #[error("dynamic tree-chain query arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript differs from independent stable replay.
    #[error("dynamic tree-chain query trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Copy)]
enum WorkItem {
    Intermediate { level: usize, core_edge: usize },
    Terminal { branch: usize, edge: usize },
}

#[derive(Clone, Copy)]
struct PathArc {
    edge: usize,
    direction: i8,
}

struct CandidateBuild {
    candidate: Option<DynamicTreeChainCycleCandidate>,
    embedding_arcs: u64,
    path_arcs: u64,
}

/// Initializes terminal spanning trees for a checked stable graph.
///
/// # Errors
///
/// Rejects malformed/out-of-band graphs, disconnected nontrivial graphs, or
/// certified MWU failure.
pub fn trace_dynamic_tree_chain_terminal_collection(
    graph: &DynamicLevelGraphSnapshot,
    branches: usize,
) -> Result<DynamicTreeChainTerminalTraceResult, DynamicTreeChainCycleQueryError> {
    validate_terminal_graph(graph, branches)?;
    let connected = is_connected(graph);
    let trace = if graph.active_node_count == 1 || !connected {
        let tree_edges = canonical_spanning_forest(graph)?;
        DynamicTreeChainTerminalTraceResult {
            mwu_trace: None,
            result: DynamicTreeChainTerminalResult {
                graph: graph.clone(),
                branches: vec![DynamicTreeChainTerminalBranch { tree_edges }; branches],
            },
        }
    } else {
        let shifted = shifted_graph(graph);
        let mwu_trace = trace_low_stretch_forest_mwu_collection(
            &shifted,
            super::LowStretchForestMwuConfig { rounds: branches },
        )?;
        let converted = convert_terminal_branches(&shifted, &mwu_trace)?;
        DynamicTreeChainTerminalTraceResult {
            mwu_trace: Some(mwu_trace),
            result: DynamicTreeChainTerminalResult {
                graph: graph.clone(),
                branches: converted,
            },
        }
    };
    check_dynamic_tree_chain_terminal_collection_trace(graph, branches, &trace)?;
    Ok(trace)
}

/// Independently checks the terminal base case or certified MWU conversion.
///
/// # Errors
///
/// Rejects graph, MWU transcript, stable tree mapping, branch order, or result
/// drift.
pub fn check_dynamic_tree_chain_terminal_collection_trace(
    graph: &DynamicLevelGraphSnapshot,
    branches: usize,
    trace: &DynamicTreeChainTerminalTraceResult,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    validate_terminal_graph(graph, branches).map_err(audit_error)?;
    if trace.result.graph != *graph || trace.result.branches.len() != branches {
        return Err(DynamicTreeChainCycleQueryError::TraceVerification);
    }
    let connected = is_connected(graph);
    if graph.active_node_count == 1 || !connected {
        let expected = canonical_spanning_forest(graph)
            .map_err(|_| DynamicTreeChainCycleQueryError::TraceVerification)?;
        if trace.mwu_trace.is_some()
            || trace
                .result
                .branches
                .iter()
                .any(|branch| branch.tree_edges != expected)
        {
            return Err(DynamicTreeChainCycleQueryError::TraceVerification);
        }
        return Ok(());
    }
    let shifted = shifted_graph(graph);
    let mwu_trace = trace
        .mwu_trace
        .as_ref()
        .ok_or(DynamicTreeChainCycleQueryError::TraceVerification)?;
    let config = super::LowStretchForestMwuConfig { rounds: branches };
    check_low_stretch_forest_mwu_trace(&shifted, config, mwu_trace)?;
    let expected = audit_convert_terminal_branches(&shifted, mwu_trace)?;
    if trace.result.branches != expected {
        return Err(DynamicTreeChainCycleQueryError::TraceVerification);
    }
    Ok(())
}

/// Finds the best current cycle without retaining query events.
///
/// # Errors
///
/// Rejects invalid epoch state, terminal initialization, lifting invariants,
/// admission, or exact arithmetic overflow.
pub fn find_dynamic_tree_chain_cycle(
    state: &DynamicTreeChainEpochRuntimeState,
    terminal_branches: usize,
) -> Result<DynamicTreeChainCycleQueryResult, DynamicTreeChainCycleQueryError> {
    let materialization = materialize_dynamic_tree_chain_epoch_runtime(state)?;
    let terminal_graph = terminal_graph(&materialization)?.clone();
    let terminal =
        trace_dynamic_tree_chain_terminal_collection(&terminal_graph, terminal_branches)?;
    run_query(state, &materialization, &terminal.result, false).map(|(_, _, result)| result)
}

/// Records the checked epoch, terminal initializer, and every query work item.
///
/// # Errors
///
/// Returns any runtime, terminal, query, or independent replay failure.
pub fn trace_dynamic_tree_chain_cycle_query(
    state: &DynamicTreeChainEpochRuntimeState,
    terminal_branches: usize,
) -> Result<DynamicTreeChainCycleQueryTraceResult, DynamicTreeChainCycleQueryError> {
    let materialization = materialize_dynamic_tree_chain_epoch_runtime(state)?;
    let terminal_graph = terminal_graph(&materialization)?.clone();
    let terminal_trace =
        trace_dynamic_tree_chain_terminal_collection(&terminal_graph, terminal_branches)?;
    let (base_snapshot, events, result) =
        run_query(state, &materialization, &terminal_trace.result, true)?;
    let trace = DynamicTreeChainCycleQueryTraceResult {
        materialization,
        terminal_trace,
        base_snapshot,
        events,
        result,
    };
    check_dynamic_tree_chain_cycle_query_trace(state, terminal_branches, &trace)?;
    Ok(trace)
}

/// Independently checks materialization, terminal trees, work order and state.
///
/// This checker never calls the query runner. It validates each supplied
/// component transcript, reconstructs every stable work item and its exact
/// state transition, then compares the final candidate and metrics.
///
/// # Errors
///
/// Rejects component, candidate, coefficient, event, metric, ordering, or
/// completion drift.
pub fn check_dynamic_tree_chain_cycle_query_trace(
    state: &DynamicTreeChainEpochRuntimeState,
    terminal_branches: usize,
    trace: &DynamicTreeChainCycleQueryTraceResult,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    check_dynamic_tree_chain_epoch_runtime_materialization(state, &trace.materialization)?;
    let terminal_graph = terminal_graph(&trace.materialization)?;
    check_dynamic_tree_chain_terminal_collection_trace(
        terminal_graph,
        terminal_branches,
        &trace.terminal_trace,
    )?;
    if trace.terminal_trace.result.graph != *terminal_graph
        || trace.base_snapshot != DynamicTreeChainCycleQuerySnapshot::default()
    {
        return Err(DynamicTreeChainCycleQueryError::TraceVerification);
    }
    let work = work_items(&trace.materialization, &trace.terminal_trace.result)?;
    if trace.events.len() != work.len() + 1 {
        return Err(DynamicTreeChainCycleQueryError::TraceVerification);
    }
    let mut cursor = DynamicTreeChainCycleQuerySnapshot::default();
    for (index, item) in work.into_iter().enumerate() {
        let event = &trace.events[index];
        if event.catalog_id != CATALOG_ID || event.before != cursor {
            return Err(DynamicTreeChainCycleQueryError::TraceVerification);
        }
        let kind = process_item(
            state,
            &trace.materialization,
            &trace.terminal_trace.result,
            item,
            &mut cursor,
            true,
        )?;
        cursor.next_work_item = cursor
            .next_work_item
            .checked_add(1)
            .ok_or(DynamicTreeChainCycleQueryError::TraceVerification)?;
        cursor.metrics.state_transitions = audit_increment(cursor.metrics.state_transitions)?;
        if event.kind != kind || event.after != cursor {
            return Err(DynamicTreeChainCycleQueryError::TraceVerification);
        }
    }
    let completion = trace
        .events
        .last()
        .ok_or(DynamicTreeChainCycleQueryError::TraceVerification)?;
    let before = cursor.clone();
    cursor.complete = true;
    cursor.metrics.state_transitions = audit_increment(cursor.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != DynamicTreeChainCycleQueryEventKind::Completed
        || completion.before != before
        || completion.after != cursor
        || trace.result.best_candidate != cursor.best_candidate
        || trace.result.final_snapshot != cursor
    {
        return Err(DynamicTreeChainCycleQueryError::TraceVerification);
    }
    Ok(())
}

fn run_query(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    terminal: &DynamicTreeChainTerminalResult,
    record: bool,
) -> Result<
    (
        DynamicTreeChainCycleQuerySnapshot,
        Vec<DynamicTreeChainCycleQueryTraceEvent>,
        DynamicTreeChainCycleQueryResult,
    ),
    DynamicTreeChainCycleQueryError,
> {
    let work = work_items(materialization, terminal)?;
    if work.len() + 1 > DYNAMIC_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS {
        return Err(DynamicTreeChainCycleQueryError::AdmissionLimit);
    }
    let mut snapshot = DynamicTreeChainCycleQuerySnapshot::default();
    let base = snapshot.clone();
    let mut events = Vec::with_capacity(if record { work.len() + 1 } else { 0 });
    for item in work {
        let before = snapshot.clone();
        let kind = process_item(state, materialization, terminal, item, &mut snapshot, false)?;
        snapshot.next_work_item = snapshot
            .next_work_item
            .checked_add(1)
            .ok_or(DynamicTreeChainCycleQueryError::ArithmeticOverflow)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        if record {
            events.push(DynamicTreeChainCycleQueryTraceEvent {
                catalog_id: CATALOG_ID,
                kind,
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicTreeChainCycleQueryTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicTreeChainCycleQueryEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    let result = DynamicTreeChainCycleQueryResult {
        best_candidate: snapshot.best_candidate.clone(),
        final_snapshot: snapshot,
    };
    Ok((base, events, result))
}

fn process_item(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    terminal: &DynamicTreeChainTerminalResult,
    item: WorkItem,
    snapshot: &mut DynamicTreeChainCycleQuerySnapshot,
    audit: bool,
) -> Result<DynamicTreeChainCycleQueryEventKind, DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    let (source, build) = match item {
        WorkItem::Intermediate { level, core_edge } => {
            snapshot.metrics.intermediate_edge_inspections =
                add_one(snapshot.metrics.intermediate_edge_inspections, audit)?;
            let sparse = selected_sparse(materialization, level).ok_or_else(fail)?;
            if sparse
                .spanner_edge_slots
                .get(core_edge)
                .and_then(Option::as_ref)
                .is_some()
            {
                snapshot.metrics.spanner_edges_skipped =
                    add_one(snapshot.metrics.spanner_edges_skipped, audit)?;
                return Ok(DynamicTreeChainCycleQueryEventKind::SpannerEdgeSkipped {
                    level,
                    core_edge,
                });
            }
            let source = DynamicTreeChainCycleSource::FundamentalSpanner { level, core_edge };
            let build = build_intermediate_candidate(
                state,
                materialization,
                level,
                core_edge,
                source,
                audit,
            )?;
            (source, build)
        }
        WorkItem::Terminal { branch, edge } => {
            snapshot.metrics.terminal_edge_inspections =
                add_one(snapshot.metrics.terminal_edge_inspections, audit)?;
            let tree = terminal.branches.get(branch).ok_or_else(fail)?;
            if tree.tree_edges.binary_search(&edge).is_ok() {
                snapshot.metrics.tree_edges_skipped =
                    add_one(snapshot.metrics.tree_edges_skipped, audit)?;
                return Ok(DynamicTreeChainCycleQueryEventKind::TreeEdgeSkipped { branch, edge });
            }
            let source = DynamicTreeChainCycleSource::TerminalTree { branch, edge };
            let build = build_terminal_candidate(
                state,
                materialization,
                terminal,
                branch,
                edge,
                source,
                audit,
            )?;
            (source, build)
        }
    };
    snapshot.metrics.spanner_embedding_arcs = add_metric(
        snapshot.metrics.spanner_embedding_arcs,
        build.embedding_arcs,
        audit,
    )?;
    snapshot.metrics.path_arcs = add_metric(snapshot.metrics.path_arcs, build.path_arcs, audit)?;
    let Some(candidate) = build.candidate else {
        snapshot.metrics.zero_lifts_skipped = add_one(snapshot.metrics.zero_lifts_skipped, audit)?;
        return Ok(DynamicTreeChainCycleQueryEventKind::ZeroLiftSkipped { source });
    };
    snapshot.metrics.candidates_evaluated = add_one(snapshot.metrics.candidates_evaluated, audit)?;
    let became_best = snapshot
        .best_candidate
        .as_ref()
        .is_none_or(|current| candidate.ratio > current.ratio);
    if became_best {
        snapshot.best_candidate = Some(candidate.clone());
    }
    Ok(DynamicTreeChainCycleQueryEventKind::CandidateEvaluated {
        candidate: Box::new(candidate),
        became_best,
    })
}

fn work_items(
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    terminal: &DynamicTreeChainTerminalResult,
) -> Result<Vec<WorkItem>, DynamicTreeChainCycleQueryError> {
    if materialization.epoch_snapshot.levels.is_empty() {
        return Err(DynamicTreeChainCycleQueryError::InvalidInput);
    }
    let mut work = Vec::new();
    for level in 0..materialization.epoch_snapshot.levels.len() {
        let sparse = selected_sparse(materialization, level)
            .ok_or(DynamicTreeChainCycleQueryError::InvalidInput)?;
        work.extend(
            sparse
                .core_edge_slots
                .iter()
                .enumerate()
                .filter_map(|(core_edge, row)| {
                    row.as_ref()
                        .map(|_| WorkItem::Intermediate { level, core_edge })
                }),
        );
    }
    for branch in 0..terminal.branches.len() {
        work.extend(
            terminal
                .graph
                .edge_slots
                .iter()
                .enumerate()
                .filter_map(|(edge, row)| {
                    row.as_ref().map(|_| WorkItem::Terminal { branch, edge })
                }),
        );
    }
    if work.len() + 1 > DYNAMIC_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS {
        return Err(DynamicTreeChainCycleQueryError::AdmissionLimit);
    }
    Ok(work)
}

fn build_intermediate_candidate(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    level: usize,
    core_edge: usize,
    source: DynamicTreeChainCycleSource,
    audit: bool,
) -> Result<CandidateBuild, DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    let sparse = selected_sparse(materialization, level).ok_or_else(fail)?;
    let slots = sparse.core_edge_slots.len();
    if sparse
        .core_edge_slots
        .get(core_edge)
        .and_then(Option::as_ref)
        .is_none()
    {
        return Err(fail());
    }
    let mut coefficients = vec![BigInt::zero(); slots];
    coefficients[core_edge] += 1;
    let embedding = sparse.core_to_spanner.get(core_edge).ok_or_else(fail)?;
    for arc in embedding {
        if sparse
            .spanner_edge_slots
            .get(arc.edge)
            .and_then(Option::as_ref)
            .is_none()
            || !matches!(arc.direction, -1 | 1)
        {
            return Err(fail());
        }
        coefficients[arc.edge] -= BigInt::from(arc.direction);
    }
    ensure_core_circulation(sparse, &coefficients, audit)?;
    let (coefficients, path_arcs) =
        lift_to_root(state, materialization, level, coefficients, audit)?;
    let candidate = score_root(materialization, source, coefficients, audit)?;
    Ok(CandidateBuild {
        candidate,
        embedding_arcs: u64::try_from(embedding.len()).map_err(|_| fail())?,
        path_arcs,
    })
}

fn build_terminal_candidate(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    terminal: &DynamicTreeChainTerminalResult,
    branch: usize,
    edge: usize,
    source: DynamicTreeChainCycleSource,
    audit: bool,
) -> Result<CandidateBuild, DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    let row = terminal
        .graph
        .edge_slots
        .get(edge)
        .and_then(Option::as_ref)
        .ok_or_else(fail)?;
    let tree = &terminal.branches.get(branch).ok_or_else(fail)?.tree_edges;
    let path = graph_path(&terminal.graph, tree, row.to, row.from, audit)?.ok_or_else(fail)?;
    let mut coefficients = vec![BigInt::zero(); terminal.graph.edge_slots.len()];
    coefficients[edge] += 1;
    for arc in &path {
        coefficients[arc.edge] += BigInt::from(arc.direction);
    }
    ensure_graph_circulation(&terminal.graph, &coefficients, audit)?;
    let last = state.levels.len().checked_sub(1).ok_or_else(fail)?;
    let initial_arcs = u64::try_from(path.len()).map_err(|_| fail())?;
    let (coefficients, lift_arcs) =
        lift_to_root(state, materialization, last, coefficients, audit)?;
    let candidate = score_root(materialization, source, coefficients, audit)?;
    Ok(CandidateBuild {
        candidate,
        embedding_arcs: 0,
        path_arcs: initial_arcs.checked_add(lift_arcs).ok_or_else(fail)?,
    })
}

fn lift_to_root(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    start_level: usize,
    mut coefficients: Vec<BigInt>,
    audit: bool,
) -> Result<(Vec<BigInt>, u64), DynamicTreeChainCycleQueryError> {
    let mut path_arcs = 0_u64;
    for level in (0..=start_level).rev() {
        let (lifted, traversed) =
            lift_one_level(state, materialization, level, &coefficients, audit)?;
        coefficients = lifted;
        path_arcs = path_arcs
            .checked_add(traversed)
            .ok_or_else(|| failure(audit))?;
    }
    Ok((coefficients, path_arcs))
}

fn lift_one_level(
    state: &DynamicTreeChainEpochRuntimeState,
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    level: usize,
    child_coefficients: &[BigInt],
    audit: bool,
) -> Result<(Vec<BigInt>, u64), DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    let source_graph = &materialization
        .epoch_snapshot
        .levels
        .get(level)
        .ok_or_else(fail)?
        .source_graph;
    if child_coefficients.len() != source_graph.edge_slots.len() {
        return Err(fail());
    }
    let active = state
        .levels
        .get(level)
        .ok_or_else(fail)?
        .input
        .active_branch;
    let branch_trace = materialization
        .level_traces
        .get(level)
        .and_then(|trace| trace.collection_trace.branch_traces.get(active))
        .ok_or_else(fail)?;
    let forest = &branch_trace.core_trace.forest_trace.result.final_snapshot;
    if forest.edge_slots.len() != source_graph.edge_slots.len()
        || forest.component_roots.len() != source_graph.active_node_count
    {
        return Err(fail());
    }
    let mut lifted = vec![BigInt::zero(); source_graph.edge_slots.len()];
    let mut traversed = 0_u64;
    for (edge, coefficient) in child_coefficients.iter().enumerate() {
        if coefficient.is_zero() {
            continue;
        }
        let row = source_graph
            .edge_slots
            .get(edge)
            .and_then(Option::as_ref)
            .ok_or_else(fail)?;
        lifted[edge] += coefficient;
        let root_from = *forest.component_roots.get(row.from).ok_or_else(fail)?;
        let root_to = *forest.component_roots.get(row.to).ok_or_else(fail)?;
        let prefix = graph_path(
            source_graph,
            &forest.forest_edges,
            root_from,
            row.from,
            audit,
        )?
        .ok_or_else(fail)?;
        let suffix = graph_path(source_graph, &forest.forest_edges, row.to, root_to, audit)?
            .ok_or_else(fail)?;
        traversed = traversed
            .checked_add(u64::try_from(prefix.len() + suffix.len()).map_err(|_| fail())?)
            .ok_or_else(fail)?;
        for arc in prefix.into_iter().chain(suffix) {
            lifted[arc.edge] += coefficient * BigInt::from(arc.direction);
        }
    }
    ensure_coefficient_width(&lifted, audit)?;
    ensure_graph_circulation(source_graph, &lifted, audit)?;
    Ok((lifted, traversed))
}

fn score_root(
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    source: DynamicTreeChainCycleSource,
    mut coefficients: Vec<BigInt>,
    audit: bool,
) -> Result<Option<DynamicTreeChainCycleCandidate>, DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    let root = &materialization
        .epoch_snapshot
        .levels
        .first()
        .ok_or_else(fail)?
        .source_graph;
    if coefficients.len() != root.edge_slots.len() {
        return Err(fail());
    }
    ensure_graph_circulation(root, &coefficients, audit)?;
    if coefficients.iter().all(BigInt::is_zero) {
        return Ok(None);
    }
    let mut gradient = BigRational::zero();
    for (slot, coefficient) in root.edge_slots.iter().zip(&coefficients) {
        match slot {
            Some(edge) => gradient += &edge.gradient * coefficient,
            None if !coefficient.is_zero() => return Err(fail()),
            None => {}
        }
    }
    if gradient.is_positive() {
        for coefficient in &mut coefficients {
            *coefficient = -coefficient.clone();
        }
        gradient = -gradient;
    }
    let weighted_length = root
        .edge_slots
        .iter()
        .zip(&coefficients)
        .filter_map(|(edge, coefficient)| {
            edge.as_ref().map(|edge| &edge.length * coefficient.abs())
        })
        .fold(BigRational::zero(), |sum, value| sum + value);
    if weighted_length <= BigRational::zero()
        || rational_too_wide(&gradient)
        || rational_too_wide(&weighted_length)
    {
        return Err(fail());
    }
    let ratio = gradient.abs() / &weighted_length;
    if rational_too_wide(&ratio) {
        return Err(fail());
    }
    Ok(Some(DynamicTreeChainCycleCandidate {
        source,
        coefficients,
        gradient,
        weighted_length,
        ratio,
    }))
}

fn graph_path(
    graph: &DynamicLevelGraphSnapshot,
    allowed_edges: &[usize],
    start: usize,
    target: usize,
    audit: bool,
) -> Result<Option<Vec<PathArc>>, DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    if start >= graph.active_node_count || target >= graph.active_node_count {
        return Err(fail());
    }
    if start == target {
        return Ok(Some(Vec::new()));
    }
    let allowed = allowed_edges.iter().copied().collect::<BTreeSet<_>>();
    if allowed.len() != allowed_edges.len() {
        return Err(fail());
    }
    let mut seen = vec![false; graph.active_node_count];
    let mut previous = vec![None; graph.active_node_count];
    let mut queue = VecDeque::new();
    seen[start] = true;
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        for &edge_id in &allowed {
            let edge = graph
                .edge_slots
                .get(edge_id)
                .and_then(Option::as_ref)
                .ok_or_else(fail)?;
            if edge.from == edge.to {
                continue;
            }
            let next = if edge.from == node {
                Some((edge.to, 1_i8))
            } else if edge.to == node {
                Some((edge.from, -1_i8))
            } else {
                None
            };
            if let Some((next, direction)) = next
                && !seen[next]
            {
                seen[next] = true;
                previous[next] = Some((node, edge_id, direction));
                if next == target {
                    break;
                }
                queue.push_back(next);
            }
        }
    }
    if !seen[target] {
        return Ok(None);
    }
    let mut reversed = Vec::new();
    let mut node = target;
    while node != start {
        let (parent, edge, direction) = previous[node].ok_or_else(fail)?;
        reversed.push(PathArc { edge, direction });
        node = parent;
    }
    reversed.reverse();
    Ok(Some(reversed))
}

fn ensure_core_circulation(
    sparse: &DynamicSparseCoreSnapshot,
    coefficients: &[BigInt],
    audit: bool,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    if coefficients.len() != sparse.core_edge_slots.len() {
        return Err(fail());
    }
    let maximum_vertex = sparse.core_vertices.iter().copied().max().unwrap_or(0);
    let mut divergence = vec![BigInt::zero(); maximum_vertex + 1];
    for (slot, coefficient) in sparse.core_edge_slots.iter().zip(coefficients) {
        match slot {
            Some(edge) => {
                divergence[edge.from] += coefficient;
                divergence[edge.to] -= coefficient;
            }
            None if !coefficient.is_zero() => return Err(fail()),
            None => {}
        }
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(fail());
    }
    ensure_coefficient_width(coefficients, audit)
}

fn ensure_graph_circulation(
    graph: &DynamicLevelGraphSnapshot,
    coefficients: &[BigInt],
    audit: bool,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    let fail = || failure(audit);
    if coefficients.len() != graph.edge_slots.len() {
        return Err(fail());
    }
    let mut divergence = vec![BigInt::zero(); graph.active_node_count];
    for (slot, coefficient) in graph.edge_slots.iter().zip(coefficients) {
        match slot {
            Some(edge) => {
                divergence[edge.from] += coefficient;
                divergence[edge.to] -= coefficient;
            }
            None if !coefficient.is_zero() => return Err(fail()),
            None => {}
        }
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(fail());
    }
    ensure_coefficient_width(coefficients, audit)
}

fn ensure_coefficient_width(
    coefficients: &[BigInt],
    audit: bool,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    if coefficients
        .iter()
        .any(|value| value.bits() > DYNAMIC_TREE_CHAIN_QUERY_MAX_SCALAR_BITS)
    {
        return Err(failure(audit));
    }
    Ok(())
}

fn terminal_graph(
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
) -> Result<&DynamicLevelGraphSnapshot, DynamicTreeChainCycleQueryError> {
    materialization
        .level_traces
        .last()
        .map(|trace| &trace.final_projection.graph)
        .ok_or(DynamicTreeChainCycleQueryError::InvalidInput)
}

fn selected_sparse(
    materialization: &DynamicTreeChainEpochRuntimeMaterialization,
    level: usize,
) -> Option<&DynamicSparseCoreSnapshot> {
    let epoch_level = materialization.epoch_snapshot.levels.get(level)?;
    epoch_level.branch_snapshots.get(epoch_level.active_branch)
}

fn validate_terminal_graph(
    graph: &DynamicLevelGraphSnapshot,
    branches: usize,
) -> Result<(), DynamicTreeChainCycleQueryError> {
    if graph.active_node_count == 0 || branches == 0 {
        return Err(DynamicTreeChainCycleQueryError::InvalidInput);
    }
    if graph.active_node_count > DYNAMIC_SPARSE_CORE_MAX_NODES
        || graph.edge_slots.len() > DYNAMIC_SPARSE_CORE_MAX_EDGES
        || branches > DYNAMIC_SPARSE_CORE_COLLECTION_MAX_BRANCHES
    {
        return Err(DynamicTreeChainCycleQueryError::AdmissionLimit);
    }
    for (slot, edge) in graph.edge_slots.iter().enumerate() {
        if let Some(edge) = edge
            && (edge.edge != slot
                || edge.from >= graph.active_node_count
                || edge.to >= graph.active_node_count
                || edge.length <= BigRational::zero()
                || rational_too_wide(&edge.length)
                || rational_too_wide(&edge.gradient))
        {
            return Err(DynamicTreeChainCycleQueryError::InvalidInput);
        }
    }
    Ok(())
}

fn shifted_graph(graph: &DynamicLevelGraphSnapshot) -> ShiftedTreeChainGraph {
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

fn is_connected(graph: &DynamicLevelGraphSnapshot) -> bool {
    if graph.active_node_count <= 1 {
        return true;
    }
    let mut seen = vec![false; graph.active_node_count];
    let mut queue = VecDeque::new();
    seen[0] = true;
    queue.push_back(0);
    while let Some(node) = queue.pop_front() {
        for edge in graph.edge_slots.iter().flatten() {
            let next = if edge.from == node {
                Some(edge.to)
            } else if edge.to == node {
                Some(edge.from)
            } else {
                None
            };
            if let Some(next) = next
                && !seen[next]
            {
                seen[next] = true;
                queue.push_back(next);
            }
        }
    }
    seen.into_iter().all(|value| value)
}

fn canonical_spanning_forest(
    graph: &DynamicLevelGraphSnapshot,
) -> Result<Vec<usize>, DynamicTreeChainCycleQueryError> {
    let mut parent = (0..graph.active_node_count).collect::<Vec<_>>();
    let mut rank = vec![0_u8; graph.active_node_count];
    let mut selected = Vec::new();
    for (slot, edge) in graph.edge_slots.iter().enumerate() {
        let Some(edge) = edge else { continue };
        if edge.from == edge.to {
            continue;
        }
        let left = dsu_find(&mut parent, edge.from);
        let right = dsu_find(&mut parent, edge.to);
        if left == right {
            continue;
        }
        if rank[left] < rank[right] {
            parent[left] = right;
        } else {
            parent[right] = left;
            if rank[left] == rank[right] {
                rank[left] = rank[left]
                    .checked_add(1)
                    .ok_or(DynamicTreeChainCycleQueryError::ArithmeticOverflow)?;
            }
        }
        selected.push(slot);
    }
    Ok(selected)
}

fn dsu_find(parent: &mut [usize], node: usize) -> usize {
    if parent[node] != node {
        parent[node] = dsu_find(parent, parent[node]);
    }
    parent[node]
}

fn convert_terminal_branches(
    graph: &ShiftedTreeChainGraph,
    trace: &LowStretchForestMwuTraceResult,
) -> Result<Vec<DynamicTreeChainTerminalBranch>, DynamicTreeChainCycleQueryError> {
    trace
        .result
        .branches
        .iter()
        .map(|branch| {
            let tree_edges = graph
                .edges
                .iter()
                .enumerate()
                .filter_map(|(index, edge)| {
                    (branch.tree_mask & (1_u64 << index) != 0).then_some(edge.source_edge)
                })
                .collect();
            Ok(DynamicTreeChainTerminalBranch { tree_edges })
        })
        .collect()
}

fn audit_convert_terminal_branches(
    graph: &ShiftedTreeChainGraph,
    trace: &LowStretchForestMwuTraceResult,
) -> Result<Vec<DynamicTreeChainTerminalBranch>, DynamicTreeChainCycleQueryError> {
    let mut result = Vec::with_capacity(trace.result.branches.len());
    for branch in &trace.result.branches {
        let mut tree_edges = Vec::new();
        for index in 0..graph.edges.len() {
            if branch.tree_mask & (1_u64 << index) != 0 {
                tree_edges.push(graph.edges[index].source_edge);
            }
        }
        if tree_edges.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(DynamicTreeChainCycleQueryError::TraceVerification);
        }
        result.push(DynamicTreeChainTerminalBranch { tree_edges });
    }
    Ok(result)
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_TREE_CHAIN_QUERY_MAX_SCALAR_BITS
        || value.denom().bits() > DYNAMIC_TREE_CHAIN_QUERY_MAX_SCALAR_BITS
}

fn add_one(value: u64, audit: bool) -> Result<u64, DynamicTreeChainCycleQueryError> {
    value.checked_add(1).ok_or_else(|| failure(audit))
}

fn add_metric(
    value: u64,
    additional: u64,
    audit: bool,
) -> Result<u64, DynamicTreeChainCycleQueryError> {
    value.checked_add(additional).ok_or_else(|| failure(audit))
}

fn increment(value: u64) -> Result<u64, DynamicTreeChainCycleQueryError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainCycleQueryError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicTreeChainCycleQueryError> {
    value
        .checked_add(1)
        .ok_or(DynamicTreeChainCycleQueryError::TraceVerification)
}

fn failure(audit: bool) -> DynamicTreeChainCycleQueryError {
    if audit {
        DynamicTreeChainCycleQueryError::TraceVerification
    } else {
        DynamicTreeChainCycleQueryError::InvariantViolation
    }
}

fn audit_error(error: DynamicTreeChainCycleQueryError) -> DynamicTreeChainCycleQueryError {
    match error {
        DynamicTreeChainCycleQueryError::InvalidInput
        | DynamicTreeChainCycleQueryError::AdmissionLimit => error,
        _ => DynamicTreeChainCycleQueryError::TraceVerification,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::algorithms::{
        DynamicActiveBranchProjectionInput, DynamicMwuCollectionBridgeConfig,
        DynamicTreeChainPropagationInput, initialize_dynamic_level_projection,
        initialize_dynamic_tree_chain_epoch_runtime, trace_dynamic_mwu_sparse_core_collection,
        trace_dynamic_tree_chain_propagation,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
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

    fn config() -> DynamicMwuCollectionBridgeConfig {
        DynamicMwuCollectionBridgeConfig {
            branches: 2,
            maximum_node_count: 5,
            stable_edge_slots: 5,
        }
    }

    pub(crate) fn runtime_fixture() -> DynamicTreeChainEpochRuntimeState {
        let root = trace_dynamic_mwu_sparse_core_collection(&root_graph(), config())
            .expect("root initializer");
        let root_input = DynamicActiveBranchProjectionInput {
            collection: root.result.collection,
            active_branch: 0,
        };
        let root_snapshot = &root.result.initialized.final_snapshot.branch_snapshots[0];
        let child_graph = initialize_dynamic_level_projection(root_snapshot)
            .expect("projection")
            .graph;
        let child_shifted = shifted_graph(&child_graph);
        let child = trace_dynamic_mwu_sparse_core_collection(&child_shifted, config())
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
        let propagation = trace_dynamic_tree_chain_propagation(&input, &[]).expect("propagation");
        initialize_dynamic_tree_chain_epoch_runtime(&input, &[], &propagation).expect("runtime")
    }

    #[test]
    fn stable_holes_survive_query_and_candidate_is_a_root_circulation() {
        let state = runtime_fixture();
        let trace = trace_dynamic_tree_chain_cycle_query(&state, 2).expect("query");
        let candidate = trace.result.best_candidate.as_ref().expect("candidate");
        assert_eq!(candidate.coefficients.len(), 5);
        assert_eq!(candidate.coefficients[1], BigInt::zero());
        assert!(candidate.gradient <= BigRational::zero());
        assert!(candidate.weighted_length > BigRational::zero());
        check_dynamic_tree_chain_cycle_query_trace(&state, 2, &trace).expect("check");
    }

    #[test]
    fn one_vertex_terminal_uses_empty_tree_base_case() {
        let graph = DynamicLevelGraphSnapshot {
            active_node_count: 1,
            edge_slots: vec![Some(super::super::DynamicLevelEdge {
                edge: 0,
                from: 0,
                to: 0,
                length: rational(2),
                gradient: rational(-3),
            })],
            stage: 0,
        };
        let trace = trace_dynamic_tree_chain_terminal_collection(&graph, 2).expect("terminal");
        assert!(trace.mwu_trace.is_none());
        assert!(
            trace
                .result
                .branches
                .iter()
                .all(|branch| branch.tree_edges.is_empty())
        );
        check_dynamic_tree_chain_terminal_collection_trace(&graph, 2, &trace).expect("check");
    }

    #[test]
    fn checker_rejects_candidate_terminal_and_materialization_tampering() {
        let state = runtime_fixture();
        let trace = trace_dynamic_tree_chain_cycle_query(&state, 2).expect("query");

        let mut candidate = trace.clone();
        let event = candidate
            .events
            .iter_mut()
            .find_map(|event| match &mut event.kind {
                DynamicTreeChainCycleQueryEventKind::CandidateEvaluated { candidate, .. } => {
                    Some(candidate)
                }
                _ => None,
            })
            .expect("candidate event");
        event.coefficients[0] += 1;
        assert_eq!(
            check_dynamic_tree_chain_cycle_query_trace(&state, 2, &candidate),
            Err(DynamicTreeChainCycleQueryError::TraceVerification)
        );

        let mut terminal = trace.clone();
        terminal.terminal_trace.result.branches[0]
            .tree_edges
            .clear();
        assert_eq!(
            check_dynamic_tree_chain_cycle_query_trace(&state, 2, &terminal),
            Err(DynamicTreeChainCycleQueryError::TraceVerification)
        );

        let mut materialization = trace;
        materialization.materialization.epoch_snapshot.levels[0]
            .source_graph
            .stage += 1;
        assert!(check_dynamic_tree_chain_cycle_query_trace(&state, 2, &materialization).is_err());
    }
}
