//! Exact small-graph shifted-tree-chain construction and lifecycle.
//!
//! This module realizes the structural state and lifecycle in Definitions
//! 5.8--5.10 of van den Brand et al.,
//! "A Deterministic Almost-Linear Time Algorithm for Minimum-Cost Flow"
//! (arXiv:2309.16629v1). Every branch contains an actual spanning tree,
//! rooted forest, stretch vector, contracted core, sparsified core, and
//! embedding. Its bounded Definition 5.7 construction groups core edges by
//! factor-two length buckets and runs the deterministic source `Sparsify`
//! tasks with exact small-graph expander/path subroutines; contracted self-loops
//! use the valid empty path. Each level's branch collection is built
//! by the source multiplicative-weights update, using a certified exhaustive
//! rooted-forest oracle in place of the paper's dynamic low-stretch-forest data
//! structure. Thus the module checks the MWU/core/spanner/embedding and
//! Shift/Rebuild definitions, but does not claim the paper's dynamic spanner,
//! Lemma 5.5's asymptotic width, the dynamic LSF recourse bound, or Lemma 5.11's
//! runtime.

use std::collections::{BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Zero;
use thiserror::Error;

use super::deterministic_spanner_sparsify::{
    DeterministicSpannerInputEdge, DeterministicSpannerSparsifyCertificate,
    bounded_deterministic_sparsify,
};
use super::low_stretch_forest_mwu::{
    LowStretchForestMwuChainWork, audit_low_stretch_forest_mwu_for_chain,
};
use super::{
    LowStretchForestMwuBranch, LowStretchForestMwuConfig, build_low_stretch_forest_mwu_collection,
};

/// Maximum nodes in the explicit realization.
pub const SHIFTED_TREE_CHAIN_MAX_NODES: usize = 8;
/// Maximum stable edges at every level.
pub const SHIFTED_TREE_CHAIN_MAX_EDGES: usize = 12;
/// Maximum recursion depth `d`; levels are `0..=d`.
pub const SHIFTED_TREE_CHAIN_MAX_DEPTH: usize = 3;
/// Maximum branches `k` retained at each level.
pub const SHIFTED_TREE_CHAIN_MAX_BRANCHES: usize = 8;
/// Maximum requested Shift/Rebuild calls.
pub const SHIFTED_TREE_CHAIN_MAX_OPERATIONS: usize = 1_024;
/// Maximum bit width of one rational numerator or denominator.
pub const SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS: u64 = 512;
/// Maximum subsets inspected by one spanning-tree enumeration.
pub const SHIFTED_TREE_CHAIN_MAX_TREE_SUBSETS: u64 = 4_096;
/// Maximum public reversible boundaries.
pub const SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS: usize = SHIFTED_TREE_CHAIN_MAX_OPERATIONS + 1;

const CATALOG_ID: &str = "explicit-shifted-tree-chain";

/// One stable oriented edge in a level graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainEdge {
    /// Stable original-edge index.
    pub source_edge: usize,
    /// Current tail/component index.
    pub from: usize,
    /// Current head/component index.
    pub to: usize,
    /// Exact positive length.
    pub length: BigRational,
    /// Exact signed gradient.
    pub gradient: BigRational,
}

/// One exact graph in the recursive chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainGraph {
    /// Current contracted vertex count.
    pub node_count: usize,
    /// Stable edges in original-edge order.
    pub edges: Vec<ShiftedTreeChainEdge>,
}

/// One signed spanner edge on an explicit embedding path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainEmbeddingArc {
    /// Stable edge index.
    pub edge: usize,
    /// `1` follows the stored orientation and `-1` opposes it.
    pub direction: i8,
}

/// Recursive branch data from Definition 5.8.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainRecursiveBranch {
    /// Rooted spanning-forest mask, a subset of the tree mask.
    pub forest_mask: u64,
    /// Distinguished root of every current vertex's component.
    pub roots: Vec<usize>,
    /// Exact stretches, used as their own overestimates.
    pub stretch_overestimates: Vec<BigRational>,
    /// Contracted core graph from Definition 5.6.
    pub core_graph: ShiftedTreeChainGraph,
    /// Edge-reducing deterministic source sparsifier from Definition 5.7.
    pub sparsified_core_graph: ShiftedTreeChainGraph,
    /// Explicit signed embedding path for every core edge.
    pub core_to_spanner: Vec<Vec<ShiftedTreeChainEmbeddingArc>>,
    /// Exact three-task source certificates, one per nonempty length bucket.
    pub sparsify_certificates: Vec<DeterministicSpannerSparsifyCertificate>,
    /// Minimal positive embedding-length/sparsity parameter for this snapshot.
    pub gamma_length: usize,
    /// Minimal positive congestion parameter for this snapshot.
    pub gamma_congestion: usize,
}

/// One retained branch at a source level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainBranch {
    /// Spanning-tree edge mask.
    pub tree_mask: u64,
    /// Recursive data below level `d`; terminal branches are trees only.
    pub recursive: Option<ShiftedTreeChainRecursiveBranch>,
}

/// One source level with its complete branch collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainLevel {
    /// Zero-based level.
    pub level: usize,
    /// Current graph `G_i`.
    pub graph: ShiftedTreeChainGraph,
    /// Current Shift index.
    pub active_branch: usize,
    /// Retained deterministic branches.
    pub branches: Vec<ShiftedTreeChainBranch>,
}

/// Fixed bounded-chain parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainConfig {
    /// Recursion depth `d`.
    pub depth: usize,
    /// Branch count `k`.
    pub branches: usize,
}

/// One requested source lifecycle call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainOperation {
    /// Definition 5.10: increment a branch and rebuild deeper levels.
    Shift {
        /// Shifted level.
        level: usize,
    },
    /// Definition 5.9: reinitialize the selected suffix.
    Rebuild {
        /// First reinitialized level.
        level: usize,
    },
}

/// Current lifecycle stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainStage {
    /// Initial Definition 5.8 chain.
    Ready,
    /// The last transition was Shift.
    Shifted,
    /// The last transition was Rebuild.
    Rebuilt,
    /// Every supplied call completed.
    Complete,
}

/// Exact work and lifecycle counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShiftedTreeChainMetrics {
    /// Candidate subsets inspected by spanning-tree enumeration.
    pub tree_subsets_inspected: u64,
    /// Rooted LSF candidates materialized by exhaustive MWU oracles.
    pub lsf_candidates_enumerated: u64,
    /// Candidate exponential objectives scored by MWU rounds.
    pub lsf_candidate_scores: u64,
    /// Certified exponential interval refinements used by MWU comparisons.
    pub lsf_exponential_refinements: u64,
    /// Level collections initialized or reinitialized.
    pub level_initializations: u64,
    /// Shift counts by level.
    pub shifts: Vec<u64>,
    /// Rebuild counts by selected level.
    pub rebuilds: Vec<u64>,
    /// Suffix levels reinitialized after initial construction.
    pub suffix_reinitializations: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainSnapshot {
    /// Current lifecycle stage.
    pub stage: ShiftedTreeChainStage,
    /// Levels `0..=d`.
    pub levels: Vec<ShiftedTreeChainLevel>,
    /// Exact counters.
    pub metrics: ShiftedTreeChainMetrics,
}

/// Source meaning of one public transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainEventKind {
    /// One Shift transition.
    Shifted {
        /// Shifted level.
        level: usize,
        /// Previous branch index.
        previous_branch: usize,
        /// New branch index.
        next_branch: usize,
        /// Whether the new index is zero.
        wrapped: bool,
    },
    /// One Rebuild transition.
    Rebuilt {
        /// First reinitialized level.
        level: usize,
    },
    /// Every supplied operation completed.
    Completed,
}

/// One fully reversible lifecycle event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source transition meaning.
    pub kind: ShiftedTreeChainEventKind,
    /// State before the transition.
    pub before: ShiftedTreeChainSnapshot,
    /// State after the transition.
    pub after: ShiftedTreeChainSnapshot,
}

/// Exact final tree-chain state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainResult {
    /// Final branch at every level.
    pub active_branches: Vec<usize>,
    /// Final replay state.
    pub final_snapshot: ShiftedTreeChainSnapshot,
}

/// Complete reversible transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainTraceResult {
    /// Initial chain.
    pub base_snapshot: ShiftedTreeChainSnapshot,
    /// Atomic Shift/Rebuild/completion events.
    pub events: Vec<ShiftedTreeChainTraceEvent>,
    /// Final source state.
    pub result: ShiftedTreeChainResult,
}

/// Explicit shifted-tree-chain failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShiftedTreeChainError {
    /// The graph or lifecycle exceeds the published small-instance band.
    #[error("explicit shifted tree chain exceeds its admission band")]
    AdmissionLimit,
    /// Graph shape, parameters, or operation indices are invalid.
    #[error("explicit shifted tree chain input is invalid")]
    InvalidInput,
    /// A level graph has no spanning tree.
    #[error("explicit shifted tree chain level is disconnected")]
    DisconnectedLevel,
    /// Checked work or lifecycle accounting overflowed.
    #[error("explicit shifted tree chain arithmetic overflow")]
    ArithmeticOverflow,
    /// A source-definition invariant failed.
    #[error("explicit shifted tree chain invariant failed")]
    InvariantViolation,
    /// A supplied trace violates source transition semantics.
    #[error("explicit shifted tree chain trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: ShiftedTreeChainSnapshot,
    events: Vec<ShiftedTreeChainTraceEvent>,
    result: ShiftedTreeChainResult,
}

/// Constructs a chain and executes source Shift/Rebuild calls.
///
/// # Errors
///
/// Rejects malformed/out-of-band graphs, disconnected level graphs, invalid
/// operations, checked overflow, or a source-invariant failure.
pub fn execute_shifted_tree_chain(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
) -> Result<ShiftedTreeChainResult, ShiftedTreeChainError> {
    run_internal(graph, config, operations, false).map(|run| run.result)
}

/// Records every Definition 5.9/5.10 boundary.
///
/// # Errors
///
/// Returns any execution or independent checker failure.
pub fn trace_shifted_tree_chain(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
) -> Result<ShiftedTreeChainTraceResult, ShiftedTreeChainError> {
    let run = run_internal(graph, config, operations, true)?;
    let trace = ShiftedTreeChainTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_shifted_tree_chain_trace(graph, config, operations, &trace)?;
    Ok(trace)
}

/// Checks construction, lifecycle, metrics, and replay without invoking the
/// production runner.
///
/// # Errors
///
/// Rejects structural, deterministic-construction, transition, metric, or
/// final-result drift.
pub fn check_shifted_tree_chain_trace(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
    trace: &ShiftedTreeChainTraceResult,
) -> Result<(), ShiftedTreeChainError> {
    validate_input(graph, config, operations)?;
    audit_snapshot(graph, config, &trace.base_snapshot)?;
    if trace.base_snapshot.stage != ShiftedTreeChainStage::Ready
        || !base_metrics_are_exact(config, &trace.base_snapshot)?
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    let mut operation_index = 0_usize;
    for event in &trace.events {
        if event.catalog_id != CATALOG_ID || &event.before != cursor {
            return Err(ShiftedTreeChainError::TraceVerification);
        }
        validate_transition(config, operations, &mut operation_index, event)?;
        audit_snapshot(graph, config, &event.after)?;
        cursor = &event.after;
    }
    let branches = cursor
        .levels
        .iter()
        .map(|level| level.active_branch)
        .collect::<Vec<_>>();
    if operation_index != operations.len()
        || cursor.stage != ShiftedTreeChainStage::Complete
        || cursor != &trace.result.final_snapshot
        || branches != trace.result.active_branches
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    Ok(())
}

/// Checks one supplied shifted-tree-chain snapshot without invoking the
/// production constructor.
///
/// This entry point lets source-faithful query components consume a chain
/// snapshot while retaining the same independent structural validation used
/// by the lifecycle trace checker.
///
/// # Errors
///
/// Rejects invalid graph/configuration data or any structural, metric,
/// embedding, or deterministic-construction drift in `snapshot`.
pub fn check_shifted_tree_chain_snapshot(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    snapshot: &ShiftedTreeChainSnapshot,
) -> Result<(), ShiftedTreeChainError> {
    validate_input(graph, config, &[])?;
    audit_snapshot(graph, config, snapshot)
}

fn run_internal(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
    record: bool,
) -> Result<InternalRun, ShiftedTreeChainError> {
    validate_input(graph, config, operations)?;
    let level_count = config
        .depth
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    let mut metrics = ShiftedTreeChainMetrics {
        shifts: vec![0; level_count],
        rebuilds: vec![0; level_count],
        ..ShiftedTreeChainMetrics::default()
    };
    let levels = build_suffix(graph.clone(), 0, config, &mut metrics)?;
    let mut snapshot = ShiftedTreeChainSnapshot {
        stage: ShiftedTreeChainStage::Ready,
        levels,
        metrics,
    };
    audit_snapshot(graph, config, &snapshot)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    for &operation in operations {
        match operation {
            ShiftedTreeChainOperation::Shift { level } => {
                apply_shift(&mut snapshot, config, level, &mut events, record)?;
            }
            ShiftedTreeChainOperation::Rebuild { level } => {
                apply_rebuild(&mut snapshot, config, level, &mut events, record)?;
            }
        }
    }
    transition(
        &mut snapshot,
        &mut events,
        record,
        ShiftedTreeChainEventKind::Completed,
        |state| {
            state.stage = ShiftedTreeChainStage::Complete;
            Ok(())
        },
    )?;
    let result = ShiftedTreeChainResult {
        active_branches: snapshot
            .levels
            .iter()
            .map(|level| level.active_branch)
            .collect(),
        final_snapshot: snapshot,
    };
    Ok(InternalRun {
        base_snapshot,
        events,
        result,
    })
}

fn apply_shift(
    snapshot: &mut ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
    level: usize,
    events: &mut Vec<ShiftedTreeChainTraceEvent>,
    record: bool,
) -> Result<(), ShiftedTreeChainError> {
    let previous = snapshot
        .levels
        .get(level)
        .ok_or(ShiftedTreeChainError::InvalidInput)?
        .active_branch;
    let next = previous
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?
        % config.branches;
    transition(
        snapshot,
        events,
        record,
        ShiftedTreeChainEventKind::Shifted {
            level,
            previous_branch: previous,
            next_branch: next,
            wrapped: next == 0,
        },
        |state| shift_state(state, config, level, next),
    )
}

fn shift_state(
    state: &mut ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
    level: usize,
    next_branch: usize,
) -> Result<(), ShiftedTreeChainError> {
    state.levels[level].active_branch = next_branch;
    state.metrics.shifts[level] = checked_increment(state.metrics.shifts[level])?;
    if level < config.depth {
        let next_graph = recursive_graph(&state.levels[level], next_branch)?.clone();
        let deeper_start = level
            .checked_add(1)
            .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
        let suffix = build_suffix(next_graph, deeper_start, config, &mut state.metrics)?;
        record_reinitialized_suffix(&mut state.metrics, suffix.len())?;
        state.levels.truncate(deeper_start);
        state.levels.extend(suffix);
    }
    state.stage = ShiftedTreeChainStage::Shifted;
    Ok(())
}

fn apply_rebuild(
    snapshot: &mut ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
    level: usize,
    events: &mut Vec<ShiftedTreeChainTraceEvent>,
    record: bool,
) -> Result<(), ShiftedTreeChainError> {
    transition(
        snapshot,
        events,
        record,
        ShiftedTreeChainEventKind::Rebuilt { level },
        |state| rebuild_state(state, config, level),
    )
}

fn rebuild_state(
    state: &mut ShiftedTreeChainSnapshot,
    config: ShiftedTreeChainConfig,
    level: usize,
) -> Result<(), ShiftedTreeChainError> {
    let input = if level == 0 {
        state.levels[0].graph.clone()
    } else {
        let parent = state
            .levels
            .get(level - 1)
            .ok_or(ShiftedTreeChainError::InvalidInput)?;
        recursive_graph(parent, parent.active_branch)?.clone()
    };
    let suffix = build_suffix(input, level, config, &mut state.metrics)?;
    record_reinitialized_suffix(&mut state.metrics, suffix.len())?;
    state.metrics.rebuilds[level] = checked_increment(state.metrics.rebuilds[level])?;
    state.levels.truncate(level);
    state.levels.extend(suffix);
    state.stage = ShiftedTreeChainStage::Rebuilt;
    Ok(())
}

fn record_reinitialized_suffix(
    metrics: &mut ShiftedTreeChainMetrics,
    levels: usize,
) -> Result<(), ShiftedTreeChainError> {
    let levels = u64::try_from(levels).map_err(|_| ShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.suffix_reinitializations = metrics
        .suffix_reinitializations
        .checked_add(levels)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    Ok(())
}

fn transition(
    snapshot: &mut ShiftedTreeChainSnapshot,
    events: &mut Vec<ShiftedTreeChainTraceEvent>,
    record: bool,
    kind: ShiftedTreeChainEventKind,
    update: impl FnOnce(&mut ShiftedTreeChainSnapshot) -> Result<(), ShiftedTreeChainError>,
) -> Result<(), ShiftedTreeChainError> {
    let before = snapshot.clone();
    update(snapshot)?;
    snapshot.metrics.state_transitions = checked_increment(snapshot.metrics.state_transitions)?;
    if record {
        if events.len() >= SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS {
            return Err(ShiftedTreeChainError::AdmissionLimit);
        }
        events.push(ShiftedTreeChainTraceEvent {
            catalog_id: CATALOG_ID,
            kind,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(())
}

fn build_suffix(
    mut graph: ShiftedTreeChainGraph,
    start_level: usize,
    config: ShiftedTreeChainConfig,
    metrics: &mut ShiftedTreeChainMetrics,
) -> Result<Vec<ShiftedTreeChainLevel>, ShiftedTreeChainError> {
    let capacity = config
        .depth
        .checked_sub(start_level)
        .and_then(|remaining| remaining.checked_add(1))
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    let mut levels = Vec::with_capacity(capacity);
    for level in start_level..=config.depth {
        let terminal = level == config.depth;
        let built = build_level(graph, level, terminal, config.branches, metrics)?;
        graph = if terminal {
            built.graph.clone()
        } else {
            recursive_graph(&built, 0)?.clone()
        };
        levels.push(built);
    }
    Ok(levels)
}

fn build_level(
    graph: ShiftedTreeChainGraph,
    level: usize,
    terminal: bool,
    branch_count: usize,
    metrics: &mut ShiftedTreeChainMetrics,
) -> Result<ShiftedTreeChainLevel, ShiftedTreeChainError> {
    let mwu = build_low_stretch_forest_mwu_collection(
        &graph,
        LowStretchForestMwuConfig {
            rounds: branch_count,
        },
    )
    .map_err(|_| ShiftedTreeChainError::InvariantViolation)?;
    let mwu_metrics = mwu.final_snapshot.metrics;
    metrics.tree_subsets_inspected = metrics
        .tree_subsets_inspected
        .checked_add(mwu_metrics.tree_subsets_inspected)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.lsf_candidates_enumerated = metrics
        .lsf_candidates_enumerated
        .checked_add(mwu_metrics.candidates_enumerated)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.lsf_candidate_scores = metrics
        .lsf_candidate_scores
        .checked_add(mwu_metrics.candidate_scores)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.lsf_exponential_refinements = metrics
        .lsf_exponential_refinements
        .checked_add(mwu_metrics.exponential_refinements)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.level_initializations = checked_increment(metrics.level_initializations)?;
    let mut branches = Vec::with_capacity(branch_count);
    for selected in &mwu.branches {
        let tree_mask = selected.tree_mask;
        let recursive = if terminal {
            None
        } else {
            Some(build_recursive_branch(&graph, selected, branch_count)?)
        };
        branches.push(ShiftedTreeChainBranch {
            tree_mask,
            recursive,
        });
    }
    Ok(ShiftedTreeChainLevel {
        level,
        graph,
        active_branch: 0,
        branches,
    })
}

fn build_recursive_branch(
    graph: &ShiftedTreeChainGraph,
    selected: &LowStretchForestMwuBranch,
    branch_count: usize,
) -> Result<ShiftedTreeChainRecursiveBranch, ShiftedTreeChainError> {
    let tree_mask = selected.tree_mask;
    let forest_mask = selected.forest_mask;
    let roots = selected.roots.clone();
    let stretches = selected.stretch_overestimates.clone();
    let core = contract_core(graph, tree_mask, forest_mask, &stretches)?;
    let sparsified = sparsify_core(&core, branch_count)?;
    Ok(ShiftedTreeChainRecursiveBranch {
        forest_mask,
        roots,
        stretch_overestimates: stretches,
        sparsified_core_graph: sparsified.graph,
        core_graph: core,
        core_to_spanner: sparsified.embedding,
        sparsify_certificates: sparsified.certificates,
        gamma_length: sparsified.gamma_length,
        gamma_congestion: sparsified.gamma_congestion,
    })
}

struct SparsifiedCore {
    graph: ShiftedTreeChainGraph,
    embedding: Vec<Vec<ShiftedTreeChainEmbeddingArc>>,
    certificates: Vec<DeterministicSpannerSparsifyCertificate>,
    gamma_length: usize,
    gamma_congestion: usize,
}

fn sparsify_core(
    core: &ShiftedTreeChainGraph,
    branch_count: usize,
) -> Result<SparsifiedCore, ShiftedTreeChainError> {
    let mut selected = BTreeSet::new();
    let mut source_embedding = vec![Vec::new(); core.edges.len()];
    let mut certificates = Vec::new();
    for bucket in static_length_buckets(core) {
        let input_edges = bucket
            .iter()
            .map(|&edge| DeterministicSpannerInputEdge {
                edge,
                from: core.edges[edge].from,
                to: core.edges[edge].to,
            })
            .collect::<Vec<_>>();
        let sparse =
            bounded_deterministic_sparsify(core.node_count, core.edges.len(), &input_edges)
                .map_err(|_| ShiftedTreeChainError::InvariantViolation)?;
        selected.extend(sparse.selected_edges);
        for &edge in &bucket {
            source_embedding[edge].clone_from(&sparse.embedding[edge]);
        }
        certificates.push(sparse.certificate);
    }
    let selected = selected.into_iter().collect::<Vec<_>>();
    let mut spanner_index = vec![None; core.edges.len()];
    for (index, &core_index) in selected.iter().enumerate() {
        spanner_index[core_index] = Some(index);
    }
    let spanner = ShiftedTreeChainGraph {
        node_count: core.node_count,
        edges: selected
            .iter()
            .map(|&index| core.edges[index].clone())
            .collect(),
    };
    let mut congestion = vec![0_usize; spanner.edges.len()];
    let mut embedding = vec![Vec::new(); core.edges.len()];
    for (core_edge, path) in source_embedding.iter().enumerate() {
        for arc in path {
            let mapped =
                spanner_index[arc.edge].ok_or(ShiftedTreeChainError::InvariantViolation)?;
            congestion[mapped] = congestion[mapped]
                .checked_add(1)
                .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?;
            embedding[core_edge].push(ShiftedTreeChainEmbeddingArc {
                edge: mapped,
                direction: arc.direction,
            });
        }
    }
    let sparsity_gamma = ceil_div(
        branch_count
            .checked_mul(spanner.edges.len())
            .ok_or(ShiftedTreeChainError::ArithmeticOverflow)?,
        core.edges.len(),
    )?
    .max(1);
    let maximum_path_length = embedding.iter().map(Vec::len).max().unwrap_or(0);
    let gamma_length = sparsity_gamma.max(maximum_path_length).max(1);
    let gamma_congestion =
        ceil_div(congestion.into_iter().max().unwrap_or(0), branch_count)?.max(1);
    Ok(SparsifiedCore {
        graph: spanner,
        embedding,
        certificates,
        gamma_length,
        gamma_congestion,
    })
}

fn static_length_buckets(core: &ShiftedTreeChainGraph) -> Vec<Vec<usize>> {
    let mut edges = core
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.from != edge.to)
        .map(|(index, edge)| (edge.length.clone(), edge.source_edge, index))
        .collect::<Vec<_>>();
    edges.sort();
    let mut buckets = Vec::new();
    let mut cursor = 0;
    while cursor < edges.len() {
        let upper = &edges[cursor].0 * BigInt::from(2_u8);
        let mut end = cursor + 1;
        while end < edges.len() && edges[end].0 <= upper {
            end += 1;
        }
        let mut bucket = edges[cursor..end]
            .iter()
            .map(|(_, _, edge)| *edge)
            .collect::<Vec<_>>();
        bucket.sort_unstable();
        buckets.push(bucket);
        cursor = end;
    }
    buckets
}

fn ceil_div(numerator: usize, denominator: usize) -> Result<usize, ShiftedTreeChainError> {
    if denominator == 0 {
        return Err(ShiftedTreeChainError::InvariantViolation);
    }
    numerator
        .checked_add(denominator - 1)
        .and_then(|value| value.checked_div(denominator))
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)
}

fn contract_core(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    forest_mask: u64,
    stretches: &[BigRational],
) -> Result<ShiftedTreeChainGraph, ShiftedTreeChainError> {
    let roots = forest_roots(graph, forest_mask)?;
    let mut distinct = roots.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let components = roots
        .iter()
        .map(|root| {
            distinct
                .binary_search(root)
                .map_err(|_| ShiftedTreeChainError::InvariantViolation)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut edges = Vec::with_capacity(graph.edges.len());
    for (index, edge) in graph.edges.iter().enumerate() {
        let path = tree_path(graph, tree_mask, edge.to, edge.from)?
            .ok_or(ShiftedTreeChainError::InvariantViolation)?;
        let path_gradient = signed_path_sum(graph, &path, |item| item.gradient.clone());
        edges.push(ShiftedTreeChainEdge {
            source_edge: edge.source_edge,
            from: components[edge.from],
            to: components[edge.to],
            length: &stretches[index] * &edge.length,
            gradient: &edge.gradient + path_gradient,
        });
    }
    Ok(ShiftedTreeChainGraph {
        node_count: distinct.len(),
        edges,
    })
}

fn recursive_graph(
    level: &ShiftedTreeChainLevel,
    branch: usize,
) -> Result<&ShiftedTreeChainGraph, ShiftedTreeChainError> {
    level
        .branches
        .get(branch)
        .and_then(|item| item.recursive.as_ref())
        .map(|recursive| &recursive.sparsified_core_graph)
        .ok_or(ShiftedTreeChainError::InvariantViolation)
}

fn forest_roots(
    graph: &ShiftedTreeChainGraph,
    forest_mask: u64,
) -> Result<Vec<usize>, ShiftedTreeChainError> {
    let mut dsu = DisjointSet::new(graph.node_count);
    for (index, edge) in graph.edges.iter().enumerate() {
        if forest_mask & (1_u64 << index) != 0
            && (edge.from == edge.to || !dsu.union(edge.from, edge.to))
        {
            return Err(ShiftedTreeChainError::InvariantViolation);
        }
    }
    let mut minimum = vec![usize::MAX; graph.node_count];
    for node in 0..graph.node_count {
        let component = dsu.find(node);
        minimum[component] = minimum[component].min(node);
    }
    Ok((0..graph.node_count)
        .map(|node| {
            let component = dsu.find(node);
            minimum[component]
        })
        .collect())
}

fn signed_path_sum(
    graph: &ShiftedTreeChainGraph,
    path: &[ShiftedTreeChainEmbeddingArc],
    value: impl Fn(&ShiftedTreeChainEdge) -> BigRational,
) -> BigRational {
    path.iter().fold(BigRational::zero(), |sum, arc| {
        sum + value(&graph.edges[arc.edge]) * BigInt::from(arc.direction)
    })
}

fn tree_path(
    graph: &ShiftedTreeChainGraph,
    mask: u64,
    start: usize,
    target: usize,
) -> Result<Option<Vec<ShiftedTreeChainEmbeddingArc>>, ShiftedTreeChainError> {
    if start == target {
        return Ok(Some(Vec::new()));
    }
    let mut previous = vec![None; graph.node_count];
    let mut seen = vec![false; graph.node_count];
    let mut queue = VecDeque::new();
    seen[start] = true;
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        visit_tree_neighbors(graph, mask, node, &mut seen, &mut previous, &mut queue);
    }
    if !seen[target] {
        return Ok(None);
    }
    let mut reversed = Vec::new();
    let mut cursor = target;
    while cursor != start {
        let (parent, edge, direction) =
            previous[cursor].ok_or(ShiftedTreeChainError::InvariantViolation)?;
        reversed.push(ShiftedTreeChainEmbeddingArc { edge, direction });
        cursor = parent;
    }
    reversed.reverse();
    Ok(Some(reversed))
}

fn visit_tree_neighbors(
    graph: &ShiftedTreeChainGraph,
    mask: u64,
    node: usize,
    seen: &mut [bool],
    previous: &mut [Option<(usize, usize, i8)>],
    queue: &mut VecDeque<usize>,
) {
    for (index, edge) in graph.edges.iter().enumerate() {
        if mask & (1_u64 << index) == 0 || edge.from == edge.to {
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

fn validate_input(
    graph: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
) -> Result<(), ShiftedTreeChainError> {
    if graph.node_count == 0 || graph.edges.is_empty() || config.depth == 0 || config.branches == 0
    {
        return Err(ShiftedTreeChainError::InvalidInput);
    }
    if graph.node_count > SHIFTED_TREE_CHAIN_MAX_NODES
        || graph.edges.len() > SHIFTED_TREE_CHAIN_MAX_EDGES
        || config.depth > SHIFTED_TREE_CHAIN_MAX_DEPTH
        || config.branches > SHIFTED_TREE_CHAIN_MAX_BRANCHES
        || operations.len() > SHIFTED_TREE_CHAIN_MAX_OPERATIONS
        || graph.edges.iter().any(rational_too_wide)
    {
        return Err(ShiftedTreeChainError::AdmissionLimit);
    }
    let malformed_edge = graph.edges.iter().enumerate().any(|(index, edge)| {
        edge.source_edge != index
            || edge.from >= graph.node_count
            || edge.to >= graph.node_count
            || edge.length <= BigRational::zero()
    });
    let malformed_operation = operations.iter().any(|operation| match operation {
        ShiftedTreeChainOperation::Shift { level }
        | ShiftedTreeChainOperation::Rebuild { level } => *level > config.depth,
    });
    if malformed_edge || malformed_operation || config.branches > graph.edges.len() {
        return Err(ShiftedTreeChainError::InvalidInput);
    }
    if !underlying_connected(graph) {
        return Err(ShiftedTreeChainError::DisconnectedLevel);
    }
    Ok(())
}

fn rational_too_wide(edge: &ShiftedTreeChainEdge) -> bool {
    edge.length.numer().bits() > SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
        || edge.length.denom().bits() > SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
        || edge.gradient.numer().bits() > SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
        || edge.gradient.denom().bits() > SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
}

fn underlying_connected(graph: &ShiftedTreeChainGraph) -> bool {
    let mut seen = vec![false; graph.node_count];
    let mut queue = VecDeque::new();
    seen[0] = true;
    queue.push_back(0);
    while let Some(node) = queue.pop_front() {
        for edge in &graph.edges {
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
    seen.into_iter().all(|visited| visited)
}

fn audit_snapshot(
    original: &ShiftedTreeChainGraph,
    config: ShiftedTreeChainConfig,
    snapshot: &ShiftedTreeChainSnapshot,
) -> Result<(), ShiftedTreeChainError> {
    let level_count = config
        .depth
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    if snapshot.levels.len() != level_count
        || snapshot.metrics.shifts.len() != level_count
        || snapshot.metrics.rebuilds.len() != level_count
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    let mut expected_graph = original.clone();
    let mut base_work = LowStretchForestMwuChainWork::default();
    for (level_index, level) in snapshot.levels.iter().enumerate() {
        audit_level(config, level_index, &expected_graph, level, &mut base_work)?;
        if level_index < config.depth {
            expected_graph = recursive_graph(level, level.active_branch)
                .map_err(|_| ShiftedTreeChainError::TraceVerification)?
                .clone();
        }
    }
    let level_count_u64 =
        u64::try_from(level_count).map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    if snapshot.metrics.level_initializations
        != level_count_u64
            .checked_add(snapshot.metrics.suffix_reinitializations)
            .ok_or(ShiftedTreeChainError::TraceVerification)?
        || snapshot.metrics.tree_subsets_inspected < base_work.tree_subsets_inspected
        || snapshot.metrics.lsf_candidates_enumerated < base_work.candidates_enumerated
        || snapshot.metrics.lsf_candidate_scores < base_work.candidate_scores
        || snapshot.metrics.lsf_exponential_refinements < base_work.exponential_refinements
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    Ok(())
}

fn base_metrics_are_exact(
    config: ShiftedTreeChainConfig,
    snapshot: &ShiftedTreeChainSnapshot,
) -> Result<bool, ShiftedTreeChainError> {
    let level_count = config
        .depth
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_initializations =
        u64::try_from(level_count).map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    let expected_work = audit_levels_mwu_work(config, &snapshot.levels)?;
    Ok(
        snapshot.metrics.level_initializations == expected_initializations
            && snapshot.metrics.tree_subsets_inspected == expected_work.tree_subsets_inspected
            && snapshot.metrics.lsf_candidates_enumerated == expected_work.candidates_enumerated
            && snapshot.metrics.lsf_candidate_scores == expected_work.candidate_scores
            && snapshot.metrics.lsf_exponential_refinements
                == expected_work.exponential_refinements
            && snapshot.metrics.suffix_reinitializations == 0
            && snapshot.metrics.state_transitions == 0
            && snapshot.metrics.shifts.iter().all(|&count| count == 0)
            && snapshot.metrics.rebuilds.iter().all(|&count| count == 0),
    )
}

fn audit_levels_mwu_work(
    config: ShiftedTreeChainConfig,
    levels: &[ShiftedTreeChainLevel],
) -> Result<LowStretchForestMwuChainWork, ShiftedTreeChainError> {
    let mut total = LowStretchForestMwuChainWork::default();
    for level in levels {
        let (_, work) = audit_low_stretch_forest_mwu_for_chain(&level.graph, config.branches)
            .map_err(|_| ShiftedTreeChainError::TraceVerification)?;
        add_audit_work(&mut total, work)?;
    }
    Ok(total)
}

fn add_audit_work(
    total: &mut LowStretchForestMwuChainWork,
    level: LowStretchForestMwuChainWork,
) -> Result<(), ShiftedTreeChainError> {
    total.tree_subsets_inspected = total
        .tree_subsets_inspected
        .checked_add(level.tree_subsets_inspected)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    total.candidates_enumerated = total
        .candidates_enumerated
        .checked_add(level.candidates_enumerated)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    total.candidate_scores = total
        .candidate_scores
        .checked_add(level.candidate_scores)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    total.exponential_refinements = total
        .exponential_refinements
        .checked_add(level.exponential_refinements)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn audit_level(
    config: ShiftedTreeChainConfig,
    level_index: usize,
    expected_graph: &ShiftedTreeChainGraph,
    level: &ShiftedTreeChainLevel,
    work: &mut LowStretchForestMwuChainWork,
) -> Result<(), ShiftedTreeChainError> {
    if level.level != level_index
        || &level.graph != expected_graph
        || level.active_branch >= config.branches
        || level.branches.len() != config.branches
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    let (selected, level_work) =
        audit_low_stretch_forest_mwu_for_chain(&level.graph, config.branches)
            .map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    add_audit_work(work, level_work)?;
    for (branch, expected) in level.branches.iter().zip(&selected) {
        if branch.tree_mask != expected.tree_mask {
            return Err(ShiftedTreeChainError::TraceVerification);
        }
        if level_index == config.depth {
            if branch.recursive.is_some() {
                return Err(ShiftedTreeChainError::TraceVerification);
            }
        } else {
            audit_recursive_branch(
                &level.graph,
                expected,
                config.branches,
                branch
                    .recursive
                    .as_ref()
                    .ok_or(ShiftedTreeChainError::TraceVerification)?,
            )?;
        }
    }
    Ok(())
}

fn audit_recursive_branch(
    graph: &ShiftedTreeChainGraph,
    selected: &LowStretchForestMwuBranch,
    branch_count: usize,
    actual: &ShiftedTreeChainRecursiveBranch,
) -> Result<(), ShiftedTreeChainError> {
    let tree_mask = selected.tree_mask;
    let forest = selected.forest_mask;
    let roots = selected.roots.clone();
    let stretches = selected.stretch_overestimates.clone();
    let core = contract_core(graph, tree_mask, forest, &stretches)
        .map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    let sparsified =
        sparsify_core(&core, branch_count).map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    if actual.forest_mask != forest
        || actual.roots != roots
        || actual.stretch_overestimates != stretches
        || actual.core_graph != core
        || actual.sparsified_core_graph != sparsified.graph
        || actual.core_to_spanner != sparsified.embedding
        || actual.sparsify_certificates != sparsified.certificates
        || actual.gamma_length != sparsified.gamma_length
        || actual.gamma_congestion != sparsified.gamma_congestion
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    audit_sparsified_core(branch_count, actual)
}

fn audit_sparsified_core(
    branches: usize,
    actual: &ShiftedTreeChainRecursiveBranch,
) -> Result<(), ShiftedTreeChainError> {
    let core = &actual.core_graph;
    let spanner = &actual.sparsified_core_graph;
    let edge_bound_holds = spanner
        .edges
        .len()
        .checked_mul(branches)
        .zip(core.edges.len().checked_mul(actual.gamma_length))
        .is_some_and(|(left, right)| left <= right);
    if !edge_bound_holds
        || actual.gamma_length == 0
        || actual.gamma_congestion < 1
        || actual.core_to_spanner.len() != core.edges.len()
        || spanner.node_count != core.node_count
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    for spanner_edge in &spanner.edges {
        let Some(core_edge) = core
            .edges
            .iter()
            .find(|edge| edge.source_edge == spanner_edge.source_edge)
        else {
            return Err(ShiftedTreeChainError::TraceVerification);
        };
        if core_edge != spanner_edge {
            return Err(ShiftedTreeChainError::TraceVerification);
        }
    }
    let mut congestion = vec![0_usize; spanner.edges.len()];
    for (core_edge, path) in core.edges.iter().zip(&actual.core_to_spanner) {
        if path.len() > actual.gamma_length {
            return Err(ShiftedTreeChainError::TraceVerification);
        }
        let mut cursor = core_edge.from;
        for arc in path {
            let edge = spanner
                .edges
                .get(arc.edge)
                .ok_or(ShiftedTreeChainError::TraceVerification)?;
            let next = match arc.direction {
                1 if edge.from == cursor => edge.to,
                -1 if edge.to == cursor => edge.from,
                _ => return Err(ShiftedTreeChainError::TraceVerification),
            };
            let shorter = core_edge.length.clone().min(edge.length.clone());
            let longer = core_edge.length.clone().max(edge.length.clone());
            if longer > shorter * BigInt::from(2_u8) {
                return Err(ShiftedTreeChainError::TraceVerification);
            }
            congestion[arc.edge] = congestion[arc.edge]
                .checked_add(1)
                .ok_or(ShiftedTreeChainError::TraceVerification)?;
            cursor = next;
        }
        if cursor != core_edge.to {
            return Err(ShiftedTreeChainError::TraceVerification);
        }
    }
    let congestion_bound = branches
        .checked_mul(actual.gamma_congestion)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    if congestion.into_iter().any(|value| value > congestion_bound) {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    Ok(())
}

fn validate_transition(
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
    operation_index: &mut usize,
    event: &ShiftedTreeChainTraceEvent,
) -> Result<(), ShiftedTreeChainError> {
    let before = &event.before;
    let after = &event.after;
    let expected_transition = before
        .metrics
        .state_transitions
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    if after.metrics.state_transitions != expected_transition {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    match &event.kind {
        ShiftedTreeChainEventKind::Shifted {
            level,
            previous_branch,
            next_branch,
            wrapped,
        } => validate_shift_transition(
            config,
            operations,
            operation_index,
            before,
            after,
            (*level, *previous_branch, *next_branch, *wrapped),
        ),
        ShiftedTreeChainEventKind::Rebuilt { level } => {
            validate_rebuild_transition(config, operations, operation_index, before, after, *level)
        }
        ShiftedTreeChainEventKind::Completed => {
            if *operation_index != operations.len()
                || after.stage != ShiftedTreeChainStage::Complete
                || before.levels != after.levels
                || metrics_except_transitions(before) != metrics_except_transitions(after)
            {
                return Err(ShiftedTreeChainError::TraceVerification);
            }
            Ok(())
        }
    }
}

fn validate_shift_transition(
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
    operation_index: &mut usize,
    before: &ShiftedTreeChainSnapshot,
    after: &ShiftedTreeChainSnapshot,
    declared: (usize, usize, usize, bool),
) -> Result<(), ShiftedTreeChainError> {
    let (level, previous, next, wrapped) = declared;
    let before_level = before
        .levels
        .get(level)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let after_level = after
        .levels
        .get(level)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_next = previous
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?
        % config.branches;
    if operations.get(*operation_index) != Some(&ShiftedTreeChainOperation::Shift { level })
        || before_level.active_branch != previous
        || after_level.active_branch != next
        || next != expected_next
        || wrapped != (next == 0)
        || after.stage != ShiftedTreeChainStage::Shifted
        || before.levels.get(..level) != after.levels.get(..level)
        || metrics_changed_wrongly(config, before, after, level, true)?
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    *operation_index = operation_index
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn validate_rebuild_transition(
    config: ShiftedTreeChainConfig,
    operations: &[ShiftedTreeChainOperation],
    operation_index: &mut usize,
    before: &ShiftedTreeChainSnapshot,
    after: &ShiftedTreeChainSnapshot,
    level: usize,
) -> Result<(), ShiftedTreeChainError> {
    let suffix = after
        .levels
        .get(level..)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    if operations.get(*operation_index) != Some(&ShiftedTreeChainOperation::Rebuild { level })
        || after.stage != ShiftedTreeChainStage::Rebuilt
        || before.levels.get(..level) != after.levels.get(..level)
        || suffix.iter().any(|current| current.active_branch != 0)
        || metrics_changed_wrongly(config, before, after, level, false)?
    {
        return Err(ShiftedTreeChainError::TraceVerification);
    }
    *operation_index = operation_index
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn metrics_changed_wrongly(
    config: ShiftedTreeChainConfig,
    before: &ShiftedTreeChainSnapshot,
    after: &ShiftedTreeChainSnapshot,
    level: usize,
    shift: bool,
) -> Result<bool, ShiftedTreeChainError> {
    let first_reinitialized = if shift {
        level
            .checked_add(1)
            .ok_or(ShiftedTreeChainError::TraceVerification)?
    } else {
        level
    };
    let suffix_len = after
        .levels
        .len()
        .checked_sub(first_reinitialized)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let suffix_len =
        u64::try_from(suffix_len).map_err(|_| ShiftedTreeChainError::TraceVerification)?;
    let expected_initializations = before
        .metrics
        .level_initializations
        .checked_add(suffix_len)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_reinitializations = before
        .metrics
        .suffix_reinitializations
        .checked_add(suffix_len)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let suffix = after
        .levels
        .get(first_reinitialized..)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let work_delta = audit_levels_mwu_work(config, suffix)?;
    let expected_subsets = before
        .metrics
        .tree_subsets_inspected
        .checked_add(work_delta.tree_subsets_inspected)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_candidates = before
        .metrics
        .lsf_candidates_enumerated
        .checked_add(work_delta.candidates_enumerated)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_scores = before
        .metrics
        .lsf_candidate_scores
        .checked_add(work_delta.candidate_scores)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    let expected_refinements = before
        .metrics
        .lsf_exponential_refinements
        .checked_add(work_delta.exponential_refinements)
        .ok_or(ShiftedTreeChainError::TraceVerification)?;
    Ok(
        after.metrics.level_initializations != expected_initializations
            || after.metrics.suffix_reinitializations != expected_reinitializations
            || after.metrics.tree_subsets_inspected != expected_subsets
            || after.metrics.lsf_candidates_enumerated != expected_candidates
            || after.metrics.lsf_candidate_scores != expected_scores
            || after.metrics.lsf_exponential_refinements != expected_refinements
            || !counter_vector_changed_once(
                &before.metrics.shifts,
                &after.metrics.shifts,
                level,
                shift,
            )?
            || !counter_vector_changed_once(
                &before.metrics.rebuilds,
                &after.metrics.rebuilds,
                level,
                !shift,
            )?,
    )
}

fn counter_vector_changed_once(
    before: &[u64],
    after: &[u64],
    level: usize,
    increment_selected: bool,
) -> Result<bool, ShiftedTreeChainError> {
    if before.len() != after.len() || level >= before.len() {
        return Ok(false);
    }
    for (index, (&old, &new)) in before.iter().zip(after).enumerate() {
        let expected = if increment_selected && index == level {
            old.checked_add(1)
                .ok_or(ShiftedTreeChainError::TraceVerification)?
        } else {
            old
        };
        if new != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn metrics_except_transitions(
    snapshot: &ShiftedTreeChainSnapshot,
) -> (u64, u64, u64, u64, u64, &[u64], &[u64], u64) {
    (
        snapshot.metrics.tree_subsets_inspected,
        snapshot.metrics.lsf_candidates_enumerated,
        snapshot.metrics.lsf_candidate_scores,
        snapshot.metrics.lsf_exponential_refinements,
        snapshot.metrics.level_initializations,
        &snapshot.metrics.shifts,
        &snapshot.metrics.rebuilds,
        snapshot.metrics.suffix_reinitializations,
    )
}

fn checked_increment(value: u64) -> Result<u64, ShiftedTreeChainError> {
    value
        .checked_add(1)
        .ok_or(ShiftedTreeChainError::ArithmeticOverflow)
}

#[derive(Clone, Debug)]
struct DisjointSet {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl DisjointSet {
    fn new(nodes: usize) -> Self {
        Self {
            parent: (0..nodes).collect(),
            size: vec![1; nodes],
        }
    }

    fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            self.parent[node] = self.find(self.parent[node]);
        }
        self.parent[node]
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        if self.size[left_root] < self.size[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        self.size[left_root] += self.size[right_root];
        true
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_traits::One;

    use super::*;

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

    #[test]
    fn constructs_real_forests_cores_spanners_and_embeddings() {
        let result = execute_shifted_tree_chain(&triangle(), config(), &[]).expect("chain");
        assert_eq!(result.final_snapshot.levels.len(), 3);
        let mwu = build_low_stretch_forest_mwu_collection(
            &triangle(),
            LowStretchForestMwuConfig { rounds: 2 },
        )
        .expect("MWU");
        assert_eq!(
            result.final_snapshot.levels[0]
                .branches
                .iter()
                .map(|branch| branch.tree_mask)
                .collect::<Vec<_>>(),
            mwu.branches
                .iter()
                .map(|branch| branch.tree_mask)
                .collect::<Vec<_>>()
        );
        assert!(result.final_snapshot.metrics.lsf_candidate_scores > 0);
        assert!(result.final_snapshot.metrics.lsf_exponential_refinements > 0);
        let branch = result.final_snapshot.levels[0].branches[0]
            .recursive
            .as_ref()
            .expect("recursive branch");
        assert_eq!(branch.forest_mask, 0);
        assert_eq!(branch.roots, vec![0, 1, 2]);
        assert_eq!(branch.gamma_length, 2);
        assert_eq!(branch.gamma_congestion, 1);
        assert_eq!(branch.sparsified_core_graph.edges.len(), 3);
        assert!(!branch.sparsify_certificates.is_empty());
        assert_eq!(
            branch.sparsified_core_graph.edges.len(),
            branch.core_graph.edges.len()
        );
        assert_eq!(branch.core_to_spanner.len(), triangle().edges.len());
        assert_eq!(
            branch
                .core_graph
                .edges
                .iter()
                .map(|edge| edge.length.clone())
                .collect::<Vec<_>>(),
            vec![rational(10), rational(18), rational(24)]
        );
        assert_eq!(
            branch
                .core_graph
                .edges
                .iter()
                .map(|edge| edge.gradient.clone())
                .collect::<Vec<_>>(),
            vec![rational(0), rational(0), rational(5)]
        );
        assert!(
            branch
                .stretch_overestimates
                .iter()
                .all(|value| value >= &BigRational::one())
        );
    }

    #[test]
    fn shift_rebuilds_only_deeper_suffix_and_wraps() {
        let operations = [
            ShiftedTreeChainOperation::Shift { level: 0 },
            ShiftedTreeChainOperation::Shift { level: 0 },
        ];
        let trace = trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        assert_eq!(trace.result.active_branches, vec![0, 0, 0]);
        assert_eq!(trace.result.final_snapshot.metrics.shifts, vec![2, 0, 0]);
        assert_eq!(
            trace.result.final_snapshot.metrics.suffix_reinitializations,
            4
        );
        let wraps = trace
            .events
            .iter()
            .filter_map(|event| match event.kind {
                ShiftedTreeChainEventKind::Shifted { wrapped, .. } => Some(wrapped),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(wraps, vec![false, true]);
    }

    #[test]
    fn rebuild_resets_suffix_and_preserves_shallower_level() {
        let operations = [
            ShiftedTreeChainOperation::Shift { level: 0 },
            ShiftedTreeChainOperation::Shift { level: 1 },
            ShiftedTreeChainOperation::Rebuild { level: 1 },
        ];
        let trace = trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        assert_eq!(trace.result.active_branches, vec![1, 0, 0]);
        assert_eq!(trace.result.final_snapshot.metrics.rebuilds, vec![0, 1, 0]);
        let rebuild = trace
            .events
            .iter()
            .find(|event| matches!(event.kind, ShiftedTreeChainEventKind::Rebuilt { .. }))
            .expect("rebuild");
        assert_eq!(rebuild.before.levels[0], rebuild.after.levels[0]);
    }

    #[test]
    fn fast_and_trace_terminal_states_match() {
        let operations = [
            ShiftedTreeChainOperation::Shift { level: 1 },
            ShiftedTreeChainOperation::Rebuild { level: 0 },
            ShiftedTreeChainOperation::Shift { level: 2 },
        ];
        let fast = execute_shifted_tree_chain(&triangle(), config(), &operations).expect("fast");
        let trace = trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        assert_eq!(fast, trace.result);
    }

    #[test]
    fn checker_rejects_core_and_shift_tampering() {
        let operations = [ShiftedTreeChainOperation::Shift { level: 0 }];
        let mut trace =
            trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        trace.base_snapshot.levels[0].branches[0]
            .recursive
            .as_mut()
            .expect("recursive")
            .core_graph
            .edges[0]
            .gradient += rational(1);
        assert_eq!(
            check_shifted_tree_chain_trace(&triangle(), config(), &operations, &trace),
            Err(ShiftedTreeChainError::TraceVerification)
        );

        let mut trace =
            trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        trace.base_snapshot.levels[0].branches[0]
            .recursive
            .as_mut()
            .expect("recursive")
            .sparsify_certificates[0]
            .phi += rational(1);
        assert_eq!(
            check_shifted_tree_chain_trace(&triangle(), config(), &operations, &trace),
            Err(ShiftedTreeChainError::TraceVerification)
        );

        let mut trace =
            trace_shifted_tree_chain(&triangle(), config(), &operations).expect("trace");
        let event = trace.events.first_mut().expect("shift event");
        let ShiftedTreeChainEventKind::Shifted { next_branch, .. } = &mut event.kind else {
            panic!("shift event");
        };
        *next_branch = 0;
        assert_eq!(
            check_shifted_tree_chain_trace(&triangle(), config(), &operations, &trace),
            Err(ShiftedTreeChainError::TraceVerification)
        );
    }

    #[test]
    fn rejects_disconnected_and_nonpositive_length_graphs() {
        let mut graph = triangle();
        graph.edges.truncate(1);
        let disconnected_config = ShiftedTreeChainConfig {
            depth: 2,
            branches: 1,
        };
        assert_eq!(
            execute_shifted_tree_chain(&graph, disconnected_config, &[]),
            Err(ShiftedTreeChainError::DisconnectedLevel)
        );
        let mut graph = triangle();
        graph.edges[0].length = BigRational::zero();
        assert_eq!(
            execute_shifted_tree_chain(&graph, config(), &[]),
            Err(ShiftedTreeChainError::InvalidInput)
        );
    }
}
