//! Exact bounded static-graph slice of Algorithm 2's update schedule.
//!
//! This module composes the explicit shifted tree chain, both `FindCycle`
//! candidate classes, and the largest-eligible-level shift rule from Algorithm
//! 2 of van den Brand et al. (arXiv:2309.16629v1). Starting from one fixed
//! graph, it repeatedly queries; a candidate is accepted exactly when its
//! absolute ratio is greater than `kappa_alpha`. Otherwise it shifts the
//! largest level whose pass count is below `2 * psi`, increments a pass on
//! branch wrap, resets deeper pass counts, and queries again. On acceptance it
//! computes `beta = eta / <g, Delta>` and applies `f <- f - beta Delta` from
//! the source interface.
//!
//! Dynamic graph updates, periodic rebuilds triggered by update counts, hidden
//! witness isolation, link-cut-tree maintenance, and IPM integration are not
//! part of this fixed-topology slice.

use num_rational::BigRational;
use num_traits::Zero;
use thiserror::Error;

use super::{
    ShiftedTreeChainConfig, ShiftedTreeChainCycleCandidate, ShiftedTreeChainError,
    ShiftedTreeChainGraph, ShiftedTreeChainOperation, ShiftedTreeChainQueryError,
    ShiftedTreeChainQueryMetrics, ShiftedTreeChainSnapshot, execute_shifted_tree_chain,
    find_shifted_tree_chain_cycle, trace_shifted_tree_chain, trace_shifted_tree_chain_cycle_query,
};

/// Maximum shifts in one fixed-topology update.
pub const SHIFTED_TREE_CHAIN_UPDATE_MAX_SHIFTS: usize = 1_024;
/// Maximum public boundaries: query/shift pairs, flow, completion.
pub const SHIFTED_TREE_CHAIN_UPDATE_MAX_TRACE_EVENTS: usize = 2_051;
/// Maximum exact scalar width produced by normalization.
pub const SHIFTED_TREE_CHAIN_UPDATE_MAX_RATIONAL_BITS: u64 = 512;
/// Maximum explicit pass-range parameter in the bounded realization.
pub const SHIFTED_TREE_CHAIN_UPDATE_MAX_PSI: u64 = 16;

const CATALOG_ID: &str = "shifted-tree-chain-update";

/// Fixed-topology Algorithm 2 parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateConfig {
    /// Explicit tree-chain depth and branch count.
    pub chain: ShiftedTreeChainConfig,
    /// Explicit bounded replacement for the paper's polylogarithmic range.
    pub psi: u64,
    /// Exact `kappa * alpha` acceptance threshold.
    pub kappa_alpha: BigRational,
    /// Exact positive progress target.
    pub eta: BigRational,
}

/// Exact aggregate work for one update call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateMetrics {
    /// `FindCycle` calls.
    pub queries: u64,
    /// Shift calls.
    pub shifts: u64,
    /// Shift calls by nonterminal level.
    pub level_shifts: Vec<u64>,
    /// Intermediate core edges inspected across all queries.
    pub intermediate_edge_inspections: u64,
    /// Terminal tree/edge pairs inspected across all queries.
    pub terminal_edge_inspections: u64,
    /// Nonzero normalized coordinates applied to flow.
    pub flow_coordinate_updates: u64,
    /// Reversible public transitions.
    pub state_transitions: u64,
}

/// Complete state at one composed boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateSnapshot {
    /// Current independently checkable tree chain.
    pub chain: ShiftedTreeChainSnapshot,
    /// Applied source-level lifecycle operations.
    pub chain_operations: Vec<ShiftedTreeChainOperation>,
    /// Completed full branch passes by nonterminal level.
    pub passes: Vec<u64>,
    /// Accepted source candidate, if the query loop completed.
    pub accepted_candidate: Option<ShiftedTreeChainCycleCandidate>,
    /// Exact maintained flow after this update, initialized at zero.
    pub flow: Vec<BigRational>,
    /// Whether completion was emitted.
    pub complete: bool,
    /// Exact aggregate work.
    pub metrics: ShiftedTreeChainUpdateMetrics,
}

/// Exact certificate for one query decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainQueryDecision {
    /// Best candidate returned by `FindCycle`, if any.
    pub candidate: Option<ShiftedTreeChainCycleCandidate>,
    /// Exact fixed acceptance threshold.
    pub threshold: BigRational,
    /// Whether the source strict-goodness test accepted the candidate.
    pub accepted: bool,
    /// Exact work for this query.
    pub query_metrics: ShiftedTreeChainQueryMetrics,
}

/// Exact certificate for the final flow application.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainFlowApplication {
    /// Exact `<g, Delta>`, strictly negative.
    pub gradient_dot: BigRational,
    /// Exact `eta / <g, Delta>`.
    pub beta: BigRational,
    /// Exact normalized vector subtracted from flow.
    pub normalized_delta: Vec<BigRational>,
    /// Exact flow after subtraction.
    pub flow: Vec<BigRational>,
}

/// Source meaning of one composed boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShiftedTreeChainUpdateEventKind {
    /// One `FindCycle` result was compared with `kappa * alpha`.
    CycleQueried {
        /// Exact query-decision certificate.
        decision: Box<ShiftedTreeChainQueryDecision>,
    },
    /// The largest eligible nonterminal level was shifted.
    LevelShifted {
        /// Shifted level.
        level: usize,
        /// Active branch before the shift.
        previous_branch: usize,
        /// Active branch after the shift.
        next_branch: usize,
        /// Whether a complete branch pass finished.
        wrapped: bool,
    },
    /// The accepted cycle was normalized and applied to flow.
    FlowApplied {
        /// Exact flow-application certificate.
        application: Box<ShiftedTreeChainFlowApplication>,
    },
    /// The fixed-topology update completed.
    Completed,
}

/// One fully reversible composed event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level event meaning.
    pub kind: ShiftedTreeChainUpdateEventKind,
    /// State before the event.
    pub before: ShiftedTreeChainUpdateSnapshot,
    /// State after the event.
    pub after: ShiftedTreeChainUpdateSnapshot,
}

/// Exact bounded update result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateResult {
    /// Accepted candidate.
    pub candidate: ShiftedTreeChainCycleCandidate,
    /// Exact normalized vector subtracted from zero flow.
    pub normalized_delta: Vec<BigRational>,
    /// Exact final maintained flow.
    pub flow: Vec<BigRational>,
    /// Terminal composed state.
    pub final_snapshot: ShiftedTreeChainUpdateSnapshot,
}

/// Complete reversible update transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftedTreeChainUpdateTraceResult {
    /// Initial tree-chain/zero-flow state.
    pub base_snapshot: ShiftedTreeChainUpdateSnapshot,
    /// Query/shift boundaries, flow application, then completion.
    pub events: Vec<ShiftedTreeChainUpdateTraceEvent>,
    /// Exact result.
    pub result: ShiftedTreeChainUpdateResult,
}

/// Explicit bounded-composition failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ShiftedTreeChainUpdateError {
    /// The fixed update parameters are invalid.
    #[error("shifted tree chain update input is invalid")]
    InvalidInput,
    /// The fixed update exceeds the published bounded realization.
    #[error("shifted tree chain update exceeds its admission band")]
    AdmissionLimit,
    /// Every level exhausted its `2 * psi` pass budget without a good cycle.
    #[error("shifted tree chain update strategy exhausted")]
    StrategyExhausted,
    /// A component tree-chain operation failed.
    #[error(transparent)]
    TreeChain(#[from] ShiftedTreeChainError),
    /// A component `FindCycle` operation failed.
    #[error(transparent)]
    Query(#[from] ShiftedTreeChainQueryError),
    /// Exact work accounting overflowed.
    #[error("shifted tree chain update arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact compositional replay.
    #[error("shifted tree chain update trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: ShiftedTreeChainUpdateSnapshot,
    events: Vec<ShiftedTreeChainUpdateTraceEvent>,
    result: ShiftedTreeChainUpdateResult,
}

type AuditQueryOutcome = (
    ShiftedTreeChainUpdateSnapshot,
    usize,
    Option<ShiftedTreeChainCycleCandidate>,
);

type QueryOutcome = (
    ShiftedTreeChainUpdateEventKind,
    Option<ShiftedTreeChainCycleCandidate>,
    bool,
);

/// Runs the fixed-topology Algorithm 2 update without recording events.
///
/// # Errors
///
/// Rejects invalid/admission-exceeding input, component failures, exact
/// arithmetic overflow, or exhaustion of every source pass budget.
pub fn execute_shifted_tree_chain_update(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
) -> Result<ShiftedTreeChainUpdateResult, ShiftedTreeChainUpdateError> {
    run_internal(graph, config, false).map(|run| run.result)
}

/// Records each query, shift, flow application, and completion boundary.
///
/// # Errors
///
/// Returns an execution failure or compositional replay-checker failure.
pub fn trace_shifted_tree_chain_update(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
) -> Result<ShiftedTreeChainUpdateTraceResult, ShiftedTreeChainUpdateError> {
    let run = run_internal(graph, config, true)?;
    let trace = ShiftedTreeChainUpdateTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_shifted_tree_chain_update_trace(graph, config, &trace)?;
    Ok(trace)
}

/// Checks the wrapper by composing the independent tree-chain/query checkers.
///
/// The wrapper runner is never called. Each component transcript is rebuilt
/// and independently checked; pass scheduling and flow normalization are then
/// recomputed locally.
///
/// # Errors
///
/// Rejects component-invalid input or any wrapper event/state drift.
pub fn check_shifted_tree_chain_update_trace(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    trace: &ShiftedTreeChainUpdateTraceResult,
) -> Result<(), ShiftedTreeChainUpdateError> {
    validate_config(config)?;
    let base_chain_trace = trace_shifted_tree_chain(graph, config.chain, &[])?;
    let mut snapshot = initial_snapshot(graph, config, base_chain_trace.result.final_snapshot);
    if trace.base_snapshot != snapshot {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let mut event_index = 0;
    let candidate = loop {
        let (queried, next_event, accepted) =
            audit_query_boundary(graph, config, trace, &snapshot, event_index)?;
        snapshot = queried;
        event_index = next_event;
        if let Some(candidate) = accepted {
            break candidate;
        }
        (snapshot, event_index) =
            audit_shift_boundary(graph, config, trace, &snapshot, event_index)?;
    };
    audit_flow_and_completion(graph, config, trace, &snapshot, event_index, &candidate)
}

fn audit_query_boundary(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    trace: &ShiftedTreeChainUpdateTraceResult,
    snapshot: &ShiftedTreeChainUpdateSnapshot,
    event_index: usize,
) -> Result<AuditQueryOutcome, ShiftedTreeChainUpdateError> {
    let event = audit_event(trace, snapshot, event_index)?;
    let query_trace = trace_shifted_tree_chain_cycle_query(graph, config.chain, &snapshot.chain)?;
    let candidate = query_trace.result.best_candidate.clone();
    let accepted = candidate
        .as_ref()
        .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
    let mut after = snapshot.clone();
    add_query_metrics(
        &mut after.metrics,
        query_trace.result.final_snapshot.metrics,
    )?;
    if accepted {
        after.accepted_candidate.clone_from(&candidate);
    }
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = ShiftedTreeChainUpdateEventKind::CycleQueried {
        decision: Box::new(ShiftedTreeChainQueryDecision {
            candidate: candidate.clone(),
            threshold: config.kappa_alpha.clone(),
            accepted,
            query_metrics: query_trace.result.final_snapshot.metrics,
        }),
    };
    if event.kind != kind || event.after != after {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let accepted_candidate = accepted.then_some(candidate).flatten();
    Ok((after, audit_next_event(event_index)?, accepted_candidate))
}

fn audit_shift_boundary(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    trace: &ShiftedTreeChainUpdateTraceResult,
    snapshot: &ShiftedTreeChainUpdateSnapshot,
    event_index: usize,
) -> Result<(ShiftedTreeChainUpdateSnapshot, usize), ShiftedTreeChainUpdateError> {
    let event = audit_event(trace, snapshot, event_index)?;
    let level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)?;
    let previous = snapshot.chain.levels[level].active_branch;
    let mut operations = snapshot.chain_operations.clone();
    operations.push(ShiftedTreeChainOperation::Shift { level });
    let chain_trace = trace_shifted_tree_chain(graph, config.chain, &operations)?;
    let next = chain_trace.result.final_snapshot.levels[level].active_branch;
    let wrapped = next == 0;
    let mut after = snapshot.clone();
    after.chain = chain_trace.result.final_snapshot;
    after.chain_operations = operations;
    update_passes(&mut after.passes, level, wrapped)?;
    after.metrics.shifts = audit_increment(after.metrics.shifts)?;
    after.metrics.level_shifts[level] = audit_increment(after.metrics.level_shifts[level])?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = ShiftedTreeChainUpdateEventKind::LevelShifted {
        level,
        previous_branch: previous,
        next_branch: next,
        wrapped,
    };
    if event.kind != kind || event.after != after {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    Ok((after, audit_next_event(event_index)?))
}

fn audit_flow_and_completion(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    trace: &ShiftedTreeChainUpdateTraceResult,
    snapshot: &ShiftedTreeChainUpdateSnapshot,
    event_index: usize,
    candidate: &ShiftedTreeChainCycleCandidate,
) -> Result<(), ShiftedTreeChainUpdateError> {
    let flow_event = trace
        .events
        .get(event_index)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)?;
    if flow_event.catalog_id != CATALOG_ID || &flow_event.before != snapshot {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let application = audit_flow_application(graph, config, candidate)?;
    let mut flowed = snapshot.clone();
    flowed.flow.clone_from(&application.flow);
    flowed.metrics.flow_coordinate_updates = u64::try_from(
        application
            .normalized_delta
            .iter()
            .filter(|value| !value.is_zero())
            .count(),
    )
    .map_err(|_| ShiftedTreeChainUpdateError::TraceVerification)?;
    flowed.metrics.state_transitions = audit_increment(flowed.metrics.state_transitions)?;
    let flow_kind = ShiftedTreeChainUpdateEventKind::FlowApplied {
        application: Box::new(application.clone()),
    };
    if flow_event.kind != flow_kind || flow_event.after != flowed {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let event_index = audit_next_event(event_index)?;
    let completion = trace
        .events
        .get(event_index)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)?;
    let mut final_snapshot = flowed.clone();
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    if event_index + 1 != trace.events.len()
        || completion.catalog_id != CATALOG_ID
        || completion.before != flowed
        || completion.kind != ShiftedTreeChainUpdateEventKind::Completed
        || completion.after != final_snapshot
        || &trace.result.candidate != candidate
        || trace.result.normalized_delta != application.normalized_delta
        || trace.result.flow != application.flow
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    Ok(())
}

fn audit_event<'a>(
    trace: &'a ShiftedTreeChainUpdateTraceResult,
    snapshot: &ShiftedTreeChainUpdateSnapshot,
    event_index: usize,
) -> Result<&'a ShiftedTreeChainUpdateTraceEvent, ShiftedTreeChainUpdateError> {
    let event = trace
        .events
        .get(event_index)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)?;
    if event.catalog_id != CATALOG_ID || &event.before != snapshot {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    Ok(event)
}

fn audit_next_event(event_index: usize) -> Result<usize, ShiftedTreeChainUpdateError> {
    event_index
        .checked_add(1)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)
}

fn run_internal(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    record: bool,
) -> Result<InternalRun, ShiftedTreeChainUpdateError> {
    validate_config(config)?;
    let chain = execute_shifted_tree_chain(graph, config.chain, &[])?.final_snapshot;
    let mut snapshot = initial_snapshot(graph, config, chain);
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    let candidate = loop {
        let before = snapshot.clone();
        let (kind, candidate, accepted) = apply_query(graph, config, &mut snapshot)?;
        if record {
            push_event(&mut events, kind, before, &snapshot)?;
        }
        if accepted {
            break candidate.ok_or(ShiftedTreeChainUpdateError::StrategyExhausted)?;
        }
        let before = snapshot.clone();
        let kind = apply_shift(graph, config, &mut snapshot)?;
        if record {
            push_event(&mut events, kind, before, &snapshot)?;
        }
    };
    let before = snapshot.clone();
    let application = flow_application(graph, config, &candidate)?;
    snapshot.flow.clone_from(&application.flow);
    snapshot.metrics.flow_coordinate_updates = u64::try_from(
        application
            .normalized_delta
            .iter()
            .filter(|value| !value.is_zero())
            .count(),
    )
    .map_err(|_| ShiftedTreeChainUpdateError::ArithmeticOverflow)?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        push_event(
            &mut events,
            ShiftedTreeChainUpdateEventKind::FlowApplied {
                application: Box::new(application.clone()),
            },
            before,
            &snapshot,
        )?;
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        push_event(
            &mut events,
            ShiftedTreeChainUpdateEventKind::Completed,
            before,
            &snapshot,
        )?;
    }
    Ok(InternalRun {
        base_snapshot,
        events,
        result: ShiftedTreeChainUpdateResult {
            candidate,
            normalized_delta: application.normalized_delta,
            flow: application.flow,
            final_snapshot: snapshot,
        },
    })
}

fn apply_query(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    snapshot: &mut ShiftedTreeChainUpdateSnapshot,
) -> Result<QueryOutcome, ShiftedTreeChainUpdateError> {
    let query = find_shifted_tree_chain_cycle(graph, config.chain, &snapshot.chain)?;
    let candidate = query.best_candidate;
    let accepted = candidate
        .as_ref()
        .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
    add_query_metrics(&mut snapshot.metrics, query.final_snapshot.metrics)?;
    if accepted {
        snapshot.accepted_candidate.clone_from(&candidate);
    }
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    let kind = ShiftedTreeChainUpdateEventKind::CycleQueried {
        decision: Box::new(ShiftedTreeChainQueryDecision {
            candidate: candidate.clone(),
            threshold: config.kappa_alpha.clone(),
            accepted,
            query_metrics: query.final_snapshot.metrics,
        }),
    };
    Ok((kind, candidate, accepted))
}

fn apply_shift(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    snapshot: &mut ShiftedTreeChainUpdateSnapshot,
) -> Result<ShiftedTreeChainUpdateEventKind, ShiftedTreeChainUpdateError> {
    if snapshot.chain_operations.len() >= SHIFTED_TREE_CHAIN_UPDATE_MAX_SHIFTS {
        return Err(ShiftedTreeChainUpdateError::AdmissionLimit);
    }
    let level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(ShiftedTreeChainUpdateError::StrategyExhausted)?;
    let previous = snapshot.chain.levels[level].active_branch;
    snapshot
        .chain_operations
        .push(ShiftedTreeChainOperation::Shift { level });
    snapshot.chain =
        execute_shifted_tree_chain(graph, config.chain, &snapshot.chain_operations)?.final_snapshot;
    let next = snapshot.chain.levels[level].active_branch;
    let wrapped = next == 0;
    update_passes(&mut snapshot.passes, level, wrapped)?;
    snapshot.metrics.shifts = increment(snapshot.metrics.shifts)?;
    snapshot.metrics.level_shifts[level] = increment(snapshot.metrics.level_shifts[level])?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    Ok(ShiftedTreeChainUpdateEventKind::LevelShifted {
        level,
        previous_branch: previous,
        next_branch: next,
        wrapped,
    })
}

fn initial_snapshot(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    chain: ShiftedTreeChainSnapshot,
) -> ShiftedTreeChainUpdateSnapshot {
    ShiftedTreeChainUpdateSnapshot {
        chain,
        chain_operations: Vec::new(),
        passes: vec![0; config.chain.depth],
        accepted_candidate: None,
        flow: vec![BigRational::zero(); graph.edges.len()],
        complete: false,
        metrics: ShiftedTreeChainUpdateMetrics {
            queries: 0,
            shifts: 0,
            level_shifts: vec![0; config.chain.depth],
            intermediate_edge_inspections: 0,
            terminal_edge_inspections: 0,
            flow_coordinate_updates: 0,
            state_transitions: 0,
        },
    }
}

fn largest_eligible_level(passes: &[u64], psi: u64) -> Option<usize> {
    let ceiling = psi.checked_mul(2)?;
    passes.iter().rposition(|&passes| passes < ceiling)
}

fn update_passes(
    passes: &mut [u64],
    level: usize,
    wrapped: bool,
) -> Result<(), ShiftedTreeChainUpdateError> {
    if wrapped {
        passes[level] = increment(passes[level])?;
    }
    for pass in passes
        .get_mut(level + 1..)
        .ok_or(ShiftedTreeChainUpdateError::ArithmeticOverflow)?
    {
        *pass = 0;
    }
    Ok(())
}

fn add_query_metrics(
    total: &mut ShiftedTreeChainUpdateMetrics,
    query: ShiftedTreeChainQueryMetrics,
) -> Result<(), ShiftedTreeChainUpdateError> {
    total.queries = increment(total.queries)?;
    total.intermediate_edge_inspections = total
        .intermediate_edge_inspections
        .checked_add(query.intermediate_edge_inspections)
        .ok_or(ShiftedTreeChainUpdateError::ArithmeticOverflow)?;
    total.terminal_edge_inspections = total
        .terminal_edge_inspections
        .checked_add(query.terminal_edge_inspections)
        .ok_or(ShiftedTreeChainUpdateError::ArithmeticOverflow)?;
    Ok(())
}

fn flow_application(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    candidate: &ShiftedTreeChainCycleCandidate,
) -> Result<ShiftedTreeChainFlowApplication, ShiftedTreeChainUpdateError> {
    if candidate.gradient >= BigRational::zero()
        || candidate.coefficients.len() != graph.edges.len()
    {
        return Err(ShiftedTreeChainUpdateError::InvalidInput);
    }
    let beta = &config.eta / &candidate.gradient;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    let flow = normalized_delta
        .iter()
        .map(|value| -value)
        .collect::<Vec<_>>();
    if normalized_delta.iter().any(rational_too_wide) || flow.iter().any(rational_too_wide) {
        return Err(ShiftedTreeChainUpdateError::AdmissionLimit);
    }
    Ok(ShiftedTreeChainFlowApplication {
        gradient_dot: candidate.gradient.clone(),
        beta,
        normalized_delta,
        flow,
    })
}

fn audit_flow_application(
    graph: &ShiftedTreeChainGraph,
    config: &ShiftedTreeChainUpdateConfig,
    candidate: &ShiftedTreeChainCycleCandidate,
) -> Result<ShiftedTreeChainFlowApplication, ShiftedTreeChainUpdateError> {
    if candidate.coefficients.len() != graph.edges.len() {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let recomputed_dot = graph
        .edges
        .iter()
        .zip(&candidate.coefficients)
        .map(|(edge, coefficient)| &edge.gradient * coefficient)
        .sum::<BigRational>();
    if recomputed_dot != candidate.gradient || recomputed_dot >= BigRational::zero() {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let beta = &config.eta / &recomputed_dot;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    let flow = normalized_delta
        .iter()
        .map(|value| -value)
        .collect::<Vec<_>>();
    if normalized_delta.iter().any(rational_too_wide) || flow.iter().any(rational_too_wide) {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    let mut divergence = vec![BigRational::zero(); graph.node_count];
    for (edge, value) in graph.edges.iter().zip(&flow) {
        divergence[edge.from] += value;
        divergence[edge.to] -= value;
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(ShiftedTreeChainUpdateError::TraceVerification);
    }
    Ok(ShiftedTreeChainFlowApplication {
        gradient_dot: recomputed_dot,
        beta,
        normalized_delta,
        flow,
    })
}

fn validate_config(
    config: &ShiftedTreeChainUpdateConfig,
) -> Result<(), ShiftedTreeChainUpdateError> {
    if config.chain.depth == 0
        || config.psi == 0
        || config.kappa_alpha <= BigRational::zero()
        || config.eta <= BigRational::zero()
    {
        return Err(ShiftedTreeChainUpdateError::InvalidInput);
    }
    if config.psi > SHIFTED_TREE_CHAIN_UPDATE_MAX_PSI
        || rational_too_wide(&config.kappa_alpha)
        || rational_too_wide(&config.eta)
    {
        return Err(ShiftedTreeChainUpdateError::AdmissionLimit);
    }
    let maximum_shifts = usize::try_from(config.psi)
        .ok()
        .and_then(|psi| psi.checked_mul(2))
        .and_then(|passes| passes.checked_mul(config.chain.branches))
        .and_then(|per_level| per_level.checked_mul(config.chain.depth))
        .ok_or(ShiftedTreeChainUpdateError::AdmissionLimit)?;
    if maximum_shifts > SHIFTED_TREE_CHAIN_UPDATE_MAX_SHIFTS {
        return Err(ShiftedTreeChainUpdateError::AdmissionLimit);
    }
    Ok(())
}

fn push_event(
    events: &mut Vec<ShiftedTreeChainUpdateTraceEvent>,
    kind: ShiftedTreeChainUpdateEventKind,
    before: ShiftedTreeChainUpdateSnapshot,
    after: &ShiftedTreeChainUpdateSnapshot,
) -> Result<(), ShiftedTreeChainUpdateError> {
    if events.len() >= SHIFTED_TREE_CHAIN_UPDATE_MAX_TRACE_EVENTS {
        return Err(ShiftedTreeChainUpdateError::AdmissionLimit);
    }
    events.push(ShiftedTreeChainUpdateTraceEvent {
        catalog_id: CATALOG_ID,
        kind,
        before,
        after: after.clone(),
    });
    Ok(())
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > SHIFTED_TREE_CHAIN_UPDATE_MAX_RATIONAL_BITS
        || value.denom().bits() > SHIFTED_TREE_CHAIN_UPDATE_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, ShiftedTreeChainUpdateError> {
    value
        .checked_add(1)
        .ok_or(ShiftedTreeChainUpdateError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, ShiftedTreeChainUpdateError> {
    value
        .checked_add(1)
        .ok_or(ShiftedTreeChainUpdateError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::{ShiftedTreeChainCycleSource, ShiftedTreeChainEdge};

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn triangle() -> ShiftedTreeChainGraph {
        ShiftedTreeChainGraph {
            node_count: 3,
            edges: vec![
                ShiftedTreeChainEdge {
                    source_edge: 0,
                    from: 0,
                    to: 1,
                    length: rational(1, 1),
                    gradient: rational(2, 1),
                },
                ShiftedTreeChainEdge {
                    source_edge: 1,
                    from: 1,
                    to: 2,
                    length: rational(2, 1),
                    gradient: rational(-1, 1),
                },
                ShiftedTreeChainEdge {
                    source_edge: 2,
                    from: 2,
                    to: 0,
                    length: rational(3, 1),
                    gradient: rational(4, 1),
                },
            ],
        }
    }

    fn config(threshold: BigRational) -> ShiftedTreeChainUpdateConfig {
        ShiftedTreeChainUpdateConfig {
            chain: ShiftedTreeChainConfig {
                depth: 2,
                branches: 2,
            },
            psi: 1,
            kappa_alpha: threshold,
            eta: rational(1, 1),
        }
    }

    #[test]
    fn accepts_initial_terminal_tree_cycle_and_applies_exact_source_normalization() {
        let result = execute_shifted_tree_chain_update(&triangle(), &config(rational(1, 2)))
            .expect("update");
        assert_eq!(
            result.candidate.source,
            ShiftedTreeChainCycleSource::TerminalTree { branch: 0, edge: 2 }
        );
        assert_eq!(result.candidate.ratio, rational(5, 6));
        assert_eq!(result.normalized_delta, vec![rational(1, 5); 3]);
        assert_eq!(result.flow, vec![rational(-1, 5); 3]);
        assert_eq!(result.final_snapshot.metrics.queries, 1);
        assert_eq!(result.final_snapshot.metrics.shifts, 0);
        assert_eq!(result.final_snapshot.metrics.state_transitions, 3);
    }

    #[test]
    fn fast_trace_and_compositional_checker_match() {
        let config = config(rational(1, 2));
        let fast = execute_shifted_tree_chain_update(&triangle(), &config).expect("fast");
        let trace = trace_shifted_tree_chain_update(&triangle(), &config).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), 3);
        check_shifted_tree_chain_update_trace(&triangle(), &config, &trace).expect("check");
    }

    #[test]
    fn checker_rejects_query_and_flow_tampering() {
        let config = config(rational(1, 2));
        let mut trace = trace_shifted_tree_chain_update(&triangle(), &config).expect("trace");
        let ShiftedTreeChainUpdateEventKind::CycleQueried { decision } = &mut trace.events[0].kind
        else {
            panic!("query");
        };
        decision.accepted = false;
        assert_eq!(
            check_shifted_tree_chain_update_trace(&triangle(), &config, &trace),
            Err(ShiftedTreeChainUpdateError::TraceVerification)
        );

        let mut trace = trace_shifted_tree_chain_update(&triangle(), &config).expect("trace");
        let ShiftedTreeChainUpdateEventKind::FlowApplied { application } =
            &mut trace.events[1].kind
        else {
            panic!("flow");
        };
        application.beta = rational(-1, 4);
        assert_eq!(
            check_shifted_tree_chain_update_trace(&triangle(), &config, &trace),
            Err(ShiftedTreeChainUpdateError::TraceVerification)
        );
    }

    #[test]
    fn exhausting_every_pass_budget_is_explicit() {
        assert_eq!(
            execute_shifted_tree_chain_update(&triangle(), &config(rational(1, 1))),
            Err(ShiftedTreeChainUpdateError::StrategyExhausted)
        );
    }

    #[test]
    fn largest_eligible_level_wraps_twice_then_moves_shallower() {
        let graph = triangle();
        let config = config(rational(1, 1));
        let chain = execute_shifted_tree_chain(&graph, config.chain, &[])
            .expect("chain")
            .final_snapshot;
        let mut snapshot = initial_snapshot(&graph, &config, chain);
        for expected_pass in [0, 1, 1, 2] {
            let event = apply_shift(&graph, &config, &mut snapshot).expect("shift");
            assert!(matches!(
                event,
                ShiftedTreeChainUpdateEventKind::LevelShifted { level: 1, .. }
            ));
            assert_eq!(snapshot.passes[1], expected_pass);
        }
        let event = apply_shift(&graph, &config, &mut snapshot).expect("shallower shift");
        assert!(matches!(
            event,
            ShiftedTreeChainUpdateEventKind::LevelShifted { level: 0, .. }
        ));
        assert_eq!(snapshot.passes[1], 0);
        assert_eq!(snapshot.metrics.level_shifts, vec![1, 4]);
    }

    #[test]
    fn config_admission_is_closed() {
        let mut invalid = config(rational(1, 2));
        invalid.psi = 0;
        assert_eq!(
            execute_shifted_tree_chain_update(&triangle(), &invalid),
            Err(ShiftedTreeChainUpdateError::InvalidInput)
        );
        let mut oversized = config(rational(1, 2));
        oversized.psi = SHIFTED_TREE_CHAIN_UPDATE_MAX_PSI + 1;
        assert_eq!(
            execute_shifted_tree_chain_update(&triangle(), &oversized),
            Err(ShiftedTreeChainUpdateError::AdmissionLimit)
        );
    }
}
