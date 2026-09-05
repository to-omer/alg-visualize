//! Certified small-graph multiplicative-weights LSF collection.
//!
//! This module implements the multiplicative update in the proof of Lemma 5.5
//! of van den Brand et al., "A Deterministic Almost-Linear Time Algorithm for
//! Minimum-Cost Flow" (arXiv:2309.16629v1): `v_1 = 1`, an LSF is selected for
//! the current weights, and
//! `v_{i+1,e} = v_{i,e} exp(wstr^i_e / rho)` for exactly `k` rounds.
//!
//! The source calls the dynamic low-stretch-forest data structure of Lemma 5.4
//! as a black box. In each round this bounded realization first constructs the
//! source normalized copy multiplicities
//! `ceil(m v_e / ||v||_1)`, selects an exact minimum-stretch spanning tree of
//! that copy multigraph, measures its average stretch, adds the endpoints of
//! explicitly classified large-stretch edges to the root seeds, and refines
//! the seeds until every final piece has at most `k` original edges adjacent
//! to non-root vertices. The selected tree is then materialized as its HLD LSF.
//! Exponential comparisons never use floating point: exact rational Taylor
//! intervals certify every copy-count boundary. This preserves the source
//! MWU -> weighted LSST -> decomposition seeds -> LSF order. The source hides
//! decomposition constants in asymptotic notation, so this bounded realization
//! publishes a stronger finite-volume contract and makes no recourse or
//! asymptotic-runtime claim.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Zero};
use thiserror::Error;

use super::{
    DynamicLowStretchForestEdge, DynamicLowStretchForestError, DynamicLowStretchForestInput,
    DynamicLowStretchForestSnapshot, ShiftedTreeChainEdge, ShiftedTreeChainGraph,
    execute_dynamic_low_stretch_forest, trace_dynamic_low_stretch_forest,
};

/// Maximum nodes admitted by the exhaustive LSF oracle.
pub const LOW_STRETCH_FOREST_MWU_MAX_NODES: usize = 8;
/// Maximum edges admitted by the exhaustive LSF oracle.
pub const LOW_STRETCH_FOREST_MWU_MAX_EDGES: usize = 12;
/// Maximum source MWU rounds `k`.
pub const LOW_STRETCH_FOREST_MWU_MAX_ROUNDS: usize = 8;
/// Maximum enumerated spanning-tree candidates before HLD refinement.
pub const LOW_STRETCH_FOREST_MWU_MAX_CANDIDATES: usize = 28_672;
/// Maximum subsets inspected during spanning-tree enumeration.
pub const LOW_STRETCH_FOREST_MWU_MAX_TREE_SUBSETS: u64 = 4_096;
/// Maximum exact input/output rational width.
pub const LOW_STRETCH_FOREST_MWU_MAX_RATIONAL_BITS: u64 = 512;
/// Maximum Taylor terms used to separate two exponential polynomials.
pub const LOW_STRETCH_FOREST_MWU_MAX_TAYLOR_TERMS: usize = 256;
/// Maximum public round/completion events.
pub const LOW_STRETCH_FOREST_MWU_MAX_TRACE_EVENTS: usize = LOW_STRETCH_FOREST_MWU_MAX_ROUNDS + 1;

const CATALOG_ID: &str = "low-stretch-forest-mwu";

/// Fixed number of source MWU rounds and output forests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuConfig {
    /// Positive round count `k`.
    pub rounds: usize,
}

/// One exhaustive rooted low-stretch-forest candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuBranch {
    /// Stable exhaustive candidate index.
    pub candidate_index: usize,
    /// Spanning-tree edge mask.
    pub tree_mask: u64,
    /// HLD-refined rooted-forest mask, always a subset of the tree mask.
    pub forest_mask: u64,
    /// Stable root used to orient the selected reference tree.
    pub reference_root: usize,
    /// Stable HLD ancestor-closure seeds used by the Dynamic LSF initializer.
    pub root_seeds: Vec<usize>,
    /// HLD component root of every vertex.
    pub roots: Vec<usize>,
    /// Exact Definition 5.3 stretch vector, used as its own overestimate.
    pub stretch_overestimates: Vec<BigRational>,
    /// Source copy multiplicities `ceil(m v_e / ||v||_1)` for this round.
    pub weight_copy_counts: Vec<u64>,
    /// Exact stretch of every original edge in the selected spanning tree.
    pub tree_stretches: Vec<BigRational>,
    /// Exact copy-weighted tree-stretch objective minimized by the bounded LSST.
    pub weighted_tree_stretch: BigRational,
    /// Exact measured average tree stretch of the normalized copy graph.
    pub measured_lsst_gamma: BigRational,
    /// Explicit threshold used to classify source large-stretch edges.
    pub large_stretch_threshold: BigRational,
    /// Stable original-edge indices classified as large stretch.
    pub large_stretch_edges: Vec<usize>,
    /// Stable seeds added by the bounded tree-decomposition refinement.
    pub decomposition_seeds: Vec<usize>,
    /// Maximum adjacent-edge volume allowed outside each distinguished root.
    pub decomposition_volume_limit: usize,
    /// Final edge-disjoint component partition of the selected HLD forest.
    pub tree_partition: Vec<LowStretchForestTreePiece>,
}

/// One final source partition piece after refining by HLD-forest connectivity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestTreePiece {
    /// Distinguished component root.
    pub root: usize,
    /// Stable vertices in this connected component.
    pub vertices: Vec<usize>,
    /// Stable reference-tree edge indices retained inside this piece.
    pub tree_edges: Vec<usize>,
    /// Original edges adjacent to at least one non-root piece vertex.
    pub adjacent_non_root_edges: Vec<usize>,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LowStretchForestMwuMetrics {
    /// Graph subsets inspected once by the exhaustive oracle.
    pub tree_subsets_inspected: u64,
    /// Candidate rooted forests materialized once.
    pub candidates_enumerated: u64,
    /// Candidate exponential objectives scored across all rounds.
    pub candidate_scores: u64,
    /// Taylor interval refinements used by certified comparisons.
    pub exponential_refinements: u64,
    /// Completed source MWU rounds.
    pub rounds_completed: u64,
    /// Public reversible transitions.
    pub state_transitions: u64,
}

/// Complete source state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuSnapshot {
    /// Next zero-based round.
    pub round: usize,
    /// Exact oracle width `W` used in `rho`.
    pub width_bound: BigRational,
    /// Exact `rho = 10 k W ceil(log_2 n)^2` used by the update.
    pub rho: BigRational,
    /// Exact exponents `sum_{j < round} wstr^j_e / rho` of current weights.
    pub weight_exponents: Vec<BigRational>,
    /// Selected branches in round order.
    pub selected_branches: Vec<LowStretchForestMwuBranch>,
    /// Whether the completion boundary has been emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: LowStretchForestMwuMetrics,
}

/// Meaning of one public source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LowStretchForestMwuEventKind {
    /// One minimum weighted-stretch LSF was selected and weights were updated.
    ForestSelected {
        /// Zero-based source MWU round.
        round: usize,
        /// Stable selected candidate.
        branch: Box<LowStretchForestMwuBranch>,
        /// Total Taylor interval refinements for this selection.
        comparison_refinements: u64,
    },
    /// Exactly `k` source MWU rounds completed.
    Completed,
}

/// One fully reversible MWU event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level transition meaning.
    pub kind: LowStretchForestMwuEventKind,
    /// State before the transition.
    pub before: LowStretchForestMwuSnapshot,
    /// State after the transition.
    pub after: LowStretchForestMwuSnapshot,
}

/// Exact final collection and observed coordinate widths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuResult {
    /// Selected LSFs in source round order.
    pub branches: Vec<LowStretchForestMwuBranch>,
    /// Exact uniform average stretch of every edge.
    pub average_stretches: Vec<BigRational>,
    /// Maximum coordinate of `average_stretches`.
    pub maximum_average_stretch: BigRational,
    /// Final replay state.
    pub final_snapshot: LowStretchForestMwuSnapshot,
}

/// Complete reversible transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LowStretchForestMwuTraceResult {
    /// Initial all-one-weight state.
    pub base_snapshot: LowStretchForestMwuSnapshot,
    /// One event per round, then completion.
    pub events: Vec<LowStretchForestMwuTraceEvent>,
    /// Exact final result.
    pub result: LowStretchForestMwuResult,
}

/// Explicit bounded MWU failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum LowStretchForestMwuError {
    /// Graph shape or `k` is invalid.
    #[error("low-stretch-forest MWU input is invalid")]
    InvalidInput,
    /// The request exceeds the explicit enumeration band.
    #[error("low-stretch-forest MWU exceeds its admission band")]
    AdmissionLimit,
    /// The underlying graph has no spanning tree.
    #[error("low-stretch-forest MWU graph is disconnected")]
    Disconnected,
    /// Exact work accounting overflowed.
    #[error("low-stretch-forest MWU arithmetic overflow")]
    ArithmeticOverflow,
    /// Taylor intervals could not certify a strict symbolic comparison.
    #[error("low-stretch-forest MWU exponential comparison is unresolved")]
    ComparisonUnresolved,
    /// A source/root/stretch/update invariant failed.
    #[error("low-stretch-forest MWU invariant failed")]
    InvariantViolation,
    /// A supplied transcript is not the exact stable replay.
    #[error("low-stretch-forest MWU trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: LowStretchForestMwuSnapshot,
    events: Vec<LowStretchForestMwuTraceEvent>,
    result: LowStretchForestMwuResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LowStretchForestMwuCandidate {
    candidate_index: usize,
    tree_mask: u64,
    tree_stretches: Vec<BigRational>,
    stretch_overestimates: Vec<BigRational>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct LowStretchForestMwuChainWork {
    /// Exhaustive graph subsets inspected.
    pub tree_subsets_inspected: u64,
    /// Rooted-forest candidates materialized.
    pub candidates_enumerated: u64,
    /// Candidate objectives scored.
    pub candidate_scores: u64,
    /// Certified exponential interval refinements.
    pub exponential_refinements: u64,
}

/// Independently reconstructs the MWU branch collection for tree-chain audits.
pub(super) fn audit_low_stretch_forest_mwu_for_chain(
    graph: &ShiftedTreeChainGraph,
    rounds: usize,
) -> Result<(Vec<LowStretchForestMwuBranch>, LowStretchForestMwuChainWork), LowStretchForestMwuError>
{
    let config = LowStretchForestMwuConfig { rounds };
    validate_input(graph, config)?;
    let (candidates, subsets) = audit_enumerate_candidates(graph)?;
    let width = candidate_width(&candidates)?;
    let rho = source_rho(graph.node_count, rounds, &width)?;
    let mut snapshot = initial_snapshot(graph, &candidates, subsets, width, rho)?;
    for _ in 0..rounds {
        let (selected, refinements) =
            audit_select_candidate(graph, config, &candidates, &snapshot.weight_exponents)?;
        audit_apply_round(&mut snapshot, &selected, candidates.len(), refinements)?;
    }
    Ok((
        snapshot.selected_branches,
        LowStretchForestMwuChainWork {
            tree_subsets_inspected: snapshot.metrics.tree_subsets_inspected,
            candidates_enumerated: snapshot.metrics.candidates_enumerated,
            candidate_scores: snapshot.metrics.candidate_scores,
            exponential_refinements: snapshot.metrics.exponential_refinements,
        },
    ))
}

/// Executes `k` certified source MWU rounds without recording events.
///
/// # Errors
///
/// Rejects malformed, disconnected, or out-of-band graphs, checked overflow,
/// unresolved certified comparisons, and invariant failures.
pub fn build_low_stretch_forest_mwu_collection(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
) -> Result<LowStretchForestMwuResult, LowStretchForestMwuError> {
    run_internal(graph, config, false).map(|run| run.result)
}

/// Records each source MWU selection/update and completion boundary.
///
/// # Errors
///
/// Returns any execution or independent replay-checker failure.
pub fn trace_low_stretch_forest_mwu_collection(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
) -> Result<LowStretchForestMwuTraceResult, LowStretchForestMwuError> {
    let run = run_internal(graph, config, true)?;
    let trace = LowStretchForestMwuTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_low_stretch_forest_mwu_trace(graph, config, &trace)?;
    Ok(trace)
}

/// Independently enumerates the oracle family and checks the supplied replay.
///
/// This checker never invokes the production runner.
///
/// # Errors
///
/// Rejects any candidate, root, stretch, exponent, certified selection,
/// metric, average, cursor, or completion drift.
pub fn check_low_stretch_forest_mwu_trace(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    trace: &LowStretchForestMwuTraceResult,
) -> Result<(), LowStretchForestMwuError> {
    validate_input(graph, config)?;
    let (candidates, subsets) = audit_enumerate_candidates(graph)?;
    let width = candidate_width(&candidates)?;
    let rho = source_rho(graph.node_count, config.rounds, &width)?;
    let base = initial_snapshot(graph, &candidates, subsets, width, rho)?;
    if trace.base_snapshot != base || trace.events.len() != config.rounds + 1 {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut cursor = base;
    for round in 0..config.rounds {
        let event = trace
            .events
            .get(round)
            .ok_or(LowStretchForestMwuError::TraceVerification)?;
        if event.catalog_id != CATALOG_ID || event.before != cursor {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
        let (selected, refinements) =
            audit_select_candidate(graph, config, &candidates, &cursor.weight_exponents)?;
        let mut expected = cursor.clone();
        audit_apply_round(&mut expected, &selected, candidates.len(), refinements)?;
        let expected_kind = LowStretchForestMwuEventKind::ForestSelected {
            round,
            branch: Box::new(selected),
            comparison_refinements: refinements,
        };
        if event.kind != expected_kind || event.after != expected {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
        cursor = expected;
    }
    audit_completion(config, trace, &cursor)
}

fn run_internal(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    record: bool,
) -> Result<InternalRun, LowStretchForestMwuError> {
    validate_input(graph, config)?;
    let (candidates, subsets) = enumerate_candidates(graph)?;
    let width = candidate_width(&candidates)?;
    let rho = source_rho(graph.node_count, config.rounds, &width)?;
    let mut snapshot = initial_snapshot(graph, &candidates, subsets, width, rho)?;
    let base_snapshot = snapshot.clone();
    let mut events = Vec::with_capacity(if record { config.rounds + 1 } else { 0 });
    for round in 0..config.rounds {
        let before = snapshot.clone();
        let (selected, refinements) =
            select_candidate(graph, config, &candidates, &snapshot.weight_exponents)?;
        apply_round(&mut snapshot, &selected, candidates.len(), refinements)?;
        if record {
            events.push(LowStretchForestMwuTraceEvent {
                catalog_id: CATALOG_ID,
                kind: LowStretchForestMwuEventKind::ForestSelected {
                    round,
                    branch: Box::new(selected),
                    comparison_refinements: refinements,
                },
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(LowStretchForestMwuTraceEvent {
            catalog_id: CATALOG_ID,
            kind: LowStretchForestMwuEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    let result = make_result(config, snapshot)?;
    Ok(InternalRun {
        base_snapshot,
        events,
        result,
    })
}

fn initial_snapshot(
    graph: &ShiftedTreeChainGraph,
    candidates: &[LowStretchForestMwuCandidate],
    subsets: u64,
    width_bound: BigRational,
    rho: BigRational,
) -> Result<LowStretchForestMwuSnapshot, LowStretchForestMwuError> {
    let candidates_enumerated = u64::try_from(candidates.len())
        .map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?;
    Ok(LowStretchForestMwuSnapshot {
        round: 0,
        width_bound,
        rho,
        weight_exponents: vec![BigRational::zero(); graph.edges.len()],
        selected_branches: Vec::new(),
        complete: false,
        metrics: LowStretchForestMwuMetrics {
            tree_subsets_inspected: subsets,
            candidates_enumerated,
            ..LowStretchForestMwuMetrics::default()
        },
    })
}

fn apply_round(
    snapshot: &mut LowStretchForestMwuSnapshot,
    selected: &LowStretchForestMwuBranch,
    candidate_count: usize,
    refinements: u64,
) -> Result<(), LowStretchForestMwuError> {
    if selected.stretch_overestimates.len() != snapshot.weight_exponents.len() {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    for (exponent, stretch) in snapshot
        .weight_exponents
        .iter_mut()
        .zip(&selected.stretch_overestimates)
    {
        *exponent += stretch / &snapshot.rho;
        check_rational(exponent)?;
    }
    snapshot.selected_branches.push(selected.clone());
    snapshot.round = snapshot
        .round
        .checked_add(1)
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
    snapshot.metrics.candidate_scores = snapshot
        .metrics
        .candidate_scores
        .checked_add(
            u64::try_from(candidate_count)
                .map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
        )
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
    snapshot.metrics.exponential_refinements = snapshot
        .metrics
        .exponential_refinements
        .checked_add(refinements)
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
    snapshot.metrics.rounds_completed = increment(snapshot.metrics.rounds_completed)?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    Ok(())
}

fn make_result(
    config: LowStretchForestMwuConfig,
    snapshot: LowStretchForestMwuSnapshot,
) -> Result<LowStretchForestMwuResult, LowStretchForestMwuError> {
    let divisor = BigInt::from(
        u64::try_from(config.rounds).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
    );
    let mut sums = vec![BigRational::zero(); snapshot.weight_exponents.len()];
    for branch in &snapshot.selected_branches {
        for (sum, stretch) in sums.iter_mut().zip(&branch.stretch_overestimates) {
            *sum += stretch;
        }
    }
    let average_stretches = sums
        .into_iter()
        .map(|sum| sum / &divisor)
        .collect::<Vec<_>>();
    let maximum_average_stretch = average_stretches
        .iter()
        .max()
        .cloned()
        .ok_or(LowStretchForestMwuError::InvariantViolation)?;
    Ok(LowStretchForestMwuResult {
        branches: snapshot.selected_branches.clone(),
        average_stretches,
        maximum_average_stretch,
        final_snapshot: snapshot,
    })
}

fn audit_apply_round(
    snapshot: &mut LowStretchForestMwuSnapshot,
    selected: &LowStretchForestMwuBranch,
    candidate_count: usize,
    refinements: u64,
) -> Result<(), LowStretchForestMwuError> {
    if selected.stretch_overestimates.len() != snapshot.weight_exponents.len() {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    for (exponent, stretch) in snapshot
        .weight_exponents
        .iter_mut()
        .zip(&selected.stretch_overestimates)
    {
        *exponent += stretch / &snapshot.rho;
        if rational_too_wide(exponent) {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
    }
    snapshot.selected_branches.push(selected.clone());
    snapshot.round = snapshot
        .round
        .checked_add(1)
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    let scores =
        u64::try_from(candidate_count).map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    snapshot.metrics.candidate_scores = snapshot
        .metrics
        .candidate_scores
        .checked_add(scores)
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    snapshot.metrics.exponential_refinements = snapshot
        .metrics
        .exponential_refinements
        .checked_add(refinements)
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    snapshot.metrics.rounds_completed = audit_increment(snapshot.metrics.rounds_completed)?;
    snapshot.metrics.state_transitions = audit_increment(snapshot.metrics.state_transitions)?;
    Ok(())
}

fn audit_completion(
    config: LowStretchForestMwuConfig,
    trace: &LowStretchForestMwuTraceResult,
    cursor: &LowStretchForestMwuSnapshot,
) -> Result<(), LowStretchForestMwuError> {
    let completion = trace
        .events
        .get(config.rounds)
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    let mut expected = cursor.clone();
    expected.complete = true;
    expected.metrics.state_transitions = audit_increment(expected.metrics.state_transitions)?;
    let expected_result = make_result(config, expected.clone())
        .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    if completion.catalog_id != CATALOG_ID
        || completion.kind != LowStretchForestMwuEventKind::Completed
        || &completion.before != cursor
        || completion.after != expected
        || trace.result != expected_result
    {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok(())
}

fn select_candidate(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    candidates: &[LowStretchForestMwuCandidate],
    exponents: &[BigRational],
) -> Result<(LowStretchForestMwuBranch, u64), LowStretchForestMwuError> {
    let (copy_counts, refinements) = normalized_copy_counts(exponents)?;
    let selected = select_lsst_candidate(candidates, &copy_counts)?;
    materialize_selected_branch(graph, config, selected, copy_counts, refinements)
}

fn audit_select_candidate(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    candidates: &[LowStretchForestMwuCandidate],
    exponents: &[BigRational],
) -> Result<(LowStretchForestMwuBranch, u64), LowStretchForestMwuError> {
    let (copy_counts, refinements) = audit_normalized_copy_counts(exponents)?;
    let selected = audit_select_lsst_candidate(candidates, &copy_counts)?;
    audit_materialize_selected_branch(graph, config, selected, copy_counts, refinements)
}

fn normalized_copy_counts(
    exponents: &[BigRational],
) -> Result<(Vec<u64>, u64), LowStretchForestMwuError> {
    if exponents.is_empty() {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let edge_count =
        u64::try_from(exponents.len()).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?;
    let mut counts = Vec::with_capacity(exponents.len());
    let mut refinements = 0_u64;
    for edge in 0..exponents.len() {
        let mut selected = None;
        for count in 1..=edge_count {
            let (ordering, used) = compare_normalized_weight_to_integer(exponents, edge, count)?;
            refinements = refinements
                .checked_add(used)
                .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
            if ordering != Ordering::Greater {
                selected = Some(count);
                break;
            }
        }
        counts.push(selected.ok_or(LowStretchForestMwuError::InvariantViolation)?);
    }
    let total = counts.iter().try_fold(0_u64, |sum, &count| {
        sum.checked_add(count)
            .ok_or(LowStretchForestMwuError::ArithmeticOverflow)
    })?;
    if total > edge_count.saturating_mul(2) {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    Ok((counts, refinements))
}

fn audit_normalized_copy_counts(
    exponents: &[BigRational],
) -> Result<(Vec<u64>, u64), LowStretchForestMwuError> {
    let edge_count =
        u64::try_from(exponents.len()).map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    if edge_count == 0 {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut counts = vec![0_u64; exponents.len()];
    let mut refinements = 0_u64;
    for edge in (0..exponents.len()).rev() {
        for count in 1..=edge_count {
            let (ordering, used) = compare_normalized_weight_to_integer(exponents, edge, count)
                .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
            refinements = refinements
                .checked_add(used)
                .ok_or(LowStretchForestMwuError::TraceVerification)?;
            if ordering != Ordering::Greater {
                counts[edge] = count;
                break;
            }
        }
        if counts[edge] == 0 {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
    }
    let total = counts.iter().try_fold(0_u64, |sum, &count| {
        sum.checked_add(count)
            .ok_or(LowStretchForestMwuError::TraceVerification)
    })?;
    if total > edge_count.saturating_mul(2) {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok((counts, refinements))
}

fn compare_normalized_weight_to_integer(
    exponents: &[BigRational],
    edge: usize,
    count: u64,
) -> Result<(Ordering, u64), LowStretchForestMwuError> {
    let selected = exponents
        .get(edge)
        .ok_or(LowStretchForestMwuError::InvariantViolation)?;
    let edge_count = BigRational::from_integer(BigInt::from(
        u64::try_from(exponents.len()).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
    ));
    let count = BigRational::from_integer(BigInt::from(count));
    let mut polynomial = BTreeMap::<BigRational, BigRational>::new();
    *polynomial.entry(selected.clone()).or_default() += edge_count;
    for exponent in exponents {
        *polynomial.entry(exponent.clone()).or_default() -= &count;
    }
    polynomial.retain(|_, coefficient| !coefficient.is_zero());
    compare_exponential_polynomial(&polynomial)
}

fn select_lsst_candidate<'a>(
    candidates: &'a [LowStretchForestMwuCandidate],
    copy_counts: &[u64],
) -> Result<&'a LowStretchForestMwuCandidate, LowStretchForestMwuError> {
    let mut best = candidates
        .first()
        .ok_or(LowStretchForestMwuError::Disconnected)?;
    let mut best_score = weighted_tree_stretch(copy_counts, &best.tree_stretches)?;
    for candidate in &candidates[1..] {
        let score = weighted_tree_stretch(copy_counts, &candidate.tree_stretches)?;
        if score < best_score {
            best = candidate;
            best_score = score;
        }
    }
    Ok(best)
}

fn audit_select_lsst_candidate<'a>(
    candidates: &'a [LowStretchForestMwuCandidate],
    copy_counts: &[u64],
) -> Result<&'a LowStretchForestMwuCandidate, LowStretchForestMwuError> {
    let mut selected = None;
    let mut selected_score = None;
    for candidate in candidates {
        let score = audit_weighted_tree_stretch(copy_counts, &candidate.tree_stretches)?;
        if selected_score.as_ref().is_none_or(|best| &score < best) {
            selected = Some(candidate);
            selected_score = Some(score);
        }
    }
    selected.ok_or(LowStretchForestMwuError::TraceVerification)
}

fn materialize_selected_branch(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    selected: &LowStretchForestMwuCandidate,
    copy_counts: Vec<u64>,
    refinements: u64,
) -> Result<(LowStretchForestMwuBranch, u64), LowStretchForestMwuError> {
    let weighted_tree_stretch = weighted_tree_stretch(&copy_counts, &selected.tree_stretches)?;
    let (measured_lsst_gamma, large_stretch_threshold) = lsst_threshold(
        graph.node_count,
        config.rounds,
        &copy_counts,
        &weighted_tree_stretch,
    )?;
    let large_stretch_edges = selected
        .stretch_overestimates
        .iter()
        .enumerate()
        .filter_map(|(edge, stretch)| (stretch >= &large_stretch_threshold).then_some(edge))
        .collect::<Vec<_>>();
    let mut root_seeds = large_edge_root_seeds(graph, &large_stretch_edges)?;
    let required_seeds = root_seeds.clone();
    let initial_overestimates = selected
        .stretch_overestimates
        .iter()
        .cloned()
        .map(Some)
        .collect::<Vec<_>>();
    let volume_limit = config.rounds;

    let (snapshot, tree_partition) = loop {
        let input = dynamic_lsf_input(
            graph,
            selected.tree_mask,
            root_seeds.clone(),
            Some(initial_overestimates.clone()),
        )?;
        let snapshot = execute_dynamic_low_stretch_forest(&input, &[])
            .map_err(|error| map_dynamic_lsf_error(&error))?
            .final_snapshot;
        let pieces = tree_partition(graph, selected.tree_mask, &snapshot)?;
        let violating = pieces
            .iter()
            .find(|piece| piece.adjacent_non_root_edges.len() > volume_limit);
        let Some(piece) = violating else {
            break (snapshot, pieces);
        };
        let seed = piece
            .vertices
            .iter()
            .copied()
            .find(|vertex| snapshot.roots.binary_search(vertex).is_err())
            .ok_or(LowStretchForestMwuError::InvariantViolation)?;
        insert_sorted_unique(&mut root_seeds, seed);
    };
    let decomposition_seeds = root_seeds
        .iter()
        .copied()
        .filter(|seed| required_seeds.binary_search(seed).is_err())
        .collect::<Vec<_>>();
    let mut branch = branch_from_dynamic_snapshot(
        graph,
        selected.tree_mask,
        selected.candidate_index,
        0,
        root_seeds,
        &snapshot,
        false,
    )?;
    branch.weighted_tree_stretch = weighted_tree_stretch;
    branch.measured_lsst_gamma = measured_lsst_gamma;
    branch.large_stretch_threshold = large_stretch_threshold;
    branch.large_stretch_edges = large_stretch_edges;
    branch.decomposition_seeds = decomposition_seeds;
    branch.decomposition_volume_limit = volume_limit;
    branch.tree_partition = tree_partition;
    branch.weight_copy_counts = copy_counts;
    branch.tree_stretches.clone_from(&selected.tree_stretches);
    Ok((branch, refinements))
}

fn audit_materialize_selected_branch(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
    selected: &LowStretchForestMwuCandidate,
    copy_counts: Vec<u64>,
    refinements: u64,
) -> Result<(LowStretchForestMwuBranch, u64), LowStretchForestMwuError> {
    let weighted_tree_stretch =
        audit_weighted_tree_stretch(&copy_counts, &selected.tree_stretches)?;
    let total_copies = copy_counts.iter().rev().try_fold(0_u64, |sum, &copies| {
        sum.checked_add(copies)
            .ok_or(LowStretchForestMwuError::TraceVerification)
    })?;
    if total_copies == 0 {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let measured_lsst_gamma = &weighted_tree_stretch / BigInt::from(total_copies);
    let log =
        source_log(graph.node_count).map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    let log_fourth = log
        .checked_mul(log)
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(config.rounds))
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    let large_stretch_threshold = &measured_lsst_gamma
        * BigInt::from(
            u64::try_from(log_fourth).map_err(|_| LowStretchForestMwuError::TraceVerification)?,
        );
    if rational_too_wide(&measured_lsst_gamma) || rational_too_wide(&large_stretch_threshold) {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut large_stretch_edges = Vec::new();
    for edge in (0..selected.stretch_overestimates.len()).rev() {
        if selected.stretch_overestimates[edge] >= large_stretch_threshold {
            large_stretch_edges.push(edge);
        }
    }
    large_stretch_edges.reverse();
    let mut root_seeds = audit_large_edge_root_seeds(graph, &large_stretch_edges)?;
    let required_seeds = root_seeds.clone();
    let initial_overestimates = selected
        .stretch_overestimates
        .iter()
        .cloned()
        .map(Some)
        .collect::<Vec<_>>();
    let volume_limit = config.rounds;

    let (snapshot, tree_partition) = loop {
        let input = dynamic_lsf_input(
            graph,
            selected.tree_mask,
            root_seeds.clone(),
            Some(initial_overestimates.clone()),
        )
        .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
        let trace = trace_dynamic_low_stretch_forest(&input, &[])
            .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
        let snapshot = trace.base_snapshot;
        let pieces = audit_tree_partition(graph, selected.tree_mask, &snapshot)?;
        let violating = pieces
            .iter()
            .rev()
            .filter(|piece| piece.adjacent_non_root_edges.len() > volume_limit)
            .min_by_key(|piece| piece.root);
        let Some(piece) = violating else {
            break (snapshot, pieces);
        };
        let seed = piece
            .vertices
            .iter()
            .rev()
            .copied()
            .filter(|vertex| snapshot.roots.binary_search(vertex).is_err())
            .min()
            .ok_or(LowStretchForestMwuError::TraceVerification)?;
        audit_insert_sorted_unique(&mut root_seeds, seed);
    };
    let decomposition_seeds = root_seeds
        .iter()
        .rev()
        .copied()
        .filter(|seed| required_seeds.binary_search(seed).is_err())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut branch = branch_from_dynamic_snapshot(
        graph,
        selected.tree_mask,
        selected.candidate_index,
        0,
        root_seeds,
        &snapshot,
        true,
    )?;
    branch.weighted_tree_stretch = weighted_tree_stretch;
    branch.measured_lsst_gamma = measured_lsst_gamma;
    branch.large_stretch_threshold = large_stretch_threshold;
    branch.large_stretch_edges = large_stretch_edges;
    branch.decomposition_seeds = decomposition_seeds;
    branch.decomposition_volume_limit = volume_limit;
    branch.tree_partition = tree_partition;
    branch.weight_copy_counts = copy_counts;
    branch.tree_stretches.clone_from(&selected.tree_stretches);
    Ok((branch, refinements))
}

fn lsst_threshold(
    nodes: usize,
    rounds: usize,
    copy_counts: &[u64],
    weighted_tree_stretch: &BigRational,
) -> Result<(BigRational, BigRational), LowStretchForestMwuError> {
    let total_copies = copy_counts.iter().try_fold(0_u64, |sum, &copies| {
        sum.checked_add(copies)
            .ok_or(LowStretchForestMwuError::ArithmeticOverflow)
    })?;
    if total_copies == 0 {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let measured = weighted_tree_stretch / BigInt::from(total_copies);
    let log = source_log(nodes)?;
    let scale = log
        .checked_mul(log)
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(rounds))
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
    let threshold = &measured
        * BigInt::from(
            u64::try_from(scale).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
        );
    check_rational(&measured)?;
    check_rational(&threshold)?;
    Ok((measured, threshold))
}

fn source_log(nodes: usize) -> Result<usize, LowStretchForestMwuError> {
    usize::try_from(usize::BITS - nodes.saturating_sub(1).leading_zeros())
        .map(|log| log.max(1))
        .map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)
}

fn large_edge_root_seeds(
    graph: &ShiftedTreeChainGraph,
    large_edges: &[usize],
) -> Result<Vec<usize>, LowStretchForestMwuError> {
    let mut seeds = vec![0];
    for &edge in large_edges {
        let row = graph
            .edges
            .get(edge)
            .ok_or(LowStretchForestMwuError::InvariantViolation)?;
        insert_sorted_unique(&mut seeds, row.from);
        insert_sorted_unique(&mut seeds, row.to);
    }
    Ok(seeds)
}

fn audit_large_edge_root_seeds(
    graph: &ShiftedTreeChainGraph,
    large_edges: &[usize],
) -> Result<Vec<usize>, LowStretchForestMwuError> {
    let mut seeds = BTreeSet::from([0]);
    for &edge in large_edges.iter().rev() {
        let row = graph
            .edges
            .get(edge)
            .ok_or(LowStretchForestMwuError::TraceVerification)?;
        seeds.insert(row.to);
        seeds.insert(row.from);
    }
    Ok(seeds.into_iter().collect())
}

fn weighted_tree_stretch(
    copy_counts: &[u64],
    tree_stretches: &[BigRational],
) -> Result<BigRational, LowStretchForestMwuError> {
    if copy_counts.len() != tree_stretches.len() {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let score = copy_counts
        .iter()
        .zip(tree_stretches)
        .fold(BigRational::zero(), |sum, (&copies, stretch)| {
            sum + stretch * BigInt::from(copies)
        });
    check_rational(&score)?;
    Ok(score)
}

fn audit_weighted_tree_stretch(
    copy_counts: &[u64],
    tree_stretches: &[BigRational],
) -> Result<BigRational, LowStretchForestMwuError> {
    if copy_counts.len() != tree_stretches.len() {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut score = BigRational::zero();
    for index in (0..copy_counts.len()).rev() {
        score += &tree_stretches[index] * BigInt::from(copy_counts[index]);
    }
    if rational_too_wide(&score) {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok(score)
}

#[cfg(test)]
fn compare_exponential_objectives(
    exponents: &[BigRational],
    left: &[BigRational],
    right: &[BigRational],
) -> Result<(Ordering, u64), LowStretchForestMwuError> {
    if exponents.len() != left.len() || left.len() != right.len() {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let mut polynomial = BTreeMap::<BigRational, BigRational>::new();
    for ((exponent, left), right) in exponents.iter().zip(left).zip(right) {
        *polynomial.entry(exponent.clone()).or_default() += left - right;
    }
    polynomial.retain(|_, coefficient| !coefficient.is_zero());
    compare_exponential_polynomial(&polynomial)
}

fn compare_exponential_polynomial(
    polynomial: &BTreeMap<BigRational, BigRational>,
) -> Result<(Ordering, u64), LowStretchForestMwuError> {
    if polynomial.is_empty() {
        return Ok((Ordering::Equal, 0));
    }
    let mut refinements = 0_u64;
    for terms in [4_usize, 8, 16, 32, 64, 128, 256] {
        let (lower, upper) = exponential_polynomial_interval(polynomial, terms)?;
        refinements = increment(refinements)?;
        if lower > BigRational::zero() {
            return Ok((Ordering::Greater, refinements));
        }
        if upper < BigRational::zero() {
            return Ok((Ordering::Less, refinements));
        }
    }
    Err(LowStretchForestMwuError::ComparisonUnresolved)
}

fn exponential_polynomial_interval(
    polynomial: &BTreeMap<BigRational, BigRational>,
    terms: usize,
) -> Result<(BigRational, BigRational), LowStretchForestMwuError> {
    if terms > LOW_STRETCH_FOREST_MWU_MAX_TAYLOR_TERMS {
        return Err(LowStretchForestMwuError::AdmissionLimit);
    }
    let mut lower = BigRational::zero();
    let mut upper = BigRational::zero();
    for (exponent, coefficient) in polynomial {
        let (exp_lower, exp_upper) = exp_interval(exponent, terms)?;
        if coefficient > &BigRational::zero() {
            lower += coefficient * exp_lower;
            upper += coefficient * exp_upper;
        } else {
            lower += coefficient * exp_upper;
            upper += coefficient * exp_lower;
        }
    }
    Ok((lower, upper))
}

fn exp_interval(
    exponent: &BigRational,
    terms: usize,
) -> Result<(BigRational, BigRational), LowStretchForestMwuError> {
    let one = BigRational::one();
    if exponent < &BigRational::zero() || exponent > &BigRational::new(1.into(), 10.into()) {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let mut sum = one.clone();
    let mut term = one.clone();
    for index in 1..=terms {
        let divisor = BigInt::from(
            u64::try_from(index).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
        );
        term = term * exponent / divisor;
        sum += &term;
    }
    let next_divisor = BigInt::from(
        u64::try_from(terms + 1).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
    );
    let next = term * exponent / next_divisor;
    let ratio_divisor = BigInt::from(
        u64::try_from(terms + 2).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
    );
    let ratio = exponent / ratio_divisor;
    let remainder = next / (one - ratio);
    Ok((sum.clone(), sum + remainder))
}

fn candidate_width(
    candidates: &[LowStretchForestMwuCandidate],
) -> Result<BigRational, LowStretchForestMwuError> {
    candidates
        .iter()
        .flat_map(|candidate| &candidate.stretch_overestimates)
        .max()
        .cloned()
        .ok_or(LowStretchForestMwuError::Disconnected)
}

fn source_rho(
    nodes: usize,
    rounds: usize,
    width: &BigRational,
) -> Result<BigRational, LowStretchForestMwuError> {
    let log = source_log(nodes)?;
    let scale = rounds
        .checked_mul(log)
        .and_then(|value| value.checked_mul(log))
        .and_then(|value| value.checked_mul(10))
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)?;
    let scale = BigInt::from(
        u64::try_from(scale).map_err(|_| LowStretchForestMwuError::ArithmeticOverflow)?,
    );
    Ok(width * scale)
}

fn enumerate_candidates(
    graph: &ShiftedTreeChainGraph,
) -> Result<(Vec<LowStretchForestMwuCandidate>, u64), LowStretchForestMwuError> {
    let subsets = subset_count(graph.edges.len())?;
    let required = graph.node_count - 1;
    let mut candidates = Vec::new();
    for tree_mask in 0..subsets {
        if usize::try_from(tree_mask.count_ones()).ok() != Some(required)
            || !is_spanning_tree(graph, tree_mask)
        {
            continue;
        }
        if candidates.len() >= LOW_STRETCH_FOREST_MWU_MAX_CANDIDATES {
            return Err(LowStretchForestMwuError::AdmissionLimit);
        }
        candidates.push(dynamic_hld_candidate(graph, tree_mask, candidates.len())?);
    }
    if candidates.is_empty() {
        return Err(LowStretchForestMwuError::Disconnected);
    }
    Ok((candidates, subsets))
}

fn audit_enumerate_candidates(
    graph: &ShiftedTreeChainGraph,
) -> Result<(Vec<LowStretchForestMwuCandidate>, u64), LowStretchForestMwuError> {
    let subsets = audit_subset_count(graph.edges.len())?;
    let required = graph.node_count - 1;
    let mut candidates = Vec::new();
    for tree_mask in 0..subsets {
        if usize::try_from(tree_mask.count_ones()).ok() != Some(required)
            || !audit_is_spanning_tree(graph, tree_mask)
        {
            continue;
        }
        if candidates.len() >= LOW_STRETCH_FOREST_MWU_MAX_CANDIDATES {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
        candidates.push(audit_dynamic_hld_candidate(
            graph,
            tree_mask,
            candidates.len(),
        )?);
    }
    if candidates.is_empty() {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok((candidates, subsets))
}

fn dynamic_hld_candidate(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    candidate_index: usize,
) -> Result<LowStretchForestMwuCandidate, LowStretchForestMwuError> {
    let input = dynamic_lsf_input(graph, tree_mask, vec![0], None)?;
    let snapshot = execute_dynamic_low_stretch_forest(&input, &[])
        .map_err(|error| map_dynamic_lsf_error(&error))?
        .final_snapshot;
    let stretch_overestimates = snapshot
        .stretch_overestimates
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(LowStretchForestMwuError::InvariantViolation)?;
    Ok(LowStretchForestMwuCandidate {
        candidate_index,
        tree_mask,
        tree_stretches: spanning_tree_stretches(graph, tree_mask)?,
        stretch_overestimates,
    })
}

// The audit path composes the Dynamic LSF trace checker rather than invoking
// the MWU production runner. It then reconstructs every mask and exact stretch
// directly from the checked base snapshot.
fn audit_dynamic_hld_candidate(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    candidate_index: usize,
) -> Result<LowStretchForestMwuCandidate, LowStretchForestMwuError> {
    let input = dynamic_lsf_input(graph, tree_mask, vec![0], None)
        .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    let trace = trace_dynamic_low_stretch_forest(&input, &[])
        .map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    let stretch_overestimates = trace
        .base_snapshot
        .stretch_overestimates
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    Ok(LowStretchForestMwuCandidate {
        candidate_index,
        tree_mask,
        tree_stretches: audit_spanning_tree_stretches(graph, tree_mask)?,
        stretch_overestimates,
    })
}

fn dynamic_lsf_input(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    root_seeds: Vec<usize>,
    initial_stretch_overestimates: Option<Vec<Option<BigRational>>>,
) -> Result<DynamicLowStretchForestInput, LowStretchForestMwuError> {
    if root_seeds.is_empty()
        || root_seeds.windows(2).any(|pair| pair[0] >= pair[1])
        || root_seeds.iter().any(|&seed| seed >= graph.node_count)
    {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let edge_slots = graph
        .edges
        .iter()
        .enumerate()
        .map(|(edge, row)| {
            Some(DynamicLowStretchForestEdge {
                edge,
                from: row.from,
                to: row.to,
                length: row.length.clone(),
            })
        })
        .collect();
    Ok(DynamicLowStretchForestInput {
        initial_node_count: graph.node_count,
        maximum_node_count: graph.node_count,
        edge_slots,
        reference_tree_edges: mask_indices(tree_mask, graph.edges.len()).collect(),
        reference_root: 0,
        initial_root_seeds: root_seeds,
        initial_stretch_overestimates,
    })
}

fn tree_partition(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    snapshot: &DynamicLowStretchForestSnapshot,
) -> Result<Vec<LowStretchForestTreePiece>, LowStretchForestMwuError> {
    if snapshot.active_node_count != graph.node_count
        || snapshot.component_roots.len() != graph.node_count
        || snapshot
            .component_roots
            .iter()
            .any(|&root| root >= graph.node_count)
    {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let mut vertices_by_root = BTreeMap::<usize, Vec<usize>>::new();
    for (vertex, &root) in snapshot.component_roots.iter().enumerate() {
        vertices_by_root.entry(root).or_default().push(vertex);
    }
    let mut tree_edges_by_root = BTreeMap::<usize, Vec<usize>>::new();
    for &edge in &snapshot.forest_edges {
        let row = graph
            .edges
            .get(edge)
            .ok_or(LowStretchForestMwuError::InvariantViolation)?;
        if tree_mask & edge_bit(edge)? == 0
            || snapshot.component_roots[row.from] != snapshot.component_roots[row.to]
        {
            return Err(LowStretchForestMwuError::InvariantViolation);
        }
        tree_edges_by_root
            .entry(snapshot.component_roots[row.from])
            .or_default()
            .push(edge);
    }
    let mut pieces = Vec::with_capacity(vertices_by_root.len());
    for (root, vertices) in vertices_by_root {
        if vertices.binary_search(&root).is_err() {
            return Err(LowStretchForestMwuError::InvariantViolation);
        }
        let adjacent_non_root_edges = graph
            .edges
            .iter()
            .enumerate()
            .filter_map(|(edge, row)| {
                ((snapshot.component_roots[row.from] == root && row.from != root)
                    || (snapshot.component_roots[row.to] == root && row.to != root))
                    .then_some(edge)
            })
            .collect();
        pieces.push(LowStretchForestTreePiece {
            root,
            vertices,
            tree_edges: tree_edges_by_root.remove(&root).unwrap_or_default(),
            adjacent_non_root_edges,
        });
    }
    if !tree_edges_by_root.is_empty()
        || pieces
            .iter()
            .map(|piece| piece.vertices.len())
            .sum::<usize>()
            != graph.node_count
        || pieces
            .iter()
            .map(|piece| piece.tree_edges.len())
            .sum::<usize>()
            != snapshot.forest_edges.len()
    {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    Ok(pieces)
}

fn audit_tree_partition(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    snapshot: &DynamicLowStretchForestSnapshot,
) -> Result<Vec<LowStretchForestTreePiece>, LowStretchForestMwuError> {
    if snapshot.active_node_count != graph.node_count
        || snapshot.component_roots.len() != graph.node_count
    {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut roots = snapshot.component_roots.clone();
    roots.sort_unstable();
    roots.dedup();
    if roots.iter().any(|&root| root >= graph.node_count) {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut pieces = Vec::with_capacity(roots.len());
    for &root in roots.iter().rev() {
        let mut vertices = (0..graph.node_count)
            .rev()
            .filter(|&vertex| snapshot.component_roots[vertex] == root)
            .collect::<Vec<_>>();
        vertices.reverse();
        if vertices.binary_search(&root).is_err() {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
        let mut tree_edges = Vec::new();
        for &edge in snapshot.forest_edges.iter().rev() {
            let row = graph
                .edges
                .get(edge)
                .ok_or(LowStretchForestMwuError::TraceVerification)?;
            let bit = audit_edge_bit(edge)?;
            if tree_mask & bit == 0
                || snapshot.component_roots[row.from] != snapshot.component_roots[row.to]
            {
                return Err(LowStretchForestMwuError::TraceVerification);
            }
            if snapshot.component_roots[row.from] == root {
                tree_edges.push(edge);
            }
        }
        tree_edges.reverse();
        let mut adjacent_non_root_edges = Vec::new();
        for edge in (0..graph.edges.len()).rev() {
            let row = &graph.edges[edge];
            if (snapshot.component_roots[row.to] == root && row.to != root)
                || (snapshot.component_roots[row.from] == root && row.from != root)
            {
                adjacent_non_root_edges.push(edge);
            }
        }
        adjacent_non_root_edges.reverse();
        pieces.push(LowStretchForestTreePiece {
            root,
            vertices,
            tree_edges,
            adjacent_non_root_edges,
        });
    }
    pieces.sort_by_key(|piece| piece.root);
    let covered_vertices = pieces
        .iter()
        .flat_map(|piece| piece.vertices.iter().copied())
        .collect::<BTreeSet<_>>();
    let covered_tree_edges = pieces
        .iter()
        .flat_map(|piece| piece.tree_edges.iter().copied())
        .collect::<BTreeSet<_>>();
    if covered_vertices != (0..graph.node_count).collect()
        || covered_tree_edges != snapshot.forest_edges.iter().copied().collect()
    {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok(pieces)
}

fn insert_sorted_unique(values: &mut Vec<usize>, value: usize) {
    match values.binary_search(&value) {
        Ok(_) => {}
        Err(index) => values.insert(index, value),
    }
}

fn audit_insert_sorted_unique(values: &mut Vec<usize>, value: usize) {
    if values.iter().all(|&current| current != value) {
        values.push(value);
        values.sort_unstable();
    }
}

fn spanning_tree_stretches(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
) -> Result<Vec<BigRational>, LowStretchForestMwuError> {
    graph
        .edges
        .iter()
        .map(|edge| {
            let stretch = BigRational::one()
                + path_length(graph, tree_mask, edge.from, edge.to)? / &edge.length;
            check_rational(&stretch)?;
            Ok(stretch)
        })
        .collect()
}

fn audit_spanning_tree_stretches(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
) -> Result<Vec<BigRational>, LowStretchForestMwuError> {
    let mut stretches = Vec::with_capacity(graph.edges.len());
    for edge in graph.edges.iter().rev() {
        let stretch = BigRational::one()
            + audit_path_length(graph, tree_mask, edge.from, edge.to)? / &edge.length;
        if rational_too_wide(&stretch) {
            return Err(LowStretchForestMwuError::TraceVerification);
        }
        stretches.push(stretch);
    }
    stretches.reverse();
    Ok(stretches)
}

#[allow(clippy::too_many_arguments)]
fn branch_from_dynamic_snapshot(
    graph: &ShiftedTreeChainGraph,
    tree_mask: u64,
    candidate_index: usize,
    reference_root: usize,
    root_seeds: Vec<usize>,
    snapshot: &DynamicLowStretchForestSnapshot,
    audit: bool,
) -> Result<LowStretchForestMwuBranch, LowStretchForestMwuError> {
    let mut forest_mask = 0_u64;
    for &edge in &snapshot.forest_edges {
        forest_mask |= if audit {
            audit_edge_bit(edge)?
        } else {
            edge_bit(edge)?
        };
    }
    if forest_mask & !tree_mask != 0
        || snapshot.component_roots.len() != graph.node_count
        || snapshot.stretch_overestimates.len() != graph.edges.len()
        || snapshot.current_stretches.len() != graph.edges.len()
    {
        return Err(if audit {
            LowStretchForestMwuError::TraceVerification
        } else {
            LowStretchForestMwuError::InvariantViolation
        });
    }
    let stretches = snapshot
        .stretch_overestimates
        .iter()
        .cloned()
        .collect::<Option<Vec<_>>>()
        .ok_or(if audit {
            LowStretchForestMwuError::TraceVerification
        } else {
            LowStretchForestMwuError::InvariantViolation
        })?;
    for (index, edge) in graph.edges.iter().enumerate() {
        let exact = if audit {
            audit_forest_stretch(graph, forest_mask, &snapshot.component_roots, edge)?
        } else {
            forest_stretch(graph, forest_mask, &snapshot.component_roots, edge)?
        };
        if snapshot.current_stretches[index].as_ref() != Some(&exact) || stretches[index] < exact {
            return Err(if audit {
                LowStretchForestMwuError::TraceVerification
            } else {
                LowStretchForestMwuError::InvariantViolation
            });
        }
    }
    Ok(LowStretchForestMwuBranch {
        candidate_index,
        tree_mask,
        forest_mask,
        reference_root,
        root_seeds,
        roots: snapshot.component_roots.clone(),
        stretch_overestimates: stretches,
        weight_copy_counts: Vec::new(),
        tree_stretches: Vec::new(),
        weighted_tree_stretch: BigRational::zero(),
        measured_lsst_gamma: BigRational::zero(),
        large_stretch_threshold: BigRational::zero(),
        large_stretch_edges: Vec::new(),
        decomposition_seeds: Vec::new(),
        decomposition_volume_limit: 0,
        tree_partition: Vec::new(),
    })
}

fn map_dynamic_lsf_error(error: &DynamicLowStretchForestError) -> LowStretchForestMwuError {
    match error {
        DynamicLowStretchForestError::AdmissionLimit => LowStretchForestMwuError::AdmissionLimit,
        DynamicLowStretchForestError::ArithmeticOverflow => {
            LowStretchForestMwuError::ArithmeticOverflow
        }
        DynamicLowStretchForestError::InvalidInput
        | DynamicLowStretchForestError::InvariantViolation
        | DynamicLowStretchForestError::TraceVerification => {
            LowStretchForestMwuError::InvariantViolation
        }
    }
}

fn forest_stretch(
    graph: &ShiftedTreeChainGraph,
    forest_mask: u64,
    roots: &[usize],
    edge: &ShiftedTreeChainEdge,
) -> Result<BigRational, LowStretchForestMwuError> {
    let path_length = if roots[edge.from] == roots[edge.to] {
        path_length(graph, forest_mask, edge.from, edge.to)?
    } else {
        path_length(graph, forest_mask, edge.from, roots[edge.from])?
            + path_length(graph, forest_mask, edge.to, roots[edge.to])?
    };
    let stretch = BigRational::one() + path_length / &edge.length;
    check_rational(&stretch)?;
    Ok(stretch)
}

fn path_length(
    graph: &ShiftedTreeChainGraph,
    mask: u64,
    start: usize,
    target: usize,
) -> Result<BigRational, LowStretchForestMwuError> {
    if start == target {
        return Ok(BigRational::zero());
    }
    let mut previous = vec![None; graph.node_count];
    let mut seen = vec![false; graph.node_count];
    let mut queue = VecDeque::from([start]);
    seen[start] = true;
    while let Some(node) = queue.pop_front() {
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            if mask & edge_bit(edge_index)? == 0 {
                continue;
            }
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
                previous[next] = Some((node, edge_index));
                queue.push_back(next);
            }
        }
    }
    if !seen[target] {
        return Err(LowStretchForestMwuError::InvariantViolation);
    }
    let mut sum = BigRational::zero();
    let mut cursor = target;
    while cursor != start {
        let (parent, edge) =
            previous[cursor].ok_or(LowStretchForestMwuError::InvariantViolation)?;
        sum += &graph.edges[edge].length;
        cursor = parent;
    }
    Ok(sum)
}

fn is_spanning_tree(graph: &ShiftedTreeChainGraph, mask: u64) -> bool {
    let mut dsu = DisjointSet::new(graph.node_count);
    for (index, edge) in graph.edges.iter().enumerate() {
        if mask & (1_u64 << index) != 0 && (edge.from == edge.to || !dsu.union(edge.from, edge.to))
        {
            return false;
        }
    }
    let root = dsu.find(0);
    (1..graph.node_count).all(|node| dsu.find(node) == root)
}

// The checker uses a separate DFS/parent implementation for candidate shape,
// roots, and Definition 5.3 stretches.
fn audit_subset_count(edges: usize) -> Result<u64, LowStretchForestMwuError> {
    let shift = u32::try_from(edges).map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    let count = 1_u64
        .checked_shl(shift)
        .ok_or(LowStretchForestMwuError::TraceVerification)?;
    if count > LOW_STRETCH_FOREST_MWU_MAX_TREE_SUBSETS {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok(count)
}

fn audit_edge_bit(index: usize) -> Result<u64, LowStretchForestMwuError> {
    let shift = u32::try_from(index).map_err(|_| LowStretchForestMwuError::TraceVerification)?;
    1_u64
        .checked_shl(shift)
        .ok_or(LowStretchForestMwuError::TraceVerification)
}

fn audit_is_spanning_tree(graph: &ShiftedTreeChainGraph, mask: u64) -> bool {
    let mut adjacency = vec![Vec::new(); graph.node_count];
    let mut selected = 0_usize;
    for (index, edge) in graph.edges.iter().enumerate() {
        if mask & (1_u64 << index) == 0 {
            continue;
        }
        if edge.from == edge.to {
            return false;
        }
        selected += 1;
        adjacency[edge.from].push(edge.to);
        adjacency[edge.to].push(edge.from);
    }
    if selected != graph.node_count - 1 {
        return false;
    }
    let mut seen = vec![false; graph.node_count];
    let mut stack = vec![0_usize];
    seen[0] = true;
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    seen.into_iter().all(|visited| visited)
}

fn audit_forest_stretch(
    graph: &ShiftedTreeChainGraph,
    forest_mask: u64,
    roots: &[usize],
    edge: &ShiftedTreeChainEdge,
) -> Result<BigRational, LowStretchForestMwuError> {
    let numerator = if roots[edge.from] == roots[edge.to] {
        audit_path_length(graph, forest_mask, edge.from, edge.to)?
    } else {
        audit_path_length(graph, forest_mask, edge.from, roots[edge.from])?
            + audit_path_length(graph, forest_mask, edge.to, roots[edge.to])?
    };
    let stretch = BigRational::one() + numerator / &edge.length;
    if rational_too_wide(&stretch) {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    Ok(stretch)
}

fn audit_path_length(
    graph: &ShiftedTreeChainGraph,
    mask: u64,
    start: usize,
    target: usize,
) -> Result<BigRational, LowStretchForestMwuError> {
    if start == target {
        return Ok(BigRational::zero());
    }
    let mut parent = vec![None; graph.node_count];
    let mut stack = vec![start];
    parent[start] = Some((start, usize::MAX));
    while let Some(node) = stack.pop() {
        for (index, edge) in graph.edges.iter().enumerate() {
            if mask & audit_edge_bit(index)? == 0 {
                continue;
            }
            let next = if edge.from == node {
                Some(edge.to)
            } else if edge.to == node {
                Some(edge.from)
            } else {
                None
            };
            if let Some(next) = next
                && parent[next].is_none()
            {
                parent[next] = Some((node, index));
                stack.push(next);
            }
        }
    }
    if parent[target].is_none() {
        return Err(LowStretchForestMwuError::TraceVerification);
    }
    let mut length = BigRational::zero();
    let mut cursor = target;
    while cursor != start {
        let (previous, edge) = parent[cursor].ok_or(LowStretchForestMwuError::TraceVerification)?;
        length += &graph.edges[edge].length;
        cursor = previous;
    }
    Ok(length)
}

fn validate_input(
    graph: &ShiftedTreeChainGraph,
    config: LowStretchForestMwuConfig,
) -> Result<(), LowStretchForestMwuError> {
    if graph.node_count < 2 || graph.edges.is_empty() || config.rounds == 0 {
        return Err(LowStretchForestMwuError::InvalidInput);
    }
    if graph.node_count > LOW_STRETCH_FOREST_MWU_MAX_NODES
        || graph.edges.len() > LOW_STRETCH_FOREST_MWU_MAX_EDGES
        || config.rounds > LOW_STRETCH_FOREST_MWU_MAX_ROUNDS
    {
        return Err(LowStretchForestMwuError::AdmissionLimit);
    }
    let mut source_edges = BTreeSet::new();
    for edge in &graph.edges {
        if !source_edges.insert(edge.source_edge)
            || edge.from >= graph.node_count
            || edge.to >= graph.node_count
            || edge.length <= BigRational::zero()
        {
            return Err(LowStretchForestMwuError::InvalidInput);
        }
        check_rational(&edge.length)?;
    }
    Ok(())
}

fn subset_count(edges: usize) -> Result<u64, LowStretchForestMwuError> {
    let shift = u32::try_from(edges).map_err(|_| LowStretchForestMwuError::AdmissionLimit)?;
    let subsets = 1_u64
        .checked_shl(shift)
        .ok_or(LowStretchForestMwuError::AdmissionLimit)?;
    if subsets > LOW_STRETCH_FOREST_MWU_MAX_TREE_SUBSETS {
        return Err(LowStretchForestMwuError::AdmissionLimit);
    }
    Ok(subsets)
}

fn mask_indices(mask: u64, edge_count: usize) -> impl Iterator<Item = usize> {
    (0..edge_count).filter(move |&index| mask & (1_u64 << index) != 0)
}

fn edge_bit(index: usize) -> Result<u64, LowStretchForestMwuError> {
    let shift = u32::try_from(index).map_err(|_| LowStretchForestMwuError::AdmissionLimit)?;
    1_u64
        .checked_shl(shift)
        .ok_or(LowStretchForestMwuError::AdmissionLimit)
}

fn check_rational(value: &BigRational) -> Result<(), LowStretchForestMwuError> {
    if rational_too_wide(value) {
        Err(LowStretchForestMwuError::AdmissionLimit)
    } else {
        Ok(())
    }
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > LOW_STRETCH_FOREST_MWU_MAX_RATIONAL_BITS
        || value.denom().bits() > LOW_STRETCH_FOREST_MWU_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, LowStretchForestMwuError> {
    value
        .checked_add(1)
        .ok_or(LowStretchForestMwuError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, LowStretchForestMwuError> {
    value
        .checked_add(1)
        .ok_or(LowStretchForestMwuError::TraceVerification)
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
    use super::*;

    fn rational(value: i64) -> BigRational {
        BigRational::from_integer(BigInt::from(value))
    }

    fn asymmetric_diamond() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 1,
                    to: 3,
                    length: rational(1),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 2,
                    from: 0,
                    to: 2,
                    length: rational(2),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 3,
                    from: 2,
                    to: 3,
                    length: rational(3),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 4,
                    from: 1,
                    to: 2,
                    length: rational(1),
                    gradient: rational(0),
                },
            ],
        }
    }

    fn reordered_path() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 4,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 2,
                    to: 3,
                    length: rational(1),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 0,
                    to: 1,
                    length: rational(1),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 2,
                    from: 1,
                    to: 2,
                    length: rational(1),
                    gradient: rational(0),
                },
            ],
        }
    }

    fn parallel_large_stretch_edge() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 2,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1),
                    gradient: rational(0),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 0,
                    to: 1,
                    length: rational(100),
                    gradient: rational(0),
                },
            ],
        }
    }

    fn config() -> LowStretchForestMwuConfig {
        LowStretchForestMwuConfig { rounds: 4 }
    }

    #[test]
    fn source_exponents_equal_cumulative_stretch_over_rho() {
        let result =
            build_low_stretch_forest_mwu_collection(&asymmetric_diamond(), config()).expect("MWU");
        for edge in 0..asymmetric_diamond().edges.len() {
            let cumulative = result
                .branches
                .iter()
                .fold(BigRational::zero(), |sum, branch| {
                    sum + &branch.stretch_overestimates[edge]
                });
            assert_eq!(
                result.final_snapshot.weight_exponents[edge],
                cumulative / &result.final_snapshot.rho
            );
            assert_eq!(
                result.average_stretches[edge],
                result.final_snapshot.weight_exponents[edge].clone() * &result.final_snapshot.rho
                    / BigInt::from(4_u8)
            );
        }
    }

    #[test]
    fn fast_trace_and_independent_checker_match() {
        let graph = asymmetric_diamond();
        let fast = build_low_stretch_forest_mwu_collection(&graph, config()).expect("fast");
        let trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 5);
        assert_eq!(trace.result.branches.len(), 4);
        assert!(trace.result.final_snapshot.complete);
        assert_eq!(trace.result.final_snapshot.metrics.rounds_completed, 4);
        assert!(trace.result.final_snapshot.metrics.exponential_refinements > 0);
    }

    #[test]
    fn each_round_uses_source_copy_counts_and_the_exact_bounded_lsst() {
        let graph = asymmetric_diamond();
        let trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let (candidates, _) = audit_enumerate_candidates(&graph).expect("candidates");
        for (round, event) in trace.events.iter().take(config().rounds).enumerate() {
            let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &event.kind else {
                panic!("selection");
            };
            let (minimum, _) = audit_select_candidate(
                &graph,
                config(),
                &candidates,
                &event.before.weight_exponents,
            )
            .expect("minimum");
            assert_eq!(branch.as_ref(), &minimum);
            assert_eq!(
                branch.weighted_tree_stretch,
                audit_weighted_tree_stretch(&branch.weight_copy_counts, &branch.tree_stretches)
                    .expect("score")
            );
            assert!(
                candidates.iter().all(|candidate| {
                    audit_weighted_tree_stretch(
                        &branch.weight_copy_counts,
                        &candidate.tree_stretches,
                    )
                    .is_ok_and(|score| score >= branch.weighted_tree_stretch)
                }),
                "round {round} did not select a minimum-stretch copy-graph tree"
            );
            assert!(branch.weight_copy_counts.iter().all(|&copies| copies > 0));
            assert!(
                branch.weight_copy_counts.iter().sum::<u64>()
                    <= 2 * u64::try_from(graph.edges.len()).expect("edge count")
            );
        }
        assert_eq!(trace.result.branches[0].weight_copy_counts, vec![1; 5]);
        assert!(
            trace
                .result
                .branches
                .iter()
                .skip(1)
                .any(|branch| branch.weight_copy_counts.iter().any(|&copies| copies > 1))
        );
    }

    #[test]
    fn selected_lsst_is_materialized_as_the_exact_multi_edge_hld_refinement() {
        let graph = reordered_path();
        let result = build_low_stretch_forest_mwu_collection(
            &graph,
            LowStretchForestMwuConfig { rounds: 1 },
        )
        .expect("MWU");
        let selected = &result.branches[0];
        assert_eq!(selected.reference_root, 0);
        assert!(selected.root_seeds.binary_search(&0).is_ok());
        assert!(selected.root_seeds.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(selected.weight_copy_counts, vec![1; 3]);
        assert_eq!(selected.weighted_tree_stretch, rational(6));
        assert_eq!(selected.measured_lsst_gamma, rational(2));
        assert_eq!(selected.decomposition_volume_limit, 1);
        assert!(
            selected
                .tree_partition
                .iter()
                .all(|piece| piece.adjacent_non_root_edges.len() <= 1)
        );
        assert_eq!(selected.tree_mask.count_ones(), 3);
        assert!((selected.tree_mask ^ selected.forest_mask).count_ones() >= 2);

        let input = dynamic_lsf_input(
            &graph,
            selected.tree_mask,
            selected.root_seeds.clone(),
            Some(
                selected
                    .stretch_overestimates
                    .iter()
                    .cloned()
                    .map(Some)
                    .collect(),
            ),
        )
        .expect("input");
        let dynamic = execute_dynamic_low_stretch_forest(&input, &[]).expect("dynamic LSF");
        let exact_mask = dynamic
            .final_snapshot
            .forest_edges
            .iter()
            .fold(0_u64, |mask, &edge| mask | (1_u64 << edge));
        assert_eq!(selected.forest_mask, exact_mask);
        assert_eq!(
            selected.stretch_overestimates,
            dynamic
                .final_snapshot
                .stretch_overestimates
                .iter()
                .cloned()
                .collect::<Option<Vec<_>>>()
                .expect("stretch upper bounds")
        );
        assert_eq!(
            selected.tree_partition,
            tree_partition(&graph, selected.tree_mask, &dynamic.final_snapshot).expect("partition")
        );
    }

    #[test]
    fn large_stretch_endpoints_are_explicit_roots_before_decomposition() {
        let graph = parallel_large_stretch_edge();
        let result = build_low_stretch_forest_mwu_collection(
            &graph,
            LowStretchForestMwuConfig { rounds: 1 },
        )
        .expect("MWU");
        let selected = &result.branches[0];
        assert_eq!(selected.measured_lsst_gamma, rational(301) / rational(200));
        assert_eq!(
            selected.large_stretch_threshold,
            selected.measured_lsst_gamma
        );
        assert!(!selected.large_stretch_edges.is_empty());
        assert_eq!(selected.root_seeds, vec![0, 1]);
        assert!(selected.decomposition_seeds.is_empty());
        assert_eq!(selected.forest_mask, 0);
        assert!(selected.tree_partition.iter().all(|piece| {
            piece.vertices == vec![piece.root] && piece.adjacent_non_root_edges.is_empty()
        }));
    }

    #[test]
    fn checker_rejects_branch_and_exponent_tampering() {
        let graph = asymmetric_diamond();
        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.candidate_index += 1;
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.reference_root = 1;
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.root_seeds = vec![0, 2];
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.weight_copy_counts[0] += 1;
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.weighted_tree_stretch += BigRational::one();
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.large_stretch_threshold += BigRational::one();
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        let LowStretchForestMwuEventKind::ForestSelected { branch, .. } = &mut trace.events[0].kind
        else {
            panic!("selection");
        };
        branch.tree_partition[0].adjacent_non_root_edges.push(0);
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );

        let mut trace = trace_low_stretch_forest_mwu_collection(&graph, config()).expect("trace");
        trace.events[0].after.weight_exponents[0] += BigRational::one();
        assert_eq!(
            check_low_stretch_forest_mwu_trace(&graph, config(), &trace),
            Err(LowStretchForestMwuError::TraceVerification)
        );
    }

    #[test]
    fn rejects_disconnected_nonpositive_and_round_overflow() {
        let mut graph = asymmetric_diamond();
        graph.edges.truncate(1);
        assert_eq!(
            build_low_stretch_forest_mwu_collection(&graph, config()),
            Err(LowStretchForestMwuError::Disconnected)
        );
        let mut graph = asymmetric_diamond();
        graph.edges[0].length = BigRational::zero();
        assert_eq!(
            build_low_stretch_forest_mwu_collection(&graph, config()),
            Err(LowStretchForestMwuError::InvalidInput)
        );
        let config = LowStretchForestMwuConfig {
            rounds: LOW_STRETCH_FOREST_MWU_MAX_ROUNDS + 1,
        };
        assert_eq!(
            build_low_stretch_forest_mwu_collection(&asymmetric_diamond(), config),
            Err(LowStretchForestMwuError::AdmissionLimit)
        );
    }

    #[test]
    fn exponential_interval_contains_known_values_and_separates_signs() {
        let zero = BigRational::zero();
        let (lower, upper) = exp_interval(&zero, 4).expect("interval");
        assert_eq!(lower, BigRational::one());
        assert_eq!(upper, BigRational::one());

        let exponents = vec![BigRational::zero(), BigRational::new(1.into(), 20.into())];
        let left = vec![rational(2), rational(1)];
        let right = vec![rational(1), rational(2)];
        assert_eq!(
            compare_exponential_objectives(&exponents, &left, &right)
                .expect("comparison")
                .0,
            Ordering::Less
        );
    }
}
