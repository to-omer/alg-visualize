//! One exact Tardos network-matrix variable-fixing primitive.
//!
//! Tardos's 1986 result is a framework around an exact LP oracle, not a
//! standalone minimum-cost-flow kernel.  This module therefore publishes the
//! source-defined progress certificate used by the framework: for a feasible
//! bounded flow `f`, node labels `p`, and the least `epsilon` making every
//! residual reduced cost at least `-epsilon`, a residual direction whose
//! reduced cost is greater than `n * epsilon` fixes its original variable at
//! the current bound in every optimum.  Incidence matrices are totally
//! unimodular, so the determinant factor in the general LP statement is one.

use thiserror::Error;

use crate::certificate::{CertificateError, divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowNetwork};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};

/// Conservative interactive node limit for the explicit residual scan.
pub const TARDOS_FRAMEWORK_MAX_NODES: usize = 64;
/// Conservative interactive edge limit for the explicit residual scan.
pub const TARDOS_FRAMEWORK_MAX_EDGES: usize = 512;
/// Fixed phase, operation, and terminal transitions around per-arc detail.
pub const TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS: usize = 4;

/// Source-defined boundary of one variable-fixing invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TardosFrameworkStage {
    /// The shared feasibility kernel has supplied a checked flow; no
    /// Tardos-owned operation has run yet.
    Ready,
    /// A deterministic feasible flow has been constructed.
    ConstructFeasibleFlow,
    /// Every positive residual direction has been priced and epsilon measured.
    MeasureEpsilon,
    /// Directions beyond the strict proximity threshold have been classified.
    ClassifyFixedVariables,
    /// The independently checked primitive result is complete.
    Complete,
}

/// Whether the proof fixes an original variable at its lower or upper bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TardosFixedBound {
    /// The forward residual direction is too expensive, so flow stays at lower.
    Lower,
    /// The reverse residual direction is too expensive, so flow stays at upper.
    Upper,
}

/// One exact original-variable value fixed in every minimum-cost flow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosFixedVariable {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// The bound justified by the residual direction.
    pub bound: TardosFixedBound,
    /// Exact value shared by every optimum.
    pub value: u64,
    /// Residual direction used by the proof.
    pub witness_arc: ResidualArcId,
    /// Exact witness reduced cost, strictly greater than the threshold.
    pub reduced_cost: i128,
}

/// One positive residual direction in the theorem scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosResidualState {
    /// Stable residual identity.
    pub arc: ResidualArcId,
    /// Positive residual capacity.
    pub capacity: u64,
    /// Exact reduced cost under the configured labels.
    pub reduced_cost: i128,
    /// Whether the strict `reduced_cost > n * epsilon` test succeeds.
    pub fixes_variable: bool,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TardosFrameworkMetrics {
    /// Deterministic feasibility constructions.
    pub feasibility_constructions: u64,
    /// Positive residual directions priced by the primitive.
    pub residual_arc_scans: u128,
    /// Original variables proved fixed.
    pub fixed_variables: u64,
    /// Public source-defined transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosFrameworkSnapshot {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Configured exact node labels in canonical node order.
    pub potentials: Vec<i128>,
    /// Least nonnegative epsilon satisfying every residual inequality.
    pub epsilon: i128,
    /// Exact network-matrix threshold `n * epsilon` (`Delta(A) = 1`).
    pub threshold: i128,
    /// Positive residual directions published at this boundary.
    pub residual_arcs: Vec<TardosResidualState>,
    /// Canonical independently justified fixed variables.
    pub fixed_variables: Vec<TardosFixedVariable>,
    /// Semantic boundary.
    pub stage: TardosFrameworkStage,
    /// Exact work counters at this boundary.
    pub metrics: TardosFrameworkMetrics,
}

/// One reversible primitive transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosFrameworkTraceEvent {
    /// Stable event identity.
    pub catalog_id: &'static str,
    /// Boundary before the atomic transition.
    pub before: TardosFrameworkSnapshot,
    /// Boundary after the atomic transition.
    pub after: TardosFrameworkSnapshot,
}

/// Independently checked primitive result; this is not an optimal-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosFrameworkResult {
    /// Feasible flow to which the fixed-value theorem was applied.
    pub flows: Vec<u64>,
    /// Fixed variables in canonical edge/direction order.
    pub fixed_variables: Vec<TardosFixedVariable>,
    /// Exact terminal theorem state.
    pub final_snapshot: TardosFrameworkSnapshot,
    /// Exact work counters.
    pub metrics: TardosFrameworkMetrics,
}

/// Result plus every public theorem boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TardosFrameworkTraceResult {
    /// Same checked result as the fast profile.
    pub result: TardosFrameworkResult,
    /// Boundary after the shared feasibility construction and before the first
    /// Tardos-owned operation.
    pub base_snapshot: TardosFrameworkSnapshot,
    /// Four canonical source-defined transitions.
    pub events: Vec<TardosFrameworkTraceEvent>,
    /// Independently checked terminal boundary.
    pub final_snapshot: TardosFrameworkSnapshot,
}

/// Input, arithmetic, feasibility, invariant, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TardosFrameworkError {
    /// Input exceeds the conservative interactive theorem band.
    #[error("Tardos framework primitive input exceeds admission limits")]
    AdmissionLimit,
    /// The potential vector does not match the canonical node set.
    #[error("Tardos framework potentials must match canonical nodes")]
    PotentialShape,
    /// No flow satisfies the requested balances and bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual-state construction failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent bound or balance reconstruction failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact reduced-cost or threshold arithmetic exceeded i128.
    #[error("Tardos framework primitive arithmetic overflow")]
    ArithmeticOverflow,
    /// A published theorem precondition or fixed-bound claim is invalid.
    #[error("Tardos framework primitive invariant failed")]
    Invariant,
    /// A supplied trace differs from exact deterministic replay.
    #[error("Tardos framework primitive trace verification failed")]
    TraceVerification,
}

/// Runs one exact network-matrix variable-fixing invocation.
///
/// The caller supplies node labels only; feasibility construction is part of
/// this primitive.  Completion certifies fixed variables, not global
/// minimum-cost optimality.
///
/// # Errors
///
/// Rejects oversized input, infeasibility, a malformed potential vector,
/// arithmetic overflow, or a failed theorem invariant.
pub fn solve_tardos_framework_primitive(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
) -> Result<TardosFrameworkResult, TardosFrameworkError> {
    run_internal(graph, required_divergence, potentials, false).map(|run| run.result)
}

/// Solves while reporting the initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_tardos_framework_primitive_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<TardosFrameworkResult, TardosFrameworkError> {
    run_internal_with_feasibility(graph, required_divergence, potentials, false, feasibility)
        .map(|run| run.result)
}

/// Records feasibility, epsilon measurement, classification, and completion.
///
/// # Errors
///
/// Returns the same failures as [`solve_tardos_framework_primitive`] or a
/// deterministic replay failure.
pub fn trace_tardos_framework_primitive(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
) -> Result<TardosFrameworkTraceResult, TardosFrameworkError> {
    let run = run_internal(graph, required_divergence, potentials, true)?;
    let trace = TardosFrameworkTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_tardos_framework_trace(graph, required_divergence, potentials, &trace)?;
    Ok(trace)
}

/// Traces the Tardos primitive while explicitly publishing its initial
/// feasible-flow construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_tardos_framework_primitive_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<TardosFrameworkTraceResult, TardosFrameworkError> {
    let run =
        run_internal_with_feasibility(graph, required_divergence, potentials, true, feasibility)?;
    let trace = TardosFrameworkTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_tardos_framework_trace(graph, required_divergence, potentials, &trace)?;
    Ok(trace)
}

/// Independently checks the theorem state, fixed bounds, and deterministic replay.
///
/// # Errors
///
/// Rejects malformed snapshots, false fixed-value claims, discontinuities, or
/// disagreement with a fresh source-defined run.
pub fn check_tardos_framework_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    trace: &TardosFrameworkTraceResult,
) -> Result<(), TardosFrameworkError> {
    validate_admission(graph, required_divergence, potentials)?;
    validate_base_snapshot(graph, required_divergence, potentials, &trace.base_snapshot)?;
    let residual_count = trace.final_snapshot.residual_arcs.len();
    let fixed_count = trace.final_snapshot.fixed_variables.len();
    let expected_event_count = TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS
        .checked_add(residual_count)
        .and_then(|value| value.checked_add(fixed_count))
        .ok_or(TardosFrameworkError::ArithmeticOverflow)?;
    if trace.events.len() != expected_event_count {
        return Err(TardosFrameworkError::TraceVerification);
    }
    let mut previous = &trace.base_snapshot;
    for (index, event) in trace.events.iter().enumerate() {
        let (expected_catalog_id, expected_stage) = if index == 0 {
            (
                "tardos-framework.construct-feasible-flow",
                TardosFrameworkStage::ConstructFeasibleFlow,
            )
        } else if index <= residual_count {
            (
                "tardos-framework.scan-residual-arc",
                TardosFrameworkStage::MeasureEpsilon,
            )
        } else if index == residual_count + 1 {
            (
                "tardos-framework.measure-epsilon",
                TardosFrameworkStage::MeasureEpsilon,
            )
        } else if index <= residual_count + fixed_count + 1 {
            (
                "tardos-framework.inspect-fixed-variable",
                TardosFrameworkStage::ClassifyFixedVariables,
            )
        } else if index == residual_count + fixed_count + 2 {
            (
                "tardos-framework.classify-fixed-variables",
                TardosFrameworkStage::ClassifyFixedVariables,
            )
        } else {
            (
                "tardos-framework.complete-primitive",
                TardosFrameworkStage::Complete,
            )
        };
        if &event.before != previous
            || event.catalog_id != expected_catalog_id
            || event.after.stage != expected_stage
        {
            return Err(TardosFrameworkError::TraceVerification);
        }
        previous = &event.after;
    }
    if previous != &trace.final_snapshot
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.result.flows != trace.final_snapshot.flows
        || trace.result.fixed_variables != trace.final_snapshot.fixed_variables
    {
        return Err(TardosFrameworkError::TraceVerification);
    }
    validate_terminal_claim(graph, required_divergence, potentials, &trace.result)?;
    let replay = run_internal(graph, required_divergence, potentials, true)?;
    if replay.base_snapshot != trace.base_snapshot
        || replay.events != trace.events
        || replay.result != trace.result
    {
        return Err(TardosFrameworkError::TraceVerification);
    }
    Ok(())
}

struct InternalRun {
    result: TardosFrameworkResult,
    base_snapshot: TardosFrameworkSnapshot,
    events: Vec<TardosFrameworkTraceEvent>,
}

fn run_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    record_events: bool,
) -> Result<InternalRun, TardosFrameworkError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(
        graph,
        required_divergence,
        potentials,
        record_events,
        &mut feasibility,
    )
}

fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, TardosFrameworkError> {
    validate_admission(graph, required_divergence, potentials)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let base_snapshot = TardosFrameworkSnapshot {
        // Feasibility owns the construction trace. The Tardos source trace
        // therefore begins at the exact flow handed to this primitive instead
        // of replaying a hidden lower-bound state underneath that prefix.
        flows: feasible.flows.clone(),
        potentials: potentials.to_vec(),
        epsilon: 0,
        threshold: 0,
        residual_arcs: Vec::new(),
        fixed_variables: Vec::new(),
        stage: TardosFrameworkStage::Ready,
        metrics: TardosFrameworkMetrics::default(),
    };
    let mut events = Vec::with_capacity(
        TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS.saturating_add(graph.edges().len() * 2),
    );
    let mut current = base_snapshot.clone();
    transition(
        &mut current,
        &mut events,
        record_events,
        "tardos-framework.construct-feasible-flow",
        |next| {
            next.flows.clone_from(&feasible.flows);
            next.stage = TardosFrameworkStage::ConstructFeasibleFlow;
            next.metrics.feasibility_constructions = 1;
            bump_transition(&mut next.metrics)
        },
    )?;

    let priced = price_residual_arcs(graph, &current.flows, potentials)?;
    let epsilon = priced
        .iter()
        .filter_map(|arc| arc.reduced_cost.checked_neg())
        .max()
        .unwrap_or(0)
        .max(0);
    let node_count = i128::try_from(graph.nodes().len())
        .map_err(|_| TardosFrameworkError::ArithmeticOverflow)?;
    let threshold = epsilon
        .checked_mul(node_count)
        .ok_or(TardosFrameworkError::ArithmeticOverflow)?;
    record_priced_residual_arcs(&mut current, &mut events, record_events, &priced)?;
    transition(
        &mut current,
        &mut events,
        record_events,
        "tardos-framework.measure-epsilon",
        |next| {
            next.epsilon = epsilon;
            next.threshold = threshold;
            next.stage = TardosFrameworkStage::MeasureEpsilon;
            bump_transition(&mut next.metrics)
        },
    )?;

    let fixed_variables = classify_fixed_variables(graph, &current.flows, &priced, threshold)?;
    record_fixed_variable_candidates(&mut current, &mut events, record_events, &fixed_variables)?;
    transition(
        &mut current,
        &mut events,
        record_events,
        "tardos-framework.classify-fixed-variables",
        |next| {
            next.metrics.fixed_variables = u64::try_from(next.fixed_variables.len())
                .map_err(|_| TardosFrameworkError::ArithmeticOverflow)?;
            next.stage = TardosFrameworkStage::ClassifyFixedVariables;
            bump_transition(&mut next.metrics)
        },
    )?;
    transition(
        &mut current,
        &mut events,
        record_events,
        "tardos-framework.complete-primitive",
        |next| {
            next.stage = TardosFrameworkStage::Complete;
            bump_transition(&mut next.metrics)
        },
    )?;
    let result = TardosFrameworkResult {
        flows: current.flows.clone(),
        fixed_variables: current.fixed_variables.clone(),
        metrics: current.metrics,
        final_snapshot: current,
    };
    validate_terminal_claim(graph, required_divergence, potentials, &result)?;
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
    })
}

fn record_priced_residual_arcs(
    current: &mut TardosFrameworkSnapshot,
    events: &mut Vec<TardosFrameworkTraceEvent>,
    record_events: bool,
    priced: &[TardosResidualState],
) -> Result<(), TardosFrameworkError> {
    for residual in priced {
        transition(
            current,
            events,
            record_events,
            "tardos-framework.scan-residual-arc",
            |next| {
                next.residual_arcs.push(residual.clone());
                next.epsilon = next
                    .epsilon
                    .max(residual.reduced_cost.checked_neg().unwrap_or(0))
                    .max(0);
                next.metrics.residual_arc_scans = next.residual_arcs.len() as u128;
                next.stage = TardosFrameworkStage::MeasureEpsilon;
                bump_transition(&mut next.metrics)
            },
        )?;
    }
    Ok(())
}

fn record_fixed_variable_candidates(
    current: &mut TardosFrameworkSnapshot,
    events: &mut Vec<TardosFrameworkTraceEvent>,
    record_events: bool,
    fixed_variables: &[TardosFixedVariable],
) -> Result<(), TardosFrameworkError> {
    for fixed in fixed_variables {
        transition(
            current,
            events,
            record_events,
            "tardos-framework.inspect-fixed-variable",
            |next| {
                let residual = next
                    .residual_arcs
                    .iter_mut()
                    .find(|residual| residual.arc == fixed.witness_arc)
                    .ok_or(TardosFrameworkError::Invariant)?;
                residual.fixes_variable = true;
                next.fixed_variables.push(fixed.clone());
                next.stage = TardosFrameworkStage::ClassifyFixedVariables;
                bump_transition(&mut next.metrics)
            },
        )?;
    }
    Ok(())
}

fn transition<F>(
    current: &mut TardosFrameworkSnapshot,
    events: &mut Vec<TardosFrameworkTraceEvent>,
    record: bool,
    catalog_id: &'static str,
    update: F,
) -> Result<(), TardosFrameworkError>
where
    F: FnOnce(&mut TardosFrameworkSnapshot) -> Result<(), TardosFrameworkError>,
{
    let before = current.clone();
    update(current)?;
    if record {
        events.push(TardosFrameworkTraceEvent {
            catalog_id,
            before,
            after: current.clone(),
        });
    }
    Ok(())
}

fn bump_transition(metrics: &mut TardosFrameworkMetrics) -> Result<(), TardosFrameworkError> {
    metrics.state_transitions = metrics
        .state_transitions
        .checked_add(1)
        .ok_or(TardosFrameworkError::ArithmeticOverflow)?;
    Ok(())
}

fn price_residual_arcs(
    graph: &FlowNetwork,
    flows: &[u64],
    potentials: &[i128],
) -> Result<Vec<TardosResidualState>, TardosFrameworkError> {
    let state = ResidualState::from_flows(graph, flows)?;
    let mut priced = Vec::with_capacity(graph.edges().len().saturating_mul(2));
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            let reduced_cost = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(TardosFrameworkError::ArithmeticOverflow)?;
            priced.push(TardosResidualState {
                arc: arc.id,
                capacity: arc.capacity,
                reduced_cost,
                fixes_variable: false,
            });
        }
    }
    priced.sort_unstable_by(|left, right| left.arc.cmp(&right.arc));
    if priced.windows(2).any(|pair| pair[0].arc == pair[1].arc) {
        return Err(TardosFrameworkError::Invariant);
    }
    Ok(priced)
}

fn classify_fixed_variables(
    graph: &FlowNetwork,
    flows: &[u64],
    priced: &[TardosResidualState],
    threshold: i128,
) -> Result<Vec<TardosFixedVariable>, TardosFrameworkError> {
    let mut fixed = Vec::new();
    for residual in priced
        .iter()
        .filter(|residual| residual.reduced_cost > threshold)
    {
        let edge_index = graph
            .edge_index(residual.arc.original_edge())
            .ok_or(TardosFrameworkError::Invariant)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(TardosFrameworkError::Invariant)?;
        let flow = flows[edge_index.as_usize()];
        let bound = match residual.arc.direction() {
            ResidualDirection::Forward if flow == edge.lower() => TardosFixedBound::Lower,
            ResidualDirection::Reverse if flow == edge.capacity() => TardosFixedBound::Upper,
            ResidualDirection::Forward | ResidualDirection::Reverse => {
                return Err(TardosFrameworkError::Invariant);
            }
        };
        fixed.push(TardosFixedVariable {
            edge: edge.id().clone(),
            bound,
            value: flow,
            witness_arc: residual.arc.clone(),
            reduced_cost: residual.reduced_cost,
        });
    }
    if fixed.windows(2).any(|pair| pair[0].edge == pair[1].edge) {
        return Err(TardosFrameworkError::Invariant);
    }
    Ok(fixed)
}

fn validate_admission(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
) -> Result<(), TardosFrameworkError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > TARDOS_FRAMEWORK_MAX_NODES
        || graph.edges().len() > TARDOS_FRAMEWORK_MAX_EDGES
    {
        return Err(TardosFrameworkError::AdmissionLimit);
    }
    if potentials.len() != graph.nodes().len() {
        return Err(TardosFrameworkError::PotentialShape);
    }
    if required_divergence.len() != graph.nodes().len()
        || required_divergence
            .iter()
            .try_fold(0_i128, |sum, value| sum.checked_add(*value))
            != Some(0)
    {
        return Err(TardosFrameworkError::Invariant);
    }
    Ok(())
}

fn validate_base_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    snapshot: &TardosFrameworkSnapshot,
) -> Result<(), TardosFrameworkError> {
    let feasible = ResidualState::from_flows(graph, &snapshot.flows).is_ok()
        && divergences(graph, &snapshot.flows)? == required_divergence;
    if snapshot.stage != TardosFrameworkStage::Ready
        || !feasible
        || snapshot.potentials != potentials
        || snapshot.epsilon != 0
        || snapshot.threshold != 0
        || !snapshot.residual_arcs.is_empty()
        || !snapshot.fixed_variables.is_empty()
        || snapshot.metrics != TardosFrameworkMetrics::default()
    {
        return Err(TardosFrameworkError::TraceVerification);
    }
    Ok(())
}

fn validate_terminal_claim(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    result: &TardosFrameworkResult,
) -> Result<(), TardosFrameworkError> {
    let snapshot = &result.final_snapshot;
    if snapshot.stage != TardosFrameworkStage::Complete
        || snapshot.flows != result.flows
        || snapshot.potentials != potentials
        || snapshot.fixed_variables != result.fixed_variables
        || snapshot.metrics != result.metrics
        || snapshot.metrics.feasibility_constructions != 1
        || usize::try_from(snapshot.metrics.state_transitions)
            .ok()
            .is_none_or(|count| {
                count
                    != TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS
                        .saturating_add(snapshot.residual_arcs.len())
                        .saturating_add(snapshot.fixed_variables.len())
            })
    {
        return Err(TardosFrameworkError::Invariant);
    }
    let actual = divergences(graph, &result.flows)?;
    if actual != required_divergence {
        return Err(TardosFrameworkError::Invariant);
    }
    let priced = price_residual_arcs(graph, &result.flows, potentials)?;
    let epsilon = priced
        .iter()
        .filter_map(|arc| arc.reduced_cost.checked_neg())
        .max()
        .unwrap_or(0)
        .max(0);
    let threshold = epsilon
        .checked_mul(
            i128::try_from(graph.nodes().len())
                .map_err(|_| TardosFrameworkError::ArithmeticOverflow)?,
        )
        .ok_or(TardosFrameworkError::ArithmeticOverflow)?;
    let fixed = classify_fixed_variables(graph, &result.flows, &priced, threshold)?;
    let mut classified = priced;
    for residual in &mut classified {
        residual.fixes_variable = residual.reduced_cost > threshold;
        if residual.reduced_cost < -epsilon {
            return Err(TardosFrameworkError::Invariant);
        }
    }
    if snapshot.epsilon != epsilon
        || snapshot.threshold != threshold
        || snapshot.residual_arcs != classified
        || snapshot.fixed_variables != fixed
        || snapshot.metrics.residual_arc_scans != snapshot.residual_arcs.len() as u128
        || snapshot.metrics.fixed_variables != snapshot.fixed_variables.len() as u64
    {
        return Err(TardosFrameworkError::Invariant);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn edge(
        id: &str,
        from: &str,
        to: &str,
        lower: u64,
        capacity: u64,
        cost: i64,
    ) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower,
            capacity,
            cost,
        }
    }

    fn transshipment_graph() -> FlowNetwork {
        FlowNetwork::new(
            [("s", 2), ("a", 0), ("t", -2)]
                .into_iter()
                .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), supply))
                .collect(),
            vec![
                edge("cheap-1", "s", "a", 0, 2, 1),
                edge("cheap-2", "a", "t", 0, 2, 1),
                edge("expensive", "s", "t", 0, 2, 20),
            ],
        )
        .expect("graph")
    }

    #[test]
    fn fixes_an_expensive_unused_arc_at_its_lower_bound() {
        let graph = transshipment_graph();
        let result =
            solve_tardos_framework_primitive(&graph, &[0, 2, -2], &[0, 0, 0]).expect("primitive");

        assert_eq!(result.flows, vec![2, 2, 0]);
        assert_eq!(result.final_snapshot.epsilon, 1);
        assert_eq!(result.final_snapshot.threshold, 3);
        assert_eq!(result.fixed_variables.len(), 1);
        assert_eq!(result.fixed_variables[0].edge.as_str(), "expensive");
        assert_eq!(result.fixed_variables[0].bound, TardosFixedBound::Lower);
    }

    #[test]
    fn negative_self_loop_is_fixed_at_its_upper_bound() {
        let node = NodeId::parse("v").expect("node");
        let graph = FlowNetwork::new(
            vec![FlowNode::new(node.clone(), 0)],
            vec![edge("reward", "v", "v", 0, 4, -7)],
        )
        .expect("graph");
        // The feasibility constructor starts at the lower bound, so the zero
        // label theorem sees the negative forward arc and has epsilon 7.  A
        // different feasible bound state is required for an upper witness;
        // verify the classifier independently on that exact state.
        let priced = price_residual_arcs(&graph, &[4], &[0]).expect("priced");
        let fixed = classify_fixed_variables(&graph, &[4], &priced, 0).expect("fixed");
        assert_eq!(fixed.len(), 1);
        assert_eq!(fixed[0].bound, TardosFixedBound::Upper);
        assert_eq!(fixed[0].value, 4);
    }

    #[test]
    fn trace_replays_and_rejects_corruption() {
        let graph = transshipment_graph();
        let trace =
            trace_tardos_framework_primitive(&graph, &[0, 2, -2], &[0, 0, 0]).expect("trace");
        assert_eq!(
            trace.events.len(),
            TARDOS_FRAMEWORK_FIXED_TRACE_EVENTS
                + trace.final_snapshot.residual_arcs.len()
                + trace.final_snapshot.fixed_variables.len()
        );
        check_tardos_framework_trace(&graph, &[0, 2, -2], &[0, 0, 0], &trace).expect("replay");

        let mut corrupt = trace;
        corrupt.final_snapshot.threshold += 1;
        assert_eq!(
            check_tardos_framework_trace(&graph, &[0, 2, -2], &[0, 0, 0], &corrupt),
            Err(TardosFrameworkError::TraceVerification)
        );
    }

    #[test]
    fn fixed_claim_holds_for_every_enumerated_optimum() {
        let graph = transshipment_graph();
        let result =
            solve_tardos_framework_primitive(&graph, &[0, 2, -2], &[0, 0, 0]).expect("primitive");
        let mut optimum = i128::MAX;
        let mut optimal_flows = Vec::new();
        for first in 0..=2_u64 {
            for second in 0..=2_u64 {
                for direct in 0..=2_u64 {
                    let flows = vec![first, second, direct];
                    if divergences(&graph, &flows).ok().as_deref() != Some(&[0, 2, -2]) {
                        continue;
                    }
                    let cost = i128::from(first + second) + 20 * i128::from(direct);
                    match cost.cmp(&optimum) {
                        std::cmp::Ordering::Less => {
                            optimum = cost;
                            optimal_flows = vec![flows];
                        }
                        std::cmp::Ordering::Equal => optimal_flows.push(flows),
                        std::cmp::Ordering::Greater => {}
                    }
                }
            }
        }
        assert!(!optimal_flows.is_empty());
        for fixed in &result.fixed_variables {
            let index = graph.edge_index(&fixed.edge).expect("edge").as_usize();
            assert!(
                optimal_flows
                    .iter()
                    .all(|flows| flows[index] == fixed.value)
            );
        }
    }

    #[test]
    fn potential_shape_and_admission_are_closed() {
        let graph = transshipment_graph();
        assert_eq!(
            solve_tardos_framework_primitive(&graph, &[0, 2, -2], &[0, 0]),
            Err(TardosFrameworkError::PotentialShape)
        );
    }
}
