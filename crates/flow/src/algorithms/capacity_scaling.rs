//! Exact capacity-scaling minimum-cost flow.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow,
    check_residual_min_cost_optimality, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowEdge, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive admission limit for capacity scaling.
pub const CAPACITY_SCALING_MAX_NODES: usize = 2_000;
/// Conservative interactive edge admission limit for capacity scaling.
pub const CAPACITY_SCALING_MAX_EDGES: usize = 20_000;
/// Deterministic guard against unexpectedly many path augmentations.
pub const CAPACITY_SCALING_MAX_AUGMENTATIONS: u64 = 2_000_000;
/// Deterministic guard against pathological residual scanning.
pub const CAPACITY_SCALING_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;

/// Exact deterministic counters from capacity scaling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapacityScalingMetrics {
    /// Successful eligible residual-path augmentations.
    pub augmentations: u64,
    /// Complete reduced-cost Dijkstra searches.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled across all searches.
    pub settled_nodes: u128,
    /// Positive residual arcs inspected by Dijkstra.
    pub residual_arc_scans: u128,
    /// Dual-potential update phases after successful searches.
    pub potential_updates: u64,
    /// Powers-of-two capacity scales entered, including empty phases.
    pub scaling_phases: u64,
    /// Negative reduced-cost residual arcs saturated at phase boundaries.
    pub phase_saturations: u64,
}

/// Certified canonical capacity-scaling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapacityScalingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: CapacityScalingMetrics,
}

/// Certified capacity-scaling result with reversible phase events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapacityScalingTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: CapacityScalingResult,
    /// Replay boundary at the lower-bound pseudoflow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent minimum-cost certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Capacity-scaling construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CapacityScalingError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds capacity-scaling admission limits")]
    AdmissionLimit,
    /// A deterministic work ceiling was reached.
    #[error("capacity-scaling work limit reached")]
    WorkLimit,
    /// A feasibility precheck proved that the requested balances are impossible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Initial compatibility or final independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("capacity-scaling arithmetic overflow")]
    ArithmeticOverflow,
    /// A feasible precheck and the scaling residual searches disagreed.
    #[error("capacity scaling could not route a feasible remaining imbalance")]
    MissingPath,
    /// A shortest-path predecessor chain was inconsistent.
    #[error("capacity-scaling predecessor invariant failed")]
    PredecessorInvariant,
    /// Orlin excess scaling is exposed only on the source algorithm's
    /// uncapacitated transshipment domain. Finite project arcs represent that
    /// domain only when every residual upper width is nonbinding for the total
    /// remaining supply.
    #[error("excess scaling requires nonbinding capacities for the transshipment domain")]
    ExcessScalingCapacityBound,
    /// The lower-bound pseudoflow contains a negative residual cycle, which
    /// would make the corresponding uncapacitated transshipment problem
    /// unbounded rather than a valid excess-scaling input.
    #[error("excess-scaling transshipment residual contains a negative cycle")]
    ExcessScalingUnbounded,
    /// A scale-eligible residual arc violated the reduced-cost invariant.
    #[error("capacity scaling encountered a negative eligible reduced cost")]
    NegativeEligibleReducedCost,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by powers-of-two capacity scaling.
///
/// The algorithm starts from the lower-bound pseudoflow and a globally feasible
/// potential reconstructed by an independent Bellman--Ford checker. At scale
/// `delta`, it first saturates every newly eligible residual arc with negative
/// reduced cost. It then repeatedly sends at least `delta` units from an
/// eligible excess to an eligible deficit along a shortest path containing only
/// residual arcs of capacity at least `delta`. The final scale is one, so the
/// scale-restricted optimality condition becomes the ordinary residual
/// optimality condition.
///
/// # Errors
///
/// Rejects admission, feasibility, initial negative cycles, arithmetic,
/// residual mutation, work-limit, predecessor, invariant, or certificate
/// failures.
pub fn solve_capacity_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CapacityScalingResult, CapacityScalingError> {
    solve_internal(graph, required_divergence, false, ScalingVariant::Capacity)
        .map(|run| run.result)
}

/// Solves capacity scaling while reporting its feasibility precheck to the
/// enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_capacity_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CapacityScalingResult, CapacityScalingError> {
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        ScalingVariant::Capacity,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves capacity scaling while recording phase initialization, shortest
/// paths, dual updates, and augmentations.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace invariant failures.
pub fn trace_capacity_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CapacityScalingTraceResult, CapacityScalingError> {
    let run = solve_internal(graph, required_divergence, true, ScalingVariant::Capacity)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(CapacityScalingError::PredecessorInvariant)?;
    Ok(CapacityScalingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Solves the uncapacitated transshipment specialization by Orlin-style
/// excess scaling.
///
/// The project has only finite-capacity arc syntax, so this exact variant
/// accepts an arc only when its residual upper width is at least the total
/// positive lower-adjusted imbalance. Such a bound cannot constrain any
/// source-to-deficit routing and therefore represents the source algorithm's
/// uncapacitated domain without silently performing a graph transformation.
/// Each `delta` phase augments exactly `delta` units between vertices with at
/// least `delta` excess and deficit.
///
/// # Errors
///
/// Returns admission, feasibility, nonbinding-capacity, unbounded
/// transshipment, arithmetic, residual, work-limit, invariant, or independent
/// certificate failures.
pub fn solve_excess_scaling_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CapacityScalingResult, CapacityScalingError> {
    solve_internal(graph, required_divergence, false, ScalingVariant::Excess).map(|run| run.result)
}

/// Solves excess scaling while reporting its feasibility precheck to the
/// enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_excess_scaling_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CapacityScalingResult, CapacityScalingError> {
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        ScalingVariant::Excess,
        feasibility,
    )
    .map(|run| run.result)
}

/// Records every excess-scale, shortest path, potential update, and exact
/// `delta` augmentation boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_excess_scaling_mcf`] plus trace
/// invariant failures.
pub fn trace_excess_scaling_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<CapacityScalingTraceResult, CapacityScalingError> {
    let run = solve_internal(graph, required_divergence, true, ScalingVariant::Excess)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(CapacityScalingError::PredecessorInvariant)?;
    Ok(CapacityScalingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces capacity scaling while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_capacity_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CapacityScalingTraceResult, CapacityScalingError> {
    trace_variant_with_feasibility(
        graph,
        required_divergence,
        ScalingVariant::Capacity,
        feasibility,
    )
}

/// Traces excess scaling while explicitly publishing its feasibility precheck
/// to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_excess_scaling_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<CapacityScalingTraceResult, CapacityScalingError> {
    trace_variant_with_feasibility(
        graph,
        required_divergence,
        ScalingVariant::Excess,
        feasibility,
    )
}

fn trace_variant_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    variant: ScalingVariant,
    feasibility: &mut FeasibilityExecution,
) -> Result<CapacityScalingTraceResult, CapacityScalingError> {
    let run =
        solve_internal_with_feasibility(graph, required_divergence, true, variant, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(CapacityScalingError::PredecessorInvariant)?;
    Ok(CapacityScalingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScalingVariant {
    Capacity,
    Excess,
}

struct InternalRun {
    result: CapacityScalingResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct WorkingState<'graph> {
    residual: ResidualState<'graph>,
    remaining: Vec<i128>,
    potentials: Vec<i128>,
    metrics: CapacityScalingMetrics,
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    variant: ScalingVariant,
) -> Result<InternalRun, CapacityScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        trace_enabled,
        variant,
        &mut feasibility,
    )
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    variant: ScalingVariant,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, CapacityScalingError> {
    let (mut work, mut scale) = initialize(graph, required_divergence, variant, feasibility)?;
    let mut recorder = start_trace_recorder(graph, &work, trace_enabled)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        variant.metadata(ScalingEvent::Initialize),
        TraceView::potentials(&work),
        None,
    )?;

    loop {
        run_scaling_phase(graph, &mut work, scale, variant, &mut recorder)?;
        if scale == 1 {
            break;
        }
        scale /= 2;
    }
    if work.remaining.iter().any(|&value| value != 0) {
        return Err(CapacityScalingError::MissingPath);
    }

    let flows = work.residual.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &work,
        variant.metadata(ScalingEvent::Optimal),
        TraceView::potentials(&work),
        Some(("total-cost", certificate.total_cost)),
    )?;
    Ok(InternalRun {
        result: CapacityScalingResult {
            flows,
            certificate,
            metrics: work.metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn initialize<'graph>(
    graph: &'graph FlowNetwork,
    required_divergence: &[i128],
    variant: ScalingVariant,
    feasibility: &mut FeasibilityExecution,
) -> Result<(WorkingState<'graph>, u64), CapacityScalingError> {
    validate_admission(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let potentials = match check_residual_min_cost_optimality(graph, &lower_flows) {
        Ok(potentials) => potentials,
        Err(CertificateError::NegativeCycle) if variant == ScalingVariant::Excess => {
            return Err(CapacityScalingError::ExcessScalingUnbounded);
        }
        Err(error) => return Err(error.into()),
    };
    let actual = divergences(graph, &lower_flows)?;
    if actual.len() != required_divergence.len() {
        return Err(CapacityScalingError::MissingPath);
    }
    let remaining = required_divergence
        .iter()
        .zip(actual)
        .map(|(&required, current)| {
            required
                .checked_sub(current)
                .ok_or(CapacityScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if variant == ScalingVariant::Excess {
        validate_excess_scaling_domain(graph, &remaining)?;
    }
    let scale = initial_scale(&remaining)?;
    Ok((
        WorkingState {
            residual: ResidualState::from_flows(graph, &lower_flows)?,
            remaining,
            potentials,
            metrics: CapacityScalingMetrics::default(),
        },
        scale,
    ))
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), CapacityScalingError> {
    if graph.nodes().len() > CAPACITY_SCALING_MAX_NODES
        || graph.edges().len() > CAPACITY_SCALING_MAX_EDGES
    {
        return Err(CapacityScalingError::AdmissionLimit);
    }
    Ok(())
}

fn validate_excess_scaling_domain(
    graph: &FlowNetwork,
    remaining: &[i128],
) -> Result<(), CapacityScalingError> {
    let total_positive = remaining.iter().try_fold(0_u128, |sum, &value| {
        if value <= 0 {
            return Ok(sum);
        }
        sum.checked_add(
            u128::try_from(value).map_err(|_| CapacityScalingError::ArithmeticOverflow)?,
        )
        .ok_or(CapacityScalingError::ArithmeticOverflow)
    })?;
    let required_width =
        u64::try_from(total_positive).map_err(|_| CapacityScalingError::ArithmeticOverflow)?;
    if graph.edges().iter().any(|edge| {
        edge.capacity()
            .checked_sub(edge.lower())
            .is_none_or(|width| width < required_width)
    }) {
        return Err(CapacityScalingError::ExcessScalingCapacityBound);
    }
    Ok(())
}

fn initial_scale(remaining: &[i128]) -> Result<u64, CapacityScalingError> {
    let max_remaining = remaining.iter().try_fold(0_u64, |maximum, value| {
        let magnitude = value.unsigned_abs().min(u128::from(u64::MAX));
        let bounded =
            u64::try_from(magnitude).map_err(|_| CapacityScalingError::ArithmeticOverflow)?;
        Ok::<_, CapacityScalingError>(maximum.max(bounded))
    })?;
    let maximum = max_remaining.max(1);
    Ok(1_u64 << (u64::BITS - 1 - maximum.leading_zeros()))
}

fn run_scaling_phase(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CapacityScalingError> {
    work.metrics.scaling_phases = work
        .metrics
        .scaling_phases
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        variant.metadata(ScalingEvent::StartPhase),
        TraceView::potentials(work),
        Some(("scale", i128::from(scale))),
    )?;
    if variant == ScalingVariant::Capacity {
        saturate_negative_eligible_arcs(graph, work, scale, variant, recorder)?;
    }
    validate_eligible_reduced_costs(work, scale)?;
    route_eligible_imbalances(graph, work, scale, variant, recorder)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        variant.metadata(ScalingEvent::CompletePhase),
        TraceView::potentials(work),
        Some(("scale", i128::from(scale))),
    )?;
    Ok(())
}

fn saturate_negative_eligible_arcs(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CapacityScalingError> {
    for edge in graph.edges() {
        for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
            let id = ResidualArcId::new(edge.id().clone(), direction);
            let Some(arc) = work.residual.arc(&id) else {
                return Err(CapacityScalingError::PredecessorInvariant);
            };
            if arc.capacity < scale || reduced_cost(&arc, &work.potentials)? >= 0 {
                continue;
            }
            let amount = arc.capacity;
            work.residual.augment(std::slice::from_ref(&id), amount)?;
            update_remaining(&mut work.remaining, arc.from, arc.to, amount)?;
            work.metrics.phase_saturations = work
                .metrics
                .phase_saturations
                .checked_add(1)
                .ok_or(CapacityScalingError::ArithmeticOverflow)?;
            record_trace(
                recorder.as_mut(),
                graph,
                work,
                variant.metadata(ScalingEvent::Saturate),
                TraceView::path(work, vec![id]),
                Some(("delta", i128::from(amount))),
            )?;
        }
    }
    Ok(())
}

fn route_eligible_imbalances(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CapacityScalingError> {
    loop {
        let sources = graph
            .node_indices()
            .filter(|node| work.remaining[node.as_usize()] >= i128::from(scale))
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Ok(());
        }
        let mut selected = None;
        for source in sources {
            let search =
                shortest_path_to_eligible_deficit(graph, work, source, scale, variant, recorder)?;
            record_search(graph, work, scale, variant, recorder, &search)?;
            if search.path.is_some() {
                selected = Some(search);
                break;
            }
        }
        let Some(search) = selected else {
            return Ok(());
        };
        apply_search_augmentation(graph, work, scale, variant, recorder, search)?;
    }
}

fn record_search(
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    search: &Search,
) -> Result<(), CapacityScalingError> {
    let path_cost = search
        .path
        .as_deref()
        .map(|path| residual_path_cost(&work.residual, path))
        .transpose()?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        variant.metadata(if search.path.is_some() {
            ScalingEvent::ShortestPath
        } else {
            ScalingEvent::NoDeficit
        }),
        TraceView::search(work, search)?,
        path_cost.map_or(Some(("scale", i128::from(scale))), |cost| {
            Some(("path-cost", cost))
        }),
    )?;
    Ok(())
}

fn apply_search_augmentation(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    search: Search,
) -> Result<(), CapacityScalingError> {
    let sink = search.sink.ok_or(CapacityScalingError::MissingPath)?;
    let path = search.path.ok_or(CapacityScalingError::MissingPath)?;
    update_potentials(work, &search.distances, sink)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        variant.metadata(ScalingEvent::UpdatePotentials),
        TraceView::path_with_order(work, path.clone(), search.settled_order.clone()),
        Some(("scale", i128::from(scale))),
    )?;
    let amount = if variant == ScalingVariant::Excess {
        scale
    } else {
        augmentation_amount(work, search.source, sink, &path)?
    };
    if amount < scale {
        return Err(CapacityScalingError::PredecessorInvariant);
    }
    work.residual.augment(&path, amount)?;
    update_remaining(&mut work.remaining, search.source, sink, amount)?;
    work.metrics.augmentations = work
        .metrics
        .augmentations
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    if work.metrics.augmentations > CAPACITY_SCALING_MAX_AUGMENTATIONS {
        return Err(CapacityScalingError::WorkLimit);
    }
    validate_eligible_reduced_costs(work, scale)?;
    record_trace(
        recorder.as_mut(),
        graph,
        work,
        variant.metadata(ScalingEvent::Augment),
        TraceView::path_with_order(work, path, search.settled_order),
        Some(("delta", i128::from(amount))),
    )?;
    Ok(())
}

struct Search {
    source: NodeIndex,
    sink: Option<NodeIndex>,
    path: Option<Vec<ResidualArcId>>,
    distances: Vec<Option<i128>>,
    settled_order: Vec<NodeIndex>,
}

fn shortest_path_to_eligible_deficit(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    source: NodeIndex,
    scale: u64,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<Search, CapacityScalingError> {
    let node_count = work.residual.graph().nodes().len();
    if work.potentials.len() != node_count {
        return Err(CapacityScalingError::PredecessorInvariant);
    }
    work.metrics.dijkstra_runs = work
        .metrics
        .dijkstra_runs
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    let mut state = DijkstraState::new(node_count, source);
    while let Some(Reverse((distance, hops, node))) = state.heap.pop() {
        if state.settled[node.as_usize()]
            || state.distances[node.as_usize()] != Some(distance)
            || state.hops[node.as_usize()] != hops
        {
            continue;
        }
        settle_node(
            graph, work, scale, &mut state, distance, hops, node, variant, recorder,
        )?;
    }
    finish_search(work, source, scale, state)
}

struct DijkstraState {
    distances: Vec<Option<i128>>,
    hops: Vec<usize>,
    predecessor: Vec<Option<ResidualArcId>>,
    settled: Vec<bool>,
    settled_order: Vec<NodeIndex>,
    heap: BinaryHeap<Reverse<(i128, usize, NodeIndex)>>,
}

impl DijkstraState {
    fn new(node_count: usize, source: NodeIndex) -> Self {
        let mut distances = vec![None; node_count];
        let mut hops = vec![usize::MAX; node_count];
        let mut heap = BinaryHeap::new();
        distances[source.as_usize()] = Some(0);
        hops[source.as_usize()] = 0;
        heap.push(Reverse((0, 0, source)));
        Self {
            distances,
            hops,
            predecessor: vec![None; node_count],
            settled: vec![false; node_count],
            settled_order: Vec::new(),
            heap,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn settle_node(
    graph: &FlowNetwork,
    work: &mut WorkingState<'_>,
    scale: u64,
    state: &mut DijkstraState,
    distance: i128,
    hops: usize,
    node: NodeIndex,
    variant: ScalingVariant,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), CapacityScalingError> {
    state.settled[node.as_usize()] = true;
    state.settled_order.push(node);
    work.metrics.settled_nodes = work
        .metrics
        .settled_nodes
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    for arc in work.residual.outgoing_arcs(node) {
        work.metrics.residual_arc_scans = work
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(CapacityScalingError::ArithmeticOverflow)?;
        if work.metrics.residual_arc_scans > CAPACITY_SCALING_MAX_RESIDUAL_ARC_SCANS {
            return Err(CapacityScalingError::WorkLimit);
        }
        let id = arc.id.clone();
        let arc_from = arc.from;
        let arc_to = arc.to;
        let residual_capacity = i128::from(arc.capacity);
        if arc.capacity >= scale {
            let reduced = reduced_cost(&arc, &work.potentials)?;
            if reduced < 0 {
                return Err(CapacityScalingError::NegativeEligibleReducedCost);
            }
            relax_arc(state, distance, hops, arc, reduced)?;
        }
        record_trace(
            recorder.as_mut(),
            graph,
            work,
            variant.metadata(ScalingEvent::InspectResidualArc),
            TraceView::dijkstra_scan(state, id, arc_from, arc_to),
            Some(("residual-capacity", residual_capacity)),
        )?;
    }
    Ok(())
}

fn relax_arc(
    state: &mut DijkstraState,
    distance: i128,
    hops: usize,
    arc: crate::residual::ResidualArc,
    reduced: i128,
) -> Result<(), CapacityScalingError> {
    let candidate_distance = distance
        .checked_add(reduced)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    let candidate_hops = hops
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    let current =
        state.distances[arc.to.as_usize()].map(|value| (value, state.hops[arc.to.as_usize()]));
    if current.is_none_or(|value| (candidate_distance, candidate_hops) < value) {
        state.distances[arc.to.as_usize()] = Some(candidate_distance);
        state.hops[arc.to.as_usize()] = candidate_hops;
        state.predecessor[arc.to.as_usize()] = Some(arc.id);
        state
            .heap
            .push(Reverse((candidate_distance, candidate_hops, arc.to)));
    }
    Ok(())
}

fn finish_search(
    work: &WorkingState<'_>,
    source: NodeIndex,
    scale: u64,
    state: DijkstraState,
) -> Result<Search, CapacityScalingError> {
    let sink = work
        .residual
        .graph()
        .node_indices()
        .filter(|node| work.remaining[node.as_usize()] <= -i128::from(scale))
        .filter_map(|node| {
            state.distances[node.as_usize()]
                .map(|distance| (distance, state.hops[node.as_usize()], node))
        })
        .min()
        .map(|(_, _, node)| node);
    let path = sink
        .map(|sink| reconstruct_path(&work.residual, source, sink, &state.predecessor))
        .transpose()?;
    Ok(Search {
        source,
        sink,
        path,
        distances: state.distances,
        settled_order: state.settled_order,
    })
}

fn reconstruct_path(
    residual: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, CapacityScalingError> {
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(CapacityScalingError::PredecessorInvariant)?;
        let arc = residual
            .arc(&id)
            .ok_or(CapacityScalingError::PredecessorInvariant)?;
        if arc.to != cursor || arc.capacity == 0 {
            return Err(CapacityScalingError::PredecessorInvariant);
        }
        cursor = arc.from;
        reversed.push(id);
        if reversed.len() > predecessor.len() {
            return Err(CapacityScalingError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

fn update_potentials(
    work: &mut WorkingState<'_>,
    distances: &[Option<i128>],
    sink: NodeIndex,
) -> Result<(), CapacityScalingError> {
    if work.potentials.len() != distances.len() {
        return Err(CapacityScalingError::PredecessorInvariant);
    }
    let cutoff = distances[sink.as_usize()].ok_or(CapacityScalingError::PredecessorInvariant)?;
    for (potential, distance) in work.potentials.iter_mut().zip(distances) {
        let adjustment = distance.map_or(cutoff, |value| value.min(cutoff));
        *potential = potential
            .checked_add(adjustment)
            .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    }
    work.metrics.potential_updates = work
        .metrics
        .potential_updates
        .checked_add(1)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    Ok(())
}

fn augmentation_amount(
    work: &WorkingState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    path: &[ResidualArcId],
) -> Result<u64, CapacityScalingError> {
    let bottleneck = path
        .iter()
        .map(|id| {
            work.residual
                .arc(id)
                .filter(|arc| arc.capacity > 0)
                .map(|arc| arc.capacity)
                .ok_or(CapacityScalingError::PredecessorInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(CapacityScalingError::PredecessorInvariant)?;
    let source_excess = positive_u64(work.remaining[source.as_usize()])?;
    let sink_deficit = positive_u64(
        work.remaining[sink.as_usize()]
            .checked_neg()
            .ok_or(CapacityScalingError::ArithmeticOverflow)?,
    )?;
    Ok(bottleneck.min(source_excess).min(sink_deficit))
}

fn positive_u64(value: i128) -> Result<u64, CapacityScalingError> {
    let positive = u128::try_from(value).map_err(|_| CapacityScalingError::ArithmeticOverflow)?;
    u64::try_from(positive.min(u128::from(u64::MAX)))
        .map_err(|_| CapacityScalingError::ArithmeticOverflow)
}

fn update_remaining(
    remaining: &mut [i128],
    from: NodeIndex,
    to: NodeIndex,
    amount: u64,
) -> Result<(), CapacityScalingError> {
    let delta = i128::from(amount);
    remaining[from.as_usize()] = remaining[from.as_usize()]
        .checked_sub(delta)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    remaining[to.as_usize()] = remaining[to.as_usize()]
        .checked_add(delta)
        .ok_or(CapacityScalingError::ArithmeticOverflow)?;
    Ok(())
}

fn reduced_cost(
    arc: &crate::residual::ResidualArc,
    potentials: &[i128],
) -> Result<i128, CapacityScalingError> {
    arc.cost
        .checked_add(potentials[arc.from.as_usize()])
        .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
        .ok_or(CapacityScalingError::ArithmeticOverflow)
}

fn residual_path_cost(
    residual: &ResidualState<'_>,
    path: &[ResidualArcId],
) -> Result<i128, CapacityScalingError> {
    path.iter().try_fold(0_i128, |sum, id| {
        let arc = residual
            .arc(id)
            .ok_or(CapacityScalingError::PredecessorInvariant)?;
        sum.checked_add(arc.cost)
            .ok_or(CapacityScalingError::ArithmeticOverflow)
    })
}

fn validate_eligible_reduced_costs(
    work: &WorkingState<'_>,
    scale: u64,
) -> Result<(), CapacityScalingError> {
    for node in work.residual.graph().node_indices() {
        for arc in work.residual.outgoing_arcs(node) {
            if arc.capacity >= scale && reduced_cost(&arc, &work.potentials)? < 0 {
                return Err(CapacityScalingError::NegativeEligibleReducedCost);
            }
        }
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    work: &WorkingState<'_>,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !enabled {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        work.remaining.clone(),
        trace_metrics(work.metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

#[derive(Clone, Copy)]
enum ScalingEvent {
    Initialize,
    StartPhase,
    Saturate,
    InspectResidualArc,
    ShortestPath,
    NoDeficit,
    UpdatePotentials,
    Augment,
    CompletePhase,
    Optimal,
}

impl ScalingVariant {
    #[allow(clippy::too_many_lines)]
    const fn metadata(self, event: ScalingEvent) -> FlowTraceEventMetadata {
        let (catalog_id, granularity, pseudocode_line) = match (self, event) {
            (Self::Capacity, ScalingEvent::Initialize) => (
                "capacity-scaling-mcf.initialize-potentials",
                TraceGranularityV1::Phase,
                "capacity-scaling:initialize-lower-pseudoflow-and-potentials",
            ),
            (Self::Capacity, ScalingEvent::StartPhase) => (
                "capacity-scaling-mcf.start-scaling-phase",
                TraceGranularityV1::Phase,
                "capacity-scaling:start-delta-phase",
            ),
            (Self::Capacity, ScalingEvent::Saturate) => (
                "capacity-scaling-mcf.saturate-negative-arc",
                TraceGranularityV1::Operation,
                "capacity-scaling:saturate-negative-eligible-residual-arc",
            ),
            (Self::Capacity, ScalingEvent::InspectResidualArc) => (
                "capacity-scaling-mcf.inspect-residual-arc",
                TraceGranularityV1::Micro,
                "capacity-scaling:inspect-one-delta-residual-arc",
            ),
            (Self::Capacity, ScalingEvent::ShortestPath) => (
                "capacity-scaling-mcf.shortest-eligible-path",
                TraceGranularityV1::Phase,
                "capacity-scaling:dijkstra-delta-residual",
            ),
            (Self::Capacity, ScalingEvent::NoDeficit) => (
                "capacity-scaling-mcf.no-eligible-deficit",
                TraceGranularityV1::Phase,
                "capacity-scaling:try-next-excess-or-complete-phase",
            ),
            (Self::Capacity, ScalingEvent::UpdatePotentials) => (
                "capacity-scaling-mcf.update-potentials",
                TraceGranularityV1::Phase,
                "capacity-scaling:update-delta-feasible-potentials",
            ),
            (Self::Capacity, ScalingEvent::Augment) => (
                "capacity-scaling-mcf.augment",
                TraceGranularityV1::Operation,
                "capacity-scaling:augment-eligible-path",
            ),
            (Self::Capacity, ScalingEvent::CompletePhase) => (
                "capacity-scaling-mcf.complete-scaling-phase",
                TraceGranularityV1::Phase,
                "capacity-scaling:complete-delta-phase",
            ),
            (Self::Capacity, ScalingEvent::Optimal) => (
                "capacity-scaling-mcf.optimal",
                TraceGranularityV1::Phase,
                "capacity-scaling:return-minimum-cost-flow",
            ),
            (Self::Excess, ScalingEvent::Initialize) => (
                "excess-scaling-mcf.initialize-potentials",
                TraceGranularityV1::Phase,
                "excess-scaling:initialize-lower-pseudoflow-and-feasible-potentials",
            ),
            (Self::Excess, ScalingEvent::StartPhase) => (
                "excess-scaling-mcf.start-excess-phase",
                TraceGranularityV1::Phase,
                "excess-scaling:start-delta-excess-phase",
            ),
            (Self::Excess, ScalingEvent::ShortestPath) => (
                "excess-scaling-mcf.shortest-large-excess-path",
                TraceGranularityV1::Phase,
                "excess-scaling:dijkstra-from-large-excess-to-large-deficit",
            ),
            (Self::Excess, ScalingEvent::InspectResidualArc) => (
                "excess-scaling-mcf.inspect-residual-arc",
                TraceGranularityV1::Micro,
                "excess-scaling:inspect-one-residual-arc",
            ),
            (Self::Excess, ScalingEvent::NoDeficit) => (
                "excess-scaling-mcf.no-reachable-large-deficit",
                TraceGranularityV1::Phase,
                "excess-scaling:try-next-large-excess-or-complete-phase",
            ),
            (Self::Excess, ScalingEvent::UpdatePotentials) => (
                "excess-scaling-mcf.update-potentials",
                TraceGranularityV1::Phase,
                "excess-scaling:update-shortest-path-potentials",
            ),
            (Self::Excess, ScalingEvent::Augment) => (
                "excess-scaling-mcf.augment-exact-delta",
                TraceGranularityV1::Operation,
                "excess-scaling:send-exact-delta-units",
            ),
            (Self::Excess, ScalingEvent::CompletePhase) => (
                "excess-scaling-mcf.complete-excess-phase",
                TraceGranularityV1::Phase,
                "excess-scaling:complete-large-excess-to-deficit-routing",
            ),
            (Self::Excess, ScalingEvent::Optimal) => (
                "excess-scaling-mcf.optimal",
                TraceGranularityV1::Phase,
                "excess-scaling:return-independent-minimum-cost-certificate",
            ),
            (Self::Excess, ScalingEvent::Saturate) => (
                "excess-scaling-mcf.unused-capacity-saturation",
                TraceGranularityV1::Operation,
                "excess-scaling:uncapacitated-domain-has-no-capacity-saturation-step",
            ),
        };
        metadata(catalog_id, granularity, pseudocode_line)
    }
}

impl TraceView {
    fn potentials(work: &WorkingState<'_>) -> Self {
        Self {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }

    fn path(work: &WorkingState<'_>, path: Vec<ResidualArcId>) -> Self {
        Self {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path,
        }
    }

    fn path_with_order(
        work: &WorkingState<'_>,
        path: Vec<ResidualArcId>,
        search_order: Vec<NodeIndex>,
    ) -> Self {
        Self {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order,
            path,
        }
    }

    fn search(work: &WorkingState<'_>, search: &Search) -> Result<Self, CapacityScalingError> {
        let path = search.path.clone().unwrap_or_default();
        let mut search_order = vec![search.source];
        let mut cursor = search.source;
        for arc_id in &path {
            let arc = work
                .residual
                .arc(arc_id)
                .ok_or(CapacityScalingError::PredecessorInvariant)?;
            if arc.from != cursor {
                return Err(CapacityScalingError::PredecessorInvariant);
            }
            cursor = arc.to;
            search_order.push(cursor);
        }
        if search.sink.is_some_and(|sink| sink != cursor) {
            return Err(CapacityScalingError::PredecessorInvariant);
        }
        Ok(Self {
            labels: search.distances.clone(),
            search_order,
            path,
        })
    }

    fn dijkstra_scan(
        state: &DijkstraState,
        arc: ResidualArcId,
        _from: NodeIndex,
        to: NodeIndex,
    ) -> Self {
        Self {
            labels: state.distances.clone(),
            // The arc already communicates its source. Mark only the endpoint
            // whose tentative distance can change at this source operation;
            // highlighting both endpoints turns a single-arc inspection into
            // a graph-wide flash on two-node instances.
            search_order: vec![to],
            path: vec![arc],
        }
    }
}

const fn metadata(
    catalog_id: &'static str,
    minimum_granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
) -> FlowTraceEventMetadata {
    FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    work: &WorkingState<'_>,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let focus_arc_ids = view.path.clone();
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        &work.residual,
        view.labels,
        view.search_order,
        view.path,
        work.remaining.clone(),
        trace_metrics(work.metrics),
    );
    if metadata.minimum_granularity == TraceGranularityV1::Micro || !focus_arc_ids.is_empty() {
        let mut focus = Vec::new();
        for arc_id in focus_arc_ids {
            let arc = work
                .residual
                .arc(&arc_id)
                .ok_or(FlowTraceError::MissingEntity)?;
            if metadata.minimum_granularity != TraceGranularityV1::Micro {
                focus.push(FlowTraceEntityRef::Node(
                    graph.nodes()[arc.from.as_usize()].id().clone(),
                ));
                focus.push(FlowTraceEntityRef::Node(
                    graph.nodes()[arc.to.as_usize()].id().clone(),
                ));
            }
            focus.push(FlowTraceEntityRef::Edge(arc_id.original_edge().clone()));
            focus.push(FlowTraceEntityRef::ResidualArc(arc_id));
        }
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, detail)
    }
}

const fn trace_metrics(metrics: CapacityScalingMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.dijkstra_runs as u128,
        scaling_phases: metrics.scaling_phases as u128,
        blocking_flow_phases: 0,
        relabels: metrics.potential_updates as u128,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: metrics.phase_saturations as u128,
        saturating_pushes: metrics.phase_saturations as u128,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.settled_nodes,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_bellman_ford_ssp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node id"), supply))
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge id"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("valid network")
    }

    fn target(graph: &FlowNetwork) -> Vec<i128> {
        graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect()
    }

    #[test]
    fn routes_large_imbalances_in_multiple_power_of_two_phases() {
        let graph = network(
            &[("s", 13), ("t", -13)],
            &[
                ("cheap", "s", "t", 0, 8, 1),
                ("expensive", "s", "t", 0, 8, 3),
            ],
        );
        let result = solve_capacity_scaling(&graph, &target(&graph)).expect("capacity scaling");

        assert_eq!(result.flows, [8, 5]);
        assert_eq!(result.certificate.total_cost, 23);
        assert_eq!(result.metrics.augmentations, 2);
        assert_eq!(result.metrics.scaling_phases, 4);
    }

    #[test]
    fn phase_transition_saturates_new_negative_reduced_cost_arc() {
        let graph = network(
            &[("a", 0), ("s", 7), ("t", -7)],
            &[
                ("at", "a", "t", 0, 4, 5),
                ("direct", "s", "t", 0, 3, 0),
                ("sa", "s", "a", 0, 4, 5),
            ],
        );
        let traced = trace_capacity_scaling(&graph, &target(&graph)).expect("capacity scaling");

        assert_eq!(traced.result.flows, [4, 3, 4]);
        assert_eq!(traced.result.certificate.total_cost, 40);
        assert_eq!(traced.result.metrics.phase_saturations, 1);
        let saturation = traced
            .events
            .iter()
            .find(|event| event.catalog_id.ends_with(".saturate-negative-arc"))
            .expect("phase saturation event");
        assert_eq!(
            saturation
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 3))
        );
    }

    #[test]
    fn supports_lower_bounds_and_negative_edges_without_negative_cycles() {
        let graph = network(
            &[("s", 3), ("t", -3)],
            &[
                ("negative", "s", "t", 1, 2, -4),
                ("positive", "s", "t", 0, 2, 3),
            ],
        );
        let result = solve_capacity_scaling(&graph, &target(&graph)).expect("capacity scaling");
        assert_eq!(result.flows, [2, 1]);
        assert_eq!(result.certificate.total_cost, -5);
    }

    #[test]
    fn trace_replays_in_both_directions_and_matches_fast_profile() {
        let graph = network(
            &[("a", 0), ("s", 7), ("t", -7)],
            &[
                ("at", "a", "t", 0, 4, 5),
                ("direct", "s", "t", 0, 3, 0),
                ("sa", "s", "a", 0, 4, 5),
            ],
        );
        let target = target(&graph);
        let fast = solve_capacity_scaling(&graph, &target).expect("fast");
        let traced = trace_capacity_scaling(&graph, &target).expect("trace");
        assert_eq!(traced.result, fast);

        let mut replay = traced.base_snapshot.clone();
        let mut inspected = 0;
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id.ends_with(".inspect-residual-arc") {
                inspected += 1;
                assert!(
                    event
                        .entity_refs
                        .iter()
                        .filter(|entity| matches!(entity, FlowTraceEntityRef::Node(_)))
                        .count()
                        <= 1
                );
                assert_eq!(
                    event
                        .entity_refs
                        .iter()
                        .filter(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                        .count(),
                    1
                );
                assert!(replay.search_order.len() <= 1);
                assert_eq!(replay.active_path.len(), 1);
            }
        }
        assert!(inspected > 0);
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn deterministic_small_graphs_match_independent_bellman_ford_ssp_costs() {
        for mask in 0_u64..32 {
            let supply = i64::try_from(1 + (mask & 3)).expect("small supply");
            let graph = network(
                &[("a", 0), ("b", 0), ("s", supply), ("t", -supply)],
                &[
                    ("at", "a", "t", 0, 4, i64::try_from(mask & 3).expect("cost")),
                    ("bt", "b", "t", 0, 4, 3),
                    ("sa", "s", "a", 0, 4, 1),
                    (
                        "sb",
                        "s",
                        "b",
                        0,
                        4,
                        i64::try_from((mask >> 2) & 3).expect("cost"),
                    ),
                ],
            );
            let target = target(&graph);
            let scaled = solve_capacity_scaling(&graph, &target).expect("capacity scaling");
            let ssp = solve_bellman_ford_ssp(&graph, &target).expect("Bellman-Ford SSP");
            assert_eq!(scaled.certificate.total_cost, ssp.certificate.total_cost);
            check_min_cost_flow(&graph, &target, &scaled.flows).expect("independent certificate");
        }
    }

    #[test]
    fn excess_scaling_routes_exact_powers_of_two_on_nonbinding_transshipment_arcs() {
        let graph = network(
            &[("s", 13), ("t", -13)],
            &[
                ("cheap", "s", "t", 0, 13, 1),
                ("expensive", "s", "t", 0, 13, 3),
            ],
        );
        let target = target(&graph);
        let traced = trace_excess_scaling_mcf(&graph, &target).expect("excess scaling");

        assert_eq!(traced.result.flows, [13, 0]);
        assert_eq!(traced.result.certificate.total_cost, 13);
        assert_eq!(traced.result.metrics.scaling_phases, 4);
        assert_eq!(traced.result.metrics.augmentations, 3);
        assert_eq!(traced.result.metrics.phase_saturations, 0);
        let deltas = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "excess-scaling-mcf.augment-exact-delta")
            .map(|event| event.detail.as_ref().expect("exact delta").value)
            .collect::<Vec<_>>();
        assert_eq!(deltas, [8, 4, 1]);
        assert!(
            traced
                .events
                .iter()
                .all(|event| event.catalog_id.starts_with("excess-scaling-mcf."))
        );

        let fast = solve_excess_scaling_mcf(&graph, &target).expect("fast excess scaling");
        assert_eq!(traced.result, fast);
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn shortest_path_boundaries_focus_the_selected_path_not_every_settled_node() {
        let graph = network(
            &[("s", 5), ("a", 0), ("b", 0), ("t", -5)],
            &[
                ("sa", "s", "a", 0, 5, 1),
                ("at", "a", "t", 0, 5, 1),
                ("sb", "s", "b", 0, 5, 10),
                ("bt", "b", "t", 0, 5, 10),
            ],
        );
        let traced = trace_excess_scaling_mcf(&graph, &target(&graph)).expect("excess scaling");
        let excluded = NodeId::parse("b").expect("node id");
        let expected_order = ["s", "a", "t"]
            .into_iter()
            .map(|id| NodeId::parse(id).expect("node id"))
            .collect::<Vec<_>>();
        let mut replay = traced.base_snapshot.clone();
        let mut shortest_path_events = 0;
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id != "excess-scaling-mcf.shortest-large-excess-path" {
                continue;
            }
            shortest_path_events += 1;
            assert_eq!(event.minimum_granularity, TraceGranularityV1::Phase);
            assert_eq!(replay.search_order, expected_order);
            assert!(
                !event
                    .entity_refs
                    .contains(&FlowTraceEntityRef::Node(excluded.clone()))
            );
            assert_eq!(
                event
                    .entity_refs
                    .iter()
                    .filter(|entity| matches!(entity, FlowTraceEntityRef::Node(_)))
                    .count(),
                expected_order.len(),
            );
        }
        assert!(shortest_path_events > 0);
    }

    #[test]
    fn excess_scaling_rejects_binding_capacities_and_unbounded_transshipment_cycles() {
        let binding = network(
            &[("s", 5), ("t", -5)],
            &[("narrow", "s", "t", 0, 4, 0), ("wide", "s", "t", 0, 5, 1)],
        );
        assert_eq!(
            solve_excess_scaling_mcf(&binding, &target(&binding)),
            Err(CapacityScalingError::ExcessScalingCapacityBound)
        );

        let negative_cycle = network(
            &[("a", 0), ("b", 0)],
            &[("ab", "a", "b", 0, 1, -2), ("ba", "b", "a", 0, 1, 1)],
        );
        assert_eq!(
            solve_excess_scaling_mcf(&negative_cycle, &target(&negative_cycle)),
            Err(CapacityScalingError::ExcessScalingUnbounded)
        );
    }

    #[test]
    fn excess_scaling_matches_ssp_on_small_nonbinding_transshipment_dags() {
        for mask in 0_u64..32 {
            let supply = i64::try_from(1 + (mask & 3)).expect("small supply");
            let bound = u64::try_from(supply).expect("positive bound");
            let graph = network(
                &[("a", 0), ("b", 0), ("s", supply), ("t", -supply)],
                &[
                    (
                        "at",
                        "a",
                        "t",
                        0,
                        bound,
                        i64::try_from(mask & 3).expect("cost"),
                    ),
                    ("bt", "b", "t", 0, bound, 3),
                    ("sa", "s", "a", 0, bound, 1),
                    (
                        "sb",
                        "s",
                        "b",
                        0,
                        bound,
                        i64::try_from((mask >> 2) & 3).expect("cost"),
                    ),
                ],
            );
            let target = target(&graph);
            let scaled = solve_excess_scaling_mcf(&graph, &target)
                .unwrap_or_else(|error| panic!("excess case {mask}: {error}"));
            let ssp = solve_bellman_ford_ssp(&graph, &target)
                .unwrap_or_else(|error| panic!("SSP case {mask}: {error}"));
            assert_eq!(
                scaled.certificate.total_cost, ssp.certificate.total_cost,
                "case {mask}"
            );
            check_min_cost_flow(&graph, &target, &scaled.flows).expect("independent certificate");
        }
    }
}
