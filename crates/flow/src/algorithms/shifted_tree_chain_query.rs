//! Exact `FindCycle` query for the explicit shifted-tree-chain realization.
//!
//! The query follows `FindCycle` in Algorithm 2 and the candidate description
//! in the proof of Lemma 5.11 of van den Brand et al., "A Deterministic
//! Almost-Linear Time Algorithm for Minimum-Cost Flow"
//! (arXiv:2309.16629v1). It examines every intermediate spanner fundamental
//! cycle and every fundamental cycle of every retained terminal tree, maps
//! each cycle through the active chain back to `G_0`, and returns the candidate
//! with largest exact
//! `|<g, Delta>| / ||l o Delta||_1` ratio.
//!
//! It evaluates both source candidate classes: every omitted core edge plus
//! the reverse of its explicit spanner embedding at intermediate levels, and
//! every fundamental cycle of every retained level-`d` tree. It does not claim
//! the paper's dynamic heap runtime or approximation theorem.

use std::collections::VecDeque;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    ShiftedTreeChainConfig, ShiftedTreeChainError, ShiftedTreeChainGraph, ShiftedTreeChainSnapshot,
    check_shifted_tree_chain_snapshot,
};

/// Maximum public events for intermediate and terminal pairs plus completion.
pub const SHIFTED_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS: usize = 133;
/// Maximum bit width of a lifted integral cycle coefficient.
pub const SHIFTED_TREE_CHAIN_QUERY_MAX_COEFFICIENT_BITS: u64 = 512;

const CATALOG_ID: &str = "shifted-tree-chain-find-cycle";

/// One exact source candidate mapped back to `G_0`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainCycleCandidate {
    /// Source candidate identity before recursive lifting.
    pub source: ShiftedTreeChainCycleSource,
    /// Integral circulation coefficients in stable `G_0` edge order.
    pub coefficients: Vec<BigInt>,
    /// Exact oriented value `<g, Delta>`, always non-positive.
    pub gradient: BigRational,
    /// Exact positive value `||l o Delta||_1`.
    pub weighted_length: BigRational,
    /// Exact non-negative absolute quality ratio.
    pub ratio: BigRational,
}

/// Source class and stable identity of one cycle candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainCycleSource {
    /// Omitted core edge plus the reverse of its spanner embedding.
    FundamentalSpanner {
        /// Intermediate tree-chain level.
        level: usize,
        /// Core-graph edge index.
        core_edge: usize,
    },
    /// Off-tree edge plus its terminal retained-tree path.
    TerminalTree {
        /// Terminal retained-tree index.
        branch: usize,
        /// Terminal graph edge index.
        edge: usize,
    },
}

/// Exact work counters for one bounded query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShiftedTreeChainQueryMetrics {
    /// Intermediate core edges inspected.
    pub intermediate_edge_inspections: u64,
    /// Core edges skipped because they are retained by the spanner.
    pub spanner_edges_skipped: u64,
    /// Spanner embedding arcs traversed to form intermediate cycles.
    pub spanner_embedding_arcs: u64,
    /// Terminal tree/edge pairs inspected.
    pub terminal_edge_inspections: u64,
    /// Pairs skipped because the edge belongs to the terminal tree.
    pub tree_edges_skipped: u64,
    /// Source candidates whose recursive lift cancels to the zero vector.
    pub zero_lifts_skipped: u64,
    /// Source cycles evaluated after lifting.
    pub candidates_evaluated: u64,
    /// Tree-path arcs traversed at the terminal level.
    pub terminal_path_arcs: u64,
    /// Tree-path arcs traversed while lifting through parent levels.
    pub lift_path_arcs: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete query state at one publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainQuerySnapshot {
    /// Next stable intermediate/terminal work-item index.
    pub next_work_item: usize,
    /// Best stable candidate seen so far.
    pub best_candidate: Option<ShiftedTreeChainCycleCandidate>,
    /// Whether the completion boundary has been emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: ShiftedTreeChainQueryMetrics,
}

/// Source meaning of one query boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainQueryEventKind {
    /// The inspected core edge is retained by the active spanner.
    SpannerEdgeSkipped {
        /// Intermediate tree-chain level.
        level: usize,
        /// Core-graph edge index.
        core_edge: usize,
    },
    /// The inspected terminal edge is part of the selected tree.
    TreeEdgeSkipped {
        /// Terminal retained-tree index.
        terminal_branch: usize,
        /// Terminal graph edge index.
        terminal_edge: usize,
    },
    /// The source cycle maps to the zero vector in `G_0`.
    ZeroLiftSkipped {
        /// Source candidate that canceled during lifting.
        source: ShiftedTreeChainCycleSource,
    },
    /// One source cycle was lifted and scored.
    CandidateEvaluated {
        /// Exact candidate witness.
        candidate: Box<ShiftedTreeChainCycleCandidate>,
        /// Whether this candidate became the stable incumbent.
        became_best: bool,
    },
    /// Every intermediate core edge and terminal tree/edge pair was inspected.
    Completed,
}

/// One fully reversible `FindCycle` event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainQueryTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level event meaning.
    pub kind: ShiftedTreeChainQueryEventKind,
    /// State before the event.
    pub before: ShiftedTreeChainQuerySnapshot,
    /// State after the event.
    pub after: ShiftedTreeChainQuerySnapshot,
}

/// Exact bounded `FindCycle` result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainQueryResult {
    /// Best candidate, or `None` when no source cycle has a nonzero lift.
    pub best_candidate: Option<ShiftedTreeChainCycleCandidate>,
    /// Final replay state.
    pub final_snapshot: ShiftedTreeChainQuerySnapshot,
}

/// Complete reversible query transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainQueryTraceResult {
    /// Initial empty query state.
    pub base_snapshot: ShiftedTreeChainQuerySnapshot,
    /// One event per stable work item, then completion.
    pub events: Vec<ShiftedTreeChainQueryTraceEvent>,
    /// Exact query result.
    pub result: ShiftedTreeChainQueryResult,
}

/// Explicit bounded-query failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShiftedTreeChainQueryError {
    /// The supplied chain snapshot is invalid.
    #[error(transparent)]
    ShiftedTreeChain(#[from] ShiftedTreeChainError),
    /// An exact coefficient or event count exceeds the published band.
    #[error("shifted tree chain query exceeds its admission band")]
    AdmissionLimit,
    /// Checked work accounting overflowed.
    #[error("shifted tree chain query arithmetic overflow")]
    ArithmeticOverflow,
    /// The structurally valid chain cannot be lifted as promised.
    #[error("shifted tree chain query invariant failed")]
    InvariantViolation,
    /// A supplied transcript is not the exact stable query replay.
    #[error("shifted tree chain query trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: ShiftedTreeChainQuerySnapshot,
    events: Vec<ShiftedTreeChainQueryTraceEvent>,
    result: ShiftedTreeChainQueryResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueryWorkItem {
    Intermediate { level: usize, core_edge: usize },
    Terminal { branch: usize, edge: usize },
}

/// Runs the exact bounded `FindCycle` query without recording events.
///
/// # Errors
///
/// Rejects an invalid chain snapshot, checked overflow, a broken lift
/// invariant, or a coefficient outside the published small-instance band.
pub fn find_shifted_tree_chain_cycle(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
) -> Result<ShiftedTreeChainQueryResult, ShiftedTreeChainQueryError> {
    run_internal(graph, config, chain, false).map(|run| run.result)
}

/// Records every intermediate/terminal inspection and completion boundary.
///
/// # Errors
///
/// Returns any execution or independent replay-checker failure.
pub fn trace_shifted_tree_chain_cycle_query(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
) -> Result<ShiftedTreeChainQueryTraceResult, ShiftedTreeChainQueryError> {
    let run = run_internal(graph, config, chain, true)?;
    let trace = ShiftedTreeChainQueryTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_shifted_tree_chain_cycle_query_trace(graph, config, chain, &trace)?;
    Ok(trace)
}

/// Independently reconstructs every candidate and checks an exact query trace.
///
/// This checker does not invoke the production query runner or its path/lift
/// helpers.
///
/// # Errors
///
/// Rejects any cursor, candidate, circulation, score, best-choice, metric, or
/// completion drift.
pub fn check_shifted_tree_chain_cycle_query_trace(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    trace: &ShiftedTreeChainQueryTraceResult,
) -> Result<(), ShiftedTreeChainQueryError> {
    check_shifted_tree_chain_snapshot(graph, config, chain)?;
    let work = audit_query_work_items(chain, config)?;
    let base = empty_snapshot();
    let expected_events = work
        .len()
        .checked_add(1)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    if trace.base_snapshot != base
        || expected_events > SHIFTED_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS
        || trace.events.len() != expected_events
    {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    let (cursor, event_index) =
        audit_query_events(graph, config, chain, &work, &trace.events, base)?;
    audit_query_completion(trace, &cursor, event_index)
}

fn audit_query_events(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    work: &[QueryWorkItem],
    events: &[ShiftedTreeChainQueryTraceEvent],
    mut cursor: ShiftedTreeChainQuerySnapshot,
) -> Result<(ShiftedTreeChainQuerySnapshot, usize), ShiftedTreeChainQueryError> {
    for (event_index, &item) in work.iter().enumerate() {
        let event = events
            .get(event_index)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        if event.catalog_id != CATALOG_ID || event.before != cursor {
            return Err(ShiftedTreeChainQueryError::TraceVerification);
        }
        let mut expected_after = cursor.clone();
        let expected_kind = audit_process_item(graph, config, chain, item, &mut expected_after)?;
        expected_after.next_work_item = expected_after
            .next_work_item
            .checked_add(1)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        expected_after.metrics.state_transitions =
            audit_increment(expected_after.metrics.state_transitions)?;
        if event.kind != expected_kind || event.after != expected_after {
            return Err(ShiftedTreeChainQueryError::TraceVerification);
        }
        cursor = expected_after;
    }
    Ok((cursor, work.len()))
}

fn audit_query_completion(
    trace: &ShiftedTreeChainQueryTraceResult,
    cursor: &ShiftedTreeChainQuerySnapshot,
    event_index: usize,
) -> Result<(), ShiftedTreeChainQueryError> {
    let completion = trace
        .events
        .get(event_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let mut expected_final = cursor.clone();
    expected_final.complete = true;
    expected_final.metrics.state_transitions =
        audit_increment(expected_final.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || &completion.before != cursor
        || completion.kind != ShiftedTreeChainQueryEventKind::Completed
        || completion.after != expected_final
        || trace.result.best_candidate != expected_final.best_candidate
        || trace.result.final_snapshot != expected_final
    {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    record: bool,
) -> Result<InternalRun, ShiftedTreeChainQueryError> {
    check_shifted_tree_chain_snapshot(graph, config, chain)?;
    let work = query_work_items(chain, config)?;
    let expected_events = work
        .len()
        .checked_add(1)
        .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
    if expected_events > SHIFTED_TREE_CHAIN_QUERY_MAX_TRACE_EVENTS {
        return Err(ShiftedTreeChainQueryError::AdmissionLimit);
    }
    let mut snapshot = empty_snapshot();
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { expected_events } else { 0 });
    for item in work {
        let before = snapshot.clone();
        let kind = process_item(graph, config, chain, item, &mut snapshot)?;
        snapshot.next_work_item = snapshot
            .next_work_item
            .checked_add(1)
            .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        if record {
            events.push(ShiftedTreeChainQueryTraceEvent {
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
        events.push(ShiftedTreeChainQueryTraceEvent {
            catalog_id: CATALOG_ID,
            kind: ShiftedTreeChainQueryEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    let result = ShiftedTreeChainQueryResult {
        best_candidate: snapshot.best_candidate.clone(),
        final_snapshot: snapshot,
    };
    Ok(InternalRun {
        base_snapshot,
        events,
        result,
    })
}

fn empty_snapshot() -> ShiftedTreeChainQuerySnapshot {
    ShiftedTreeChainQuerySnapshot {
        next_work_item: 0,
        best_candidate: None,
        complete: false,
        metrics: ShiftedTreeChainQueryMetrics::default(),
    }
}

fn query_work_items(
    chain: &ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
) -> Result<Vec<QueryWorkItem>, ShiftedTreeChainQueryError> {
    let mut work = Vec::new();
    for level_index in 0..config.depth {
        let level = chain
            .levels
            .get(level_index)
            .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        let recursive = active_recursive(level)?;
        work.extend((0..recursive.core_graph.edges.len()).map(|core_edge| {
            QueryWorkItem::Intermediate {
                level: level_index,
                core_edge,
            }
        }));
    }
    let terminal = chain
        .levels
        .get(config.depth)
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
    for branch in 0..terminal.branches.len() {
        work.extend(
            (0..terminal.graph.edges.len()).map(|edge| QueryWorkItem::Terminal { branch, edge }),
        );
    }
    Ok(work)
}

fn active_recursive(
    level: &super::ShiftedTreeChainLevel,
) -> Result<&super::ShiftedTreeChainRecursiveBranch, ShiftedTreeChainQueryError> {
    level
        .branches
        .get(level.active_branch)
        .and_then(|branch| branch.recursive.as_ref())
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)
}

fn process_item(
    original: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    item: QueryWorkItem,
    snapshot: &mut ShiftedTreeChainQuerySnapshot,
) -> Result<ShiftedTreeChainQueryEventKind, ShiftedTreeChainQueryError> {
    match item {
        QueryWorkItem::Intermediate { level, core_edge } => {
            snapshot.metrics.intermediate_edge_inspections =
                increment(snapshot.metrics.intermediate_edge_inspections)?;
            let source = ShiftedTreeChainCycleSource::FundamentalSpanner { level, core_edge };
            let recursive = active_recursive(&chain.levels[level])?;
            let edge = recursive
                .core_graph
                .edges
                .get(core_edge)
                .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
            if recursive
                .sparsified_core_graph
                .edges
                .iter()
                .any(|candidate| candidate.source_edge == edge.source_edge)
            {
                snapshot.metrics.spanner_edges_skipped =
                    increment(snapshot.metrics.spanner_edges_skipped)?;
                return Ok(ShiftedTreeChainQueryEventKind::SpannerEdgeSkipped { level, core_edge });
            }
            let (candidate, embedding_arcs, lift_arcs) =
                build_spanner_candidate(original, chain, level, core_edge)?;
            snapshot.metrics.spanner_embedding_arcs = snapshot
                .metrics
                .spanner_embedding_arcs
                .checked_add(embedding_arcs)
                .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
            snapshot.metrics.lift_path_arcs = snapshot
                .metrics
                .lift_path_arcs
                .checked_add(lift_arcs)
                .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
            publish_candidate(snapshot, source, candidate)
        }
        QueryWorkItem::Terminal { branch, edge } => {
            let terminal = &chain.levels[config.depth];
            snapshot.metrics.terminal_edge_inspections =
                increment(snapshot.metrics.terminal_edge_inspections)?;
            if terminal.branches[branch].tree_mask & edge_bit(edge)? != 0 {
                snapshot.metrics.tree_edges_skipped =
                    increment(snapshot.metrics.tree_edges_skipped)?;
                return Ok(ShiftedTreeChainQueryEventKind::TreeEdgeSkipped {
                    terminal_branch: branch,
                    terminal_edge: edge,
                });
            }
            let source = ShiftedTreeChainCycleSource::TerminalTree { branch, edge };
            let (candidate, terminal_arcs, lift_arcs) =
                build_terminal_candidate(original, config, chain, branch, edge)?;
            snapshot.metrics.terminal_path_arcs = snapshot
                .metrics
                .terminal_path_arcs
                .checked_add(terminal_arcs)
                .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
            snapshot.metrics.lift_path_arcs = snapshot
                .metrics
                .lift_path_arcs
                .checked_add(lift_arcs)
                .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
            publish_candidate(snapshot, source, candidate)
        }
    }
}

fn publish_candidate(
    snapshot: &mut ShiftedTreeChainQuerySnapshot,
    source: ShiftedTreeChainCycleSource,
    candidate: Option<ShiftedTreeChainCycleCandidate>,
) -> Result<ShiftedTreeChainQueryEventKind, ShiftedTreeChainQueryError> {
    let Some(candidate) = candidate else {
        snapshot.metrics.zero_lifts_skipped = increment(snapshot.metrics.zero_lifts_skipped)?;
        return Ok(ShiftedTreeChainQueryEventKind::ZeroLiftSkipped { source });
    };
    snapshot.metrics.candidates_evaluated = increment(snapshot.metrics.candidates_evaluated)?;
    let became_best = is_better(&candidate, snapshot.best_candidate.as_ref());
    if became_best {
        snapshot.best_candidate = Some(candidate.clone());
    }
    Ok(ShiftedTreeChainQueryEventKind::CandidateEvaluated {
        candidate: Box::new(candidate),
        became_best,
    })
}

fn build_spanner_candidate(
    original: &ShiftedTreeChainGraph,
    chain: &ShiftedTreeChainSnapshot,
    level_index: usize,
    core_edge_index: usize,
) -> Result<(Option<ShiftedTreeChainCycleCandidate>, u64, u64), ShiftedTreeChainQueryError> {
    let level = chain
        .levels
        .get(level_index)
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
    let recursive = active_recursive(level)?;
    let mut coefficients = vec![BigInt::zero(); recursive.core_graph.edges.len()];
    coefficients[core_edge_index] += 1;
    let embedding = recursive
        .core_to_spanner
        .get(core_edge_index)
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
    for arc in embedding {
        let spanner_edge = recursive
            .sparsified_core_graph
            .edges
            .get(arc.edge)
            .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        let core_index = recursive
            .core_graph
            .edges
            .iter()
            .position(|edge| edge.source_edge == spanner_edge.source_edge)
            .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        coefficients[core_index] -= BigInt::from(arc.direction);
    }
    let (mut coefficients, mut lift_arcs) =
        lift_one_level(level, &recursive.core_graph, &coefficients)?;
    for parent_index in (0..level_index).rev() {
        let parent = &chain.levels[parent_index];
        let child = &chain.levels[parent_index + 1].graph;
        let (lifted, traversed) = lift_one_level(parent, child, &coefficients)?;
        coefficients = lifted;
        lift_arcs = lift_arcs
            .checked_add(traversed)
            .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
    }
    let source = ShiftedTreeChainCycleSource::FundamentalSpanner {
        level: level_index,
        core_edge: core_edge_index,
    };
    let candidate = orient_and_score(original, source, coefficients)?;
    let embedding_arcs = u64::try_from(embedding.len())
        .map_err(|_| ShiftedTreeChainQueryError::ArithmeticOverflow)?;
    Ok((candidate, embedding_arcs, lift_arcs))
}

fn build_terminal_candidate(
    original: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    branch_index: usize,
    edge_index: usize,
) -> Result<(Option<ShiftedTreeChainCycleCandidate>, u64, u64), ShiftedTreeChainQueryError> {
    let terminal = &chain.levels[config.depth];
    let tree_mask = terminal.branches[branch_index].tree_mask;
    let (mut coefficients, terminal_arcs) =
        fundamental_coefficients(&terminal.graph, tree_mask, edge_index)?;
    let mut lift_arcs = 0_u64;
    for level_index in (0..config.depth).rev() {
        let parent = &chain.levels[level_index];
        let child = &chain.levels[level_index + 1].graph;
        let (lifted, traversed) = lift_one_level(parent, child, &coefficients)?;
        coefficients = lifted;
        lift_arcs = lift_arcs
            .checked_add(traversed)
            .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
    }
    let candidate = orient_and_score(
        original,
        ShiftedTreeChainCycleSource::TerminalTree {
            branch: branch_index,
            edge: edge_index,
        },
        coefficients,
    )?;
    Ok((candidate, terminal_arcs, lift_arcs))
}

fn fundamental_coefficients(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    edge_index: usize,
) -> Result<(Vec<BigInt>, u64), ShiftedTreeChainQueryError> {
    let edge = graph
        .edges
        .get(edge_index)
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
    let path = query_tree_path(graph, tree_mask, edge.to, edge.from)?
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
    let mut coefficients = vec![BigInt::zero(); graph.edges.len()];
    coefficients[edge_index] += 1;
    for arc in &path {
        coefficients[arc.edge] += BigInt::from(arc.direction);
    }
    ensure_coefficients(&coefficients)?;
    let traversed =
        u64::try_from(path.len()).map_err(|_| ShiftedTreeChainQueryError::ArithmeticOverflow)?;
    Ok((coefficients, traversed))
}

fn lift_one_level(
    parent: &super::ShiftedTreeChainLevel,
    child: &ShiftedTreeChainGraph,
    child_coefficients: &[BigInt],
) -> Result<(Vec<BigInt>, u64), ShiftedTreeChainQueryError> {
    if child.edges.len() != child_coefficients.len() {
        return Err(ShiftedTreeChainQueryError::InvariantViolation);
    }
    let tree_mask = parent
        .branches
        .get(parent.active_branch)
        .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?
        .tree_mask;
    let mut lifted = vec![BigInt::zero(); parent.graph.edges.len()];
    let mut traversed = 0_u64;
    for (child_edge, coefficient) in child.edges.iter().zip(child_coefficients) {
        if coefficient.is_zero() {
            continue;
        }
        let parent_index = parent
            .graph
            .edges
            .iter()
            .position(|edge| edge.source_edge == child_edge.source_edge)
            .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        let parent_edge = &parent.graph.edges[parent_index];
        lifted[parent_index] += coefficient;
        let path = query_tree_path(&parent.graph, tree_mask, parent_edge.to, parent_edge.from)?
            .ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        traversed = traversed
            .checked_add(
                u64::try_from(path.len())
                    .map_err(|_| ShiftedTreeChainQueryError::ArithmeticOverflow)?,
            )
            .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)?;
        for arc in path {
            lifted[arc.edge] += coefficient * BigInt::from(arc.direction);
        }
    }
    ensure_coefficients(&lifted)?;
    Ok((lifted, traversed))
}

#[derive(Clone, Copy)]
struct PathArc {
    edge: usize,
    direction: i8,
}

fn query_tree_path(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    start: usize,
    target: usize,
) -> Result<Option<Vec<PathArc>>, ShiftedTreeChainQueryError> {
    if start == target {
        return Ok(Some(Vec::new()));
    }
    let mut previous = vec![None; graph.node_count];
    let mut seen = vec![false; graph.node_count];
    let mut queue = VecDeque::new();
    seen[start] = true;
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        for (index, edge) in graph.edges.iter().enumerate() {
            if tree_mask & edge_bit(index)? == 0 || edge.from == edge.to {
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
                previous[next] = Some((node, index, direction));
                queue.push_back(next);
            }
        }
    }
    if !seen[target] {
        return Ok(None);
    }
    let mut reversed = Vec::new();
    let mut cursor = target;
    while cursor != start {
        let (parent, edge, direction) =
            previous[cursor].ok_or(ShiftedTreeChainQueryError::InvariantViolation)?;
        reversed.push(PathArc { edge, direction });
        cursor = parent;
    }
    reversed.reverse();
    Ok(Some(reversed))
}

fn orient_and_score(
    graph: &ShiftedTreeChainGraph,
    source: ShiftedTreeChainCycleSource,
    mut coefficients: Vec<BigInt>,
) -> Result<Option<ShiftedTreeChainCycleCandidate>, ShiftedTreeChainQueryError> {
    if coefficients.iter().all(BigInt::is_zero) {
        return Ok(None);
    }
    ensure_circulation(graph, &coefficients)?;
    let mut gradient = graph
        .edges
        .iter()
        .zip(&coefficients)
        .fold(BigRational::zero(), |sum, (edge, coefficient)| {
            sum + &edge.gradient * coefficient
        });
    if gradient.is_positive() {
        for value in &mut coefficients {
            *value = -&*value;
        }
        gradient = -gradient;
    }
    let weighted_length = graph
        .edges
        .iter()
        .zip(&coefficients)
        .fold(BigRational::zero(), |sum, (edge, coefficient)| {
            sum + &edge.length * coefficient.abs()
        });
    if weighted_length.is_zero() {
        return Ok(None);
    }
    let ratio = -&gradient / &weighted_length;
    Ok(Some(ShiftedTreeChainCycleCandidate {
        source,
        coefficients,
        gradient,
        weighted_length,
        ratio,
    }))
}

fn ensure_circulation(
    graph: &ShiftedTreeChainGraph,
    coefficients: &[BigInt],
) -> Result<(), ShiftedTreeChainQueryError> {
    if coefficients.len() != graph.edges.len() {
        return Err(ShiftedTreeChainQueryError::InvariantViolation);
    }
    let mut divergence = vec![BigInt::zero(); graph.node_count];
    for (edge, coefficient) in graph.edges.iter().zip(coefficients) {
        divergence[edge.from] += coefficient;
        divergence[edge.to] -= coefficient;
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(ShiftedTreeChainQueryError::InvariantViolation);
    }
    Ok(())
}

fn ensure_coefficients(coefficients: &[BigInt]) -> Result<(), ShiftedTreeChainQueryError> {
    if coefficients
        .iter()
        .any(|value| value.bits() > SHIFTED_TREE_CHAIN_QUERY_MAX_COEFFICIENT_BITS)
    {
        return Err(ShiftedTreeChainQueryError::AdmissionLimit);
    }
    Ok(())
}

fn is_better(
    candidate: &ShiftedTreeChainCycleCandidate,
    incumbent: Option<&ShiftedTreeChainCycleCandidate>,
) -> bool {
    incumbent.is_none_or(|current| candidate.ratio > current.ratio)
}

fn edge_bit(index: usize) -> Result<u64, ShiftedTreeChainQueryError> {
    let shift = u32::try_from(index).map_err(|_| ShiftedTreeChainQueryError::AdmissionLimit)?;
    1_u64
        .checked_shl(shift)
        .ok_or(ShiftedTreeChainQueryError::AdmissionLimit)
}

fn increment(value: u64) -> Result<u64, ShiftedTreeChainQueryError> {
    value
        .checked_add(1)
        .ok_or(ShiftedTreeChainQueryError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, ShiftedTreeChainQueryError> {
    value
        .checked_add(1)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)
}

// The checker intentionally duplicates path construction, recursive lifting,
// conservation checks, and exact scoring instead of trusting the query runner.
fn audit_query_work_items(
    chain: &ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
) -> Result<Vec<QueryWorkItem>, ShiftedTreeChainQueryError> {
    let mut work = Vec::new();
    for level_index in 0..config.depth {
        let level = chain
            .levels
            .get(level_index)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let branch = level
            .branches
            .get(level.active_branch)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let recursive = branch
            .recursive
            .as_ref()
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        for core_edge in 0..recursive.core_graph.edges.len() {
            work.push(QueryWorkItem::Intermediate {
                level: level_index,
                core_edge,
            });
        }
    }
    let terminal = chain
        .levels
        .get(config.depth)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    for branch in 0..terminal.branches.len() {
        for edge in 0..terminal.graph.edges.len() {
            work.push(QueryWorkItem::Terminal { branch, edge });
        }
    }
    Ok(work)
}

fn audit_process_item(
    original: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    item: QueryWorkItem,
    snapshot: &mut ShiftedTreeChainQuerySnapshot,
) -> Result<ShiftedTreeChainQueryEventKind, ShiftedTreeChainQueryError> {
    match item {
        QueryWorkItem::Intermediate { level, core_edge } => {
            snapshot.metrics.intermediate_edge_inspections =
                audit_increment(snapshot.metrics.intermediate_edge_inspections)?;
            let chain_level = chain
                .levels
                .get(level)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            let branch = chain_level
                .branches
                .get(chain_level.active_branch)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            let recursive = branch
                .recursive
                .as_ref()
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            let edge = recursive
                .core_graph
                .edges
                .get(core_edge)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            if recursive
                .sparsified_core_graph
                .edges
                .iter()
                .any(|retained| retained.source_edge == edge.source_edge)
            {
                snapshot.metrics.spanner_edges_skipped =
                    audit_increment(snapshot.metrics.spanner_edges_skipped)?;
                return Ok(ShiftedTreeChainQueryEventKind::SpannerEdgeSkipped { level, core_edge });
            }
            let source = ShiftedTreeChainCycleSource::FundamentalSpanner { level, core_edge };
            let (candidate, embedding_arcs, lift_arcs) =
                audit_spanner_candidate(original, chain, level, core_edge)?;
            snapshot.metrics.spanner_embedding_arcs = snapshot
                .metrics
                .spanner_embedding_arcs
                .checked_add(embedding_arcs)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            snapshot.metrics.lift_path_arcs = snapshot
                .metrics
                .lift_path_arcs
                .checked_add(lift_arcs)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            audit_publish_candidate(snapshot, source, candidate)
        }
        QueryWorkItem::Terminal { branch, edge } => {
            snapshot.metrics.terminal_edge_inspections =
                audit_increment(snapshot.metrics.terminal_edge_inspections)?;
            let terminal = chain
                .levels
                .get(config.depth)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            let terminal_branch = terminal
                .branches
                .get(branch)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            terminal
                .graph
                .edges
                .get(edge)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            if terminal_branch.tree_mask & edge_bit(edge)? != 0 {
                snapshot.metrics.tree_edges_skipped =
                    audit_increment(snapshot.metrics.tree_edges_skipped)?;
                return Ok(ShiftedTreeChainQueryEventKind::TreeEdgeSkipped {
                    terminal_branch: branch,
                    terminal_edge: edge,
                });
            }
            let source = ShiftedTreeChainCycleSource::TerminalTree { branch, edge };
            let (candidate, terminal_arcs, lift_arcs) =
                audit_terminal_candidate(original, config, chain, branch, edge)?;
            snapshot.metrics.terminal_path_arcs = snapshot
                .metrics
                .terminal_path_arcs
                .checked_add(terminal_arcs)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            snapshot.metrics.lift_path_arcs = snapshot
                .metrics
                .lift_path_arcs
                .checked_add(lift_arcs)
                .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
            audit_publish_candidate(snapshot, source, candidate)
        }
    }
}

fn audit_publish_candidate(
    snapshot: &mut ShiftedTreeChainQuerySnapshot,
    source: ShiftedTreeChainCycleSource,
    candidate: Option<ShiftedTreeChainCycleCandidate>,
) -> Result<ShiftedTreeChainQueryEventKind, ShiftedTreeChainQueryError> {
    let Some(candidate) = candidate else {
        snapshot.metrics.zero_lifts_skipped = audit_increment(snapshot.metrics.zero_lifts_skipped)?;
        return Ok(ShiftedTreeChainQueryEventKind::ZeroLiftSkipped { source });
    };
    snapshot.metrics.candidates_evaluated = audit_increment(snapshot.metrics.candidates_evaluated)?;
    let became_best = snapshot
        .best_candidate
        .as_ref()
        .is_none_or(|incumbent| candidate.ratio > incumbent.ratio);
    if became_best {
        snapshot.best_candidate = Some(candidate.clone());
    }
    Ok(ShiftedTreeChainQueryEventKind::CandidateEvaluated {
        candidate: Box::new(candidate),
        became_best,
    })
}

fn audit_spanner_candidate(
    original: &ShiftedTreeChainGraph,
    chain: &ShiftedTreeChainSnapshot,
    level_index: usize,
    core_edge_index: usize,
) -> Result<(Option<ShiftedTreeChainCycleCandidate>, u64, u64), ShiftedTreeChainQueryError> {
    let level = chain
        .levels
        .get(level_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let branch = level
        .branches
        .get(level.active_branch)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let recursive = branch
        .recursive
        .as_ref()
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let core_edge = recursive
        .core_graph
        .edges
        .get(core_edge_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    if recursive
        .sparsified_core_graph
        .edges
        .iter()
        .any(|edge| edge.source_edge == core_edge.source_edge)
    {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    let embedding = recursive
        .core_to_spanner
        .get(core_edge_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let mut coefficients = vec![BigInt::zero(); recursive.core_graph.edges.len()];
    coefficients[core_edge_index] += 1;
    for arc in embedding {
        let spanner_edge = recursive
            .sparsified_core_graph
            .edges
            .get(arc.edge)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let core_index = recursive
            .core_graph
            .edges
            .iter()
            .position(|edge| edge.source_edge == spanner_edge.source_edge)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        coefficients[core_index] -= BigInt::from(arc.direction);
    }
    let (mut coefficients, mut lift_arcs) =
        audit_lift_one_level(level, &recursive.core_graph, &coefficients)?;
    for parent_index in (0..level_index).rev() {
        let parent = chain
            .levels
            .get(parent_index)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let child = chain
            .levels
            .get(parent_index + 1)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let (lifted, traversed) = audit_lift_one_level(parent, &child.graph, &coefficients)?;
        coefficients = lifted;
        lift_arcs = lift_arcs
            .checked_add(traversed)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    }
    let source = ShiftedTreeChainCycleSource::FundamentalSpanner {
        level: level_index,
        core_edge: core_edge_index,
    };
    let candidate = audit_score(original, source, coefficients)?;
    let embedding_arcs = u64::try_from(embedding.len())
        .map_err(|_| ShiftedTreeChainQueryError::TraceVerification)?;
    Ok((candidate, embedding_arcs, lift_arcs))
}

fn audit_terminal_candidate(
    original: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    branch_index: usize,
    edge_index: usize,
) -> Result<(Option<ShiftedTreeChainCycleCandidate>, u64, u64), ShiftedTreeChainQueryError> {
    let terminal = chain
        .levels
        .get(config.depth)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let (coefficients, terminal_arcs) = audit_terminal_cycle(terminal, branch_index, edge_index)?;
    let (coefficients, lift_arcs) = audit_lift_chain(config, chain, coefficients)?;
    let candidate = audit_score(
        original,
        ShiftedTreeChainCycleSource::TerminalTree {
            branch: branch_index,
            edge: edge_index,
        },
        coefficients,
    )?;
    Ok((candidate, terminal_arcs, lift_arcs))
}

fn audit_terminal_cycle(
    terminal: &super::ShiftedTreeChainLevel,
    branch_index: usize,
    edge_index: usize,
) -> Result<(Vec<BigInt>, u64), ShiftedTreeChainQueryError> {
    let tree_mask = terminal
        .branches
        .get(branch_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?
        .tree_mask;
    let edge = terminal
        .graph
        .edges
        .get(edge_index)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let path = audit_tree_path(&terminal.graph, tree_mask, edge.to, edge.from)?
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    let mut coefficients = vec![BigInt::zero(); terminal.graph.edges.len()];
    coefficients[edge_index] += 1;
    for arc in &path {
        coefficients[arc.edge] += BigInt::from(arc.direction);
    }
    let terminal_arcs =
        u64::try_from(path.len()).map_err(|_| ShiftedTreeChainQueryError::TraceVerification)?;
    Ok((coefficients, terminal_arcs))
}

fn audit_lift_chain(
    config: ShiftedTreeChainConfig,
    chain: &ShiftedTreeChainSnapshot,
    mut coefficients: Vec<BigInt>,
) -> Result<(Vec<BigInt>, u64), ShiftedTreeChainQueryError> {
    let mut lift_arcs = 0_u64;
    for level_index in (0..config.depth).rev() {
        let parent = chain
            .levels
            .get(level_index)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let child = chain
            .levels
            .get(level_index + 1)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let (next, traversed) = audit_lift_one_level(parent, &child.graph, &coefficients)?;
        coefficients = next;
        lift_arcs = lift_arcs
            .checked_add(traversed)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
    }
    Ok((coefficients, lift_arcs))
}

fn audit_lift_one_level(
    parent: &super::ShiftedTreeChainLevel,
    child: &ShiftedTreeChainGraph,
    coefficients: &[BigInt],
) -> Result<(Vec<BigInt>, u64), ShiftedTreeChainQueryError> {
    if coefficients.len() != child.edges.len() {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    let tree_mask = parent
        .branches
        .get(parent.active_branch)
        .ok_or(ShiftedTreeChainQueryError::TraceVerification)?
        .tree_mask;
    let mut lifted = vec![BigInt::zero(); parent.graph.edges.len()];
    let mut traversed = 0_u64;
    for (child_edge, coefficient) in child.edges.iter().zip(coefficients) {
        if coefficient.is_zero() {
            continue;
        }
        let parent_index = parent
            .graph
            .edges
            .iter()
            .position(|edge| edge.source_edge == child_edge.source_edge)
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        let parent_edge = &parent.graph.edges[parent_index];
        lifted[parent_index] += coefficient;
        let path = audit_tree_path(&parent.graph, tree_mask, parent_edge.to, parent_edge.from)?
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        traversed = traversed
            .checked_add(
                u64::try_from(path.len())
                    .map_err(|_| ShiftedTreeChainQueryError::TraceVerification)?,
            )
            .ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        for arc in path {
            lifted[arc.edge] += coefficient * BigInt::from(arc.direction);
        }
    }
    Ok((lifted, traversed))
}

fn audit_score(
    original: &ShiftedTreeChainGraph,
    source: ShiftedTreeChainCycleSource,
    mut coefficients: Vec<BigInt>,
) -> Result<Option<ShiftedTreeChainCycleCandidate>, ShiftedTreeChainQueryError> {
    if coefficients.len() != original.edges.len()
        || coefficients
            .iter()
            .any(|value| value.bits() > SHIFTED_TREE_CHAIN_QUERY_MAX_COEFFICIENT_BITS)
    {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    if coefficients.iter().all(BigInt::is_zero) {
        return Ok(None);
    }
    let mut divergence = vec![BigInt::zero(); original.node_count];
    for (edge, coefficient) in original.edges.iter().zip(&coefficients) {
        divergence[edge.from] += coefficient;
        divergence[edge.to] -= coefficient;
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    let mut gradient = original
        .edges
        .iter()
        .zip(&coefficients)
        .fold(BigRational::zero(), |sum, (edge, coefficient)| {
            sum + &edge.gradient * coefficient
        });
    if gradient.is_positive() {
        for value in &mut coefficients {
            *value = -&*value;
        }
        gradient = -gradient;
    }
    let weighted_length = original
        .edges
        .iter()
        .zip(&coefficients)
        .fold(BigRational::zero(), |sum, (edge, coefficient)| {
            sum + &edge.length * coefficient.abs()
        });
    if weighted_length.is_zero() {
        return Err(ShiftedTreeChainQueryError::TraceVerification);
    }
    let ratio = -&gradient / &weighted_length;
    Ok(Some(ShiftedTreeChainCycleCandidate {
        source,
        coefficients,
        gradient,
        weighted_length,
        ratio,
    }))
}

fn audit_tree_path(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    start: usize,
    target: usize,
) -> Result<Option<Vec<PathArc>>, ShiftedTreeChainQueryError> {
    if start == target {
        return Ok(Some(Vec::new()));
    }
    let mut previous = vec![None; graph.node_count];
    let mut queue = VecDeque::from([start]);
    let mut seen = vec![false; graph.node_count];
    seen[start] = true;
    while let Some(node) = queue.pop_front() {
        for (index, edge) in graph.edges.iter().enumerate() {
            if tree_mask & edge_bit(index)? == 0 || edge.from == edge.to {
                continue;
            }
            let adjacent = match (edge.from == node, edge.to == node) {
                (true, _) => Some((edge.to, 1_i8)),
                (_, true) => Some((edge.from, -1_i8)),
                _ => None,
            };
            if let Some((next, direction)) = adjacent
                && !seen[next]
            {
                seen[next] = true;
                previous[next] = Some((node, index, direction));
                queue.push_back(next);
            }
        }
    }
    if !seen[target] {
        return Ok(None);
    }
    let mut path = Vec::new();
    let mut node = target;
    while node != start {
        let (parent, edge, direction) =
            previous[node].ok_or(ShiftedTreeChainQueryError::TraceVerification)?;
        path.push(PathArc { edge, direction });
        node = parent;
    }
    path.reverse();
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use num_traits::One;

    use super::*;
    use crate::algorithms::{
        ShiftedTreeChainEdge, ShiftedTreeChainOperation, execute_shifted_tree_chain,
    };

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn triangle() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 3,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1),
                    gradient: rational(2),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 1,
                    to: 2,
                    length: rational(2),
                    gradient: rational(-1),
                },
                ShiftedTreeChainEdge {
                    source_edge: 2,
                    from: 2,
                    to: 0,
                    length: rational(3),
                    gradient: rational(4),
                },
            ],
        }
    }

    fn config() -> ShiftedTreeChainConfig {
        ShiftedTreeChainConfig {
            depth: 2,
            branches: 2,
        }
    }

    fn chain(operations: &[ShiftedTreeChainOperation]) -> ShiftedTreeChainSnapshot {
        execute_shifted_tree_chain(&triangle(), config(), operations)
            .expect("chain")
            .final_snapshot
    }

    #[test]
    fn finds_exact_best_lifted_triangle_cycle() {
        let result =
            find_shifted_tree_chain_cycle(&triangle(), config(), &chain(&[])).expect("query");
        let candidate = result.best_candidate.expect("cycle");
        assert_eq!(
            candidate.coefficients,
            vec![(-1).into(), (-1).into(), (-1).into()]
        );
        assert_eq!(candidate.gradient, rational(-5));
        assert_eq!(candidate.weighted_length, rational(6));
        assert_eq!(candidate.ratio, BigRational::new(5.into(), 6.into()));
        assert_eq!(
            candidate.source,
            ShiftedTreeChainCycleSource::TerminalTree { branch: 0, edge: 2 }
        );
        assert!(result.final_snapshot.complete);
    }

    #[test]
    fn fast_and_trace_results_match_with_exact_metrics() {
        let chain = chain(&[]);
        let fast = find_shifted_tree_chain_cycle(&triangle(), config(), &chain).expect("fast");
        let trace =
            trace_shifted_tree_chain_cycle_query(&triangle(), config(), &chain).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 13);
        let metrics = trace.result.final_snapshot.metrics;
        assert_eq!(metrics.intermediate_edge_inspections, 6);
        assert_eq!(metrics.terminal_edge_inspections, 6);
        assert_eq!(metrics.spanner_edges_skipped, 6);
        assert_eq!(metrics.tree_edges_skipped, 4);
        assert_eq!(metrics.zero_lifts_skipped, 0);
        assert_eq!(metrics.candidates_evaluated, 2);
        assert_eq!(trace.result.final_snapshot.metrics.state_transitions, 13);
        assert_eq!(
            metrics.spanner_edges_skipped
                + metrics.tree_edges_skipped
                + metrics.zero_lifts_skipped
                + metrics.candidates_evaluated,
            metrics.intermediate_edge_inspections + metrics.terminal_edge_inspections
        );
    }

    #[test]
    fn all_tree_terminal_graph_has_no_candidate() {
        let graph = ShiftedTreeChainGraph {
            node_count: 2,
            edges: vec![ShiftedTreeChainEdge {
                source_edge: 0,
                from: 0,
                to: 1,
                length: BigRational::one(),
                gradient: rational(-1),
            }],
        };
        let config = ShiftedTreeChainConfig {
            depth: 1,
            branches: 1,
        };
        let chain = execute_shifted_tree_chain(&graph, config, &[])
            .expect("chain")
            .final_snapshot;
        let result = find_shifted_tree_chain_cycle(&graph, config, &chain).expect("query");
        assert_eq!(result.best_candidate, None);
        assert_eq!(result.final_snapshot.metrics.tree_edges_skipped, 1);
        assert_eq!(result.final_snapshot.metrics.candidates_evaluated, 0);
    }

    #[test]
    fn checker_rejects_candidate_and_cursor_tampering() {
        let chain = chain(&[]);
        let mut trace =
            trace_shifted_tree_chain_cycle_query(&triangle(), config(), &chain).expect("trace");
        let event = trace
            .events
            .iter_mut()
            .find(|event| {
                matches!(
                    event.kind,
                    ShiftedTreeChainQueryEventKind::CandidateEvaluated { .. }
                )
            })
            .expect("candidate event");
        let ShiftedTreeChainQueryEventKind::CandidateEvaluated { candidate, .. } = &mut event.kind
        else {
            panic!("candidate event");
        };
        candidate.ratio += BigRational::one();
        assert_eq!(
            check_shifted_tree_chain_cycle_query_trace(&triangle(), config(), &chain, &trace,),
            Err(ShiftedTreeChainQueryError::TraceVerification)
        );

        let mut trace =
            trace_shifted_tree_chain_cycle_query(&triangle(), config(), &chain).expect("trace");
        trace.events[0].after.next_work_item = 0;
        assert_eq!(
            check_shifted_tree_chain_cycle_query_trace(&triangle(), config(), &chain, &trace,),
            Err(ShiftedTreeChainQueryError::TraceVerification)
        );
    }

    #[test]
    fn shifted_parent_branch_still_lifts_a_valid_circulation() {
        let shifted = chain(&[ShiftedTreeChainOperation::Shift { level: 0 }]);
        let result = find_shifted_tree_chain_cycle(&triangle(), config(), &shifted).expect("query");
        let candidate = result.best_candidate.expect("cycle");
        assert_eq!(candidate.coefficients.len(), triangle().edges.len());
        assert!(candidate.ratio >= BigRational::zero());
        ensure_circulation(&triangle(), &candidate.coefficients).expect("circulation");
    }
}
