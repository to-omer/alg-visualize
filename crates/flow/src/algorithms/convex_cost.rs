//! Exact segment-expanded baseline for integral piecewise-linear convex costs.
//!
//! This is deliberately not named after Pinto--Shamir's faster scaling
//! algorithm.  It is the canonical parallel-segment reduction used as the
//! independent oracle for later native convex-cost implementations.

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, supply_divergences,
};
use crate::feasibility::{CapturedFeasibilityAnchor, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowModelError, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, apply_trace_event,
};

use super::minimum_mean_cycle_canceling::{
    solve_minimum_mean_cycle_canceling_with_feasibility_use,
    trace_minimum_mean_cycle_canceling_with_feasibility_use,
};
use super::{
    MinimumMeanCycleCancelingError, MinimumMeanCycleCancelingMetrics,
    solve_minimum_mean_cycle_canceling, trace_minimum_mean_cycle_canceling,
};

/// Conservative original-node limit for the expanded oracle.
pub const CONVEX_EXPANDED_MAX_NODES: usize = 128;
/// Conservative original-edge limit for the expanded oracle.
pub const CONVEX_EXPANDED_MAX_EDGES: usize = 1_024;
/// Maximum total number of linear segments after expansion.
pub const CONVEX_EXPANDED_MAX_SEGMENTS: usize = 1_024;

/// One half-open interval `(previous_end, end_flow]` of integer flow units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConvexCostSegment {
    /// Exclusive upper flow boundary of this segment.
    pub end_flow: u64,
    /// Marginal cost of every integer unit in the segment.
    pub marginal_cost: i64,
}

/// Exact separable convex objective for one original edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexEdgeCost {
    /// Constant objective contribution, including when the edge carries zero.
    pub base_cost_at_zero: i128,
    /// Strictly increasing boundaries with nondecreasing marginal costs.
    pub segments: Vec<ConvexCostSegment>,
}

/// Validated native convex-cost transshipment problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostProblem<'graph> {
    graph: &'graph FlowNetwork,
    edge_costs: Vec<ConvexEdgeCost>,
}

impl<'graph> ConvexCostProblem<'graph> {
    /// Validates the complete convex objective without applying executable
    /// node, edge, or expanded-segment admission limits.
    ///
    /// # Errors
    ///
    /// Rejects the same shape, convexity, breakpoint, and arithmetic failures
    /// as [`Self::new`].
    pub fn validate_declaration(
        graph: &FlowNetwork,
        edge_costs: &[ConvexEdgeCost],
    ) -> Result<(), ConvexCostError> {
        validate_convex_declaration(graph, edge_costs).map(|_| ())
    }

    /// Validates one objective per canonical original edge.
    ///
    /// # Errors
    ///
    /// Rejects shape, convexity, breakpoint, admission, or arithmetic failure.
    pub fn new(
        graph: &'graph FlowNetwork,
        edge_costs: Vec<ConvexEdgeCost>,
    ) -> Result<Self, ConvexCostError> {
        if graph.nodes().len() > CONVEX_EXPANDED_MAX_NODES
            || graph.edges().len() > CONVEX_EXPANDED_MAX_EDGES
        {
            return Err(ConvexCostError::AdmissionLimit);
        }
        let segment_count = validate_convex_declaration(graph, &edge_costs)?;
        if segment_count > CONVEX_EXPANDED_MAX_SEGMENTS {
            return Err(ConvexCostError::AdmissionLimit);
        }
        Ok(Self { graph, edge_costs })
    }

    /// Returns the immutable original network.
    #[must_use]
    pub const fn graph(&self) -> &'graph FlowNetwork {
        self.graph
    }

    /// Returns objectives in canonical original-edge order.
    #[must_use]
    pub fn edge_costs(&self) -> &[ConvexEdgeCost] {
        &self.edge_costs
    }
}

fn validate_convex_declaration(
    graph: &FlowNetwork,
    edge_costs: &[ConvexEdgeCost],
) -> Result<usize, ConvexCostError> {
    if edge_costs.len() != graph.edges().len() {
        return Err(ConvexCostError::Shape);
    }
    let mut segment_count = 0_usize;
    let mut absolute_bound = 0_i128;
    for (edge, cost) in graph.edges().iter().zip(edge_costs) {
        if cost.segments.is_empty() && edge.capacity() != 0 {
            return Err(ConvexCostError::EmptySegments);
        }
        segment_count = segment_count
            .checked_add(cost.segments.len())
            .ok_or(ConvexCostError::ArithmeticOverflow)?;
        let mut previous_end = 0_u64;
        let mut previous_slope = None;
        let mut value = cost.base_cost_at_zero;
        for segment in &cost.segments {
            if segment.end_flow <= previous_end {
                return Err(ConvexCostError::Breakpoints);
            }
            if previous_slope.is_some_and(|slope| segment.marginal_cost < slope) {
                return Err(ConvexCostError::NonConvex);
            }
            let length = segment.end_flow - previous_end;
            value = i128::from(length)
                .checked_mul(i128::from(segment.marginal_cost))
                .and_then(|term| value.checked_add(term))
                .ok_or(ConvexCostError::ArithmeticOverflow)?;
            previous_end = segment.end_flow;
            previous_slope = Some(segment.marginal_cost);
        }
        if previous_end != edge.capacity() {
            return Err(ConvexCostError::TerminalBreakpoint);
        }
        absolute_bound = absolute_bound
            .checked_add(
                value
                    .checked_abs()
                    .ok_or(ConvexCostError::ArithmeticOverflow)?,
            )
            .ok_or(ConvexCostError::ArithmeticOverflow)?;
    }
    let _ = absolute_bound;
    Ok(segment_count)
}

/// One segment's replay-visible state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexSegmentState {
    /// Canonical original-edge ordinal.
    pub edge: usize,
    /// Zero-based segment ordinal on that edge.
    pub segment: usize,
    /// Segment's exclusive lower flow boundary.
    pub start_flow: u64,
    /// Segment's exclusive upper flow boundary.
    pub end_flow: u64,
    /// Current canonical prefix flow assigned to this segment.
    pub flow: u64,
    /// Marginal cost of this segment.
    pub marginal_cost: i64,
}

/// Direction of a marginal residual arc.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvexResidualDirection {
    /// Add flow through the next unsaturated segment.
    Forward,
    /// Remove flow from the last positive segment.
    Reverse,
}

/// Original identity of one active expanded residual arc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexResidualArc {
    /// Canonical original-edge ordinal.
    pub edge: usize,
    /// Segment ordinal selected by the linear oracle.
    pub segment: usize,
    /// Expanded residual direction.
    pub direction: ConvexResidualDirection,
}

/// Native replay boundary projected from the expanded linear oracle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostSnapshot {
    /// Aggregate original-edge flows.
    pub flows: Vec<u64>,
    /// Canonical prefix segment occupancy.
    pub segments: Vec<ConvexSegmentState>,
    /// Current node labels from minimum-mean selection.
    pub node_labels: Vec<Option<i128>>,
    /// Stable node search order.
    pub search_order: Vec<NodeId>,
    /// Active residual cycle in expanded-segment identity.
    pub active_cycle: Vec<ConvexResidualArc>,
    /// Absolute oracle counters at this boundary.
    pub metrics: MinimumMeanCycleCancelingMetrics,
}

/// Semantic phase of the segment-expanded baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConvexCostStage {
    /// Feasible expanded segment flow has been constructed.
    Initialize,
    /// The selector inspects a residual arc or selects the minimum-mean cycle.
    SelectMinimumMeanCycle,
    /// The selected expanded cycle is canceled to its bottleneck.
    CancelCycle,
    /// No negative marginal residual cycle remains.
    Optimal,
}

/// One exact event plus the underlying reversible linear-oracle transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostTraceEvent {
    /// Closed semantic stage.
    pub stage: ConvexCostStage,
    /// Exact optional selector detail copied from the source event.
    pub detail: Option<(String, i128)>,
    /// Original marginal residual arcs explicitly inspected by this source event.
    pub focus_arcs: Vec<ConvexResidualArc>,
    /// Projected native boundary after the event.
    pub after: ConvexCostSnapshot,
    /// Reversible event on the canonical expanded graph.
    pub expanded_event: FlowTraceEvent,
}

/// Independent native certificate for a convex-cost optimum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostCertificate {
    /// Exact objective including every `base_cost_at_zero` constant.
    pub total_cost: i128,
    /// One feasible marginal-residual potential per original node.
    pub potentials: Vec<i128>,
}

/// Certified result of the segment-expanded baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostResult {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Canonical prefix flow of every segment, grouped by edge.
    pub segment_flows: Vec<Vec<u64>>,
    /// Independently reconstructed native certificate.
    pub certificate: ConvexCostCertificate,
    /// Exact counters of the bounded linear oracle.
    pub metrics: MinimumMeanCycleCancelingMetrics,
}

/// Certified result plus native segment-level replay boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConvexCostTraceResult {
    /// Same result produced by the fast profile.
    pub result: ConvexCostResult,
    /// Initial feasible segment occupancy.
    pub base_snapshot: ConvexCostSnapshot,
    /// Projected reversible oracle transitions.
    pub events: Vec<ConvexCostTraceEvent>,
    /// Verified optimum boundary.
    pub final_snapshot: ConvexCostSnapshot,
}

/// Convex model, transformation, oracle, replay, or certificate failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConvexCostError {
    /// Input exceeds the interactive expansion band.
    #[error("convex-cost problem exceeds segment-expanded admission limits")]
    AdmissionLimit,
    /// Objective count differs from the original-edge count.
    #[error("convex-cost objective shape does not match original edges")]
    Shape,
    /// An edge declared no marginal segment.
    #[error("convex-cost edge must declare at least one segment")]
    EmptySegments,
    /// Segment ends are not strictly increasing.
    #[error("convex-cost segment ends must be strictly increasing")]
    Breakpoints,
    /// The last segment end differs from the edge capacity.
    #[error("convex-cost final segment must end at edge capacity")]
    TerminalBreakpoint,
    /// Marginal costs decrease across consecutive segments.
    #[error("convex-cost marginal costs must be nondecreasing")]
    NonConvex,
    /// Checked exact arithmetic exceeded the declared domain.
    #[error("convex-cost arithmetic overflow")]
    ArithmeticOverflow,
    /// Expanded graph construction failed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// Linear minimum-cost oracle failed.
    #[error(transparent)]
    Oracle(#[from] MinimumMeanCycleCancelingError),
    /// Independent feasibility or optimality checking failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible expanded trace failed to replay.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Native and expanded boundaries disagreed.
    #[error("convex-cost trace projection invariant failed")]
    TraceInvariant,
}

#[derive(Clone, Copy)]
struct SegmentMap {
    original_edge: usize,
    segment: usize,
    start_flow: u64,
    end_flow: u64,
    marginal_cost: i64,
}

struct Expansion {
    graph: FlowNetwork,
    by_expanded_edge: Vec<SegmentMap>,
}

/// Solves the exact integral convex-cost transshipment by canonical expansion.
///
/// # Errors
///
/// Returns model, feasibility, work-limit, arithmetic, or certificate failure.
pub fn solve_segment_expanded_convex_cost(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexCostResult, ConvexCostError> {
    let expansion = expand_problem(problem)?;
    let target = supply_divergences(&expansion.graph)?;
    let linear = solve_minimum_mean_cycle_canceling(&expansion.graph, &target)?;
    result_from_expanded(problem, &expansion, &linear.flows, linear.metrics)
}

/// Solves the expanded model while reporting the nested linear oracle's
/// feasibility construction to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_segment_expanded_convex_cost_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexCostResult, ConvexCostError> {
    let expansion = expand_problem(problem)?;
    let target = supply_divergences(&expansion.graph)?;
    let linear = solve_minimum_mean_cycle_canceling_with_feasibility_use(
        &expansion.graph,
        &target,
        FeasibilityUse::BeforeEvent {
            anchor: CapturedFeasibilityAnchor {
                catalog_id: "segment-expanded-convex-mcf.start-selector",
                occurrence: 1,
            },
        },
        feasibility,
    )?;
    result_from_expanded(problem, &expansion, &linear.flows, linear.metrics)
}

/// Runs the exact baseline and projects every linear-oracle event to segments.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus replay mismatch.
pub fn trace_segment_expanded_convex_cost(
    problem: &ConvexCostProblem<'_>,
) -> Result<ConvexCostTraceResult, ConvexCostError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_segment_expanded_convex_cost_with_feasibility(problem, &mut feasibility)
}

/// Traces the expanded convex-cost reduction while explicitly publishing the
/// nested linear oracle's feasibility run before its first source boundary.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_segment_expanded_convex_cost_with_feasibility(
    problem: &ConvexCostProblem<'_>,
    feasibility: &mut FeasibilityExecution,
) -> Result<ConvexCostTraceResult, ConvexCostError> {
    let expansion = expand_problem(problem)?;
    let target = supply_divergences(&expansion.graph)?;
    let run = trace_minimum_mean_cycle_canceling_with_feasibility_use(
        &expansion.graph,
        &target,
        FeasibilityUse::BeforeEvent {
            anchor: CapturedFeasibilityAnchor {
                catalog_id: "segment-expanded-convex-mcf.start-selector",
                occurrence: 1,
            },
        },
        feasibility,
    )?;
    let result = result_from_expanded(problem, &expansion, &run.result.flows, run.result.metrics)?;
    let base_snapshot = project_snapshot(problem, &expansion, &run.base_snapshot)?;
    let mut replay = run.base_snapshot.clone();
    let mut events = Vec::with_capacity(run.events.len());
    for event in run.events {
        apply_trace_event(
            &expansion.graph,
            &mut replay,
            &event,
            FlowTraceDirection::Forward,
        )?;
        let stage = stage_for_catalog_id(&event.catalog_id)?;
        let focus_arcs = project_event_focus(&expansion, &event)?;
        let detail = projected_event_detail(problem, &event, &focus_arcs)?;
        events.push(ConvexCostTraceEvent {
            stage,
            detail,
            focus_arcs,
            after: project_snapshot(problem, &expansion, &replay)?,
            expanded_event: event,
        });
    }
    if replay != run.final_snapshot {
        return Err(ConvexCostError::TraceInvariant);
    }
    let final_snapshot = project_snapshot(problem, &expansion, &run.final_snapshot)?;
    let trace = ConvexCostTraceResult {
        result,
        base_snapshot,
        events,
        final_snapshot,
    };
    check_segment_expanded_convex_trace(problem, &trace)?;
    Ok(trace)
}

fn marginal_focus_detail(
    problem: &ConvexCostProblem<'_>,
    focus_arcs: &[ConvexResidualArc],
) -> Option<(String, i128)> {
    let focus = focus_arcs.first()?;
    let slope = problem
        .edge_costs
        .get(focus.edge)?
        .segments
        .get(focus.segment)?
        .marginal_cost;
    let signed = match focus.direction {
        ConvexResidualDirection::Forward => i128::from(slope),
        ConvexResidualDirection::Reverse => -i128::from(slope),
    };
    Some(("marginal-arc-cost".to_owned(), signed))
}

fn projected_event_detail(
    problem: &ConvexCostProblem<'_>,
    event: &FlowTraceEvent,
    focus_arcs: &[ConvexResidualArc],
) -> Result<Option<(String, i128)>, ConvexCostError> {
    if event.catalog_id == "minimum-mean-cycle-canceling.inspect-residual-arc" {
        return marginal_focus_detail(problem, focus_arcs)
            .map(Some)
            .ok_or(ConvexCostError::TraceInvariant);
    }
    Ok(event
        .detail
        .as_ref()
        .map(|detail| (detail.label.clone(), detail.value)))
}

/// Independently validates a native candidate via canonical prefix expansion.
///
/// # Errors
///
/// Rejects bounds, conservation, objective overflow, or a negative marginal
/// residual cycle.
pub fn check_convex_cost_flow(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
) -> Result<ConvexCostCertificate, ConvexCostError> {
    if flows.len() != problem.graph.edges().len() {
        return Err(ConvexCostError::Shape);
    }
    let expansion = expand_problem(problem)?;
    let segment_flows = canonical_segment_flows(problem, flows)?;
    let expanded_flows = flatten_segment_flows(&segment_flows);
    let target = supply_divergences(&expansion.graph)?;
    let linear = check_min_cost_flow(&expansion.graph, &target, &expanded_flows)?;
    native_certificate(problem, linear)
}

/// Replays every wrapped oracle event and reprojects the native boundary.
///
/// # Errors
///
/// Rejects any modified event, snapshot, stage, result, or final certificate.
pub fn check_segment_expanded_convex_trace(
    problem: &ConvexCostProblem<'_>,
    trace: &ConvexCostTraceResult,
) -> Result<(), ConvexCostError> {
    let expansion = expand_problem(problem)?;
    let target = supply_divergences(&expansion.graph)?;
    let reference = trace_minimum_mean_cycle_canceling(&expansion.graph, &target)?;
    if trace.events.len() != reference.events.len()
        || trace.base_snapshot != project_snapshot(problem, &expansion, &reference.base_snapshot)?
    {
        return Err(ConvexCostError::TraceInvariant);
    }
    let mut replay = reference.base_snapshot;
    for (wrapped, reference_event) in trace.events.iter().zip(reference.events) {
        let reference_focus = project_event_focus(&expansion, &reference_event)?;
        let reference_detail = projected_event_detail(problem, &reference_event, &reference_focus)?;
        if wrapped.expanded_event != reference_event
            || wrapped.stage != stage_for_catalog_id(&reference_event.catalog_id)?
            || wrapped.focus_arcs != reference_focus
            || wrapped.detail != reference_detail
        {
            return Err(ConvexCostError::TraceInvariant);
        }
        apply_trace_event(
            &expansion.graph,
            &mut replay,
            &wrapped.expanded_event,
            FlowTraceDirection::Forward,
        )?;
        if wrapped.after != project_snapshot(problem, &expansion, &replay)? {
            return Err(ConvexCostError::TraceInvariant);
        }
    }
    let final_snapshot = project_snapshot(problem, &expansion, &replay)?;
    if final_snapshot != trace.final_snapshot
        || final_snapshot.flows != trace.result.flows
        || check_convex_cost_flow(problem, &trace.result.flows)? != trace.result.certificate
    {
        return Err(ConvexCostError::TraceInvariant);
    }
    Ok(())
}

fn project_event_focus(
    expansion: &Expansion,
    event: &FlowTraceEvent,
) -> Result<Vec<ConvexResidualArc>, ConvexCostError> {
    event
        .entity_refs
        .iter()
        .filter_map(|entity| match entity {
            FlowTraceEntityRef::ResidualArc(arc) => Some(arc),
            FlowTraceEntityRef::Node(_) | FlowTraceEntityRef::Edge(_) => None,
        })
        .map(|arc| {
            let expanded = expansion
                .graph
                .edge_index(arc.original_edge())
                .ok_or(ConvexCostError::TraceInvariant)?;
            let mapping = expansion
                .by_expanded_edge
                .get(expanded.as_usize())
                .ok_or(ConvexCostError::TraceInvariant)?;
            Ok(ConvexResidualArc {
                edge: mapping.original_edge,
                segment: mapping.segment,
                direction: match arc.direction() {
                    crate::residual::ResidualDirection::Forward => ConvexResidualDirection::Forward,
                    crate::residual::ResidualDirection::Reverse => ConvexResidualDirection::Reverse,
                },
            })
        })
        .collect()
}

fn expand_problem(problem: &ConvexCostProblem<'_>) -> Result<Expansion, ConvexCostError> {
    let nodes = problem
        .graph
        .nodes()
        .iter()
        .map(|node| FlowNode::new(node.id().clone(), node.supply()))
        .collect::<Vec<_>>();
    let mut declarations = Vec::new();
    let mut mappings = Vec::new();
    for (edge_index, (edge, objective)) in problem
        .graph
        .edges()
        .iter()
        .zip(&problem.edge_costs)
        .enumerate()
    {
        let from = problem
            .graph
            .node(edge.from())
            .ok_or(ConvexCostError::Shape)?
            .id()
            .clone();
        let to = problem
            .graph
            .node(edge.to())
            .ok_or(ConvexCostError::Shape)?
            .id()
            .clone();
        let mut start = 0_u64;
        for (segment_index, segment) in objective.segments.iter().enumerate() {
            let length = segment.end_flow - start;
            let lower = edge.lower().saturating_sub(start).min(length);
            let id = EdgeId::parse(&format!("cvx:{edge_index:06}:{segment_index:06}"))?;
            declarations.push(UnresolvedFlowEdge {
                id,
                from: from.clone(),
                to: to.clone(),
                lower,
                capacity: length,
                cost: segment.marginal_cost,
            });
            mappings.push(SegmentMap {
                original_edge: edge_index,
                segment: segment_index,
                start_flow: start,
                end_flow: segment.end_flow,
                marginal_cost: segment.marginal_cost,
            });
            start = segment.end_flow;
        }
    }
    let graph = FlowNetwork::new(nodes, declarations)?;
    if graph.edges().len() != mappings.len() {
        return Err(ConvexCostError::Shape);
    }
    Ok(Expansion {
        graph,
        by_expanded_edge: mappings,
    })
}

fn canonical_segment_flows(
    problem: &ConvexCostProblem<'_>,
    flows: &[u64],
) -> Result<Vec<Vec<u64>>, ConvexCostError> {
    if flows.len() != problem.graph.edges().len() {
        return Err(ConvexCostError::Shape);
    }
    problem
        .graph
        .edges()
        .iter()
        .zip(&problem.edge_costs)
        .zip(flows)
        .map(|((edge, objective), &flow)| {
            if flow < edge.lower() || flow > edge.capacity() {
                return Err(ConvexCostError::Certificate(CertificateError::EdgeBounds(
                    edge.id().as_str().to_owned(),
                )));
            }
            let mut remaining = flow;
            let mut start = 0_u64;
            let mut segment_flows = Vec::with_capacity(objective.segments.len());
            for segment in &objective.segments {
                let length = segment.end_flow - start;
                let amount = remaining.min(length);
                segment_flows.push(amount);
                remaining -= amount;
                start = segment.end_flow;
            }
            if remaining != 0 {
                return Err(ConvexCostError::Shape);
            }
            Ok(segment_flows)
        })
        .collect()
}

fn aggregate_expanded_flows(
    problem: &ConvexCostProblem<'_>,
    expansion: &Expansion,
    expanded_flows: &[u64],
) -> Result<Vec<u64>, ConvexCostError> {
    if expanded_flows.len() != expansion.by_expanded_edge.len() {
        return Err(ConvexCostError::Shape);
    }
    let mut flows = vec![0_u64; problem.graph.edges().len()];
    for (&flow, mapping) in expanded_flows.iter().zip(&expansion.by_expanded_edge) {
        flows[mapping.original_edge] = flows[mapping.original_edge]
            .checked_add(flow)
            .ok_or(ConvexCostError::ArithmeticOverflow)?;
    }
    Ok(flows)
}

fn flatten_segment_flows(flows: &[Vec<u64>]) -> Vec<u64> {
    flows.iter().flatten().copied().collect()
}

fn result_from_expanded(
    problem: &ConvexCostProblem<'_>,
    expansion: &Expansion,
    expanded_flows: &[u64],
    metrics: MinimumMeanCycleCancelingMetrics,
) -> Result<ConvexCostResult, ConvexCostError> {
    let flows = aggregate_expanded_flows(problem, expansion, expanded_flows)?;
    let segment_flows = canonical_segment_flows(problem, &flows)?;
    let certificate = check_convex_cost_flow(problem, &flows)?;
    Ok(ConvexCostResult {
        flows,
        segment_flows,
        certificate,
        metrics,
    })
}

fn native_certificate(
    problem: &ConvexCostProblem<'_>,
    linear: MinCostFlowCertificate,
) -> Result<ConvexCostCertificate, ConvexCostError> {
    let base = problem.edge_costs.iter().try_fold(0_i128, |sum, cost| {
        sum.checked_add(cost.base_cost_at_zero)
            .ok_or(ConvexCostError::ArithmeticOverflow)
    })?;
    Ok(ConvexCostCertificate {
        total_cost: base
            .checked_add(linear.total_cost)
            .ok_or(ConvexCostError::ArithmeticOverflow)?,
        potentials: linear.potentials,
    })
}

fn project_snapshot(
    problem: &ConvexCostProblem<'_>,
    expansion: &Expansion,
    snapshot: &crate::trace::FlowTraceSnapshot,
) -> Result<ConvexCostSnapshot, ConvexCostError> {
    let flows = aggregate_expanded_flows(problem, expansion, &snapshot.flows)?;
    let canonical = canonical_segment_flows(problem, &flows)?;
    let mut segments = Vec::with_capacity(expansion.by_expanded_edge.len());
    for mapping in &expansion.by_expanded_edge {
        segments.push(ConvexSegmentState {
            edge: mapping.original_edge,
            segment: mapping.segment,
            start_flow: mapping.start_flow,
            end_flow: mapping.end_flow,
            flow: canonical[mapping.original_edge][mapping.segment],
            marginal_cost: mapping.marginal_cost,
        });
    }
    let mut active_cycle = Vec::with_capacity(snapshot.active_path.len());
    for arc in &snapshot.active_path {
        let expanded = expansion
            .graph
            .edge_index(arc.original_edge())
            .ok_or(ConvexCostError::TraceInvariant)?;
        let mapping = expansion
            .by_expanded_edge
            .get(expanded.as_usize())
            .ok_or(ConvexCostError::TraceInvariant)?;
        active_cycle.push(ConvexResidualArc {
            edge: mapping.original_edge,
            segment: mapping.segment,
            direction: match arc.direction() {
                crate::residual::ResidualDirection::Forward => ConvexResidualDirection::Forward,
                crate::residual::ResidualDirection::Reverse => ConvexResidualDirection::Reverse,
            },
        });
    }
    Ok(ConvexCostSnapshot {
        flows,
        segments,
        node_labels: snapshot.node_labels.clone(),
        search_order: snapshot.search_order.clone(),
        active_cycle,
        metrics: MinimumMeanCycleCancelingMetrics {
            mean_cycle_searches: u64::try_from(snapshot.metrics.path_searches)
                .map_err(|_| ConvexCostError::ArithmeticOverflow)?,
            dynamic_programming_rounds: u64::try_from(snapshot.metrics.relaxation_passes)
                .map_err(|_| ConvexCostError::ArithmeticOverflow)?,
            residual_arc_scans: snapshot.metrics.residual_arc_scans,
            canceled_cycles: u64::try_from(snapshot.metrics.augmentations)
                .map_err(|_| ConvexCostError::ArithmeticOverflow)?,
        },
    })
}

fn stage_for_catalog_id(catalog_id: &str) -> Result<ConvexCostStage, ConvexCostError> {
    match catalog_id {
        "minimum-mean-cycle-canceling.start-selector" => Ok(ConvexCostStage::Initialize),
        "minimum-mean-cycle-canceling.inspect-residual-arc"
        | "minimum-mean-cycle-canceling.select-minimum-mean-cycle" => {
            Ok(ConvexCostStage::SelectMinimumMeanCycle)
        }
        "minimum-mean-cycle-canceling.cancel-minimum-mean-cycle" => {
            Ok(ConvexCostStage::CancelCycle)
        }
        "minimum-mean-cycle-canceling.optimal" => Ok(ConvexCostStage::Optimal),
        _ => Err(ConvexCostError::TraceInvariant),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> FlowNetwork {
        FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").unwrap(), 3),
                FlowNode::new(NodeId::parse("m").unwrap(), 0),
                FlowNode::new(NodeId::parse("t").unwrap(), -3),
            ],
            vec![
                UnresolvedFlowEdge {
                    id: EdgeId::parse("direct").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 0,
                    capacity: 3,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("sm").unwrap(),
                    from: NodeId::parse("s").unwrap(),
                    to: NodeId::parse("m").unwrap(),
                    lower: 1,
                    capacity: 3,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("mt").unwrap(),
                    from: NodeId::parse("m").unwrap(),
                    to: NodeId::parse("t").unwrap(),
                    lower: 1,
                    capacity: 3,
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
                    base_cost_at_zero: 7,
                    segments: vec![
                        ConvexCostSegment {
                            end_flow: 1,
                            marginal_cost: 0,
                        },
                        ConvexCostSegment {
                            end_flow: 3,
                            marginal_cost: 5,
                        },
                    ],
                },
                ConvexEdgeCost {
                    base_cost_at_zero: 0,
                    segments: vec![ConvexCostSegment {
                        end_flow: 3,
                        marginal_cost: 1,
                    }],
                },
                ConvexEdgeCost {
                    base_cost_at_zero: 0,
                    segments: vec![ConvexCostSegment {
                        end_flow: 3,
                        marginal_cost: 1,
                    }],
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn expanded_oracle_preserves_native_convex_objective() {
        let graph = graph();
        let problem = problem(&graph);
        let result = solve_segment_expanded_convex_cost(&problem).unwrap();
        assert_eq!(result.flows, vec![1, 2, 2]);
        assert_eq!(result.segment_flows[0], vec![1, 0]);
        assert_eq!(result.certificate.total_cost, 11);
        assert_eq!(
            check_convex_cost_flow(&problem, &result.flows).unwrap(),
            result.certificate
        );
    }

    #[test]
    fn trace_replays_expanded_events_and_rejects_corruption() {
        let graph = graph();
        let problem = problem(&graph);
        let trace = trace_segment_expanded_convex_cost(&problem).unwrap();
        check_segment_expanded_convex_trace(&problem, &trace).unwrap();
        assert_eq!(trace.final_snapshot.flows, trace.result.flows);
        assert!(
            trace
                .events
                .iter()
                .filter(|event| {
                    event.expanded_event.catalog_id
                        == "minimum-mean-cycle-canceling.inspect-residual-arc"
                })
                .all(|event| event
                    .detail
                    .as_ref()
                    .is_some_and(|(label, _)| label == "marginal-arc-cost"))
        );

        let mut corrupt = trace.clone();
        corrupt.final_snapshot.flows[0] += 1;
        assert_eq!(
            check_segment_expanded_convex_trace(&problem, &corrupt),
            Err(ConvexCostError::TraceInvariant)
        );
    }

    #[test]
    fn validation_rejects_nonconvex_and_incomplete_breakpoints() {
        let graph = graph();
        let mut costs = problem(&graph).edge_costs().to_vec();
        costs[0].segments[1].marginal_cost = -1;
        assert_eq!(
            ConvexCostProblem::new(&graph, costs),
            Err(ConvexCostError::NonConvex)
        );

        let mut costs = problem(&graph).edge_costs().to_vec();
        costs[0].segments[1].end_flow = 2;
        assert_eq!(
            ConvexCostProblem::new(&graph, costs),
            Err(ConvexCostError::TerminalBreakpoint)
        );
    }
}
