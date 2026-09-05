//! Strongly-polynomial minimum-mean residual-cycle cancellation.

use std::cmp::Ordering;

use thiserror::Error;

use crate::certificate::{CertificateError, MinCostFlowCertificate, check_min_cost_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for Karp minimum-mean selection.
pub const MINIMUM_MEAN_CYCLE_CANCELING_MAX_NODES: usize = 128;
/// Conservative interactive edge limit for Karp minimum-mean selection.
pub const MINIMUM_MEAN_CYCLE_CANCELING_MAX_EDGES: usize = 1_024;
/// Deterministic ceiling on canceled minimum-mean cycles.
pub const MINIMUM_MEAN_CYCLE_CANCELING_MAX_CYCLES: u64 = 10_000;
/// Deterministic ceiling on residual-arc inspections across all selectors.
pub const MINIMUM_MEAN_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;

/// Exact deterministic counters from minimum-mean cycle canceling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumMeanCycleCancelingMetrics {
    /// Complete all-component minimum-mean-cycle selections.
    pub mean_cycle_searches: u64,
    /// Karp dynamic-programming rounds over cyclic strong components.
    pub dynamic_programming_rounds: u64,
    /// Residual arcs inspected by SCC, Karp, and tight-cycle work.
    pub residual_arc_scans: u128,
    /// Minimum-mean negative residual cycles canceled.
    pub canceled_cycles: u64,
}

/// Certified canonical minimum-mean-cycle-canceling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumMeanCycleCancelingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic selector counters.
    pub metrics: MinimumMeanCycleCancelingMetrics,
}

/// Certified result with reversible selection and cancellation events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumMeanCycleCancelingTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: MinimumMeanCycleCancelingResult,
    /// Replay boundary at the arbitrary feasible initial flow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after verified minimum-cost optimality.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Minimum-mean cycle cancellation construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MinimumMeanCycleCancelingError {
    /// Input exceeds the practical Karp-selector admission band.
    #[error("graph exceeds minimum-mean cycle canceling admission limits")]
    AdmissionLimit,
    /// A deterministic cycle or residual-scan ceiling was reached.
    #[error("minimum-mean cycle canceling work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and original bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The final independent primal/dual certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("minimum-mean cycle canceling arithmetic overflow")]
    ArithmeticOverflow,
    /// Karp's value, tight arcs, or reconstructed cycle disagreed.
    #[error("minimum-mean cycle selector invariant failed")]
    SelectorInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Minimizes an arbitrary feasible flow by canceling minimum-mean residual cycles.
///
/// # Errors
///
/// Rejects admission, infeasibility, arithmetic, residual mutation, work-limit,
/// selector-invariant, or final independent-certificate failure.
pub fn solve_minimum_mean_cycle_canceling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<MinimumMeanCycleCancelingResult, MinimumMeanCycleCancelingError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_minimum_mean_cycle_canceling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumMeanCycleCancelingResult, MinimumMeanCycleCancelingError> {
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        FeasibilityUse::InitialFlow,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves with an explicit semantic role for a nested feasibility subroutine.
/// Composite algorithms use this to preserve the trace placement contract
/// without allocating source events in fast profile.
pub(super) fn solve_minimum_mean_cycle_canceling_with_feasibility_use(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    use_kind: FeasibilityUse,
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumMeanCycleCancelingResult, MinimumMeanCycleCancelingError> {
    solve_internal_with_feasibility(graph, required_divergence, false, use_kind, feasibility)
        .map(|run| run.result)
}

/// Runs minimum-mean cycle canceling with reversible selector events.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace invariant failures.
pub fn trace_minimum_mean_cycle_canceling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<MinimumMeanCycleCancelingTraceResult, MinimumMeanCycleCancelingError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
    Ok(MinimumMeanCycleCancelingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces minimum-mean cycle canceling while explicitly publishing its
/// initial feasible-flow construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_minimum_mean_cycle_canceling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumMeanCycleCancelingTraceResult, MinimumMeanCycleCancelingError> {
    trace_minimum_mean_cycle_canceling_with_feasibility_use(
        graph,
        required_divergence,
        FeasibilityUse::InitialFlow,
        feasibility,
    )
}

/// Traces minimum-mean cycle canceling with an explicit semantic role for its
/// feasibility subroutine. Composite source algorithms use this to place the
/// transformed run before the exact owning boundary.
pub(super) fn trace_minimum_mean_cycle_canceling_with_feasibility_use(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    use_kind: FeasibilityUse,
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumMeanCycleCancelingTraceResult, MinimumMeanCycleCancelingError> {
    let run =
        solve_internal_with_feasibility(graph, required_divergence, true, use_kind, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
    Ok(MinimumMeanCycleCancelingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: MinimumMeanCycleCancelingResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
) -> Result<InternalRun, MinimumMeanCycleCancelingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        record_events,
        FeasibilityUse::InitialFlow,
        &mut feasibility,
    )
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
    use_kind: FeasibilityUse,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, MinimumMeanCycleCancelingError> {
    validate_admission(graph)?;
    let feasible = feasibility.find_feasible_flow(graph, required_divergence, use_kind)?;
    let mut state = ResidualState::from_flows(graph, &feasible.flows)?;
    let mut metrics = MinimumMeanCycleCancelingMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, metrics, record_events)?;

    loop {
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "minimum-mean-cycle-canceling.start-selector",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "minimum-mean-cycle-canceling:start-karp-selector",
            },
            SelectorTraceView::empty(graph),
            None,
        )?;
        let selection = select_minimum_mean_cycle(&state, &mut metrics, recorder.as_mut())?;
        let Some(selected) = selection.selected else {
            record_trace(
                recorder.as_mut(),
                graph,
                &state,
                metrics,
                FlowTraceEventMetadata {
                    catalog_id: "minimum-mean-cycle-canceling.optimal",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "minimum-mean-cycle-canceling:return-negative-cycle-free-flow",
                },
                SelectorTraceView::empty(graph),
                None,
            )?;
            break;
        };
        if metrics.canceled_cycles >= MINIMUM_MEAN_CYCLE_CANCELING_MAX_CYCLES {
            return Err(MinimumMeanCycleCancelingError::WorkLimit);
        }
        let search_order = cycle_nodes(&state, &selected.cycle)?;
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "minimum-mean-cycle-canceling.select-minimum-mean-cycle",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "minimum-mean-cycle-canceling:karp-minimum-cycle-mean",
            },
            SelectorTraceView {
                labels: selected.labels,
                search_order: search_order.clone(),
                cycle: selected.cycle.clone(),
            },
            Some(("cycle-cost", selected.cost)),
        )?;
        let amount = cancel_cycle(&mut state, &selected.cycle, &mut metrics)?;
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "minimum-mean-cycle-canceling.cancel-minimum-mean-cycle",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "minimum-mean-cycle-canceling:augment-cycle-to-bottleneck",
            },
            SelectorTraceView {
                labels: vec![None; graph.nodes().len()],
                search_order,
                cycle: selected.cycle,
            },
            Some(("delta", i128::from(amount))),
        )?;
    }

    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    Ok(InternalRun {
        result: MinimumMeanCycleCancelingResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), MinimumMeanCycleCancelingError> {
    if graph.nodes().len() > MINIMUM_MEAN_CYCLE_CANCELING_MAX_NODES
        || graph.edges().len() > MINIMUM_MEAN_CYCLE_CANCELING_MAX_EDGES
    {
        return Err(MinimumMeanCycleCancelingError::AdmissionLimit);
    }
    Ok(())
}

#[derive(Clone)]
struct ResidualEdge {
    id: ResidualArcId,
    from: NodeIndex,
    to: NodeIndex,
    cost: i128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MinimumMeanScanStage {
    ResidualInventory,
    KarpDynamicProgram,
    TightPotential,
    TightArc,
}

impl MinimumMeanScanStage {
    const fn detail_label(self) -> &'static str {
        match self {
            Self::ResidualInventory => "residual-inventory scan ordinal",
            Self::KarpDynamicProgram => "karp-dp scan ordinal",
            Self::TightPotential => "tight-potential scan ordinal",
            Self::TightArc => "tight-arc scan ordinal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rational {
    numerator: i128,
    denominator: u64,
}

impl Rational {
    fn new(numerator: i128, denominator: usize) -> Result<Self, MinimumMeanCycleCancelingError> {
        let denominator = u64::try_from(denominator)
            .map_err(|_| MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        if denominator == 0 {
            return Err(MinimumMeanCycleCancelingError::SelectorInvariant);
        }
        let magnitude = u128::try_from(
            numerator
                .checked_abs()
                .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?,
        )
        .map_err(|_| MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        let divisor = gcd(magnitude, u128::from(denominator));
        let divisor_i128 = i128::try_from(divisor)
            .map_err(|_| MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        let divisor_u64 = u64::try_from(divisor)
            .map_err(|_| MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        Ok(Self {
            numerator: numerator / divisor_i128,
            denominator: denominator / divisor_u64,
        })
    }

    fn compare(self, other: Self) -> Result<Ordering, MinimumMeanCycleCancelingError> {
        let left = self
            .numerator
            .checked_mul(i128::from(other.denominator))
            .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        let right = other
            .numerator
            .checked_mul(i128::from(self.denominator))
            .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        Ok(left.cmp(&right))
    }
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

struct SelectedCycle {
    cycle: Vec<ResidualArcId>,
    cost: i128,
    mean: Rational,
    labels: Vec<Option<i128>>,
}

struct MinimumMeanCycleSelection {
    selected: Option<SelectedCycle>,
}

fn select_minimum_mean_cycle(
    state: &ResidualState<'_>,
    metrics: &mut MinimumMeanCycleCancelingMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<MinimumMeanCycleSelection, MinimumMeanCycleCancelingError> {
    metrics.mean_cycle_searches = metrics
        .mean_cycle_searches
        .checked_add(1)
        .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
    let edges = collect_residual_edges(state, metrics, recorder.as_deref_mut())?;
    let nodes = state.graph().node_indices().collect::<Vec<_>>();
    let components = cyclic_strong_components(&nodes, &edges);
    let mut best: Option<SelectedCycle> = None;
    for component in components {
        let candidate = minimum_mean_cycle_in_component(
            state,
            &edges,
            &component,
            metrics,
            recorder.as_deref_mut(),
        )?;
        let replace = match best.as_ref() {
            None => true,
            Some(current) => match candidate.mean.compare(current.mean)? {
                Ordering::Less => true,
                Ordering::Greater => false,
                Ordering::Equal => candidate.cycle < current.cycle,
            },
        };
        if replace {
            best = Some(candidate);
        }
    }
    Ok(MinimumMeanCycleSelection {
        selected: best.filter(|selected| selected.mean.numerator < 0),
    })
}

fn collect_residual_edges(
    state: &ResidualState<'_>,
    metrics: &mut MinimumMeanCycleCancelingMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Vec<ResidualEdge>, MinimumMeanCycleCancelingError> {
    let mut edges = Vec::new();
    for node in state.graph().node_indices() {
        for arc in state.outgoing_arcs(node) {
            track_scan(
                metrics,
                recorder.as_deref_mut(),
                &arc.id,
                MinimumMeanScanStage::ResidualInventory,
            )?;
            let edge = ResidualEdge {
                id: arc.id.clone(),
                from: arc.from,
                to: arc.to,
                cost: arc.cost,
            };
            edges.push(edge);
        }
    }
    edges.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(edges)
}

fn cyclic_strong_components(nodes: &[NodeIndex], edges: &[ResidualEdge]) -> Vec<Vec<NodeIndex>> {
    let node_count = nodes.len();
    let mut outgoing = vec![Vec::new(); node_count];
    let mut incoming = vec![Vec::new(); node_count];
    for edge in edges {
        outgoing[edge.from.as_usize()].push(edge.to.as_usize());
        incoming[edge.to.as_usize()].push(edge.from.as_usize());
    }
    for neighbors in outgoing.iter_mut().chain(incoming.iter_mut()) {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    let mut visited = vec![false; node_count];
    let mut finish_order = Vec::with_capacity(node_count);
    for start in 0..node_count {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0_usize)];
        while let Some((node, next)) = stack.last_mut() {
            if *next < outgoing[*node].len() {
                let neighbor = outgoing[*node][*next];
                *next += 1;
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push((neighbor, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }
    visited.fill(false);
    let mut components = Vec::new();
    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(nodes[node]);
            for &neighbor in incoming[node].iter().rev() {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        let cyclic = component.len() > 1
            || edges.iter().any(|edge| {
                edge.from == component[0] && edge.to == component[0] && component.len() == 1
            });
        if cyclic {
            components.push(component);
        }
    }
    components.sort_by_key(|component| component[0]);
    components
}

fn minimum_mean_cycle_in_component(
    state: &ResidualState<'_>,
    edges: &[ResidualEdge],
    component: &[NodeIndex],
    metrics: &mut MinimumMeanCycleCancelingMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<SelectedCycle, MinimumMeanCycleCancelingError> {
    let node_count = component.len();
    let mut local_index = vec![None; state.graph().nodes().len()];
    for (local, &node) in component.iter().enumerate() {
        local_index[node.as_usize()] = Some(local);
    }
    let component_edges = edges
        .iter()
        .filter(|edge| {
            local_index[edge.from.as_usize()].is_some() && local_index[edge.to.as_usize()].is_some()
        })
        .collect::<Vec<_>>();
    let mut incoming = vec![Vec::new(); node_count];
    for edge in &component_edges {
        let to = local_index[edge.to.as_usize()]
            .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
        incoming[to].push(*edge);
    }
    let mut distance = vec![vec![None; node_count]; node_count + 1];
    distance[0][0] = Some(0_i128);
    for length in 1..=node_count {
        metrics.dynamic_programming_rounds = metrics
            .dynamic_programming_rounds
            .checked_add(1)
            .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        for (to, incoming_edges) in incoming.iter().enumerate() {
            for edge in incoming_edges {
                track_scan(
                    metrics,
                    recorder.as_deref_mut(),
                    &edge.id,
                    MinimumMeanScanStage::KarpDynamicProgram,
                )?;
                let from = local_index[edge.from.as_usize()]
                    .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
                let Some(prefix) = distance[length - 1][from] else {
                    continue;
                };
                let candidate = prefix
                    .checked_add(edge.cost)
                    .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
                if distance[length][to].is_none_or(|current| candidate < current) {
                    distance[length][to] = Some(candidate);
                }
            }
        }
    }
    let mean = karp_minimum_mean(&distance)?;
    let cycle = reconstruct_tight_cycle(
        state,
        &component_edges,
        component,
        &local_index,
        mean,
        metrics,
        recorder,
    )?;
    let cost = cycle_cost(state, &cycle)?;
    let cycle_mean = Rational::new(cost, cycle.len())?;
    if cycle_mean != mean {
        return Err(MinimumMeanCycleCancelingError::SelectorInvariant);
    }
    let mut labels = vec![None; state.graph().nodes().len()];
    for (local, &node) in component.iter().enumerate() {
        labels[node.as_usize()] = distance[node_count][local];
    }
    Ok(SelectedCycle {
        cycle,
        cost,
        mean,
        labels,
    })
}

fn karp_minimum_mean(
    distance: &[Vec<Option<i128>>],
) -> Result<Rational, MinimumMeanCycleCancelingError> {
    let node_count = distance
        .len()
        .checked_sub(1)
        .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
    let mut minimum = None;
    for node in 0..node_count {
        let Some(final_distance) = distance[node_count][node] else {
            continue;
        };
        let mut maximum = None;
        for (length, row) in distance.iter().enumerate().take(node_count) {
            let Some(prefix) = row[node] else {
                continue;
            };
            let ratio = Rational::new(
                final_distance
                    .checked_sub(prefix)
                    .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?,
                node_count - length,
            )?;
            let replace_maximum = match maximum {
                None => true,
                Some(current) => ratio.compare(current)? == Ordering::Greater,
            };
            if replace_maximum {
                maximum = Some(ratio);
            }
        }
        let candidate = maximum.ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
        let replace_minimum = match minimum {
            None => true,
            Some(current) => candidate.compare(current)? == Ordering::Less,
        };
        if replace_minimum {
            minimum = Some(candidate);
        }
    }
    minimum.ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)
}

fn reconstruct_tight_cycle(
    state: &ResidualState<'_>,
    edges: &[&ResidualEdge],
    component: &[NodeIndex],
    local_index: &[Option<usize>],
    mean: Rational,
    metrics: &mut MinimumMeanCycleCancelingMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<Vec<ResidualArcId>, MinimumMeanCycleCancelingError> {
    let node_count = component.len();
    let mut potential = vec![0_i128; node_count];
    for _ in 0..node_count {
        let mut updated = false;
        for edge in edges {
            track_scan(
                metrics,
                recorder.as_deref_mut(),
                &edge.id,
                MinimumMeanScanStage::TightPotential,
            )?;
            let from = local_index[edge.from.as_usize()]
                .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
            let to = local_index[edge.to.as_usize()]
                .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
            let candidate = potential[from]
                .checked_add(transformed_cost(edge.cost, mean)?)
                .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
            if candidate < potential[to] {
                potential[to] = candidate;
                updated = true;
            }
        }
        if !updated {
            break;
        }
    }
    let mut tight = vec![Vec::new(); node_count];
    for edge in edges {
        track_scan(
            metrics,
            recorder.as_deref_mut(),
            &edge.id,
            MinimumMeanScanStage::TightArc,
        )?;
        let from = local_index[edge.from.as_usize()]
            .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
        let to = local_index[edge.to.as_usize()]
            .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
        let reduced = transformed_cost(edge.cost, mean)?
            .checked_add(potential[from])
            .and_then(|value| value.checked_sub(potential[to]))
            .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        if reduced < 0 {
            return Err(MinimumMeanCycleCancelingError::SelectorInvariant);
        }
        if reduced == 0 {
            tight[from].push(TightEdge {
                id: edge.id.clone(),
                to,
            });
        }
    }
    for outgoing in &mut tight {
        outgoing.sort_by(|left, right| left.id.cmp(&right.id));
    }
    let mut colors = vec![0_u8; node_count];
    let mut parent_node = vec![None; node_count];
    let mut parent_edge = vec![None; node_count];
    for start in 0..node_count {
        if colors[start] != 0 {
            continue;
        }
        if let Some(cycle) = find_tight_cycle(
            start,
            &tight,
            &mut colors,
            &mut parent_node,
            &mut parent_edge,
        ) {
            let cost = cycle_cost(state, &cycle)?;
            if Rational::new(cost, cycle.len())? != mean {
                return Err(MinimumMeanCycleCancelingError::SelectorInvariant);
            }
            return Ok(cycle);
        }
    }
    Err(MinimumMeanCycleCancelingError::SelectorInvariant)
}

fn transformed_cost(cost: i128, mean: Rational) -> Result<i128, MinimumMeanCycleCancelingError> {
    cost.checked_mul(i128::from(mean.denominator))
        .and_then(|value| value.checked_sub(mean.numerator))
        .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)
}

#[derive(Clone)]
struct TightEdge {
    id: ResidualArcId,
    to: usize,
}

fn find_tight_cycle(
    node: usize,
    tight: &[Vec<TightEdge>],
    colors: &mut [u8],
    parent_node: &mut [Option<usize>],
    parent_edge: &mut [Option<ResidualArcId>],
) -> Option<Vec<ResidualArcId>> {
    colors[node] = 1;
    for edge in &tight[node] {
        if colors[edge.to] == 0 {
            parent_node[edge.to] = Some(node);
            parent_edge[edge.to] = Some(edge.id.clone());
            if let Some(cycle) = find_tight_cycle(edge.to, tight, colors, parent_node, parent_edge)
            {
                return Some(cycle);
            }
        } else if colors[edge.to] == 1 {
            let mut reversed_path = Vec::new();
            let mut cursor = node;
            while cursor != edge.to {
                reversed_path.push(parent_edge[cursor].clone()?);
                cursor = parent_node[cursor]?;
            }
            reversed_path.reverse();
            reversed_path.push(edge.id.clone());
            canonicalize_cycle(&mut reversed_path);
            return Some(reversed_path);
        }
    }
    colors[node] = 2;
    None
}

fn canonicalize_cycle(cycle: &mut [ResidualArcId]) {
    let offset = cycle
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
        .map_or(0, |(index, _)| index);
    cycle.rotate_left(offset);
}

fn cancel_cycle(
    state: &mut ResidualState<'_>,
    cycle: &[ResidualArcId],
    metrics: &mut MinimumMeanCycleCancelingMetrics,
) -> Result<u64, MinimumMeanCycleCancelingError> {
    let amount = cycle
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
    if amount == 0 || cycle_cost(state, cycle)? >= 0 {
        return Err(MinimumMeanCycleCancelingError::SelectorInvariant);
    }
    state.augment(cycle, amount)?;
    metrics.canceled_cycles = metrics
        .canceled_cycles
        .checked_add(1)
        .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
    Ok(amount)
}

fn cycle_cost(
    state: &ResidualState<'_>,
    cycle: &[ResidualArcId],
) -> Result<i128, MinimumMeanCycleCancelingError> {
    let mut cost = 0_i128;
    for id in cycle {
        let arc = state
            .arc(id)
            .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)?;
        cost = cost
            .checked_add(arc.cost)
            .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
    }
    Ok(cost)
}

fn cycle_nodes(
    state: &ResidualState<'_>,
    cycle: &[ResidualArcId],
) -> Result<Vec<NodeIndex>, MinimumMeanCycleCancelingError> {
    cycle
        .iter()
        .map(|id| {
            state
                .arc(id)
                .map(|arc| arc.to)
                .ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)
        })
        .collect()
}

fn track_scan(
    metrics: &mut MinimumMeanCycleCancelingMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    arc: &ResidualArcId,
    stage: MinimumMeanScanStage,
) -> Result<(), MinimumMeanCycleCancelingError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > MINIMUM_MEAN_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS {
        return Err(MinimumMeanCycleCancelingError::WorkLimit);
    }
    if let Some(recorder) = recorder {
        let scan_ordinal = i128::try_from(metrics.residual_arc_scans)
            .map_err(|_| MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
        recorder.record_metric_observation_with_detail(
            FlowTraceEventMetadata {
                catalog_id: "minimum-mean-cycle-canceling.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "minimum-mean-cycle-canceling:inspect-residual-arc",
            },
            FlowTraceMetricId::ResidualArcScans,
            FlowTraceEntityRef::ResidualArc(arc.clone()),
            Some((stage.detail_label(), scan_ordinal)),
        )?;
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: MinimumMeanCycleCancelingMetrics,
    record_events: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_events {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct SelectorTraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    cycle: Vec<ResidualArcId>,
}

impl SelectorTraceView {
    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            cycle: Vec::new(),
        }
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: MinimumMeanCycleCancelingMetrics,
    metadata: FlowTraceEventMetadata,
    view: SelectorTraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.labels,
        view.search_order,
        view.cycle,
        Vec::new(),
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail(metadata, &snapshot, detail)
}

const fn trace_metrics(metrics: MinimumMeanCycleCancelingMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: metrics.dynamic_programming_rounds as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.canceled_cycles as u128,
        path_searches: metrics.mean_cycle_searches as u128,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::certificate::{fixed_flow_divergences, supply_divergences};
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;
    use crate::algorithms::solve_simple_cycle_canceling;

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
        .expect("valid graph")
    }

    #[test]
    fn selects_mean_cost_instead_of_total_cycle_cost() {
        let graph = network(
            &[("a", 0), ("b", 0), ("x", 0)],
            &[
                ("ab", "a", "b", 0, 1, -3),
                ("ba", "b", "a", 0, 1, -2),
                ("loop", "x", "x", 0, 1, -3),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let traced = trace_minimum_mean_cycle_canceling(&graph, &target).expect("minimum cost");
        let selected = traced
            .events
            .iter()
            .find(|event| {
                event.catalog_id == "minimum-mean-cycle-canceling.select-minimum-mean-cycle"
            })
            .expect("selection");

        assert_eq!(
            selected
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("cycle-cost", -3))
        );
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.catalog_id == "minimum-mean-cycle-canceling.select-minimum-mean-cycle" {
                assert_eq!(replay.active_path.len(), 1);
                break;
            }
        }
        assert_eq!(traced.result.certificate.total_cost, -8);
        assert_eq!(traced.result.metrics.canceled_cycles, 2);
    }

    #[test]
    fn matches_simple_cycle_canceling_with_bounds_supplies_and_fixed_flow() {
        let graph = network(
            &[("s", 2), ("a", 0), ("t", -2), ("x", 0)],
            &[
                ("sa", "s", "a", 1, 4, 2),
                ("at", "a", "t", 0, 4, -1),
                ("st", "s", "t", 0, 2, 5),
                ("loop", "x", "x", 0, 3, -2),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let mut target = supply_divergences(&graph).expect("supply target");
        let fixed = fixed_flow_divergences(&graph, source, sink, 1).expect("fixed target");
        for (entry, addition) in target.iter_mut().zip(fixed) {
            *entry += addition;
        }

        let minimum_mean =
            solve_minimum_mean_cycle_canceling(&graph, &target).expect("minimum mean");
        let simple = solve_simple_cycle_canceling(&graph, &target).expect("simple");

        assert_eq!(
            minimum_mean.certificate.total_cost,
            simple.certificate.total_cost
        );
        check_min_cost_flow(&graph, &target, &minimum_mean.flows).expect("certificate");
    }

    #[test]
    fn trace_replays_in_both_directions_and_matches_fast_result() {
        let graph = network(
            &[("x", 0), ("y", 0)],
            &[("xy", "x", "y", 0, 2, -3), ("yx", "y", "x", 0, 2, 1)],
        );
        let target = supply_divergences(&graph).expect("target");
        let fast = solve_minimum_mean_cycle_canceling(&graph, &target).expect("fast");
        let traced = trace_minimum_mean_cycle_canceling(&graph, &target).expect("trace");

        assert_eq!(traced.result, fast);
        let scans = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "minimum-mean-cycle-canceling.inspect-residual-arc")
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scans.len()).expect("scan event count"),
            fast.metrics.residual_arc_scans
        );
        assert!(scans.iter().all(|event| {
            event.entity_refs.len() == 1
                && matches!(event.entity_refs[0], FlowTraceEntityRef::ResidualArc(_))
                && event.patches.iter().any(|patch| {
                    matches!(
                        patch,
                        crate::trace::FlowTracePatch::Metric {
                            metric: FlowTraceMetricId::ResidualArcScans,
                            before,
                            after,
                        } if *after == before.saturating_add(1)
                    )
                })
        }));
        let stage_labels = [
            "residual-inventory scan ordinal",
            "karp-dp scan ordinal",
            "tight-potential scan ordinal",
            "tight-arc scan ordinal",
        ];
        for (index, event) in scans.iter().enumerate() {
            let detail = event.detail.as_ref().expect("scan owns source ordinal");
            assert!(stage_labels.contains(&detail.label.as_str()));
            assert_eq!(
                detail.value,
                i128::try_from(index + 1).expect("bounded scan ordinal")
            );
        }
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn rejects_infeasible_balances_before_minimum_mean_search() {
        let graph = network(&[("s", 1), ("t", -1)], &[]);
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_minimum_mean_cycle_canceling(&graph, &target),
            Err(MinimumMeanCycleCancelingError::Feasibility(
                FeasibilityError::Infeasible(_)
            ))
        ));
    }

    #[test]
    fn karp_selector_matches_exhaustive_simple_cycles_on_dense_small_graphs() {
        for case in 0_i64..32 {
            let node_count = usize::try_from(2 + case % 4).expect("small node count");
            let nodes = (0..node_count)
                .map(|node| FlowNode::new(NodeId::parse(&format!("v{node}")).expect("node id"), 0))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    let cost = (case * 13
                        + i64::try_from(from).expect("from") * 7
                        + i64::try_from(to).expect("to") * 11)
                        % 17
                        - 8;
                    edges.push(UnresolvedFlowEdge {
                        id: EdgeId::parse(&format!("e{from}-{to}")).expect("edge id"),
                        from: NodeId::parse(&format!("v{from}")).expect("tail"),
                        to: NodeId::parse(&format!("v{to}")).expect("head"),
                        lower: 0,
                        capacity: 1,
                        cost,
                    });
                }
            }
            let graph = FlowNetwork::new(nodes, edges).expect("dense graph");
            let state = ResidualState::from_flows(&graph, &vec![0; graph.edges().len()])
                .expect("zero residual state");
            let expected = brute_minimum_cycle_mean(&state).expect("brute mean");
            let selected = select_minimum_mean_cycle(
                &state,
                &mut MinimumMeanCycleCancelingMetrics::default(),
                None,
            )
            .expect("Karp selection")
            .selected;

            if expected.numerator < 0 {
                assert_eq!(
                    selected.expect("negative cycle").mean,
                    expected,
                    "case {case}"
                );
            } else {
                assert!(selected.is_none(), "case {case}");
            }
        }
    }

    fn brute_minimum_cycle_mean(
        state: &ResidualState<'_>,
    ) -> Result<Rational, MinimumMeanCycleCancelingError> {
        let node_count = state.graph().nodes().len();
        let mut best = None;
        for start in state.graph().node_indices() {
            let mut visited = vec![false; node_count];
            visited[start.as_usize()] = true;
            brute_cycles_from(state, start, start, &mut visited, 0, 0, &mut best)?;
        }
        best.ok_or(MinimumMeanCycleCancelingError::SelectorInvariant)
    }

    fn brute_cycles_from(
        state: &ResidualState<'_>,
        start: NodeIndex,
        node: NodeIndex,
        visited: &mut [bool],
        path_cost: i128,
        path_length: usize,
        best: &mut Option<Rational>,
    ) -> Result<(), MinimumMeanCycleCancelingError> {
        for arc in state.outgoing_arcs(node) {
            let next_cost = path_cost
                .checked_add(arc.cost)
                .ok_or(MinimumMeanCycleCancelingError::ArithmeticOverflow)?;
            if arc.to == start {
                let candidate = Rational::new(next_cost, path_length + 1)?;
                let replace = match *best {
                    None => true,
                    Some(current) => candidate.compare(current)? == Ordering::Less,
                };
                if replace {
                    *best = Some(candidate);
                }
            } else if !visited[arc.to.as_usize()] {
                visited[arc.to.as_usize()] = true;
                brute_cycles_from(
                    state,
                    start,
                    arc.to,
                    visited,
                    next_cost,
                    path_length + 1,
                    best,
                )?;
                visited[arc.to.as_usize()] = false;
            }
        }
        Ok(())
    }
}
