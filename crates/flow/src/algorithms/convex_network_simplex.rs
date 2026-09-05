//! Pasche's combined-pivot network simplex for piecewise-linear convex costs.
//!
//! The kernel stores one state and at most one basic segment per original arc.
//! A negative forward/backward reduced cost creates a fundamental cycle.  The
//! flow then crosses as many segment breakpoints as remain improving before a
//! single basis exchange is performed.  Thus the segment-expanded solver is an
//! independent oracle, never the implementation of this state machine.

use thiserror::Error;

use crate::certificate::{CertificateError, divergences, supply_divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};

use super::{
    ConvexCostCertificate, ConvexCostError, ConvexCostProblem, ConvexResidualDirection,
    ConvexSegmentState, check_convex_cost_flow, solve_segment_expanded_convex_cost,
};

/// Conservative original-node limit for the explicit combined-pivot kernel.
pub const CONVEX_SIMPLEX_MAX_NODES: usize = 64;
/// Conservative original-edge limit for the explicit combined-pivot kernel.
pub const CONVEX_SIMPLEX_MAX_EDGES: usize = 512;
/// Conservative total declared-segment limit.
pub const CONVEX_SIMPLEX_MAX_SEGMENTS: usize = 1_024;
/// Deterministic ceiling for completed combined pivots.
pub const CONVEX_SIMPLEX_MAX_PIVOTS: u64 = 100_000;
/// Deterministic ceiling for state changes at breakpoints.
pub const CONVEX_SIMPLEX_MAX_BREAKPOINT_CROSSINGS: u64 = 200_000;
/// Deterministic ceiling for pricing and fundamental-cycle arc scans.
pub const CONVEX_SIMPLEX_MAX_ARC_SCANS: u128 = 20_000_000;

/// Exact source-level work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConvexNetworkSimplexMetrics {
    /// Complete scans for a negative non-tree direction.
    pub pricing_searches: u64,
    /// Forward/reverse non-tree directions inspected.
    pub pricing_arc_scans: u128,
    /// Negative fundamental cycles selected.
    pub combined_pivots: u64,
    /// Flow-changing breakpoint steps inside all combined pivots.
    pub nondegenerate_crossings: u64,
    /// Zero-flow state changes forced by simultaneous breakpoints.
    pub degenerate_crossings: u64,
    /// Total original/artificial breakpoint state changes.
    pub breakpoint_crossings: u64,
    /// Pivots ending with one tree exchange.
    pub basis_exchanges: u64,
    /// Pivots whose entering arc remained nonbasic at its next breakpoint.
    pub bound_flips: u64,
    /// Directed arcs inspected while constructing or advancing cycles.
    pub cycle_arc_scans: u128,
    /// Explicit rooted-tree and potential reconstructions.
    pub tree_rebuilds: u64,
    /// Combined pivots that crossed more than one breakpoint.
    pub multi_crossing_pivots: u64,
}

/// Publication boundary of the combined-pivot state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvexNetworkSimplexStage {
    /// Big-M artificial-root star and compact arc states were installed.
    InitializeBasis,
    /// Forward/backward reduced costs of non-tree arcs were priced.
    Price,
    /// One negative fundamental cycle was formed.
    FormCycle,
    /// The cycle reached one selected segment or global breakpoint.
    CrossBreakpoint,
    /// The initial entering arc replaced the final leaving tree arc.
    ExchangeBasis,
    /// The entering arc reached a segment or global bound without exchange.
    FlipBound,
    /// Independent native and expanded certificates agree.
    Optimal,
}

/// Basis status of one compact original edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvexNetworkSimplexBasisState {
    /// Edge is one of the extended spanning-tree arcs.
    Tree,
    /// Edge is nonbasic at an effective breakpoint.
    Breakpoint,
}

/// Stable reference to one directed compact marginal arc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConvexNetworkSimplexArcRef {
    /// One original edge and currently selected declared segment.
    Original {
        /// Canonical original-edge ordinal.
        edge: usize,
        /// Canonical declared-segment ordinal.
        segment: usize,
        /// Direction of the cycle modification.
        direction: ConvexResidualDirection,
    },
    /// One artificial-root star arc, identified by its original node.
    Artificial {
        /// Canonical original-node ordinal.
        node: usize,
        /// Direction of traversal relative to the artificial arc.
        direction: ConvexResidualDirection,
    },
}

/// Replay-visible compact state of one original edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexEdgeState {
    /// Canonical original-edge ordinal.
    pub edge: usize,
    /// Aggregate actual flow, including the original lower bound.
    pub flow: u64,
    /// Tree or nonbasic-breakpoint state.
    pub basis: ConvexNetworkSimplexBasisState,
    /// Declared segment whose slope is active when the edge is basic.
    pub active_segment: Option<usize>,
}

/// Replay-visible artificial-root edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexArtificialState {
    /// Original node incident to the artificial root edge.
    pub node: usize,
    /// Extended source node ordinal.
    pub source: usize,
    /// Extended target node ordinal.
    pub target: usize,
    /// Current artificial flow.
    pub flow: i128,
    /// Whether the artificial edge remains in the basis tree.
    pub tree: bool,
}

/// Complete deterministic replay boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexSnapshot {
    /// Aggregate actual original-edge flows.
    pub flows: Vec<u64>,
    /// Canonical prefix occupancy of all declared segments.
    pub segments: Vec<ConvexSegmentState>,
    /// Compact original-edge basis and active-segment states.
    pub edges: Vec<ConvexNetworkSimplexEdgeState>,
    /// Artificial-root star state in original-node order.
    pub artificial_edges: Vec<ConvexNetworkSimplexArtificialState>,
    /// Extended-node potentials, with the artificial root last.
    pub potentials: Vec<i128>,
    /// Parent extended-node ordinal for each node; root has no parent.
    pub parents: Vec<Option<usize>>,
    /// Ordered active cycle starting at the join and following its orientation.
    pub active_cycle: Vec<ConvexNetworkSimplexArcRef>,
    /// Initial non-tree direction selected by pricing.
    pub entering: Option<ConvexNetworkSimplexArcRef>,
    /// Most recently selected Cunningham breakpoint arc.
    pub leaving: Option<ConvexNetworkSimplexArcRef>,
    /// Exact source-level counters.
    pub metrics: ConvexNetworkSimplexMetrics,
}

/// One atomic source-level transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexTraceEvent {
    /// Semantic stage closed by this transition.
    pub stage: ConvexNetworkSimplexStage,
    /// Exact stage-specific scalar.
    pub detail: Option<(&'static str, i128)>,
    /// State before the transition.
    pub before: ConvexNetworkSimplexSnapshot,
    /// State after the transition.
    pub after: ConvexNetworkSimplexSnapshot,
}

/// Certified result of the native compact simplex.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexResult {
    /// Aggregate actual original-edge flows.
    pub flows: Vec<u64>,
    /// Canonical declared-segment occupancy by original edge.
    pub segment_flows: Vec<Vec<u64>>,
    /// Independently reconstructed convex optimum certificate.
    pub certificate: ConvexCostCertificate,
    /// Exact combined-pivot counters.
    pub metrics: ConvexNetworkSimplexMetrics,
    /// Strict cost assigned to every artificial root arc.
    pub artificial_cost: i128,
    /// Certified terminal compact basis used by the fast publication profile.
    pub final_snapshot: ConvexNetworkSimplexSnapshot,
}

/// Certified result plus complete deterministic native trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexNetworkSimplexTraceResult {
    /// Same result produced by the fast profile.
    pub result: ConvexNetworkSimplexResult,
    /// Initial artificial-root star boundary.
    pub base_snapshot: ConvexNetworkSimplexSnapshot,
    /// Complete source-level transition sequence.
    pub events: Vec<ConvexNetworkSimplexTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: ConvexNetworkSimplexSnapshot,
}

/// Construction, arithmetic, replay, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConvexNetworkSimplexError {
    /// Input exceeds the conservative interactive band.
    #[error("convex network-simplex input exceeds admission limits")]
    AdmissionLimit,
    /// A deterministic pivot, crossing, or scan ceiling was reached.
    #[error("convex network-simplex work limit reached")]
    WorkLimit,
    /// The lower-bounded transshipment is infeasible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Divergence or native certificate reconstruction failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Convex model, checker, or expanded oracle failed.
    #[error(transparent)]
    Convex(#[from] ConvexCostError),
    /// Checked integer arithmetic exceeded its declared domain.
    #[error("convex network-simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// Tree, compact segment, conservation, or breakpoint state is invalid.
    #[error("convex network-simplex basis invariant failed")]
    BasisInvariant,
    /// Artificial flow remained despite a successful feasibility precheck.
    #[error("convex network-simplex terminated with artificial flow")]
    ArtificialFlow,
    /// Native result and independent expanded oracle disagree.
    #[error("convex network-simplex disagrees with expanded oracle")]
    OracleMismatch,
    /// Supplied trace differs from deterministic source-level replay.
    #[error("convex network-simplex trace invariant failed")]
    TraceInvariant,
}

/// Solves the convex transshipment by Pasche's compact combined pivots.
///
/// # Errors
///
/// Rejects admission, feasibility, arithmetic, work-limit, basis, artificial
/// flow, convex-certificate, or independent-oracle failures.
pub fn solve_convex_network_simplex(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexNetworkSimplexResult, ConvexNetworkSimplexError> {
    run_internal(problem, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_convex_network_simplex_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexNetworkSimplexResult, ConvexNetworkSimplexError> {
    run_internal_with_feasibility(problem, false, feasibility).map(|run| run.result)
}

/// Records pricing, cycle formation, every breakpoint, and the one final exchange.
///
/// # Errors
///
/// Returns the same failures as [`solve_convex_network_simplex`] plus replay
/// invariant failures.
pub fn trace_convex_network_simplex(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexNetworkSimplexTraceResult, ConvexNetworkSimplexError> {
    let run = run_internal(problem, true)?;
    let trace = ConvexNetworkSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_convex_network_simplex_trace(problem, &trace)?;
    Ok(trace)
}

/// Traces convex network simplex while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_convex_network_simplex_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexNetworkSimplexTraceResult, ConvexNetworkSimplexError> {
    let run = run_internal_with_feasibility(problem, true, feasibility)?;
    let trace = ConvexNetworkSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_convex_network_simplex_trace(problem, &trace)?;
    Ok(trace)
}

/// Independently checks the compact trace and both terminal certificates.
///
/// # Errors
///
/// Rejects any modified stage, scalar, compact basis, segment state, or result.
pub fn check_convex_network_simplex_trace(
    problem: &ConvexCostProblem<'_>,
    trace: &ConvexNetworkSimplexTraceResult,
) -> Result<(), ConvexNetworkSimplexError> {
    validate_admission(problem)?;
    if trace.base_snapshot != expected_convex_simplex_base(problem)?
        || trace.events.windows(2).any(|pair| {
            pair[0].after != pair[1].before
                || !valid_convex_simplex_stage_order(pair[0].stage, pair[1].stage)
        })
        || trace
            .events
            .first()
            .is_some_and(|event| event.before != trace.base_snapshot)
        || trace
            .events
            .last()
            .is_some_and(|event| event.after != trace.final_snapshot)
        || trace.events.is_empty()
        || trace.events.first().map(|event| event.stage)
            != Some(ConvexNetworkSimplexStage::InitializeBasis)
        || trace.events.last().map(|event| event.stage) != Some(ConvexNetworkSimplexStage::Optimal)
        || !valid_convex_simplex_crossing_counts(&trace.events)
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.flows != trace.result.flows
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.final_snapshot.flows.len() != problem.graph().edges().len()
        || trace.final_snapshot.edges.len() != problem.graph().edges().len()
        || trace.events.iter().any(|event| {
            !valid_convex_simplex_event(
                problem,
                event,
                trace.result.artificial_cost,
                trace.result.certificate.total_cost,
            ) || event.before.flows.len() != problem.graph().edges().len()
                || event.after.flows.len() != problem.graph().edges().len()
                || event.after.potentials.len() != problem.graph().nodes().len() + 1
                || event.after.parents.len() != problem.graph().nodes().len() + 1
                || event.after.artificial_edges.len() != problem.graph().nodes().len()
                || event
                    .after
                    .flows
                    .iter()
                    .zip(problem.graph().edges())
                    .any(|(flow, edge)| *flow < edge.lower() || *flow > edge.capacity())
                || event
                    .after
                    .edges
                    .iter()
                    .enumerate()
                    .any(|(index, state)| state.edge != index)
        })
    {
        return Err(ConvexNetworkSimplexError::TraceInvariant);
    }
    let certificate = check_convex_cost_flow(problem, &trace.result.flows)?;
    // This is an independent terminal checker, not a source-algorithm step.
    // Keep executing it fail-closed, but do not publish its duplicate
    // feasibility solve as work performed by the compact simplex kernel.
    let oracle = solve_segment_expanded_convex_cost(problem)?;
    if certificate != trace.result.certificate
        || oracle.certificate.total_cost != certificate.total_cost
        || trace
            .final_snapshot
            .artificial_edges
            .iter()
            .any(|edge| edge.flow != 0)
    {
        return Err(ConvexNetworkSimplexError::TraceInvariant);
    }
    Ok(())
}

fn expected_convex_simplex_base(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexNetworkSimplexSnapshot, ConvexNetworkSimplexError> {
    let graph = problem.graph();
    let required = supply_divergences(graph)?;
    let flows = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &flows)?;
    let balance = required
        .iter()
        .zip(lower_divergence)
        .map(|(&target, actual)| {
            target
                .checked_sub(actual)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let max_slope = problem
        .edge_costs()
        .iter()
        .flat_map(|cost| &cost.segments)
        .map(|segment| i128::from(segment.marginal_cost).abs())
        .max()
        .unwrap_or(0);
    let artificial_cost = i128::try_from(graph.nodes().len())
        .map_err(|_| ConvexNetworkSimplexError::ArithmeticOverflow)?
        .checked_add(1)
        .and_then(|factor| max_slope.checked_add(1)?.checked_mul(factor))
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
    let root = graph.nodes().len();
    let ExpectedConvexSimplexArtificialStar {
        edges: artificial_edges,
        potentials,
        parents,
    } = expected_convex_simplex_artificial_star(&balance, root, artificial_cost)?;
    let (segments, edge_states) = expected_convex_simplex_original_edges(problem, &flows)?;
    Ok(ConvexNetworkSimplexSnapshot {
        flows,
        segments,
        edges: edge_states,
        artificial_edges,
        potentials,
        parents,
        active_cycle: Vec::new(),
        entering: None,
        leaving: None,
        metrics: ConvexNetworkSimplexMetrics {
            tree_rebuilds: 1,
            ..ConvexNetworkSimplexMetrics::default()
        },
    })
}

struct ExpectedConvexSimplexArtificialStar {
    edges: Vec<ConvexNetworkSimplexArtificialState>,
    potentials: Vec<i128>,
    parents: Vec<Option<usize>>,
}

fn expected_convex_simplex_artificial_star(
    balance: &[i128],
    root: usize,
    artificial_cost: i128,
) -> Result<ExpectedConvexSimplexArtificialStar, ConvexNetworkSimplexError> {
    let artificial_edges = balance
        .iter()
        .enumerate()
        .map(|(node, &value)| {
            let (source, target, flow) = if value >= 0 {
                (node, root, value)
            } else {
                (
                    root,
                    node,
                    value
                        .checked_neg()
                        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?,
                )
            };
            Ok(ConvexNetworkSimplexArtificialState {
                node,
                source,
                target,
                flow,
                tree: true,
            })
        })
        .collect::<Result<Vec<_>, ConvexNetworkSimplexError>>()?;
    let mut potentials = balance
        .iter()
        .map(|value| {
            if *value >= 0 {
                artificial_cost.checked_neg()
            } else {
                Some(artificial_cost)
            }
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    potentials.push(0);
    let mut parents = vec![Some(root); balance.len()];
    parents.push(None);
    Ok(ExpectedConvexSimplexArtificialStar {
        edges: artificial_edges,
        potentials,
        parents,
    })
}

fn expected_convex_simplex_original_edges(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
) -> Result<(Vec<ConvexSegmentState>, Vec<ConvexNetworkSimplexEdgeState>), ConvexNetworkSimplexError>
{
    let graph = problem.graph();
    let mut segments = Vec::new();
    let mut edge_states = Vec::with_capacity(graph.edges().len());
    for (edge, ((graph_edge, objective), &flow)) in graph
        .edges()
        .iter()
        .zip(problem.edge_costs())
        .zip(flows)
        .enumerate()
    {
        let mut unassigned = flow;
        let mut start = 0_u64;
        let mut active_segment = None;
        for (segment, piece) in objective.segments.iter().enumerate() {
            let amount = unassigned.min(piece.end_flow - start);
            unassigned -= amount;
            segments.push(ConvexSegmentState {
                edge,
                segment,
                start_flow: start,
                end_flow: piece.end_flow,
                flow: amount,
                marginal_cost: piece.marginal_cost,
            });
            if active_segment.is_none()
                && start.max(graph_edge.lower()) < piece.end_flow.min(graph_edge.capacity())
            {
                active_segment = Some(segment);
            }
            start = piece.end_flow;
        }
        if unassigned != 0 {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        edge_states.push(ConvexNetworkSimplexEdgeState {
            edge,
            flow,
            basis: ConvexNetworkSimplexBasisState::Breakpoint,
            active_segment,
        });
    }
    Ok((segments, edge_states))
}

#[allow(clippy::unnested_or_patterns)]
const fn valid_convex_simplex_stage_order(
    before: ConvexNetworkSimplexStage,
    after: ConvexNetworkSimplexStage,
) -> bool {
    matches!(
        (before, after),
        (
            ConvexNetworkSimplexStage::InitializeBasis,
            ConvexNetworkSimplexStage::Price
        ) | (
            ConvexNetworkSimplexStage::Price,
            ConvexNetworkSimplexStage::FormCycle | ConvexNetworkSimplexStage::Optimal
        ) | (
            ConvexNetworkSimplexStage::FormCycle,
            ConvexNetworkSimplexStage::CrossBreakpoint
        ) | (
            ConvexNetworkSimplexStage::CrossBreakpoint,
            ConvexNetworkSimplexStage::CrossBreakpoint
                | ConvexNetworkSimplexStage::ExchangeBasis
                | ConvexNetworkSimplexStage::FlipBound
        ) | (
            ConvexNetworkSimplexStage::ExchangeBasis | ConvexNetworkSimplexStage::FlipBound,
            ConvexNetworkSimplexStage::Price
        )
    )
}

fn valid_convex_simplex_crossing_counts(events: &[ConvexNetworkSimplexTraceEvent]) -> bool {
    let mut crossings = 0_i128;
    for event in events {
        match event.stage {
            ConvexNetworkSimplexStage::FormCycle => crossings = 0,
            ConvexNetworkSimplexStage::CrossBreakpoint => crossings += 1,
            ConvexNetworkSimplexStage::ExchangeBasis | ConvexNetworkSimplexStage::FlipBound => {
                if event.detail != Some(("crossings", crossings)) {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

fn valid_convex_simplex_event(
    problem: &ConvexCostProblem<'_>,
    event: &ConvexNetworkSimplexTraceEvent,
    artificial_cost: i128,
    total_cost: i128,
) -> bool {
    match event.stage {
        ConvexNetworkSimplexStage::InitializeBasis => {
            let artificial_flow = event
                .after
                .artificial_edges
                .iter()
                .try_fold(0_i128, |sum, edge| sum.checked_add(edge.flow));
            artificial_flow.is_some_and(|flow| event.detail == Some(("artificial-flow", flow)))
                && event.after.metrics == event.before.metrics
        }
        ConvexNetworkSimplexStage::Price => {
            let expected = event.after.entering.as_ref().and_then(|entering| {
                convex_simplex_reduced_cost(problem, &event.after, entering, artificial_cost)
                    .map(|value| ("reduced-cost", value))
            });
            event.detail == expected
                && event.after.metrics.pricing_searches
                    == event.before.metrics.pricing_searches.saturating_add(1)
                && event.after.entering.is_some() == event.detail.is_some()
                && event.detail.is_none_or(|(_, value)| value < 0)
        }
        ConvexNetworkSimplexStage::FormCycle => {
            let cost = event
                .after
                .active_cycle
                .iter()
                .try_fold(0_i128, |sum, arc| {
                    convex_simplex_arc_cost(problem, &event.after, arc, artificial_cost)
                        .and_then(|value| sum.checked_add(value))
                });
            cost.is_some_and(|cost| cost < 0 && event.detail == Some(("cycle-cost", cost)))
                && event.after.metrics.combined_pivots
                    == event.before.metrics.combined_pivots.saturating_add(1)
                && !event.after.active_cycle.is_empty()
        }
        ConvexNetworkSimplexStage::CrossBreakpoint => {
            convex_simplex_crossing_delta(event)
                .is_some_and(|delta| event.detail == Some(("delta", delta)))
                && event.after.metrics.breakpoint_crossings
                    == event.before.metrics.breakpoint_crossings.saturating_add(1)
                && event.after.leaving.is_some()
                && event.after.active_cycle == event.before.active_cycle
        }
        ConvexNetworkSimplexStage::ExchangeBasis | ConvexNetworkSimplexStage::FlipBound => {
            matches!(event.detail, Some(("crossings", crossings)) if crossings > 0)
                && event.after.metrics.breakpoint_crossings
                    == event.before.metrics.breakpoint_crossings
                && match event.stage {
                    ConvexNetworkSimplexStage::ExchangeBasis => {
                        event.after.metrics.basis_exchanges
                            == event.before.metrics.basis_exchanges.saturating_add(1)
                    }
                    ConvexNetworkSimplexStage::FlipBound => {
                        event.after.metrics.bound_flips
                            == event.before.metrics.bound_flips.saturating_add(1)
                    }
                    _ => false,
                }
        }
        ConvexNetworkSimplexStage::Optimal => {
            event.detail == Some(("total-cost", total_cost))
                && event.after.metrics == event.before.metrics
                && event.after.active_cycle.is_empty()
                && event.after.entering.is_none()
                && event.after.leaving.is_none()
        }
    }
}

fn convex_simplex_crossing_delta(event: &ConvexNetworkSimplexTraceEvent) -> Option<i128> {
    match event.after.leaving.as_ref()? {
        ConvexNetworkSimplexArcRef::Original { edge, .. } => {
            let before = *event.before.flows.get(*edge)?;
            let after = *event.after.flows.get(*edge)?;
            Some(i128::from(before.abs_diff(after)))
        }
        ConvexNetworkSimplexArcRef::Artificial { node, .. } => {
            let before = event.before.artificial_edges.get(*node)?.flow;
            let after = event.after.artificial_edges.get(*node)?.flow;
            after.checked_sub(before)?.checked_abs()
        }
    }
}

fn convex_simplex_reduced_cost(
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexNetworkSimplexSnapshot,
    arc: &ConvexNetworkSimplexArcRef,
    artificial_cost: i128,
) -> Option<i128> {
    let (from, to) = convex_simplex_arc_endpoints(problem, snapshot, arc)?;
    convex_simplex_arc_cost(problem, snapshot, arc, artificial_cost)?
        .checked_add(*snapshot.potentials.get(from)?)?
        .checked_sub(*snapshot.potentials.get(to)?)
}

fn convex_simplex_arc_endpoints(
    problem: &ConvexCostProblem<'_>,
    snapshot: &ConvexNetworkSimplexSnapshot,
    arc: &ConvexNetworkSimplexArcRef,
) -> Option<(usize, usize)> {
    let (from, to, direction) = match arc {
        ConvexNetworkSimplexArcRef::Original {
            edge, direction, ..
        } => {
            let edge = problem.graph().edges().get(*edge)?;
            (edge.from().as_usize(), edge.to().as_usize(), *direction)
        }
        ConvexNetworkSimplexArcRef::Artificial { node, direction } => {
            let edge = snapshot.artificial_edges.get(*node)?;
            (edge.source, edge.target, *direction)
        }
    };
    Some(match direction {
        ConvexResidualDirection::Forward => (from, to),
        ConvexResidualDirection::Reverse => (to, from),
    })
}

fn convex_simplex_arc_cost(
    problem: &ConvexCostProblem<'_>,
    _snapshot: &ConvexNetworkSimplexSnapshot,
    arc: &ConvexNetworkSimplexArcRef,
    artificial_cost: i128,
) -> Option<i128> {
    let (cost, direction) = match arc {
        ConvexNetworkSimplexArcRef::Original {
            edge,
            segment,
            direction,
        } => (
            i128::from(
                problem
                    .edge_costs()
                    .get(*edge)?
                    .segments
                    .get(*segment)?
                    .marginal_cost,
            ),
            *direction,
        ),
        ConvexNetworkSimplexArcRef::Artificial { direction, .. } => (artificial_cost, *direction),
    };
    match direction {
        ConvexResidualDirection::Forward => Some(cost),
        ConvexResidualDirection::Reverse => cost.checked_neg(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BasisState {
    Tree,
    Breakpoint,
}

#[derive(Clone, Debug)]
struct Piece {
    declared_segment: usize,
    start: i128,
    end: i128,
    cost: i128,
}

#[derive(Clone, Debug)]
struct ArcData {
    source: usize,
    target: usize,
    capacity: i128,
    flow: i128,
    basis: BasisState,
    pieces: Vec<Piece>,
    active_piece: Option<usize>,
    original: Option<usize>,
    artificial_node: Option<usize>,
    artificial_cost: i128,
}

impl ArcData {
    fn selected_cost(&self) -> Result<i128, ConvexNetworkSimplexError> {
        if self.original.is_none() {
            return Ok(self.artificial_cost);
        }
        self.active_piece
            .and_then(|piece| self.pieces.get(piece))
            .map(|piece| piece.cost)
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)
    }

    fn directed_cost(&self, forward: bool) -> Result<i128, ConvexNetworkSimplexError> {
        let cost = self.selected_cost()?;
        if forward {
            Ok(cost)
        } else {
            cost.checked_neg()
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        }
    }
}

#[derive(Clone, Debug)]
struct RootedTree {
    root: usize,
    parent: Vec<Option<usize>>,
    parent_arc: Vec<Option<usize>>,
    depth: Vec<usize>,
    potentials: Vec<i128>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectedArc {
    arc: usize,
    forward: bool,
}

#[derive(Clone, Debug)]
struct Cycle {
    entering: usize,
    arcs: Vec<DirectedArc>,
}

#[derive(Clone, Debug)]
struct Pricing {
    entering: Option<DirectedArc>,
    reduced_cost: Option<i128>,
}

struct WorkingState<'graph> {
    problem: &'graph ConvexCostProblem<'graph>,
    arcs: Vec<ArcData>,
    original_arc_count: usize,
    tree: RootedTree,
    artificial_cost: i128,
    active_cycle: Vec<ConvexNetworkSimplexArcRef>,
    entering: Option<ConvexNetworkSimplexArcRef>,
    leaving: Option<ConvexNetworkSimplexArcRef>,
    metrics: ConvexNetworkSimplexMetrics,
}

struct InternalRun {
    result: ConvexNetworkSimplexResult,
    base_snapshot: ConvexNetworkSimplexSnapshot,
    events: Vec<ConvexNetworkSimplexTraceEvent>,
    final_snapshot: ConvexNetworkSimplexSnapshot,
}

struct InitializationData {
    balance: Vec<i128>,
    artificial_cost: i128,
    artificial_capacity: i128,
}

fn run_internal<'graph>(
    problem: &'graph ConvexCostProblem<'graph>,
    trace_enabled: bool,
) -> Result<InternalRun, ConvexNetworkSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(problem, trace_enabled, &mut feasibility)
}

fn run_internal_with_feasibility<'graph>(
    problem: &'graph ConvexCostProblem<'graph>,
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, ConvexNetworkSimplexError> {
    let (mut work, base_snapshot, mut events) =
        initialize_run(problem, trace_enabled, feasibility)?;
    while let Some(entering) = price_next(&mut work, &mut events, trace_enabled)? {
        perform_combined_pivot(&mut work, &mut events, trace_enabled, entering)?;
    }
    finish_run(problem, work, events, base_snapshot, trace_enabled)
}

fn initialize_run<'graph>(
    problem: &'graph ConvexCostProblem<'graph>,
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<
    (
        WorkingState<'graph>,
        ConvexNetworkSimplexSnapshot,
        Vec<ConvexNetworkSimplexTraceEvent>,
    ),
    ConvexNetworkSimplexError,
> {
    validate_admission(problem)?;
    let required = supply_divergences(problem.graph())?;
    feasibility.find_feasible_flow(problem.graph(), &required, FeasibilityUse::PrecheckOnly)?;
    let mut work = WorkingState::initialize(problem, &required)?;
    let base_snapshot = snapshot(&work)?;
    let mut events = Vec::new();
    let initial_artificial_flow = work.total_artificial_flow()?;
    record(
        &mut work,
        &mut events,
        trace_enabled,
        ConvexNetworkSimplexStage::InitializeBasis,
        Some(("artificial-flow", initial_artificial_flow)),
        |_| Ok(()),
    )?;
    Ok((work, base_snapshot, events))
}

fn price_next(
    work: &mut WorkingState<'_>,
    events: &mut Vec<ConvexNetworkSimplexTraceEvent>,
    trace_enabled: bool,
) -> Result<Option<DirectedArc>, ConvexNetworkSimplexError> {
    let (pricing, pricing_scans) = work.price()?;
    record(
        work,
        events,
        trace_enabled,
        ConvexNetworkSimplexStage::Price,
        pricing.reduced_cost.map(|value| ("reduced-cost", value)),
        |state| {
            state.metrics.pricing_searches += 1;
            state.add_pricing_scans(pricing_scans)?;
            state.active_cycle.clear();
            state.leaving = None;
            if let Some(entering) = pricing.entering {
                state.prepare_entering_piece(entering)?;
                state.entering = Some(state.arc_ref(entering)?);
            } else {
                state.entering = None;
            }
            Ok(())
        },
    )?;
    Ok(pricing.entering)
}

fn perform_combined_pivot(
    work: &mut WorkingState<'_>,
    events: &mut Vec<ConvexNetworkSimplexTraceEvent>,
    trace_enabled: bool,
    entering: DirectedArc,
) -> Result<(), ConvexNetworkSimplexError> {
    if work.metrics.combined_pivots >= CONVEX_SIMPLEX_MAX_PIVOTS {
        return Err(ConvexNetworkSimplexError::WorkLimit);
    }
    let cycle = work.form_cycle(entering)?;
    let initial_cost = work.cycle_cost(&cycle)?;
    if initial_cost >= 0 {
        return Err(ConvexNetworkSimplexError::BasisInvariant);
    }
    record(
        work,
        events,
        trace_enabled,
        ConvexNetworkSimplexStage::FormCycle,
        Some(("cycle-cost", initial_cost)),
        |state| {
            state.metrics.combined_pivots += 1;
            state.add_cycle_scans(cycle.arcs.len().saturating_mul(2))?;
            state.active_cycle = cycle
                .arcs
                .iter()
                .map(|&arc| state.arc_ref(arc))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(())
        },
    )?;
    let (leaving, crossings) = cross_cycle_breakpoints(work, events, trace_enabled, &cycle)?;
    close_combined_pivot(work, events, trace_enabled, &cycle, leaving, crossings)
}

fn cross_cycle_breakpoints(
    work: &mut WorkingState<'_>,
    events: &mut Vec<ConvexNetworkSimplexTraceEvent>,
    trace_enabled: bool,
    cycle: &Cycle,
) -> Result<(DirectedArc, u64), ConvexNetworkSimplexError> {
    let mut crossings = 0_u64;
    loop {
        let (delta, selected) = work.next_breakpoint(cycle)?;
        let selected_ref = work.arc_ref(selected)?;
        let terminal = work.crosses_global_bound(selected, delta)?;
        record(
            work,
            events,
            trace_enabled,
            ConvexNetworkSimplexStage::CrossBreakpoint,
            Some(("delta", delta)),
            |state| {
                state.augment_cycle(cycle, delta)?;
                if !terminal {
                    state.advance_piece(selected)?;
                }
                state.leaving = Some(selected_ref.clone());
                state.count_crossing(delta)?;
                state.add_cycle_scans(
                    cycle
                        .arcs
                        .len()
                        .saturating_mul(if terminal { 1 } else { 2 }),
                )
            },
        )?;
        crossings += 1;
        if terminal || work.cycle_cost(cycle)? >= 0 {
            return Ok((selected, crossings));
        }
    }
}

fn close_combined_pivot(
    work: &mut WorkingState<'_>,
    events: &mut Vec<ConvexNetworkSimplexTraceEvent>,
    trace_enabled: bool,
    cycle: &Cycle,
    leaving: DirectedArc,
    crossings: u64,
) -> Result<(), ConvexNetworkSimplexError> {
    let entering_leaves = leaving.arc == cycle.entering;
    let stage = if entering_leaves {
        ConvexNetworkSimplexStage::FlipBound
    } else {
        ConvexNetworkSimplexStage::ExchangeBasis
    };
    record(
        work,
        events,
        trace_enabled,
        stage,
        Some(("crossings", i128::from(crossings))),
        |state| {
            if entering_leaves {
                state.rebuild_tree()?;
                state.metrics.bound_flips += 1;
            } else {
                state.exchange_basis(cycle.entering, leaving.arc)?;
                state.metrics.basis_exchanges += 1;
            }
            if crossings > 1 {
                state.metrics.multi_crossing_pivots += 1;
            }
            state.validate_basis()
        },
    )
}

fn finish_run(
    problem: &ConvexCostProblem<'_>,
    mut work: WorkingState<'_>,
    mut events: Vec<ConvexNetworkSimplexTraceEvent>,
    base_snapshot: ConvexNetworkSimplexSnapshot,
    trace_enabled: bool,
) -> Result<InternalRun, ConvexNetworkSimplexError> {
    if work.total_artificial_flow()? != 0 {
        return Err(ConvexNetworkSimplexError::ArtificialFlow);
    }
    work.validate_basis()?;
    let flows = work.actual_flows()?;
    let certificate = check_convex_cost_flow(problem, &flows)?;
    // The expanded solver is an independent terminal checker rather than a
    // compact-simplex source step. It remains fail-closed, while its duplicate
    // feasibility construction stays outside the source trace.
    let oracle = solve_segment_expanded_convex_cost(problem)?;
    if certificate.total_cost != oracle.certificate.total_cost {
        return Err(ConvexNetworkSimplexError::OracleMismatch);
    }
    record(
        &mut work,
        &mut events,
        trace_enabled,
        ConvexNetworkSimplexStage::Optimal,
        Some(("total-cost", certificate.total_cost)),
        |state| {
            state.active_cycle.clear();
            state.entering = None;
            state.leaving = None;
            Ok(())
        },
    )?;
    let final_snapshot = snapshot(&work)?;
    let segment_flows = grouped_segment_flows(problem, &flows)?;
    Ok(InternalRun {
        result: ConvexNetworkSimplexResult {
            flows,
            segment_flows,
            certificate,
            metrics: work.metrics,
            artificial_cost: work.artificial_cost,
            final_snapshot: final_snapshot.clone(),
        },
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn validate_admission(problem: &ConvexCostProblem<'_>) -> Result<(), ConvexNetworkSimplexError> {
    let graph = problem.graph();
    let segments = problem
        .edge_costs()
        .iter()
        .map(|cost| cost.segments.len())
        .sum::<usize>();
    if graph.nodes().len() > CONVEX_SIMPLEX_MAX_NODES
        || graph.edges().len() > CONVEX_SIMPLEX_MAX_EDGES
        || segments > CONVEX_SIMPLEX_MAX_SEGMENTS
    {
        return Err(ConvexNetworkSimplexError::AdmissionLimit);
    }
    Ok(())
}

fn initialization_data(
    problem: &ConvexCostProblem<'_>,
    required: &[i128],
) -> Result<InitializationData, ConvexNetworkSimplexError> {
    let graph = problem.graph();
    let lower = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    let balance = required
        .iter()
        .zip(lower_divergence)
        .map(|(&target, actual)| {
            target
                .checked_sub(actual)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let max_slope = problem
        .edge_costs()
        .iter()
        .flat_map(|cost| &cost.segments)
        .try_fold(0_i128, |maximum, segment| {
            Ok::<_, ConvexNetworkSimplexError>(maximum.max(i128::from(segment.marginal_cost).abs()))
        })?;
    let artificial_cost = i128::try_from(graph.nodes().len())
        .map_err(|_| ConvexNetworkSimplexError::ArithmeticOverflow)?
        .checked_add(1)
        .and_then(|factor| max_slope.checked_add(1)?.checked_mul(factor))
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
    let total_balance = balance.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(
            value
                .checked_abs()
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?,
        )
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
    })?;
    let total_capacity = graph.edges().iter().try_fold(0_i128, |sum, edge| {
        sum.checked_add(i128::from(edge.capacity() - edge.lower()))
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
    })?;
    let artificial_capacity = total_balance
        .checked_add(total_capacity)
        .and_then(|value| value.checked_add(1))
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
    Ok(InitializationData {
        balance,
        artificial_cost,
        artificial_capacity,
    })
}

impl<'graph> WorkingState<'graph> {
    fn initialize(
        problem: &'graph ConvexCostProblem<'graph>,
        required: &[i128],
    ) -> Result<Self, ConvexNetworkSimplexError> {
        let graph = problem.graph();
        let node_count = graph.nodes().len();
        if required.len() != node_count {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        let data = initialization_data(problem, required)?;

        let mut arcs = Vec::with_capacity(graph.edges().len() + node_count);
        for (edge_index, edge) in graph.edges().iter().enumerate() {
            let pieces = effective_pieces(problem, edge_index)?;
            arcs.push(ArcData {
                source: edge.from().as_usize(),
                target: edge.to().as_usize(),
                capacity: i128::from(edge.capacity() - edge.lower()),
                flow: 0,
                basis: BasisState::Breakpoint,
                active_piece: (!pieces.is_empty()).then_some(0),
                pieces,
                original: Some(edge_index),
                artificial_node: None,
                artificial_cost: 0,
            });
        }
        let root = node_count;
        for (node, &value) in data.balance.iter().enumerate() {
            let (source, target, flow) = if value >= 0 {
                (node, root, value)
            } else {
                (
                    root,
                    node,
                    value
                        .checked_neg()
                        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?,
                )
            };
            arcs.push(ArcData {
                source,
                target,
                capacity: data.artificial_capacity,
                flow,
                basis: BasisState::Tree,
                pieces: Vec::new(),
                active_piece: None,
                original: None,
                artificial_node: Some(node),
                artificial_cost: data.artificial_cost,
            });
        }
        let mut state = Self {
            problem,
            original_arc_count: graph.edges().len(),
            arcs,
            tree: RootedTree {
                root,
                parent: Vec::new(),
                parent_arc: Vec::new(),
                depth: Vec::new(),
                potentials: Vec::new(),
            },
            artificial_cost: data.artificial_cost,
            active_cycle: Vec::new(),
            entering: None,
            leaving: None,
            metrics: ConvexNetworkSimplexMetrics::default(),
        };
        state.rebuild_tree()?;
        state.validate_basis()?;
        Ok(state)
    }

    fn count_crossing(&mut self, delta: i128) -> Result<(), ConvexNetworkSimplexError> {
        self.metrics.breakpoint_crossings = self
            .metrics
            .breakpoint_crossings
            .checked_add(1)
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        if delta == 0 {
            self.metrics.degenerate_crossings += 1;
        } else {
            self.metrics.nondegenerate_crossings += 1;
        }
        if self.metrics.breakpoint_crossings > CONVEX_SIMPLEX_MAX_BREAKPOINT_CROSSINGS {
            return Err(ConvexNetworkSimplexError::WorkLimit);
        }
        Ok(())
    }

    fn price(&self) -> Result<(Pricing, usize), ConvexNetworkSimplexError> {
        let mut best = None::<(DirectedArc, i128)>;
        let mut scans = 0_usize;
        for arc_index in 0..self.original_arc_count {
            if self.arcs[arc_index].basis == BasisState::Tree {
                continue;
            }
            for forward in [true, false] {
                scans = scans
                    .checked_add(1)
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
                let Some(piece) = self.marginal_piece(arc_index, forward)? else {
                    continue;
                };
                let reduced = self.directional_reduced_cost(arc_index, forward, piece)?;
                let candidate = DirectedArc {
                    arc: arc_index,
                    forward,
                };
                if reduced < 0
                    && best.is_none_or(|(current, cost)| {
                        reduced < cost
                            || reduced == cost
                                && (arc_index, u8::from(!forward))
                                    < (current.arc, u8::from(!current.forward))
                    })
                {
                    best = Some((candidate, reduced));
                }
            }
        }
        Ok((
            Pricing {
                entering: best.map(|(arc, _)| arc),
                reduced_cost: best.map(|(_, cost)| cost),
            },
            scans,
        ))
    }

    fn add_pricing_scans(&mut self, amount: usize) -> Result<(), ConvexNetworkSimplexError> {
        self.metrics.pricing_arc_scans = self
            .metrics
            .pricing_arc_scans
            .checked_add(
                u128::try_from(amount)
                    .map_err(|_| ConvexNetworkSimplexError::ArithmeticOverflow)?,
            )
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        self.validate_scan_limit()
    }

    fn add_cycle_scans(&mut self, amount: usize) -> Result<(), ConvexNetworkSimplexError> {
        self.metrics.cycle_arc_scans = self
            .metrics
            .cycle_arc_scans
            .checked_add(
                u128::try_from(amount)
                    .map_err(|_| ConvexNetworkSimplexError::ArithmeticOverflow)?,
            )
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        self.validate_scan_limit()
    }

    fn validate_scan_limit(&self) -> Result<(), ConvexNetworkSimplexError> {
        if self
            .metrics
            .pricing_arc_scans
            .checked_add(self.metrics.cycle_arc_scans)
            .is_none_or(|total| total > CONVEX_SIMPLEX_MAX_ARC_SCANS)
        {
            return Err(ConvexNetworkSimplexError::WorkLimit);
        }
        Ok(())
    }

    fn marginal_piece(
        &self,
        arc_index: usize,
        forward: bool,
    ) -> Result<Option<usize>, ConvexNetworkSimplexError> {
        let arc = self
            .arcs
            .get(arc_index)
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        if arc.original.is_none() {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        if forward {
            if arc.flow >= arc.capacity {
                return Ok(None);
            }
            Ok(arc.pieces.iter().position(|piece| arc.flow < piece.end))
        } else {
            if arc.flow <= 0 {
                return Ok(None);
            }
            Ok(arc.pieces.iter().rposition(|piece| arc.flow > piece.start))
        }
    }

    fn directional_reduced_cost(
        &self,
        arc_index: usize,
        forward: bool,
        piece: usize,
    ) -> Result<i128, ConvexNetworkSimplexError> {
        let arc = &self.arcs[arc_index];
        let cost = arc
            .pieces
            .get(piece)
            .map(|entry| entry.cost)
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        if forward {
            cost.checked_add(self.tree.potentials[arc.source])
                .and_then(|value| value.checked_sub(self.tree.potentials[arc.target]))
        } else {
            cost.checked_neg()
                .and_then(|value| value.checked_add(self.tree.potentials[arc.target]))
                .and_then(|value| value.checked_sub(self.tree.potentials[arc.source]))
        }
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
    }

    fn prepare_entering_piece(
        &mut self,
        entering: DirectedArc,
    ) -> Result<(), ConvexNetworkSimplexError> {
        let piece = self
            .marginal_piece(entering.arc, entering.forward)?
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        self.arcs[entering.arc].active_piece = Some(piece);
        Ok(())
    }

    fn form_cycle(&self, entering: DirectedArc) -> Result<Cycle, ConvexNetworkSimplexError> {
        let arc = self
            .arcs
            .get(entering.arc)
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        if arc.basis != BasisState::Breakpoint || arc.original.is_none() {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        let (from, to) = if entering.forward {
            (arc.source, arc.target)
        } else {
            (arc.target, arc.source)
        };
        let join = self.lowest_common_ancestor(from, to)?;
        let mut from_to_join = Vec::new();
        let mut node = from;
        while node != join {
            let parent = self.tree.parent[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            let tree_arc =
                self.tree.parent_arc[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            from_to_join.push(DirectedArc {
                arc: tree_arc,
                forward: self.arc_forward(tree_arc, parent, node)?,
            });
            node = parent;
        }
        from_to_join.reverse();
        let mut arcs = from_to_join;
        arcs.push(entering);
        node = to;
        while node != join {
            let parent = self.tree.parent[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            let tree_arc =
                self.tree.parent_arc[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            arcs.push(DirectedArc {
                arc: tree_arc,
                forward: self.arc_forward(tree_arc, node, parent)?,
            });
            node = parent;
        }
        Ok(Cycle {
            entering: entering.arc,
            arcs,
        })
    }

    fn arc_forward(
        &self,
        arc: usize,
        from: usize,
        to: usize,
    ) -> Result<bool, ConvexNetworkSimplexError> {
        let data = &self.arcs[arc];
        if data.source == from && data.target == to {
            Ok(true)
        } else if data.source == to && data.target == from {
            Ok(false)
        } else {
            Err(ConvexNetworkSimplexError::BasisInvariant)
        }
    }

    fn lowest_common_ancestor(
        &self,
        mut left: usize,
        mut right: usize,
    ) -> Result<usize, ConvexNetworkSimplexError> {
        while self.tree.depth[left] > self.tree.depth[right] {
            left = self.tree.parent[left].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        }
        while self.tree.depth[right] > self.tree.depth[left] {
            right = self.tree.parent[right].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        }
        while left != right {
            left = self.tree.parent[left].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            right = self.tree.parent[right].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        }
        Ok(left)
    }

    fn cycle_cost(&self, cycle: &Cycle) -> Result<i128, ConvexNetworkSimplexError> {
        cycle.arcs.iter().try_fold(0_i128, |sum, directed| {
            sum.checked_add(self.arcs[directed.arc].directed_cost(directed.forward)?)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        })
    }

    fn next_breakpoint(
        &self,
        cycle: &Cycle,
    ) -> Result<(i128, DirectedArc), ConvexNetworkSimplexError> {
        let mut best = None::<(i128, DirectedArc)>;
        for &directed in &cycle.arcs {
            let residual = self.to_selected_breakpoint(directed)?;
            if residual < 0 {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            if best.is_none_or(|(current, _)| residual < current) {
                best = Some((residual, directed));
            }
        }
        best.ok_or(ConvexNetworkSimplexError::BasisInvariant)
    }

    fn to_selected_breakpoint(
        &self,
        directed: DirectedArc,
    ) -> Result<i128, ConvexNetworkSimplexError> {
        let arc = &self.arcs[directed.arc];
        if arc.original.is_none() {
            return if directed.forward {
                arc.capacity
                    .checked_sub(arc.flow)
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
            } else {
                Ok(arc.flow)
            };
        }
        let piece = arc
            .active_piece
            .and_then(|piece| arc.pieces.get(piece))
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        if directed.forward {
            piece
                .end
                .checked_sub(arc.flow)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        } else {
            arc.flow
                .checked_sub(piece.start)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
        }
    }

    fn crosses_global_bound(
        &self,
        directed: DirectedArc,
        delta: i128,
    ) -> Result<bool, ConvexNetworkSimplexError> {
        let arc = &self.arcs[directed.arc];
        let next = if directed.forward {
            arc.flow.checked_add(delta)
        } else {
            arc.flow.checked_sub(delta)
        }
        .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        Ok(if directed.forward {
            next == arc.capacity
        } else {
            next == 0
        })
    }

    fn augment_cycle(
        &mut self,
        cycle: &Cycle,
        delta: i128,
    ) -> Result<(), ConvexNetworkSimplexError> {
        if delta < 0 {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        for directed in &cycle.arcs {
            let arc = &mut self.arcs[directed.arc];
            arc.flow = if directed.forward {
                arc.flow.checked_add(delta)
            } else {
                arc.flow.checked_sub(delta)
            }
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
            if arc.flow < 0 || arc.flow > arc.capacity {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
        }
        Ok(())
    }

    fn advance_piece(&mut self, directed: DirectedArc) -> Result<(), ConvexNetworkSimplexError> {
        let arc = &mut self.arcs[directed.arc];
        if arc.original.is_none() {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        let piece = arc
            .active_piece
            .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
        if directed.forward {
            if arc.pieces[piece].end != arc.flow || piece + 1 >= arc.pieces.len() {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            arc.active_piece = Some(piece + 1);
        } else {
            if arc.pieces[piece].start != arc.flow || piece == 0 {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            arc.active_piece = Some(piece - 1);
        }
        Ok(())
    }

    fn exchange_basis(
        &mut self,
        entering: usize,
        leaving: usize,
    ) -> Result<(), ConvexNetworkSimplexError> {
        if entering == leaving
            || self.arcs[entering].basis != BasisState::Breakpoint
            || self.arcs[leaving].basis != BasisState::Tree
        {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        self.arcs[entering].basis = BasisState::Tree;
        self.arcs[leaving].basis = BasisState::Breakpoint;
        self.rebuild_tree()
    }

    fn rebuild_tree(&mut self) -> Result<(), ConvexNetworkSimplexError> {
        let node_count = self.problem.graph().nodes().len() + 1;
        let root = self.tree.root;
        if self
            .arcs
            .iter()
            .filter(|arc| arc.basis == BasisState::Tree)
            .count()
            + 1
            != node_count
        {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        let mut adjacency = vec![Vec::<(usize, usize)>::new(); node_count];
        for (arc_index, arc) in self.arcs.iter().enumerate() {
            if arc.basis != BasisState::Tree {
                continue;
            }
            if arc.source == arc.target {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            adjacency[arc.source].push((arc.target, arc_index));
            adjacency[arc.target].push((arc.source, arc_index));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
        }
        let mut parent = vec![None; node_count];
        let mut parent_arc = vec![None; node_count];
        let mut depth = vec![0_usize; node_count];
        let mut order = vec![root];
        parent[root] = Some(root);
        let mut cursor = 0;
        while cursor < order.len() {
            let node = order[cursor];
            cursor += 1;
            for &(next, arc) in &adjacency[node] {
                if parent[next].is_some() {
                    continue;
                }
                parent[next] = Some(node);
                parent_arc[next] = Some(arc);
                depth[next] = depth[node]
                    .checked_add(1)
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
                order.push(next);
            }
        }
        if order.len() != node_count {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        parent[root] = None;
        let mut potentials = vec![0_i128; node_count];
        for &node in order.iter().skip(1) {
            let ancestor = parent[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            let arc_index = parent_arc[node].ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            let arc = &self.arcs[arc_index];
            let cost = arc.selected_cost()?;
            potentials[node] = if arc.source == ancestor && arc.target == node {
                potentials[ancestor].checked_add(cost)
            } else if arc.source == node && arc.target == ancestor {
                potentials[ancestor].checked_sub(cost)
            } else {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        }
        self.tree.parent = parent;
        self.tree.parent_arc = parent_arc;
        self.tree.depth = depth;
        self.tree.potentials = potentials;
        self.metrics.tree_rebuilds += 1;
        Ok(())
    }

    fn validate_basis(&self) -> Result<(), ConvexNetworkSimplexError> {
        let graph = self.problem.graph();
        let node_count = graph.nodes().len() + 1;
        if self.tree.parent.len() != node_count
            || self.tree.parent_arc.len() != node_count
            || self.tree.depth.len() != node_count
            || self.tree.potentials.len() != node_count
        {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        let mut divergence = vec![0_i128; node_count];
        for arc in &self.arcs {
            if arc.flow < 0 || arc.flow > arc.capacity {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            if arc.original.is_some() {
                if arc.pieces.is_empty() != (arc.capacity == 0) {
                    return Err(ConvexNetworkSimplexError::BasisInvariant);
                }
                if arc.basis == BasisState::Breakpoint
                    && arc.flow != 0
                    && arc.flow != arc.capacity
                    && !arc.pieces.iter().any(|piece| piece.end == arc.flow)
                {
                    return Err(ConvexNetworkSimplexError::BasisInvariant);
                }
                if arc.basis == BasisState::Tree {
                    let piece = arc
                        .active_piece
                        .and_then(|piece| arc.pieces.get(piece))
                        .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
                    if arc.flow < piece.start || arc.flow > piece.end {
                        return Err(ConvexNetworkSimplexError::BasisInvariant);
                    }
                }
            }
            if arc.basis == BasisState::Tree {
                let reduced = arc
                    .selected_cost()?
                    .checked_add(self.tree.potentials[arc.source])
                    .and_then(|value| value.checked_sub(self.tree.potentials[arc.target]))
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
                if reduced != 0 {
                    return Err(ConvexNetworkSimplexError::BasisInvariant);
                }
            }
            divergence[arc.source] = divergence[arc.source]
                .checked_add(arc.flow)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
            divergence[arc.target] = divergence[arc.target]
                .checked_sub(arc.flow)
                .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?;
        }
        let required = supply_divergences(graph)?;
        let lower = graph
            .edges()
            .iter()
            .map(crate::model::FlowEdge::lower)
            .collect::<Vec<_>>();
        let lower_divergence = divergences(graph, &lower)?;
        for node in 0..graph.nodes().len() {
            if divergence[node]
                != required[node]
                    .checked_sub(lower_divergence[node])
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)?
            {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
        }
        if divergence[self.tree.root] != 0 {
            return Err(ConvexNetworkSimplexError::BasisInvariant);
        }
        Ok(())
    }

    fn actual_flows(&self) -> Result<Vec<u64>, ConvexNetworkSimplexError> {
        self.problem
            .graph()
            .edges()
            .iter()
            .zip(&self.arcs[..self.original_arc_count])
            .map(|(edge, arc)| {
                i128::from(edge.lower())
                    .checked_add(arc.flow)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
            })
            .collect()
    }

    fn total_artificial_flow(&self) -> Result<i128, ConvexNetworkSimplexError> {
        self.arcs[self.original_arc_count..]
            .iter()
            .try_fold(0_i128, |sum, arc| {
                sum.checked_add(arc.flow)
                    .ok_or(ConvexNetworkSimplexError::ArithmeticOverflow)
            })
    }

    fn arc_ref(
        &self,
        directed: DirectedArc,
    ) -> Result<ConvexNetworkSimplexArcRef, ConvexNetworkSimplexError> {
        let arc = &self.arcs[directed.arc];
        let direction = if directed.forward {
            ConvexResidualDirection::Forward
        } else {
            ConvexResidualDirection::Reverse
        };
        if let Some(edge) = arc.original {
            let segment = arc
                .active_piece
                .and_then(|piece| arc.pieces.get(piece))
                .map(|piece| piece.declared_segment)
                .ok_or(ConvexNetworkSimplexError::BasisInvariant)?;
            Ok(ConvexNetworkSimplexArcRef::Original {
                edge,
                segment,
                direction,
            })
        } else {
            Ok(ConvexNetworkSimplexArcRef::Artificial {
                node: arc
                    .artificial_node
                    .ok_or(ConvexNetworkSimplexError::BasisInvariant)?,
                direction,
            })
        }
    }
}

fn effective_pieces(
    problem: &ConvexCostProblem<'_>,
    edge_index: usize,
) -> Result<Vec<Piece>, ConvexNetworkSimplexError> {
    let edge = &problem.graph().edges()[edge_index];
    let lower = edge.lower();
    let mut previous = 0_u64;
    let mut pieces = Vec::new();
    for (declared_segment, segment) in problem.edge_costs()[edge_index].segments.iter().enumerate()
    {
        let start = previous.max(lower);
        let end = segment.end_flow.min(edge.capacity());
        if start < end {
            pieces.push(Piece {
                declared_segment,
                start: i128::from(start - lower),
                end: i128::from(end - lower),
                cost: i128::from(segment.marginal_cost),
            });
        }
        previous = segment.end_flow;
    }
    if pieces.last().map(|piece| piece.end) != Some(i128::from(edge.capacity() - lower))
        && edge.capacity() > lower
    {
        return Err(ConvexNetworkSimplexError::BasisInvariant);
    }
    Ok(pieces)
}

fn grouped_segment_flows(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
) -> Result<Vec<Vec<u64>>, ConvexNetworkSimplexError> {
    problem
        .edge_costs()
        .iter()
        .zip(flows)
        .map(|(objective, &flow)| {
            let mut remaining = flow;
            let mut start = 0_u64;
            let mut result = Vec::with_capacity(objective.segments.len());
            for segment in &objective.segments {
                let amount = remaining.min(segment.end_flow - start);
                remaining -= amount;
                result.push(amount);
                start = segment.end_flow;
            }
            if remaining != 0 {
                return Err(ConvexNetworkSimplexError::BasisInvariant);
            }
            Ok(result)
        })
        .collect()
}

fn snapshot(
    work: &WorkingState<'_>,
) -> Result<ConvexNetworkSimplexSnapshot, ConvexNetworkSimplexError> {
    let flows = work.actual_flows()?;
    let grouped = grouped_segment_flows(work.problem, &flows)?;
    let mut segments = Vec::new();
    for (edge, objective) in work.problem.edge_costs().iter().enumerate() {
        let mut start = 0_u64;
        for (segment, piece) in objective.segments.iter().enumerate() {
            segments.push(ConvexSegmentState {
                edge,
                segment,
                start_flow: start,
                end_flow: piece.end_flow,
                flow: grouped[edge][segment],
                marginal_cost: piece.marginal_cost,
            });
            start = piece.end_flow;
        }
    }
    let edges = work.arcs[..work.original_arc_count]
        .iter()
        .enumerate()
        .map(|(edge, arc)| ConvexNetworkSimplexEdgeState {
            edge,
            flow: flows[edge],
            basis: match arc.basis {
                BasisState::Tree => ConvexNetworkSimplexBasisState::Tree,
                BasisState::Breakpoint => ConvexNetworkSimplexBasisState::Breakpoint,
            },
            active_segment: arc
                .active_piece
                .map(|piece| arc.pieces[piece].declared_segment),
        })
        .collect();
    let artificial_edges = work.arcs[work.original_arc_count..]
        .iter()
        .map(|arc| ConvexNetworkSimplexArtificialState {
            node: arc.artificial_node.unwrap_or(usize::MAX),
            source: arc.source,
            target: arc.target,
            flow: arc.flow,
            tree: arc.basis == BasisState::Tree,
        })
        .collect();
    Ok(ConvexNetworkSimplexSnapshot {
        flows,
        segments,
        edges,
        artificial_edges,
        potentials: work.tree.potentials.clone(),
        parents: work.tree.parent.clone(),
        active_cycle: work.active_cycle.clone(),
        entering: work.entering.clone(),
        leaving: work.leaving.clone(),
        metrics: work.metrics,
    })
}

fn record(
    work: &mut WorkingState<'_>,
    events: &mut Vec<ConvexNetworkSimplexTraceEvent>,
    enabled: bool,
    stage: ConvexNetworkSimplexStage,
    detail: Option<(&'static str, i128)>,
    mutation: impl FnOnce(&mut WorkingState<'_>) -> Result<(), ConvexNetworkSimplexError>,
) -> Result<(), ConvexNetworkSimplexError> {
    if enabled {
        let before = snapshot(work)?;
        mutation(work)?;
        let after = snapshot(work)?;
        events.push(ConvexNetworkSimplexTraceEvent {
            stage,
            detail,
            before,
            after,
        });
    } else {
        mutation(work)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;
    use crate::algorithms::{ConvexCostSegment, ConvexEdgeCost};

    fn graph() -> FlowNetwork {
        FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").unwrap(), 4),
                FlowNode::new(NodeId::parse("m").unwrap(), 0),
                FlowNode::new(NodeId::parse("t").unwrap(), -4),
            ],
            vec![
                UnresolvedFlowEdge {
                    id: EdgeId::parse("direct").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 0,
                    capacity: 4,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("sm").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("m").unwrap(),
                    lower: 0,
                    capacity: 4,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("mt").unwrap(),
                    from: NodeId::parse("m").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 0,
                    capacity: 4,
                    cost: 0,
                },
            ],
        )
        .unwrap()
    }

    fn problem(graph: &FlowNetwork) -> ConvexCostProblem<'_> {
        ConvexCostProblem::new(
            graph,
            vec![
                ConvexEdgeCost {
                    base_cost_at_zero: 0,
                    segments: vec![
                        ConvexCostSegment {
                            end_flow: 1,
                            marginal_cost: -2,
                        },
                        ConvexCostSegment {
                            end_flow: 2,
                            marginal_cost: 1,
                        },
                        ConvexCostSegment {
                            end_flow: 4,
                            marginal_cost: 8,
                        },
                    ],
                },
                ConvexEdgeCost {
                    base_cost_at_zero: 0,
                    segments: vec![ConvexCostSegment {
                        end_flow: 4,
                        marginal_cost: 2,
                    }],
                },
                ConvexEdgeCost {
                    base_cost_at_zero: 0,
                    segments: vec![ConvexCostSegment {
                        end_flow: 4,
                        marginal_cost: 2,
                    }],
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn combined_pivot_matches_expanded_oracle_and_crosses_multiple_breakpoints() {
        let graph = graph();
        let problem = problem(&graph);
        let result = solve_convex_network_simplex(&problem).unwrap();
        let oracle = solve_segment_expanded_convex_cost(&problem).unwrap();
        assert_eq!(result.certificate.total_cost, oracle.certificate.total_cost);
        assert!(result.metrics.breakpoint_crossings > result.metrics.combined_pivots);
        assert!(result.metrics.multi_crossing_pivots > 0);
    }

    #[test]
    fn trace_exposes_many_crossings_before_one_exchange() {
        let graph = graph();
        let problem = problem(&graph);
        let trace = trace_convex_network_simplex(&problem).unwrap();
        check_convex_network_simplex_trace(&problem, &trace).unwrap();
        let mut crossings = 0;
        let mut saw_combined = false;
        for event in &trace.events {
            if event.stage == ConvexNetworkSimplexStage::CrossBreakpoint {
                crossings += 1;
            } else if matches!(
                event.stage,
                ConvexNetworkSimplexStage::ExchangeBasis | ConvexNetworkSimplexStage::FlipBound
            ) {
                saw_combined |= crossings > 1;
                crossings = 0;
            }
        }
        assert!(saw_combined);
    }

    #[test]
    fn trace_checker_rejects_compact_basis_corruption() {
        let graph = graph();
        let problem = problem(&graph);
        let mut trace = trace_convex_network_simplex(&problem).unwrap();
        trace.events[1].after.edges[0].basis = ConvexNetworkSimplexBasisState::Tree;
        assert_eq!(
            check_convex_network_simplex_trace(&problem, &trace),
            Err(ConvexNetworkSimplexError::TraceInvariant)
        );
        let mut stage_trace = trace_convex_network_simplex(&problem).unwrap();
        stage_trace.events[0].stage = ConvexNetworkSimplexStage::Optimal;
        assert_eq!(
            check_convex_network_simplex_trace(&problem, &stage_trace),
            Err(ConvexNetworkSimplexError::TraceInvariant)
        );
        let mut detail_trace = trace_convex_network_simplex(&problem).unwrap();
        let price = detail_trace
            .events
            .iter_mut()
            .find(|event| event.stage == ConvexNetworkSimplexStage::Price)
            .expect("price event");
        price.detail = Some(("reduced-cost", 0));
        assert_eq!(
            check_convex_network_simplex_trace(&problem, &detail_trace),
            Err(ConvexNetworkSimplexError::TraceInvariant)
        );
        let mut crossing_trace = trace_convex_network_simplex(&problem).unwrap();
        let crossing = crossing_trace
            .events
            .iter_mut()
            .find(|event| event.stage == ConvexNetworkSimplexStage::CrossBreakpoint)
            .expect("cross-breakpoint event");
        let delta = crossing.detail.expect("crossing detail").1;
        crossing.detail = Some(("delta", delta + 1));
        assert_eq!(
            check_convex_network_simplex_trace(&problem, &crossing_trace),
            Err(ConvexNetworkSimplexError::TraceInvariant)
        );
        let mut base_trace = trace_convex_network_simplex(&problem).unwrap();
        base_trace.base_snapshot.potentials[0] += 1;
        base_trace.events[0].before.potentials[0] += 1;
        assert_eq!(
            check_convex_network_simplex_trace(&problem, &base_trace),
            Err(ConvexNetworkSimplexError::TraceInvariant)
        );
    }

    fn two_piece_cost(capacity: u64, first: i64, second: i64) -> ConvexEdgeCost {
        ConvexEdgeCost {
            base_cost_at_zero: i128::from(first - second),
            segments: vec![
                ConvexCostSegment {
                    end_flow: 1,
                    marginal_cost: first,
                },
                ConvexCostSegment {
                    end_flow: capacity,
                    marginal_cost: second,
                },
            ],
        }
    }

    #[test]
    fn exhaustive_compact_basis_matches_expanded_oracle() {
        const PROFILES: [(i64, i64); 4] = [(-3, -1), (-2, 2), (0, 0), (1, 4)];
        let mut cases = 0_usize;

        for required in 1_i64..=4 {
            for direct_lower in 0_u64..=1 {
                for &(direct_first, direct_second) in &PROFILES {
                    for &(path_first, path_second) in &PROFILES {
                        let graph = FlowNetwork::new(
                            vec![
                                FlowNode::new(NodeId::parse("s").unwrap(), required),
                                FlowNode::new(NodeId::parse("m").unwrap(), 0),
                                FlowNode::new(NodeId::parse("t").unwrap(), -required),
                            ],
                            vec![
                                UnresolvedFlowEdge {
                                    id: EdgeId::parse("direct").unwrap(),
                                    from: NodeId::parse("s").unwrap(),
                                    to: NodeId::parse("t").unwrap(),
                                    lower: direct_lower,
                                    capacity: 3,
                                    cost: 0,
                                },
                                UnresolvedFlowEdge {
                                    id: EdgeId::parse("sm").unwrap(),
                                    from: NodeId::parse("s").unwrap(),
                                    to: NodeId::parse("m").unwrap(),
                                    lower: 0,
                                    capacity: 3,
                                    cost: 0,
                                },
                                UnresolvedFlowEdge {
                                    id: EdgeId::parse("mt").unwrap(),
                                    from: NodeId::parse("m").unwrap(),
                                    to: NodeId::parse("t").unwrap(),
                                    lower: 0,
                                    capacity: 3,
                                    cost: 0,
                                },
                            ],
                        )
                        .unwrap();
                        let problem = ConvexCostProblem::new(
                            &graph,
                            vec![
                                two_piece_cost(3, direct_first, direct_second),
                                two_piece_cost(3, path_first, path_second),
                                two_piece_cost(3, path_first, path_second),
                            ],
                        )
                        .unwrap();

                        let native = solve_convex_network_simplex(&problem).unwrap_or_else(|error| {
                            panic!(
                                "required={required}, lower={direct_lower}, direct=({direct_first}, {direct_second}), path=({path_first}, {path_second}): {error:?}"
                            )
                        });
                        let oracle = solve_segment_expanded_convex_cost(&problem).unwrap();
                        assert_eq!(
                            check_convex_cost_flow(&problem, &native.flows).unwrap(),
                            native.certificate,
                        );
                        assert_eq!(
                            native.certificate.total_cost, oracle.certificate.total_cost,
                            "required={required}, lower={direct_lower}, direct=({direct_first}, {direct_second}), path=({path_first}, {path_second})"
                        );
                        cases += 1;
                    }
                }
            }
        }

        assert_eq!(cases, 128);
    }

    #[test]
    fn lower_clipping_parallel_opposite_self_loop_and_fixed_edges_are_certified() {
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").unwrap(), 2),
                FlowNode::new(NodeId::parse("t").unwrap(), -2),
            ],
            vec![
                UnresolvedFlowEdge {
                    id: EdgeId::parse("a-lower-inside").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 1,
                    capacity: 3,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("b-parallel").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 0,
                    capacity: 2,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("c-opposite").unwrap(),
                    from: NodeId::parse("t").unwrap(),
                    to: NodeId::parse("s").unwrap(),
                    lower: 0,
                    capacity: 2,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("d-self-loop").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("s").unwrap(),
                    lower: 0,
                    capacity: 2,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("e-zero").unwrap(),
                    from: NodeId::parse("t").unwrap(),
                    to: NodeId::parse("s").unwrap(),
                    lower: 0,
                    capacity: 0,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("f-fixed").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 1,
                    capacity: 1,
                    cost: 0,
                },
            ],
        )
        .unwrap();
        let costs = vec![
            ConvexEdgeCost {
                base_cost_at_zero: 7,
                segments: vec![
                    ConvexCostSegment {
                        end_flow: 2,
                        marginal_cost: -4,
                    },
                    ConvexCostSegment {
                        end_flow: 3,
                        marginal_cost: 2,
                    },
                ],
            },
            two_piece_cost(2, -1, 3),
            two_piece_cost(2, -5, 1),
            two_piece_cost(2, -3, 2),
            ConvexEdgeCost {
                base_cost_at_zero: -11,
                segments: Vec::new(),
            },
            ConvexEdgeCost {
                base_cost_at_zero: 13,
                segments: vec![ConvexCostSegment {
                    end_flow: 1,
                    marginal_cost: 0,
                }],
            },
        ];
        let problem = ConvexCostProblem::new(&graph, costs).unwrap();

        let native = solve_convex_network_simplex(&problem).unwrap();
        let oracle = solve_segment_expanded_convex_cost(&problem).unwrap();
        assert_eq!(native.certificate.total_cost, oracle.certificate.total_cost);
        assert_eq!(native.flows[4], 0);
        assert_eq!(native.flows[5], 1);
        assert_eq!(native.segment_flows[0], vec![2, 0]);
        assert_eq!(native.flows[3], 1);
    }

    #[test]
    fn infeasibility_and_admission_limits_are_explicit() {
        let infeasible = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").unwrap(), 2),
                FlowNode::new(NodeId::parse("t").unwrap(), -2),
            ],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("only").unwrap(),
                from: NodeId::parse("s").unwrap(),
                to: NodeId::parse("t").unwrap(),
                lower: 0,
                capacity: 1,
                cost: 0,
            }],
        )
        .unwrap();
        let infeasible_problem = ConvexCostProblem::new(
            &infeasible,
            vec![ConvexEdgeCost {
                base_cost_at_zero: 0,
                segments: vec![ConvexCostSegment {
                    end_flow: 1,
                    marginal_cost: 0,
                }],
            }],
        )
        .unwrap();
        assert!(matches!(
            solve_convex_network_simplex(&infeasible_problem),
            Err(ConvexNetworkSimplexError::Feasibility(_))
        ));

        let nodes = (0..=CONVEX_SIMPLEX_MAX_NODES)
            .map(|node| FlowNode::new(NodeId::parse(&format!("v{node:03}")).unwrap(), 0))
            .collect();
        let oversized = FlowNetwork::new(nodes, Vec::new()).unwrap();
        let oversized_problem = ConvexCostProblem::new(&oversized, Vec::new()).unwrap();
        assert_eq!(
            solve_convex_network_simplex(&oversized_problem),
            Err(ConvexNetworkSimplexError::AdmissionLimit)
        );
    }
}
