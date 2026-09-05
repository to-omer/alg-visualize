//! Native Edmonds--Karp scaling for integral piecewise-linear convex costs.
//!
//! Pinto and Shamir's first algorithm avoids treating all `M` linear pieces as
//! permanently expanded residual arcs.  Convexity means that one original arc
//! exposes only its next unused forward piece and its last used reverse piece.
//! This kernel keeps that marginal residual representation explicitly while it
//! performs powers-of-two capacity scaling, reduced-cost shortest paths, and
//! exact breakpoint crossings.  The separately implemented segment-expanded
//! solver is used only as a final objective oracle.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{CertificateError, divergences, supply_divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};

use super::{
    ConvexCostCertificate, ConvexCostError, ConvexCostProblem, ConvexResidualArc,
    ConvexResidualDirection, ConvexSegmentState, check_convex_cost_flow,
    solve_segment_expanded_convex_cost,
};

/// Conservative node limit for the interactive native solver.
pub const CONVEX_SCALING_MAX_NODES: usize = 128;
/// Conservative original-edge limit for the interactive native solver.
pub const CONVEX_SCALING_MAX_EDGES: usize = 512;
/// Conservative total-breakpoint limit for the interactive native solver.
pub const CONVEX_SCALING_MAX_SEGMENTS: usize = 1_024;
/// Deterministic ceiling for marginal path augmentations.
pub const CONVEX_SCALING_MAX_AUGMENTATIONS: u64 = 200_000;
/// Deterministic ceiling for inspected marginal residual arcs.
pub const CONVEX_SCALING_MAX_MARGINAL_ARC_SCANS: u128 = 20_000_000;
/// Deterministic ceiling for saturated marginal pieces.
pub const CONVEX_SCALING_MAX_PHASE_SATURATIONS: u64 = 200_000;

/// Exact deterministic work counters for native convex scaling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConvexCostScalingMetrics {
    /// Powers-of-two scales entered, including phases without augmentation.
    pub scaling_phases: u64,
    /// Negative reduced-cost marginal pieces saturated at phase boundaries.
    pub phase_saturations: u64,
    /// Complete reduced-cost shortest-path searches.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled by all shortest-path searches.
    pub settled_nodes: u128,
    /// Native marginal residual arcs inspected.
    pub marginal_arc_scans: u128,
    /// Successful dual-potential updates.
    pub potential_updates: u64,
    /// Successful source-to-deficit augmentations.
    pub augmentations: u64,
    /// Marginal segment boundaries reached by saturation or augmentation.
    pub breakpoint_crossings: u64,
}

/// Semantic boundary of the native scaling state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvexCostScalingStage {
    /// Lower-bound prefix occupancy and zero potentials were installed.
    Initialize,
    /// A powers-of-two capacity scale began.
    StartScale,
    /// One negative eligible marginal piece was saturated.
    SaturateMarginal,
    /// One native marginal residual arc was inspected by Dijkstra.
    InspectMarginalArc,
    /// One reduced-cost shortest path was selected.
    ShortestPath,
    /// Node potentials were updated from exact shortest-path distances.
    UpdatePotentials,
    /// Flow was augmented without crossing more than one piece per path arc.
    Augment,
    /// No eligible source-to-deficit path remains at this scale.
    CompleteScale,
    /// The native result and independent expanded oracle agree.
    Optimal,
}

/// Complete replay-visible native boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostScalingSnapshot {
    /// Aggregate original-edge flows.
    pub flows: Vec<u64>,
    /// Canonical prefix occupancy of every declared segment.
    pub segments: Vec<ConvexSegmentState>,
    /// Exact dual potential for every original node.
    pub potentials: Vec<i128>,
    /// Required divergence still missing at every node.
    pub remaining_divergence: Vec<i128>,
    /// Settled node order of the latest shortest-path search.
    pub search_order: Vec<NodeIndex>,
    /// Ordered active native marginal path, or one saturated marginal arc.
    pub active_path: Vec<ConvexResidualArc>,
    /// Current positive powers-of-two scale.
    pub scale: u64,
    /// Absolute deterministic counters at this boundary.
    pub metrics: ConvexCostScalingMetrics,
}

/// One deterministic native scaling transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostScalingTraceEvent {
    /// Closed semantic stage.
    pub stage: ConvexCostScalingStage,
    /// Exact stage-specific scalar (`scale`, `path-cost`, or `delta`).
    pub detail: Option<(&'static str, i128)>,
    /// State before the atomic transition.
    pub before: ConvexCostScalingSnapshot,
    /// State after the atomic transition.
    pub after: ConvexCostScalingSnapshot,
}

/// Certified native convex-cost scaling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostScalingResult {
    /// Aggregate original-edge flows.
    pub flows: Vec<u64>,
    /// Canonical prefix segment flows, grouped by original edge.
    pub segment_flows: Vec<Vec<u64>>,
    /// Independent exact native certificate.
    pub certificate: ConvexCostCertificate,
    /// Deterministic source-algorithm counters.
    pub metrics: ConvexCostScalingMetrics,
}

/// Certified result plus deterministic native replay boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostScalingTraceResult {
    /// Same result produced by the fast profile.
    pub result: ConvexCostScalingResult,
    /// Lower-bound prefix boundary before the first scaling phase.
    pub base_snapshot: ConvexCostScalingSnapshot,
    /// Complete native transition sequence.
    pub events: Vec<ConvexCostScalingTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: ConvexCostScalingSnapshot,
}

/// Native scaling construction, work-limit, replay, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConvexCostScalingError {
    /// Input exceeds the conservative interactive band.
    #[error("convex-cost scaling input exceeds native admission limits")]
    AdmissionLimit,
    /// A deterministic transition or scan ceiling was reached.
    #[error("convex-cost scaling work limit reached")]
    WorkLimit,
    /// The requested lower-bounded transshipment is infeasible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Supply or divergence reconstruction failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Native convex model, checker, or expanded oracle failed.
    #[error(transparent)]
    Convex(#[from] ConvexCostError),
    /// Checked integer arithmetic exceeded its declared domain.
    #[error("convex-cost scaling arithmetic overflow")]
    ArithmeticOverflow,
    /// A feasible precheck and the final scale disagreed on reachability.
    #[error("convex-cost scaling could not route a feasible remaining imbalance")]
    MissingPath,
    /// A predecessor chain or marginal identity was inconsistent.
    #[error("convex-cost scaling marginal predecessor invariant failed")]
    PredecessorInvariant,
    /// An eligible native marginal arc violated dual feasibility.
    #[error("convex-cost scaling encountered a negative eligible reduced cost")]
    NegativeEligibleReducedCost,
    /// The native result and the independent segment-expanded optimum differ.
    #[error("convex-cost scaling disagrees with the independent expanded oracle")]
    OracleMismatch,
    /// A supplied native trace differs from deterministic replay.
    #[error("convex-cost scaling trace invariant failed")]
    TraceInvariant,
}

/// Solves integral piecewise-linear convex-cost transshipment natively.
///
/// # Errors
///
/// Rejects admission, feasibility, arithmetic, work-limit, marginal-residual,
/// certificate, or independent-oracle failures.
pub fn solve_convex_cost_scaling(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexCostScalingResult, ConvexCostScalingError> {
    run_internal(problem, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_convex_cost_scaling_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexCostScalingResult, ConvexCostScalingError> {
    run_internal_with_feasibility(problem, false, feasibility).map(|run| run.result)
}

/// Runs native convex scaling while recording every source-level transition.
///
/// # Errors
///
/// Returns the same failures as [`solve_convex_cost_scaling`] plus replay
/// invariant failures.
pub fn trace_convex_cost_scaling(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexCostScalingTraceResult, ConvexCostScalingError> {
    let run = run_internal(problem, true)?;
    let trace = ConvexCostScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_convex_cost_scaling_trace(problem, &trace)?;
    Ok(trace)
}

/// Traces convex-cost scaling while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_convex_cost_scaling_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexCostScalingTraceResult, ConvexCostScalingError> {
    let run = run_internal_with_feasibility(problem, true, feasibility)?;
    let trace = ConvexCostScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_convex_cost_scaling_trace(problem, &trace)?;
    Ok(trace)
}

/// Independently checks the native trace and both terminal certificates.
///
/// # Errors
///
/// Rejects any modified stage, detail, boundary, result, or discontinuity.
pub fn check_convex_cost_scaling_trace(
    problem: &ConvexCostProblem<'_>,
    trace: &ConvexCostScalingTraceResult,
) -> Result<(), ConvexCostScalingError> {
    validate_admission(problem)?;
    if trace.base_snapshot != expected_convex_scaling_base(problem)?
        || trace.events.windows(2).any(|pair| {
            pair[0].after != pair[1].before
                || !valid_convex_scaling_stage_order(pair[0].stage, pair[1].stage)
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
        || trace.events.first().map(|event| event.stage) != Some(ConvexCostScalingStage::Initialize)
        || trace.events.last().map(|event| event.stage) != Some(ConvexCostScalingStage::Optimal)
        || trace.final_snapshot.flows != trace.result.flows
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.final_snapshot.flows.len() != problem.graph().edges().len()
        || trace.final_snapshot.potentials.len() != problem.graph().nodes().len()
        || trace.final_snapshot.remaining_divergence.len() != problem.graph().nodes().len()
        || trace.events.iter().any(|event| {
            !valid_convex_scaling_event(problem, event, trace.result.certificate.total_cost)
                || !valid_convex_scaling_stage_transition(event)
                || event.stage == ConvexCostScalingStage::Initialize
                    && event.before != trace.base_snapshot
                || event.stage == ConvexCostScalingStage::Optimal
                    && event.after != trace.final_snapshot
                || event.before.segments.len() != event.after.segments.len()
                || event.after.segments.len()
                    != problem
                        .edge_costs()
                        .iter()
                        .map(|cost| cost.segments.len())
                        .sum::<usize>()
                || event.before.remaining_divergence.len() != problem.graph().nodes().len()
                || event.after.remaining_divergence.len() != problem.graph().nodes().len()
                || event
                    .after
                    .search_order
                    .iter()
                    .any(|node| node.as_usize() >= problem.graph().nodes().len())
                || event.after.active_path.iter().any(|arc| {
                    problem
                        .edge_costs()
                        .get(arc.edge)
                        .and_then(|cost| cost.segments.get(arc.segment))
                        .is_none()
                })
                || event.before.flows.len() != problem.graph().edges().len()
                || event.after.flows.len() != problem.graph().edges().len()
                || event.before.potentials.len() != problem.graph().nodes().len()
                || event.after.potentials.len() != problem.graph().nodes().len()
                || event
                    .after
                    .flows
                    .iter()
                    .zip(problem.graph().edges())
                    .any(|(flow, edge)| *flow < edge.lower() || *flow > edge.capacity())
        })
    {
        return Err(ConvexCostScalingError::TraceInvariant);
    }
    let certificate = check_convex_cost_flow(problem, &trace.result.flows)?;
    let oracle = solve_segment_expanded_convex_cost(problem)?;
    if certificate != trace.result.certificate
        || oracle.certificate.total_cost != certificate.total_cost
        || trace
            .final_snapshot
            .remaining_divergence
            .iter()
            .any(|value| *value != 0)
    {
        return Err(ConvexCostScalingError::TraceInvariant);
    }
    Ok(())
}

fn expected_convex_scaling_base(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexCostScalingSnapshot, ConvexCostScalingError> {
    let graph = problem.graph();
    let required = supply_divergences(graph)?;
    let flows = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let actual = divergences(graph, &flows)?;
    let remaining = required
        .iter()
        .zip(actual)
        .map(|(&target, current)| {
            target
                .checked_sub(current)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut segments = Vec::new();
    for (edge, (objective, &flow)) in problem.edge_costs().iter().zip(&flows).enumerate() {
        let mut unassigned = flow;
        let mut start = 0_u64;
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
            start = piece.end_flow;
        }
        if unassigned != 0 {
            return Err(ConvexCostScalingError::PredecessorInvariant);
        }
    }
    Ok(ConvexCostScalingSnapshot {
        flows,
        segments,
        potentials: vec![0; graph.nodes().len()],
        remaining_divergence: remaining,
        search_order: Vec::new(),
        active_path: Vec::new(),
        scale: initial_scale(&required)?,
        metrics: ConvexCostScalingMetrics::default(),
    })
}

fn valid_convex_scaling_stage_transition(event: &ConvexCostScalingTraceEvent) -> bool {
    matches!(
        event.stage,
        ConvexCostScalingStage::Initialize
            if event.before.metrics == ConvexCostScalingMetrics::default()
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::InspectMarginalArc
            if event.after.metrics.marginal_arc_scans
                == event.before.metrics.marginal_arc_scans.saturating_add(1)
                && event.after.flows == event.before.flows
                && event.after.potentials == event.before.potentials
                && event.after.remaining_divergence == event.before.remaining_divergence
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::StartScale
            if event.after.metrics.scaling_phases
                == event.before.metrics.scaling_phases.saturating_add(1)
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::SaturateMarginal
            if event.after.metrics.phase_saturations
                == event.before.metrics.phase_saturations.saturating_add(1)
                && event.after.metrics.breakpoint_crossings
                    == event.before.metrics.breakpoint_crossings.saturating_add(1)
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::ShortestPath
            if valid_shortest_path_metric_transition(event)
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::UpdatePotentials
            if event.after.metrics.potential_updates
                == event.before.metrics.potential_updates.saturating_add(1)
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::Augment
            if event.after.metrics.augmentations
                == event.before.metrics.augmentations.saturating_add(1)
    ) || matches!(
        event.stage,
        ConvexCostScalingStage::CompleteScale | ConvexCostScalingStage::Optimal
            if event.after.metrics == event.before.metrics
    )
}

fn valid_shortest_path_metric_transition(event: &ConvexCostScalingTraceEvent) -> bool {
    let before = event.before.metrics;
    let after = event.after.metrics;
    let dijkstra_was_published_by_an_arc_scan = after.dijkstra_runs == before.dijkstra_runs;
    let arc_free_search_started_here = after.dijkstra_runs
        == before.dijkstra_runs.saturating_add(1)
        && after.settled_nodes > before.settled_nodes;
    (dijkstra_was_published_by_an_arc_scan || arc_free_search_started_here)
        && after.settled_nodes >= before.settled_nodes
        && after.marginal_arc_scans == before.marginal_arc_scans
        && after.scaling_phases == before.scaling_phases
        && after.phase_saturations == before.phase_saturations
        && after.potential_updates == before.potential_updates
        && after.augmentations == before.augmentations
        && after.breakpoint_crossings == before.breakpoint_crossings
}

const fn valid_convex_scaling_stage_order(
    before: ConvexCostScalingStage,
    after: ConvexCostScalingStage,
) -> bool {
    matches!(
        (before, after),
        (
            ConvexCostScalingStage::Initialize,
            ConvexCostScalingStage::StartScale
        ) | (
            ConvexCostScalingStage::StartScale | ConvexCostScalingStage::SaturateMarginal,
            ConvexCostScalingStage::SaturateMarginal
                | ConvexCostScalingStage::InspectMarginalArc
                | ConvexCostScalingStage::ShortestPath
                | ConvexCostScalingStage::CompleteScale
        ) | (
            ConvexCostScalingStage::InspectMarginalArc,
            ConvexCostScalingStage::InspectMarginalArc | ConvexCostScalingStage::ShortestPath
        ) | (
            ConvexCostScalingStage::ShortestPath,
            ConvexCostScalingStage::InspectMarginalArc
                | ConvexCostScalingStage::ShortestPath
                | ConvexCostScalingStage::UpdatePotentials
                | ConvexCostScalingStage::CompleteScale
        ) | (
            ConvexCostScalingStage::UpdatePotentials,
            ConvexCostScalingStage::Augment
        ) | (
            ConvexCostScalingStage::Augment,
            ConvexCostScalingStage::InspectMarginalArc
                | ConvexCostScalingStage::ShortestPath
                | ConvexCostScalingStage::CompleteScale
        ) | (
            ConvexCostScalingStage::CompleteScale,
            ConvexCostScalingStage::StartScale | ConvexCostScalingStage::Optimal
        )
    )
}

fn valid_convex_scaling_event(
    problem: &ConvexCostProblem<'_>,
    event: &ConvexCostScalingTraceEvent,
    total_cost: i128,
) -> bool {
    match event.stage {
        ConvexCostScalingStage::Initialize => event.detail.is_none(),
        ConvexCostScalingStage::StartScale | ConvexCostScalingStage::CompleteScale => {
            event.detail == Some(("scale", i128::from(event.after.scale)))
                && event.after.scale > 0
                && event.after.scale.is_power_of_two()
        }
        ConvexCostScalingStage::SaturateMarginal => {
            let changed = event
                .before
                .flows
                .iter()
                .zip(&event.after.flows)
                .filter_map(|(&before, &after)| (before != after).then_some(before.abs_diff(after)))
                .collect::<Vec<_>>();
            matches!(event.detail, Some(("delta", delta)) if delta > 0)
                && changed.len() == 1
                && event.detail.is_some_and(|(_, delta)| {
                    u64::try_from(delta).is_ok_and(|delta| changed == [delta])
                })
                && event.after.active_path.len() == 1
        }
        ConvexCostScalingStage::InspectMarginalArc => {
            matches!(event.detail, Some(("residual-capacity", capacity)) if capacity > 0)
                && event.after.active_path.len() == 1
        }
        ConvexCostScalingStage::ShortestPath => {
            if event.after.active_path.is_empty() {
                event.detail == Some(("scale", i128::from(event.after.scale)))
            } else {
                marginal_path_cost(problem, &event.after.active_path)
                    .is_some_and(|cost| event.detail == Some(("path-cost", cost)))
            }
        }
        ConvexCostScalingStage::UpdatePotentials => {
            let cutoff = event
                .before
                .potentials
                .iter()
                .zip(&event.after.potentials)
                .filter_map(|(&before, &after)| after.checked_sub(before))
                .max();
            cutoff.is_some_and(|cutoff| event.detail == Some(("distance", cutoff)))
                && event.after.active_path == event.before.active_path
        }
        ConvexCostScalingStage::Augment => {
            let before_positive = event
                .before
                .remaining_divergence
                .iter()
                .copied()
                .filter(|value| *value > 0)
                .sum::<i128>();
            let after_positive = event
                .after
                .remaining_divergence
                .iter()
                .copied()
                .filter(|value| *value > 0)
                .sum::<i128>();
            before_positive
                .checked_sub(after_positive)
                .is_some_and(|delta| delta > 0 && event.detail == Some(("delta", delta)))
                && !event.after.active_path.is_empty()
        }
        ConvexCostScalingStage::Optimal => event.detail == Some(("total-cost", total_cost)),
    }
}

fn marginal_path_cost(problem: &ConvexCostProblem<'_>, path: &[ConvexResidualArc]) -> Option<i128> {
    path.iter().try_fold(0_i128, |sum, arc| {
        let slope = i128::from(
            problem
                .edge_costs()
                .get(arc.edge)?
                .segments
                .get(arc.segment)?
                .marginal_cost,
        );
        let signed = match arc.direction {
            ConvexResidualDirection::Forward => slope,
            ConvexResidualDirection::Reverse => slope.checked_neg()?,
        };
        sum.checked_add(signed)
    })
}

struct InternalRun {
    result: ConvexCostScalingResult,
    base_snapshot: ConvexCostScalingSnapshot,
    events: Vec<ConvexCostScalingTraceEvent>,
    final_snapshot: ConvexCostScalingSnapshot,
}

struct WorkingState {
    flows: Vec<u64>,
    potentials: Vec<i128>,
    remaining: Vec<i128>,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ConvexResidualArc>,
    scale: u64,
    metrics: ConvexCostScalingMetrics,
}

#[derive(Clone)]
struct MarginalArc {
    reference: ConvexResidualArc,
    from: NodeIndex,
    to: NodeIndex,
    capacity: u64,
    cost: i128,
}

struct Topology {
    forward: Vec<Vec<usize>>,
    reverse: Vec<Vec<usize>>,
}

struct Search {
    source: NodeIndex,
    sink: Option<NodeIndex>,
    path: Vec<MarginalArc>,
    distances: Vec<Option<i128>>,
    settled_order: Vec<NodeIndex>,
    metrics_after: ConvexCostScalingMetrics,
    checkpoints: Vec<SearchCheckpoint>,
}

struct SearchCheckpoint {
    arc: ConvexResidualArc,
    residual_capacity: u64,
    settled_order: Vec<NodeIndex>,
    metrics_after: ConvexCostScalingMetrics,
}

fn run_internal(
    problem: &ConvexCostProblem<'_>,
    trace_enabled: bool,
) -> Result<InternalRun, ConvexCostScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(problem, trace_enabled, &mut feasibility)
}

fn run_internal_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, ConvexCostScalingError> {
    validate_admission(problem)?;
    let graph = problem.graph();
    let required = supply_divergences(graph)?;
    feasibility.find_feasible_flow(graph, &required, FeasibilityUse::PrecheckOnly)?;
    let flows = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let actual = divergences(graph, &flows)?;
    let remaining = required
        .iter()
        .zip(actual)
        .map(|(&target, current)| {
            target
                .checked_sub(current)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut work = WorkingState {
        flows,
        potentials: vec![0; graph.nodes().len()],
        remaining,
        search_order: Vec::new(),
        active_path: Vec::new(),
        scale: initial_scale(&required)?,
        metrics: ConvexCostScalingMetrics::default(),
    };
    let topology = topology(graph);
    let base_snapshot = snapshot(problem, &work)?;
    let mut events = Vec::new();
    record(
        problem,
        &mut work,
        &mut events,
        trace_enabled,
        ConvexCostScalingStage::Initialize,
        None,
        |state| {
            state.search_order.clear();
            state.active_path.clear();
            Ok(())
        },
    )?;

    let mut next_scale = work.scale;
    loop {
        run_phase(
            problem,
            &topology,
            &mut work,
            &mut events,
            trace_enabled,
            next_scale,
        )?;
        if next_scale == 1 {
            break;
        }
        next_scale /= 2;
    }
    if work.remaining.iter().any(|&value| value != 0) {
        return Err(ConvexCostScalingError::MissingPath);
    }
    let certificate = check_convex_cost_flow(problem, &work.flows)?;
    let oracle = solve_segment_expanded_convex_cost(problem)?;
    if oracle.certificate.total_cost != certificate.total_cost {
        return Err(ConvexCostScalingError::OracleMismatch);
    }
    record(
        problem,
        &mut work,
        &mut events,
        trace_enabled,
        ConvexCostScalingStage::Optimal,
        Some(("total-cost", certificate.total_cost)),
        |state| {
            state.search_order.clear();
            state.active_path.clear();
            Ok(())
        },
    )?;
    let segment_flows = segment_flows(problem, &work.flows)?;
    let final_snapshot = snapshot(problem, &work)?;
    Ok(InternalRun {
        result: ConvexCostScalingResult {
            flows: work.flows,
            segment_flows,
            certificate,
            metrics: work.metrics,
        },
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn run_phase(
    problem: &ConvexCostProblem<'_>,
    topology: &Topology,
    work: &mut WorkingState,
    events: &mut Vec<ConvexCostScalingTraceEvent>,
    trace_enabled: bool,
    scale: u64,
) -> Result<(), ConvexCostScalingError> {
    record(
        problem,
        work,
        events,
        trace_enabled,
        ConvexCostScalingStage::StartScale,
        Some(("scale", i128::from(scale))),
        |state| {
            state.scale = scale;
            state.metrics.scaling_phases = state
                .metrics
                .scaling_phases
                .checked_add(1)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            state.search_order.clear();
            state.active_path.clear();
            Ok(())
        },
    )?;
    saturate_negative_eligible(problem, work, events, trace_enabled)?;
    validate_eligible_reduced_costs(problem, work)?;
    route_eligible(problem, topology, work, events, trace_enabled)?;
    record(
        problem,
        work,
        events,
        trace_enabled,
        ConvexCostScalingStage::CompleteScale,
        Some(("scale", i128::from(scale))),
        |state| {
            state.search_order.clear();
            state.active_path.clear();
            Ok(())
        },
    )
}

fn saturate_negative_eligible(
    problem: &ConvexCostProblem<'_>,
    work: &mut WorkingState,
    events: &mut Vec<ConvexCostScalingTraceEvent>,
    trace_enabled: bool,
) -> Result<(), ConvexCostScalingError> {
    loop {
        let candidate = all_marginal_arcs(problem, work)?.into_iter().find(|arc| {
            arc.capacity >= work.scale
                && reduced_cost(arc, &work.potentials).is_ok_and(|cost| cost < 0)
        });
        let Some(arc) = candidate else {
            return Ok(());
        };
        let amount = arc.capacity;
        record(
            problem,
            work,
            events,
            trace_enabled,
            ConvexCostScalingStage::SaturateMarginal,
            Some(("delta", i128::from(amount))),
            |state| {
                apply_arc(problem, state, &arc, amount)?;
                state.search_order.clear();
                state.active_path = vec![arc.reference.clone()];
                state.metrics.phase_saturations = state
                    .metrics
                    .phase_saturations
                    .checked_add(1)
                    .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
                state.metrics.breakpoint_crossings = state
                    .metrics
                    .breakpoint_crossings
                    .checked_add(1)
                    .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
                if state.metrics.phase_saturations > CONVEX_SCALING_MAX_PHASE_SATURATIONS {
                    return Err(ConvexCostScalingError::WorkLimit);
                }
                Ok(())
            },
        )?;
    }
}

fn route_eligible(
    problem: &ConvexCostProblem<'_>,
    topology: &Topology,
    work: &mut WorkingState,
    events: &mut Vec<ConvexCostScalingTraceEvent>,
    trace_enabled: bool,
) -> Result<(), ConvexCostScalingError> {
    loop {
        let sources = problem
            .graph()
            .node_indices()
            .filter(|node| work.remaining[node.as_usize()] >= i128::from(work.scale))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Ok(());
        }
        let mut selected = None;
        for source in sources {
            let search = shortest_path(problem, topology, work, source)?;
            for checkpoint in &search.checkpoints {
                record(
                    problem,
                    work,
                    events,
                    trace_enabled,
                    ConvexCostScalingStage::InspectMarginalArc,
                    Some((
                        "residual-capacity",
                        i128::from(checkpoint.residual_capacity),
                    )),
                    |state| {
                        state.metrics = checkpoint.metrics_after;
                        state.search_order.clone_from(&checkpoint.settled_order);
                        state.active_path = vec![checkpoint.arc.clone()];
                        Ok(())
                    },
                )?;
            }
            let path_cost = path_cost(&search.path)?;
            record(
                problem,
                work,
                events,
                trace_enabled,
                ConvexCostScalingStage::ShortestPath,
                search
                    .sink
                    .map_or(Some(("scale", i128::from(work.scale))), |_| {
                        Some(("path-cost", path_cost))
                    }),
                |state| {
                    state.metrics = search.metrics_after;
                    state.search_order.clone_from(&search.settled_order);
                    state.active_path = search
                        .path
                        .iter()
                        .map(|arc| arc.reference.clone())
                        .collect();
                    Ok(())
                },
            )?;
            if search.sink.is_some() {
                selected = Some(search);
                break;
            }
        }
        let Some(search) = selected else {
            return Ok(());
        };
        apply_search(problem, work, events, trace_enabled, &search)?;
    }
}

fn apply_search(
    problem: &ConvexCostProblem<'_>,
    work: &mut WorkingState,
    events: &mut Vec<ConvexCostScalingTraceEvent>,
    trace_enabled: bool,
    search: &Search,
) -> Result<(), ConvexCostScalingError> {
    let sink = search.sink.ok_or(ConvexCostScalingError::MissingPath)?;
    let cutoff =
        search.distances[sink.as_usize()].ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    record(
        problem,
        work,
        events,
        trace_enabled,
        ConvexCostScalingStage::UpdatePotentials,
        Some(("distance", cutoff)),
        |state| {
            for (potential, distance) in state.potentials.iter_mut().zip(&search.distances) {
                let adjustment = distance.map_or(cutoff, |value| value.min(cutoff));
                *potential = potential
                    .checked_add(adjustment)
                    .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            }
            state.metrics.potential_updates = state
                .metrics
                .potential_updates
                .checked_add(1)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            state.search_order.clone_from(&search.settled_order);
            state.active_path = search
                .path
                .iter()
                .map(|arc| arc.reference.clone())
                .collect();
            Ok(())
        },
    )?;
    let amount = augmentation_amount(work, search)?;
    if amount < work.scale {
        return Err(ConvexCostScalingError::PredecessorInvariant);
    }
    record(
        problem,
        work,
        events,
        trace_enabled,
        ConvexCostScalingStage::Augment,
        Some(("delta", i128::from(amount))),
        |state| {
            let mut crossings = 0_u64;
            for arc in &search.path {
                if amount == arc.capacity {
                    crossings = crossings
                        .checked_add(1)
                        .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
                }
                apply_arc(problem, state, arc, amount)?;
            }
            state.metrics.augmentations = state
                .metrics
                .augmentations
                .checked_add(1)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            state.metrics.breakpoint_crossings = state
                .metrics
                .breakpoint_crossings
                .checked_add(crossings)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            if state.metrics.augmentations > CONVEX_SCALING_MAX_AUGMENTATIONS {
                return Err(ConvexCostScalingError::WorkLimit);
            }
            state.search_order.clone_from(&search.settled_order);
            state.active_path = search
                .path
                .iter()
                .map(|arc| arc.reference.clone())
                .collect();
            Ok(())
        },
    )?;
    validate_eligible_reduced_costs(problem, work)
}

fn shortest_path(
    problem: &ConvexCostProblem<'_>,
    topology: &Topology,
    work: &WorkingState,
    source: NodeIndex,
) -> Result<Search, ConvexCostScalingError> {
    let mut metrics = work.metrics;
    metrics.dijkstra_runs = metrics
        .dijkstra_runs
        .checked_add(1)
        .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
    let count = problem.graph().nodes().len();
    let mut distances = vec![None; count];
    let mut hops = vec![usize::MAX; count];
    let mut predecessor = vec![None::<MarginalArc>; count];
    let mut settled = vec![false; count];
    let mut settled_order = Vec::new();
    let mut heap = BinaryHeap::new();
    let mut checkpoints = Vec::new();
    distances[source.as_usize()] = Some(0_i128);
    hops[source.as_usize()] = 0;
    heap.push(Reverse((0_i128, 0_usize, source)));
    while let Some(Reverse((distance, hop_count, node))) = heap.pop() {
        if settled[node.as_usize()]
            || distances[node.as_usize()] != Some(distance)
            || hops[node.as_usize()] != hop_count
        {
            continue;
        }
        settled[node.as_usize()] = true;
        settled_order.push(node);
        metrics.settled_nodes = metrics
            .settled_nodes
            .checked_add(1)
            .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
        for arc in outgoing_marginal_arcs(problem, topology, work, node)? {
            metrics.marginal_arc_scans = metrics
                .marginal_arc_scans
                .checked_add(1)
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
            if metrics.marginal_arc_scans > CONVEX_SCALING_MAX_MARGINAL_ARC_SCANS {
                return Err(ConvexCostScalingError::WorkLimit);
            }
            if arc.capacity >= work.scale {
                let reduced = reduced_cost(&arc, &work.potentials)?;
                if reduced < 0 {
                    return Err(ConvexCostScalingError::NegativeEligibleReducedCost);
                }
                let candidate_distance = distance
                    .checked_add(reduced)
                    .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
                let candidate_hops = hop_count
                    .checked_add(1)
                    .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
                let current =
                    distances[arc.to.as_usize()].map(|value| (value, hops[arc.to.as_usize()]));
                if current.is_none_or(|value| (candidate_distance, candidate_hops) < value) {
                    distances[arc.to.as_usize()] = Some(candidate_distance);
                    hops[arc.to.as_usize()] = candidate_hops;
                    predecessor[arc.to.as_usize()] = Some(arc.clone());
                    heap.push(Reverse((candidate_distance, candidate_hops, arc.to)));
                }
            }
            checkpoints.push(SearchCheckpoint {
                arc: arc.reference.clone(),
                residual_capacity: arc.capacity,
                settled_order: settled_order.clone(),
                metrics_after: metrics,
            });
        }
    }
    let sink = problem
        .graph()
        .node_indices()
        .filter(|node| work.remaining[node.as_usize()] <= -i128::from(work.scale))
        .filter_map(|node| distances[node.as_usize()].map(|d| (d, hops[node.as_usize()], node)))
        .min()
        .map(|(_, _, node)| node);
    let path = sink
        .map(|sink| reconstruct_path(source, sink, &predecessor))
        .transpose()?
        .unwrap_or_default();
    Ok(Search {
        source,
        sink,
        path,
        distances,
        settled_order,
        metrics_after: metrics,
        checkpoints,
    })
}

fn reconstruct_path(
    source: NodeIndex,
    sink: NodeIndex,
    predecessor: &[Option<MarginalArc>],
) -> Result<Vec<MarginalArc>, ConvexCostScalingError> {
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let arc = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
        if arc.to != cursor || arc.capacity == 0 {
            return Err(ConvexCostScalingError::PredecessorInvariant);
        }
        cursor = arc.from;
        reversed.push(arc);
        if reversed.len() > predecessor.len() {
            return Err(ConvexCostScalingError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

fn augmentation_amount(
    work: &WorkingState,
    search: &Search,
) -> Result<u64, ConvexCostScalingError> {
    let sink = search.sink.ok_or(ConvexCostScalingError::MissingPath)?;
    let bottleneck = search
        .path
        .iter()
        .map(|arc| arc.capacity)
        .min()
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    Ok(bottleneck
        .min(positive_u64(work.remaining[search.source.as_usize()])?)
        .min(positive_u64(
            work.remaining[sink.as_usize()]
                .checked_neg()
                .ok_or(ConvexCostScalingError::ArithmeticOverflow)?,
        )?))
}

fn apply_arc(
    problem: &ConvexCostProblem<'_>,
    work: &mut WorkingState,
    arc: &MarginalArc,
    amount: u64,
) -> Result<(), ConvexCostScalingError> {
    if amount == 0 || amount > arc.capacity {
        return Err(ConvexCostScalingError::PredecessorInvariant);
    }
    let edge = problem
        .graph()
        .edges()
        .get(arc.reference.edge)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    let flow = work
        .flows
        .get_mut(arc.reference.edge)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    match arc.reference.direction {
        ConvexResidualDirection::Forward => {
            *flow = flow
                .checked_add(amount)
                .filter(|value| *value <= edge.capacity())
                .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
        }
        ConvexResidualDirection::Reverse => {
            *flow = flow
                .checked_sub(amount)
                .filter(|value| *value >= edge.lower())
                .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
        }
    }
    let delta = i128::from(amount);
    work.remaining[arc.from.as_usize()] = work.remaining[arc.from.as_usize()]
        .checked_sub(delta)
        .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
    work.remaining[arc.to.as_usize()] = work.remaining[arc.to.as_usize()]
        .checked_add(delta)
        .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
    Ok(())
}

fn topology(graph: &FlowNetwork) -> Topology {
    let mut forward = vec![Vec::new(); graph.nodes().len()];
    let mut reverse = vec![Vec::new(); graph.nodes().len()];
    for (index, edge) in graph.edges().iter().enumerate() {
        forward[edge.from().as_usize()].push(index);
        reverse[edge.to().as_usize()].push(index);
    }
    Topology { forward, reverse }
}

fn outgoing_marginal_arcs(
    problem: &ConvexCostProblem<'_>,
    topology: &Topology,
    work: &WorkingState,
    node: NodeIndex,
) -> Result<Vec<MarginalArc>, ConvexCostScalingError> {
    let mut arcs = Vec::new();
    for &edge in &topology.forward[node.as_usize()] {
        if let Some(arc) = forward_arc(problem, &work.flows, edge)? {
            arcs.push(arc);
        }
    }
    for &edge in &topology.reverse[node.as_usize()] {
        if let Some(arc) = reverse_arc(problem, &work.flows, edge)? {
            arcs.push(arc);
        }
    }
    arcs.sort_by_key(|arc| {
        (
            arc.reference.edge,
            match arc.reference.direction {
                ConvexResidualDirection::Forward => 0_u8,
                ConvexResidualDirection::Reverse => 1_u8,
            },
        )
    });
    Ok(arcs)
}

fn all_marginal_arcs(
    problem: &ConvexCostProblem<'_>,
    work: &WorkingState,
) -> Result<Vec<MarginalArc>, ConvexCostScalingError> {
    let mut arcs = Vec::with_capacity(problem.graph().edges().len() * 2);
    for edge in 0..problem.graph().edges().len() {
        if let Some(arc) = forward_arc(problem, &work.flows, edge)? {
            arcs.push(arc);
        }
        if let Some(arc) = reverse_arc(problem, &work.flows, edge)? {
            arcs.push(arc);
        }
    }
    Ok(arcs)
}

fn forward_arc(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
    edge_index: usize,
) -> Result<Option<MarginalArc>, ConvexCostScalingError> {
    let edge = problem
        .graph()
        .edges()
        .get(edge_index)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    let flow = *flows
        .get(edge_index)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    if flow >= edge.capacity() {
        return Ok(None);
    }
    let (segment, piece) = problem.edge_costs()[edge_index]
        .segments
        .iter()
        .enumerate()
        .find(|(_, segment)| flow < segment.end_flow)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    Ok(Some(MarginalArc {
        reference: ConvexResidualArc {
            edge: edge_index,
            segment,
            direction: ConvexResidualDirection::Forward,
        },
        from: edge.from(),
        to: edge.to(),
        capacity: piece.end_flow - flow,
        cost: i128::from(piece.marginal_cost),
    }))
}

fn reverse_arc(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
    edge_index: usize,
) -> Result<Option<MarginalArc>, ConvexCostScalingError> {
    let edge = problem
        .graph()
        .edges()
        .get(edge_index)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    let flow = *flows
        .get(edge_index)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    if flow <= edge.lower() {
        return Ok(None);
    }
    let objective = &problem.edge_costs()[edge_index];
    let (segment, piece) = objective
        .segments
        .iter()
        .enumerate()
        .find(|(_, segment)| flow <= segment.end_flow)
        .ok_or(ConvexCostScalingError::PredecessorInvariant)?;
    let start = segment
        .checked_sub(1)
        .map_or(0, |previous| objective.segments[previous].end_flow)
        .max(edge.lower());
    Ok(Some(MarginalArc {
        reference: ConvexResidualArc {
            edge: edge_index,
            segment,
            direction: ConvexResidualDirection::Reverse,
        },
        from: edge.to(),
        to: edge.from(),
        capacity: flow - start,
        cost: i128::from(piece.marginal_cost)
            .checked_neg()
            .ok_or(ConvexCostScalingError::ArithmeticOverflow)?,
    }))
}

fn reduced_cost(arc: &MarginalArc, potentials: &[i128]) -> Result<i128, ConvexCostScalingError> {
    arc.cost
        .checked_add(potentials[arc.from.as_usize()])
        .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
        .ok_or(ConvexCostScalingError::ArithmeticOverflow)
}

fn validate_eligible_reduced_costs(
    problem: &ConvexCostProblem<'_>,
    work: &WorkingState,
) -> Result<(), ConvexCostScalingError> {
    if all_marginal_arcs(problem, work)?.iter().any(|arc| {
        arc.capacity >= work.scale
            && reduced_cost(arc, &work.potentials).is_ok_and(|value| value < 0)
    }) {
        return Err(ConvexCostScalingError::NegativeEligibleReducedCost);
    }
    Ok(())
}

fn path_cost(path: &[MarginalArc]) -> Result<i128, ConvexCostScalingError> {
    path.iter().try_fold(0_i128, |sum, arc| {
        sum.checked_add(arc.cost)
            .ok_or(ConvexCostScalingError::ArithmeticOverflow)
    })
}

fn initial_scale(required: &[i128]) -> Result<u64, ConvexCostScalingError> {
    let maximum = required.iter().try_fold(1_u64, |maximum, value| {
        let magnitude = u64::try_from(value.unsigned_abs())
            .map_err(|_| ConvexCostScalingError::ArithmeticOverflow)?;
        Ok::<_, ConvexCostScalingError>(maximum.max(magnitude))
    })?;
    Ok(1_u64 << (u64::BITS - 1 - maximum.leading_zeros()))
}

fn positive_u64(value: i128) -> Result<u64, ConvexCostScalingError> {
    u64::try_from(value).map_err(|_| ConvexCostScalingError::ArithmeticOverflow)
}

fn validate_admission(problem: &ConvexCostProblem<'_>) -> Result<(), ConvexCostScalingError> {
    let graph = problem.graph();
    let segments = problem
        .edge_costs()
        .iter()
        .try_fold(0_usize, |sum, cost| sum.checked_add(cost.segments.len()))
        .ok_or(ConvexCostScalingError::ArithmeticOverflow)?;
    if graph.nodes().len() > CONVEX_SCALING_MAX_NODES
        || graph.edges().len() > CONVEX_SCALING_MAX_EDGES
        || segments > CONVEX_SCALING_MAX_SEGMENTS
    {
        return Err(ConvexCostScalingError::AdmissionLimit);
    }
    Ok(())
}

fn segment_flows(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
) -> Result<Vec<Vec<u64>>, ConvexCostScalingError> {
    problem
        .edge_costs()
        .iter()
        .zip(flows)
        .map(|(objective, &flow)| {
            let mut remaining = flow;
            let mut start = 0_u64;
            objective
                .segments
                .iter()
                .map(|segment| {
                    let amount = remaining.min(segment.end_flow - start);
                    remaining -= amount;
                    start = segment.end_flow;
                    amount
                })
                .collect::<Vec<_>>()
                .pipe(|segments| {
                    if remaining == 0 {
                        Ok(segments)
                    } else {
                        Err(ConvexCostScalingError::PredecessorInvariant)
                    }
                })
        })
        .collect()
}

fn snapshot(
    problem: &ConvexCostProblem<'_>,
    work: &WorkingState,
) -> Result<ConvexCostScalingSnapshot, ConvexCostScalingError> {
    let grouped = segment_flows(problem, &work.flows)?;
    let mut segments = Vec::new();
    for (edge, objective) in problem.edge_costs().iter().enumerate() {
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
    Ok(ConvexCostScalingSnapshot {
        flows: work.flows.clone(),
        segments,
        potentials: work.potentials.clone(),
        remaining_divergence: work.remaining.clone(),
        search_order: work.search_order.clone(),
        active_path: work.active_path.clone(),
        scale: work.scale,
        metrics: work.metrics,
    })
}

fn record(
    problem: &ConvexCostProblem<'_>,
    work: &mut WorkingState,
    events: &mut Vec<ConvexCostScalingTraceEvent>,
    enabled: bool,
    stage: ConvexCostScalingStage,
    detail: Option<(&'static str, i128)>,
    mutation: impl FnOnce(&mut WorkingState) -> Result<(), ConvexCostScalingError>,
) -> Result<(), ConvexCostScalingError> {
    if enabled {
        let before = snapshot(problem, work)?;
        mutation(work)?;
        let after = snapshot(problem, work)?;
        events.push(ConvexCostScalingTraceEvent {
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

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

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
                    base_cost_at_zero: 2,
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
    fn native_scaling_matches_the_independent_expanded_oracle() {
        let graph = graph();
        let problem = problem(&graph);
        let native = solve_convex_cost_scaling(&problem).unwrap();
        let oracle = solve_segment_expanded_convex_cost(&problem).unwrap();
        assert_eq!(native.flows, vec![2, 2, 2]);
        assert_eq!(native.certificate.total_cost, 9);
        assert_eq!(native.certificate.total_cost, oracle.certificate.total_cost);
        assert!(native.metrics.scaling_phases >= 3);
        assert!(native.metrics.breakpoint_crossings >= 3);
    }

    #[test]
    fn trace_exposes_scales_marginal_paths_and_breakpoint_crossings() {
        let graph = graph();
        let problem = problem(&graph);
        let run = run_internal(&problem, true).unwrap();
        let trace = ConvexCostScalingTraceResult {
            result: run.result,
            base_snapshot: run.base_snapshot,
            events: run.events,
            final_snapshot: run.final_snapshot,
        };
        for (index, event) in trace.events.iter().enumerate() {
            assert!(
                valid_convex_scaling_event(&problem, event, trace.result.certificate.total_cost),
                "invalid event {index}: {event:#?}"
            );
            assert!(
                valid_convex_scaling_stage_transition(event),
                "invalid transition {index}: {event:#?}"
            );
        }
        for (index, pair) in trace.events.windows(2).enumerate() {
            assert_eq!(pair[0].after, pair[1].before, "discontinuity {index}");
            assert!(
                valid_convex_scaling_stage_order(pair[0].stage, pair[1].stage),
                "invalid order {index}: {:?} -> {:?}",
                pair[0].stage,
                pair[1].stage
            );
        }
        check_convex_cost_scaling_trace(&problem, &trace).unwrap();
        assert!(trace.events.iter().any(|event| {
            event.stage == ConvexCostScalingStage::SaturateMarginal
                && event.after.active_path[0].segment == 0
        }));
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.stage == ConvexCostScalingStage::ShortestPath)
        );
        assert_eq!(trace.final_snapshot.flows, trace.result.flows);
    }

    #[test]
    fn trace_checker_rejects_scale_and_path_corruption() {
        let graph = graph();
        let problem = problem(&graph);
        let trace = trace_convex_cost_scaling(&problem).unwrap();
        let mut corrupt = trace.clone();
        corrupt.events[1].after.scale /= 2;
        assert_eq!(
            check_convex_cost_scaling_trace(&problem, &corrupt),
            Err(ConvexCostScalingError::TraceInvariant)
        );
        let mut corrupt_stage = trace.clone();
        corrupt_stage.events[0].stage = ConvexCostScalingStage::Optimal;
        assert_eq!(
            check_convex_cost_scaling_trace(&problem, &corrupt_stage),
            Err(ConvexCostScalingError::TraceInvariant)
        );
        let mut corrupt_detail = trace;
        let scale = corrupt_detail
            .events
            .iter_mut()
            .find(|event| event.stage == ConvexCostScalingStage::StartScale)
            .expect("start scale event");
        scale.detail = Some(("scale", i128::from(scale.after.scale) + 1));
        assert_eq!(
            check_convex_cost_scaling_trace(&problem, &corrupt_detail),
            Err(ConvexCostScalingError::TraceInvariant)
        );
        let mut base_trace = trace_convex_cost_scaling(&problem).unwrap();
        base_trace.base_snapshot.potentials[0] += 1;
        base_trace.events[0].before.potentials[0] += 1;
        assert_eq!(
            check_convex_cost_scaling_trace(&problem, &base_trace),
            Err(ConvexCostScalingError::TraceInvariant)
        );
    }

    #[test]
    fn marginal_residual_keeps_only_two_boundary_pieces_per_original_edge() {
        let graph = graph();
        let problem = problem(&graph);
        let work = WorkingState {
            flows: vec![2, 0, 0],
            potentials: vec![0; 3],
            remaining: vec![0; 3],
            search_order: Vec::new(),
            active_path: Vec::new(),
            scale: 1,
            metrics: ConvexCostScalingMetrics::default(),
        };
        let arcs = all_marginal_arcs(&problem, &work).unwrap();
        let direct = arcs
            .iter()
            .filter(|arc| arc.reference.edge == 0)
            .collect::<Vec<_>>();
        assert_eq!(direct.len(), 2);
        assert_eq!(direct[0].reference.segment, 2);
        assert_eq!(direct[1].reference.segment, 1);
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
    fn exhaustive_small_native_scaling_matches_expanded_oracle() {
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

                        let native = solve_convex_cost_scaling(&problem).unwrap();
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
}
