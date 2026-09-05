//! Exact costed-flow rounding by fractional-cycle cancellation.
//!
//! This implements the reduction in Sections 3 and 4 of Kang and Payor,
//! "Flow Rounding" (arXiv:1507.08139). The paper stores the fractional-edge
//! forest in dynamic trees for `O(m log n)` time. This executable uses an
//! explicit forest and deterministic breadth-first path queries: it preserves
//! the source algorithm's link/cycle/cancel transitions and exact invariants,
//! while intentionally accepting a slower `O(m(n + m))` small-graph runtime.

use std::collections::VecDeque;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};
use thiserror::Error;

use crate::model::{EdgeId, FlowNetwork};

/// Maximum graph size admitted by the explicit-forest realization.
pub const COSTED_FLOW_ROUNDING_MAX_NODES: usize = 2_000;
/// Maximum edge count admitted by the explicit-forest realization.
pub const COSTED_FLOW_ROUNDING_MAX_EDGES: usize = 8_000;
/// Maximum numerator or denominator size admitted for one rational coordinate.
pub const COSTED_FLOW_ROUNDING_MAX_RATIONAL_BITS: u64 = 4_096;
/// Maximum number of public transitions.
pub const COSTED_FLOW_ROUNDING_MAX_TRACE_EVENTS: usize = COSTED_FLOW_ROUNDING_MAX_EDGES + 1;

const CATALOG_ID: &str = "costed-flow-rounding";

/// Exact work counters for the explicit fractional-edge forest.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CostedFlowRoundingMetrics {
    /// Original edges whose integrality was inspected.
    pub processed_edges: u64,
    /// Fractional edges linked without forming a cycle.
    pub linked_edges: u64,
    /// Fractional cycles canceled.
    pub canceled_cycles: u64,
    /// Fractional coordinates made integral by cancellation.
    pub integralized_edges: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedFlowRoundingSnapshot {
    /// Exact current flow in canonical original-edge order.
    pub flows: Vec<BigRational>,
    /// Membership in the processed fractional-edge forest.
    pub fractional_forest: Vec<bool>,
    /// Number of original edges already processed.
    pub processed_edges: usize,
    /// Exact current original objective.
    pub total_cost: BigRational,
    /// Whether the integrality proof has completed.
    pub complete: bool,
    /// Exact counters.
    pub metrics: CostedFlowRoundingMetrics,
}

/// One directed original edge on a canceled fractional cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedFlowRoundingCycleArc {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// `1` follows the original orientation and `-1` opposes it.
    pub direction: i8,
}

/// Source-level meaning of one publication boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CostedFlowRoundingEventKind {
    /// An already-integral coordinate required no forest mutation.
    IntegralEdgeSkipped {
        /// Stable identity of the inspected integral edge.
        edge: EdgeId,
    },
    /// A fractional edge joined two distinct fractional-forest components.
    FractionalEdgeLinked {
        /// Stable identity of the linked fractional edge.
        edge: EdgeId,
    },
    /// Adding a fractional edge closed and canceled one fractional cycle.
    FractionalCycleCanceled {
        /// The original edge whose insertion closed the cycle.
        inserted_edge: EdgeId,
        /// Directed cycle after choosing the non-increasing-cost orientation.
        cycle: Vec<CostedFlowRoundingCycleArc>,
        /// Exact amount circulated before at least one coordinate became integral.
        delta: BigRational,
        /// Integral per-unit cost of the chosen directed cycle.
        signed_cycle_cost: i128,
        /// Coordinates removed from the fractional forest after the cancellation.
        integralized_edges: Vec<EdgeId>,
    },
    /// No fractional coordinate remains; Lemma 2 yields an integral flow.
    Completed,
}

/// One fully reversible flow-rounding transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedFlowRoundingTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level transition meaning.
    pub kind: CostedFlowRoundingEventKind,
    /// State before the atomic transition.
    pub before: CostedFlowRoundingSnapshot,
    /// State after the atomic transition.
    pub after: CostedFlowRoundingSnapshot,
}

/// Exact integral result with a no-worse-cost witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedFlowRoundingResult {
    /// Integral flow in canonical original-edge order.
    pub flows: Vec<u64>,
    /// Exact integral objective.
    pub total_cost: i128,
    /// Exact objective of the supplied fractional flow.
    pub initial_cost: BigRational,
    /// Final replay state.
    pub final_snapshot: CostedFlowRoundingSnapshot,
}

/// Complete reversible trace result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedFlowRoundingTraceResult {
    /// Validated input state before the first edge is processed.
    pub base_snapshot: CostedFlowRoundingSnapshot,
    /// Atomic link, cancel, and completion transitions.
    pub events: Vec<CostedFlowRoundingTraceEvent>,
    /// Rounded result.
    pub result: CostedFlowRoundingResult,
}

/// Costed-flow-rounding rejection reason.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CostedFlowRoundingError {
    /// The explicit-forest realization's published band was exceeded.
    #[error("costed flow rounding input exceeds the explicit-forest admission band")]
    AdmissionLimit,
    /// The flow or divergence vector has the wrong shape.
    #[error("costed flow rounding input shape is invalid")]
    InvalidShape,
    /// A fractional coordinate lies outside its integral lower/upper bounds.
    #[error("fractional flow violates bounds on edge {0}")]
    EdgeBounds(String),
    /// Exact outgoing-minus-incoming divergence differs from the requested vector.
    #[error("fractional flow does not have the requested integral divergence")]
    InvalidDivergence,
    /// Checked integral accounting overflowed.
    #[error("costed flow rounding arithmetic overflow")]
    ArithmeticOverflow,
    /// The fractional-forest invariant or integrality conclusion failed.
    #[error("costed flow rounding invariant failed")]
    InvariantViolation,
    /// A supplied trace violates source transition semantics.
    #[error("costed flow rounding trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: CostedFlowRoundingSnapshot,
    events: Vec<CostedFlowRoundingTraceEvent>,
    result: CostedFlowRoundingResult,
}

/// Rounds an exact fractional flow without increasing its original cost.
///
/// # Errors
///
/// Rejects out-of-band, malformed, infeasible, non-integral-divergence, or
/// internally inconsistent input.
pub fn round_costed_flow(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    fractional_flows: &[BigRational],
) -> Result<CostedFlowRoundingResult, CostedFlowRoundingError> {
    run_internal(graph, required_divergence, fractional_flows, false).map(|run| run.result)
}

/// Records every deterministic fractional-forest transition.
///
/// # Errors
///
/// Returns any solve or independent trace-check failure.
pub fn trace_costed_flow_rounding(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    fractional_flows: &[BigRational],
) -> Result<CostedFlowRoundingTraceResult, CostedFlowRoundingError> {
    let run = run_internal(graph, required_divergence, fractional_flows, true)?;
    let trace = CostedFlowRoundingTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_costed_flow_rounding_trace(graph, required_divergence, fractional_flows, &trace)?;
    Ok(trace)
}

/// Checks source transition semantics without invoking the production runner.
///
/// # Errors
///
/// Rejects malformed snapshots, cycles, directions, availability steps,
/// metrics, replay links, or final results.
pub fn check_costed_flow_rounding_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    fractional_flows: &[BigRational],
    trace: &CostedFlowRoundingTraceResult,
) -> Result<(), CostedFlowRoundingError> {
    validate_input(graph, required_divergence, fractional_flows)?;
    let expected_base = initial_snapshot(graph, fractional_flows);
    if trace.base_snapshot != expected_base {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    validate_snapshot(graph, required_divergence, &trace.base_snapshot)?;
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if event.catalog_id != CATALOG_ID || &event.before != cursor {
            return Err(CostedFlowRoundingError::TraceVerification);
        }
        validate_transition(graph, event)?;
        validate_snapshot(graph, required_divergence, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.result.final_snapshot || !cursor.complete {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    let rounded = exact_integral_flows(&cursor.flows)?;
    let rounded_cost = integral_cost(graph, &rounded)?;
    if rounded != trace.result.flows
        || rounded_cost != trace.result.total_cost
        || trace.result.initial_cost != expected_base.total_cost
        || cursor.total_cost > expected_base.total_cost
    {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    graph: &FlowNetwork,
    required: &[i128],
    fractional: &[BigRational],
    record: bool,
) -> Result<InternalRun, CostedFlowRoundingError> {
    validate_input(graph, required, fractional)?;
    let mut snapshot = initial_snapshot(graph, fractional);
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    for inserted in 0..graph.edges().len() {
        let before = snapshot.clone();
        let edge = &graph.edges()[inserted];
        let kind = if snapshot.flows[inserted].is_integer() {
            snapshot.processed_edges += 1;
            snapshot.metrics.processed_edges = checked_increment(snapshot.metrics.processed_edges)?;
            CostedFlowRoundingEventKind::IntegralEdgeSkipped {
                edge: edge.id().clone(),
            }
        } else if !forest_connected(
            graph,
            &snapshot.fractional_forest,
            edge.from().as_usize(),
            edge.to().as_usize(),
        ) {
            snapshot.fractional_forest[inserted] = true;
            snapshot.processed_edges += 1;
            snapshot.metrics.processed_edges = checked_increment(snapshot.metrics.processed_edges)?;
            snapshot.metrics.linked_edges = checked_increment(snapshot.metrics.linked_edges)?;
            CostedFlowRoundingEventKind::FractionalEdgeLinked {
                edge: edge.id().clone(),
            }
        } else {
            cancel_fractional_cycle(graph, inserted, &mut snapshot)?
        };
        snapshot.total_cost = exact_cost(graph, &snapshot.flows);
        snapshot.metrics.state_transitions = checked_increment(snapshot.metrics.state_transitions)?;
        if record {
            push_event(&mut events, kind, before, snapshot.clone())?;
        }
    }
    if snapshot.flows.iter().any(|flow| !flow.is_integer()) {
        return Err(CostedFlowRoundingError::InvariantViolation);
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = checked_increment(snapshot.metrics.state_transitions)?;
    if record {
        push_event(
            &mut events,
            CostedFlowRoundingEventKind::Completed,
            before,
            snapshot.clone(),
        )?;
    }
    let flows = exact_integral_flows(&snapshot.flows)?;
    let total_cost = integral_cost(graph, &flows)?;
    Ok(InternalRun {
        base_snapshot: base_snapshot.clone(),
        events,
        result: CostedFlowRoundingResult {
            flows,
            total_cost,
            initial_cost: base_snapshot.total_cost,
            final_snapshot: snapshot,
        },
    })
}

fn cancel_fractional_cycle(
    graph: &FlowNetwork,
    inserted: usize,
    snapshot: &mut CostedFlowRoundingSnapshot,
) -> Result<CostedFlowRoundingEventKind, CostedFlowRoundingError> {
    let edge = &graph.edges()[inserted];
    let path = forest_path(
        graph,
        &snapshot.fractional_forest,
        edge.to().as_usize(),
        edge.from().as_usize(),
    )
    .ok_or(CostedFlowRoundingError::InvariantViolation)?;
    let mut signed_edges = Vec::with_capacity(path.len() + 1);
    signed_edges.push((inserted, 1_i8));
    signed_edges.extend(path);
    let forward_cost = signed_cycle_cost(graph, &signed_edges)?;
    if forward_cost > 0 {
        for (_, direction) in &mut signed_edges {
            *direction = -*direction;
        }
    }
    let chosen_cost = signed_cycle_cost(graph, &signed_edges)?;
    let delta = signed_edges
        .iter()
        .map(|&(index, direction)| availability(&snapshot.flows[index], direction))
        .min()
        .ok_or(CostedFlowRoundingError::InvariantViolation)?;
    if delta <= BigRational::zero() || chosen_cost > 0 {
        return Err(CostedFlowRoundingError::InvariantViolation);
    }
    snapshot.fractional_forest[inserted] = true;
    for &(index, direction) in &signed_edges {
        snapshot.flows[index] += &delta * BigInt::from(direction);
    }
    let mut integralized = Vec::new();
    for (index, in_forest) in snapshot.fractional_forest.iter_mut().enumerate() {
        if *in_forest && snapshot.flows[index].is_integer() {
            *in_forest = false;
            integralized.push(graph.edges()[index].id().clone());
        }
    }
    if integralized.is_empty() {
        return Err(CostedFlowRoundingError::InvariantViolation);
    }
    snapshot.processed_edges += 1;
    snapshot.metrics.processed_edges = checked_increment(snapshot.metrics.processed_edges)?;
    snapshot.metrics.canceled_cycles = checked_increment(snapshot.metrics.canceled_cycles)?;
    snapshot.metrics.integralized_edges = snapshot
        .metrics
        .integralized_edges
        .checked_add(
            u64::try_from(integralized.len())
                .map_err(|_| CostedFlowRoundingError::ArithmeticOverflow)?,
        )
        .ok_or(CostedFlowRoundingError::ArithmeticOverflow)?;
    Ok(CostedFlowRoundingEventKind::FractionalCycleCanceled {
        inserted_edge: edge.id().clone(),
        cycle: signed_edges
            .into_iter()
            .map(|(index, direction)| CostedFlowRoundingCycleArc {
                edge: graph.edges()[index].id().clone(),
                direction,
            })
            .collect(),
        delta,
        signed_cycle_cost: chosen_cost,
        integralized_edges: integralized,
    })
}

fn validate_input(
    graph: &FlowNetwork,
    required: &[i128],
    flows: &[BigRational],
) -> Result<(), CostedFlowRoundingError> {
    if graph.nodes().len() > COSTED_FLOW_ROUNDING_MAX_NODES
        || graph.edges().len() > COSTED_FLOW_ROUNDING_MAX_EDGES
        || flows.iter().any(|flow| {
            flow.numer().bits() > COSTED_FLOW_ROUNDING_MAX_RATIONAL_BITS
                || flow.denom().bits() > COSTED_FLOW_ROUNDING_MAX_RATIONAL_BITS
        })
    {
        return Err(CostedFlowRoundingError::AdmissionLimit);
    }
    if flows.len() != graph.edges().len()
        || required.len() != graph.nodes().len()
        || required
            .iter()
            .try_fold(0_i128, |sum, &value| sum.checked_add(value))
            != Some(0)
    {
        return Err(CostedFlowRoundingError::InvalidShape);
    }
    for (edge, flow) in graph.edges().iter().zip(flows) {
        if flow < &integer_rational(edge.lower()) || flow > &integer_rational(edge.capacity()) {
            return Err(CostedFlowRoundingError::EdgeBounds(
                edge.id().as_str().to_owned(),
            ));
        }
    }
    if exact_divergences(graph, flows)
        != required
            .iter()
            .copied()
            .map(integer_i128)
            .collect::<Vec<_>>()
    {
        return Err(CostedFlowRoundingError::InvalidDivergence);
    }
    Ok(())
}

fn initial_snapshot(graph: &FlowNetwork, fractional: &[BigRational]) -> CostedFlowRoundingSnapshot {
    CostedFlowRoundingSnapshot {
        flows: fractional.to_vec(),
        fractional_forest: vec![false; graph.edges().len()],
        processed_edges: 0,
        total_cost: exact_cost(graph, fractional),
        complete: false,
        metrics: CostedFlowRoundingMetrics::default(),
    }
}

fn validate_snapshot(
    graph: &FlowNetwork,
    required: &[i128],
    snapshot: &CostedFlowRoundingSnapshot,
) -> Result<(), CostedFlowRoundingError> {
    if snapshot.flows.len() != graph.edges().len()
        || snapshot.fractional_forest.len() != graph.edges().len()
        || snapshot.processed_edges > graph.edges().len()
        || snapshot.metrics.processed_edges
            != u64::try_from(snapshot.processed_edges)
                .map_err(|_| CostedFlowRoundingError::TraceVerification)?
        || snapshot.total_cost != exact_cost(graph, &snapshot.flows)
        || exact_divergences(graph, &snapshot.flows)
            != required
                .iter()
                .copied()
                .map(integer_i128)
                .collect::<Vec<_>>()
    {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    let mut dsu = DisjointSet::new(graph.nodes().len());
    for (index, (&in_forest, flow)) in snapshot
        .fractional_forest
        .iter()
        .zip(&snapshot.flows)
        .enumerate()
    {
        if in_forest {
            let edge = &graph.edges()[index];
            if index >= snapshot.processed_edges
                || flow.is_integer()
                || !dsu.union(edge.from().as_usize(), edge.to().as_usize())
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
        } else if index < snapshot.processed_edges && !flow.is_integer() {
            return Err(CostedFlowRoundingError::TraceVerification);
        }
    }
    if snapshot.complete
        && (snapshot.processed_edges != graph.edges().len()
            || snapshot.flows.iter().any(|flow| !flow.is_integer()))
    {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_transition(
    graph: &FlowNetwork,
    event: &CostedFlowRoundingTraceEvent,
) -> Result<(), CostedFlowRoundingError> {
    let before = &event.before;
    let after = &event.after;
    if before.complete
        || after.metrics.state_transitions != before.metrics.state_transitions + 1
        || after.total_cost > before.total_cost
    {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    match &event.kind {
        CostedFlowRoundingEventKind::Completed => {
            if before.processed_edges != graph.edges().len()
                || after.processed_edges != before.processed_edges
                || after.flows != before.flows
                || after.fractional_forest != before.fractional_forest
                || !after.complete
                || after.metrics.processed_edges != before.metrics.processed_edges
                || after.metrics.linked_edges != before.metrics.linked_edges
                || after.metrics.canceled_cycles != before.metrics.canceled_cycles
                || after.metrics.integralized_edges != before.metrics.integralized_edges
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
        }
        CostedFlowRoundingEventKind::IntegralEdgeSkipped { edge } => {
            let index = next_edge_index(graph, before, edge)?;
            if !before.flows[index].is_integer()
                || after.processed_edges != before.processed_edges + 1
                || after.flows != before.flows
                || after.fractional_forest != before.fractional_forest
                || after.complete
                || !metrics_changed(before, after, 0, 0, 0)
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
        }
        CostedFlowRoundingEventKind::FractionalEdgeLinked { edge } => {
            let index = next_edge_index(graph, before, edge)?;
            let original = &graph.edges()[index];
            let mut expected_forest = before.fractional_forest.clone();
            expected_forest[index] = true;
            if before.flows[index].is_integer()
                || forest_connected(
                    graph,
                    &before.fractional_forest,
                    original.from().as_usize(),
                    original.to().as_usize(),
                )
                || after.processed_edges != before.processed_edges + 1
                || after.flows != before.flows
                || after.fractional_forest != expected_forest
                || after.complete
                || !metrics_changed(before, after, 1, 0, 0)
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
        }
        CostedFlowRoundingEventKind::FractionalCycleCanceled {
            inserted_edge,
            cycle,
            delta,
            signed_cycle_cost: declared_cost,
            integralized_edges,
        } => {
            let inserted = next_edge_index(graph, before, inserted_edge)?;
            if before.flows[inserted].is_integer()
                || cycle.is_empty()
                || delta <= &BigRational::zero()
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
            let mut signed = vec![0_i8; graph.edges().len()];
            for arc in cycle {
                let index = graph
                    .edge_index(&arc.edge)
                    .ok_or(CostedFlowRoundingError::TraceVerification)?
                    .as_usize();
                if !matches!(arc.direction, -1 | 1)
                    || signed[index] != 0
                    || (index != inserted && !before.fractional_forest[index])
                {
                    return Err(CostedFlowRoundingError::TraceVerification);
                }
                signed[index] = arc.direction;
            }
            if signed[inserted] == 0
                || !is_directed_circulation(graph, &signed)
                || *declared_cost
                    != signed_cycle_cost(
                        graph,
                        &signed
                            .iter()
                            .enumerate()
                            .filter(|&(_, direction)| *direction != 0)
                            .map(|(index, &direction)| (index, direction))
                            .collect::<Vec<_>>(),
                    )?
                || *declared_cost > 0
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
            let minimum = signed
                .iter()
                .enumerate()
                .filter(|&(_, direction)| *direction != 0)
                .map(|(index, &direction)| availability(&before.flows[index], direction))
                .min()
                .ok_or(CostedFlowRoundingError::TraceVerification)?;
            if &minimum != delta {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
            let mut expected_flows = before.flows.clone();
            let mut expected_forest = before.fractional_forest.clone();
            expected_forest[inserted] = true;
            for (index, &direction) in signed.iter().enumerate() {
                if direction != 0 {
                    expected_flows[index] += delta * BigInt::from(direction);
                }
            }
            let mut expected_integralized = Vec::new();
            for (index, in_forest) in expected_forest.iter_mut().enumerate() {
                if *in_forest && expected_flows[index].is_integer() {
                    *in_forest = false;
                    expected_integralized.push(graph.edges()[index].id().clone());
                }
            }
            let integralized_count = u64::try_from(expected_integralized.len())
                .map_err(|_| CostedFlowRoundingError::TraceVerification)?;
            if expected_integralized != *integralized_edges
                || after.processed_edges != before.processed_edges + 1
                || after.flows != expected_flows
                || after.fractional_forest != expected_forest
                || after.complete
                || !metrics_changed(before, after, 0, 1, integralized_count)
            {
                return Err(CostedFlowRoundingError::TraceVerification);
            }
        }
    }
    Ok(())
}

fn metrics_changed(
    before: &CostedFlowRoundingSnapshot,
    after: &CostedFlowRoundingSnapshot,
    linked: u64,
    canceled: u64,
    integralized: u64,
) -> bool {
    after.metrics.processed_edges == before.metrics.processed_edges + 1
        && after.metrics.linked_edges == before.metrics.linked_edges + linked
        && after.metrics.canceled_cycles == before.metrics.canceled_cycles + canceled
        && after.metrics.integralized_edges == before.metrics.integralized_edges + integralized
}

fn next_edge_index(
    graph: &FlowNetwork,
    before: &CostedFlowRoundingSnapshot,
    declared: &EdgeId,
) -> Result<usize, CostedFlowRoundingError> {
    let index = before.processed_edges;
    if graph
        .edges()
        .get(index)
        .is_none_or(|edge| edge.id() != declared)
    {
        return Err(CostedFlowRoundingError::TraceVerification);
    }
    Ok(index)
}

fn forest_path(
    graph: &FlowNetwork,
    forest: &[bool],
    start: usize,
    target: usize,
) -> Option<Vec<(usize, i8)>> {
    if start == target {
        return Some(Vec::new());
    }
    let mut adjacency = vec![Vec::<(usize, usize, i8)>::new(); graph.nodes().len()];
    for (index, edge) in graph.edges().iter().enumerate() {
        if forest[index] {
            adjacency[edge.from().as_usize()].push((edge.to().as_usize(), index, 1));
            adjacency[edge.to().as_usize()].push((edge.from().as_usize(), index, -1));
        }
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut previous = vec![None::<(usize, usize, i8)>; graph.nodes().len()];
    previous[start] = Some((start, usize::MAX, 0));
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for &(next, edge, direction) in &adjacency[node] {
            if previous[next].is_none() {
                previous[next] = Some((node, edge, direction));
                queue.push_back(next);
            }
        }
    }
    previous[target]?;
    let mut reversed = Vec::new();
    let mut cursor = target;
    while cursor != start {
        let (parent, edge, direction) = previous[cursor]?;
        reversed.push((edge, direction));
        cursor = parent;
    }
    reversed.reverse();
    Some(reversed)
}

fn forest_connected(graph: &FlowNetwork, forest: &[bool], start: usize, target: usize) -> bool {
    forest_path(graph, forest, start, target).is_some()
}

fn availability(flow: &BigRational, direction: i8) -> BigRational {
    let floor = BigRational::from_integer(flow.to_integer());
    if direction > 0 {
        if flow.is_integer() {
            BigRational::zero()
        } else {
            floor + BigRational::one() - flow
        }
    } else {
        flow - floor
    }
}

fn signed_cycle_cost(
    graph: &FlowNetwork,
    signed_edges: &[(usize, i8)],
) -> Result<i128, CostedFlowRoundingError> {
    signed_edges.iter().try_fold(0_i128, |sum, &(index, sign)| {
        sum.checked_add(i128::from(graph.edges()[index].cost()) * i128::from(sign))
            .ok_or(CostedFlowRoundingError::ArithmeticOverflow)
    })
}

fn is_directed_circulation(graph: &FlowNetwork, signed: &[i8]) -> bool {
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for (edge, &sign) in graph.edges().iter().zip(signed) {
        divergence[edge.from().as_usize()] += i128::from(sign);
        divergence[edge.to().as_usize()] -= i128::from(sign);
    }
    divergence.iter().all(|&value| value == 0)
}

fn exact_divergences(graph: &FlowNetwork, flows: &[BigRational]) -> Vec<BigRational> {
    let mut divergence = vec![BigRational::zero(); graph.nodes().len()];
    for (edge, flow) in graph.edges().iter().zip(flows) {
        divergence[edge.from().as_usize()] += flow;
        divergence[edge.to().as_usize()] -= flow;
    }
    divergence
}

fn exact_cost(graph: &FlowNetwork, flows: &[BigRational]) -> BigRational {
    graph
        .edges()
        .iter()
        .zip(flows)
        .fold(BigRational::zero(), |cost, (edge, flow)| {
            cost + flow * BigInt::from(edge.cost())
        })
}

fn exact_integral_flows(flows: &[BigRational]) -> Result<Vec<u64>, CostedFlowRoundingError> {
    flows
        .iter()
        .map(|flow| {
            if !flow.is_integer() {
                return Err(CostedFlowRoundingError::InvariantViolation);
            }
            flow.to_integer()
                .to_u64()
                .ok_or(CostedFlowRoundingError::ArithmeticOverflow)
        })
        .collect()
}

fn integral_cost(graph: &FlowNetwork, flows: &[u64]) -> Result<i128, CostedFlowRoundingError> {
    graph
        .edges()
        .iter()
        .zip(flows)
        .try_fold(0_i128, |cost, (edge, &flow)| {
            i128::from(edge.cost())
                .checked_mul(i128::from(flow))
                .and_then(|term| cost.checked_add(term))
                .ok_or(CostedFlowRoundingError::ArithmeticOverflow)
        })
}

fn integer_rational(value: u64) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

fn integer_i128(value: i128) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

fn checked_increment(value: u64) -> Result<u64, CostedFlowRoundingError> {
    value
        .checked_add(1)
        .ok_or(CostedFlowRoundingError::ArithmeticOverflow)
}

fn push_event(
    events: &mut Vec<CostedFlowRoundingTraceEvent>,
    kind: CostedFlowRoundingEventKind,
    before: CostedFlowRoundingSnapshot,
    after: CostedFlowRoundingSnapshot,
) -> Result<(), CostedFlowRoundingError> {
    if events.len() >= COSTED_FLOW_ROUNDING_MAX_TRACE_EVENTS {
        return Err(CostedFlowRoundingError::AdmissionLimit);
    }
    events.push(CostedFlowRoundingTraceEvent {
        catalog_id: CATALOG_ID,
        kind,
        before,
        after,
    });
    Ok(())
}

struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(nodes: usize) -> Self {
        Self {
            parent: (0..nodes).collect(),
            rank: vec![0; nodes],
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
        if self.rank[left_root] < self.rank[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        if self.rank[left_root] == self.rank[right_root] {
            self.rank[left_root] += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;
    use num_rational::BigRational;

    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn graph(edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            ["a", "b", "c"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
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
        .expect("graph")
    }

    #[test]
    fn positive_cycle_cost_rounds_backward() {
        let graph = graph(&[
            ("ab", "a", "b", 0, 1, 1),
            ("bc", "b", "c", 0, 1, 1),
            ("ca", "c", "a", 0, 1, 1),
        ]);
        let half = rational(1, 2);
        let result = round_costed_flow(&graph, &[0, 0, 0], &[half.clone(), half.clone(), half])
            .expect("round");
        assert_eq!(result.flows, vec![0, 0, 0]);
        assert_eq!(result.total_cost, 0);
        assert!(BigRational::from_integer(BigInt::from(result.total_cost)) <= result.initial_cost);
    }

    #[test]
    fn negative_cycle_cost_rounds_forward() {
        let graph = graph(&[
            ("ab", "a", "b", 0, 1, -1),
            ("bc", "b", "c", 0, 1, -1),
            ("ca", "c", "a", 0, 1, -1),
        ]);
        let half = rational(1, 2);
        let trace =
            trace_costed_flow_rounding(&graph, &[0, 0, 0], &[half.clone(), half.clone(), half])
                .expect("trace");
        assert_eq!(trace.result.flows, vec![1, 1, 1]);
        assert_eq!(trace.result.total_cost, -3);
        assert!(matches!(
            &trace.events[2].kind,
            CostedFlowRoundingEventKind::FractionalCycleCanceled {
                signed_cycle_cost: -3,
                ..
            }
        ));
    }

    #[test]
    fn parallel_path_flow_preserves_integral_divergence() {
        let graph = graph(&[
            ("expensive", "a", "b", 0, 1, 10),
            ("cheap", "a", "b", 0, 1, 1),
        ]);
        let half = rational(1, 2);
        let result = round_costed_flow(&graph, &[1, -1, 0], &[half.clone(), half]).expect("round");
        // FlowNetwork canonicalizes by edge ID: cheap precedes expensive.
        assert_eq!(result.flows, vec![1, 0]);
        assert_eq!(result.total_cost, 1);
    }

    #[test]
    fn independent_checker_rejects_cycle_direction_tampering() {
        let graph = graph(&[
            ("ab", "a", "b", 0, 1, 1),
            ("bc", "b", "c", 0, 1, 1),
            ("ca", "c", "a", 0, 1, 1),
        ]);
        let half = rational(1, 2);
        let flows = vec![half.clone(), half.clone(), half];
        let mut trace = trace_costed_flow_rounding(&graph, &[0, 0, 0], &flows).expect("trace");
        let CostedFlowRoundingEventKind::FractionalCycleCanceled { cycle, .. } =
            &mut trace.events[2].kind
        else {
            panic!("cycle event");
        };
        cycle[0].direction = -cycle[0].direction;
        assert_eq!(
            check_costed_flow_rounding_trace(&graph, &[0, 0, 0], &flows, &trace),
            Err(CostedFlowRoundingError::TraceVerification)
        );
    }

    #[test]
    fn rejects_non_integral_divergence_and_bounds() {
        let graph = graph(&[("ab", "a", "b", 0, 1, 0)]);
        assert_eq!(
            round_costed_flow(&graph, &[0, 0, 0], &[rational(1, 2)]),
            Err(CostedFlowRoundingError::InvalidDivergence)
        );
        assert!(matches!(
            round_costed_flow(&graph, &[2, -2, 0], &[rational(2, 1)]),
            Err(CostedFlowRoundingError::EdgeBounds(_))
        ));
    }
}
