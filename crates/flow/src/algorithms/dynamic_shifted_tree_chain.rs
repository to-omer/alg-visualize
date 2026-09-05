//! Bounded fixed-topology dynamic composition of Algorithm 2 and Definition 4.5.
//!
//! This module keeps the observable state of van den Brand et al.'s dynamic
//! min-ratio-cycle interface across multiple operations. Attribute updates are
//! applied to a fixed stable edge set, the explicit shifted tree chain is
//! refreshed by exhaustive reconstruction while preserving its public branch
//! schedule, periodic rebuilds reset the source suffix, `FindCycle` and Shift
//! repeat until the strict `kappa * alpha` test succeeds, and the normalized
//! circulation is accumulated into both the maintained flow and `Detect`
//! counters.
//!
//! The paper states periodic thresholds with asymptotic `log^{O(1)}` factors.
//! Such a threshold is not a uniquely executable integer. This bounded
//! realization therefore accepts one explicit strict update limit per
//! nonterminal level: a rebuild occurs when the exact derived update count is
//! greater than that limit. It does not claim the paper's dynamic LSF/spanner,
//! topology updates, hidden-witness access, link-cut-tree runtime, or
//! almost-linear bound.

use std::collections::BTreeMap;

use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use super::{
    ShiftedTreeChainConfig, ShiftedTreeChainCycleCandidate, ShiftedTreeChainError,
    ShiftedTreeChainFlowApplication, ShiftedTreeChainGraph, ShiftedTreeChainOperation,
    ShiftedTreeChainQueryDecision, ShiftedTreeChainQueryError, ShiftedTreeChainQueryMetrics,
    ShiftedTreeChainSnapshot, execute_shifted_tree_chain, find_shifted_tree_chain_cycle,
    trace_shifted_tree_chain, trace_shifted_tree_chain_cycle_query,
};

/// Maximum Definition 4.5 operations in one bounded execution.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_OPERATIONS: usize = 256;
/// Maximum public boundaries in one reversible transcript.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS: usize = 4_096;
/// Maximum cumulative Shift/Rebuild schedule entries.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_CHAIN_OPERATIONS: usize = 1_024;
/// Maximum numerator or denominator width for every exact scalar.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS: u64 = 512;
/// Maximum explicit pass-range parameter.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_PSI: u64 = 16;
/// Maximum concrete per-level update limit.
pub const DYNAMIC_SHIFTED_TREE_CHAIN_MAX_REBUILD_LIMIT: u64 = 1_000_000;

const CATALOG_ID: &str = "dynamic-shifted-tree-chain";

/// Observable dynamic Algorithm 2 parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainConfig {
    /// Explicit shifted-tree-chain depth and branch count.
    pub chain: ShiftedTreeChainConfig,
    /// Concrete replacement for the paper's polylogarithmic weight range.
    pub psi: u64,
    /// Exact strict acceptance threshold `kappa * alpha`.
    pub kappa_alpha: BigRational,
    /// Exact positive Definition 4.5 detection threshold.
    pub epsilon: BigRational,
    /// Strict update limit for each nonterminal level `0..depth`.
    pub rebuild_after_updates: Vec<u64>,
}

/// One stable observable coordinate replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainCoordinateUpdate {
    /// Stable original-edge index; a batch must be strictly increasing.
    pub edge: usize,
    /// Replacement exact positive length.
    pub length: BigRational,
    /// Replacement exact signed gradient.
    pub gradient: BigRational,
}

/// One operation from the bounded Definition 4.5 interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicShiftedTreeChainOperation {
    /// Apply attributes, find/normalize a cycle, and update maintained flow.
    Update {
        /// Nonempty stable coordinate replacements.
        coordinates: Vec<DynamicShiftedTreeChainCoordinateUpdate>,
        /// Exact positive target progress for this stage.
        eta: BigRational,
    },
    /// Return one maintained stable flow coordinate.
    Query {
        /// Stable edge index.
        edge: usize,
    },
    /// Return and reset all coordinates crossing the detection threshold.
    Detect,
}

/// Exact response from a public query or detection operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicShiftedTreeChainResponse {
    /// One maintained flow coordinate.
    Query {
        /// Stable edge index.
        edge: usize,
        /// Current exact flow.
        flow: BigRational,
    },
    /// Stable detection set after the indicated update stage.
    Detect {
        /// Number of completed update calls.
        stage: u64,
        /// Detected edges in increasing stable order.
        edges: Vec<usize>,
    },
}

/// Exact aggregate work for the dynamic composition.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainMetrics {
    /// Completed update calls.
    pub updates: u64,
    /// Public flow-coordinate queries.
    pub flow_queries: u64,
    /// Public detection calls.
    pub detect_calls: u64,
    /// Periodic rebuilds.
    pub rebuilds: u64,
    /// Periodic rebuilds by first rebuilt level.
    pub level_rebuilds: Vec<u64>,
    /// Internal `FindCycle` calls.
    pub cycle_queries: u64,
    /// Internal Shift calls.
    pub shifts: u64,
    /// Shift calls by nonterminal level.
    pub level_shifts: Vec<u64>,
    /// Exact derived coordinate updates passed across all levels.
    pub propagated_updates: u64,
    /// Intermediate core edges inspected across all queries.
    pub intermediate_edge_inspections: u64,
    /// Terminal tree/edge pairs inspected across all queries.
    pub terminal_edge_inspections: u64,
    /// Nonzero normalized flow coordinates applied.
    pub flow_coordinate_updates: u64,
    /// Edge threshold checks performed by `Detect`.
    pub detection_edge_scans: u64,
    /// Total returned detection coordinates.
    pub detected_edges: u64,
    /// Reversible public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one reversible dynamic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainSnapshot {
    /// Number of completed update calls.
    pub stage: u64,
    /// Current fixed-topology graph and observable attributes.
    pub graph: ShiftedTreeChainGraph,
    /// Current independently checkable shifted tree chain.
    pub chain: ShiftedTreeChainSnapshot,
    /// Complete public Shift/Rebuild schedule applied to the current graph.
    pub chain_operations: Vec<ShiftedTreeChainOperation>,
    /// Completed full branch passes by nonterminal level.
    pub passes: Vec<u64>,
    /// Derived update counts since the last rebuild of each nonterminal level.
    pub updates_since_rebuild: Vec<u64>,
    /// Last accepted source candidate, if any update has completed.
    pub last_candidate: Option<ShiftedTreeChainCycleCandidate>,
    /// Exact maintained flow in stable original-edge order.
    pub flow: Vec<BigRational>,
    /// Absolute normalized movement since each edge's last detection.
    pub undetected_absolute_update: Vec<BigRational>,
    /// Last stage at which each edge was returned by `Detect`.
    pub last_detected_stage: Vec<Option<u64>>,
    /// Whether the terminal completion boundary was emitted.
    pub complete: bool,
    /// Exact aggregate work.
    pub metrics: DynamicShiftedTreeChainMetrics,
}

/// Source meaning of one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicShiftedTreeChainEventKind {
    /// Observable attributes were replaced and the explicit chain refreshed.
    AttributesUpdated {
        /// New update stage.
        stage: u64,
        /// Exact derived update counts for levels `0..depth`.
        propagated_updates: Vec<u64>,
    },
    /// The smallest strict-limit violation rebuilt its whole suffix.
    PeriodicRebuilt {
        /// First rebuilt level.
        level: usize,
        /// Configured strict update limit.
        strict_limit: u64,
        /// Counter that exceeded the limit before suffix reset.
        updates_before_reset: u64,
    },
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
    /// The accepted cycle was normalized and accumulated into flow.
    FlowApplied {
        /// Exact cumulative flow-application certificate.
        application: Box<ShiftedTreeChainFlowApplication>,
    },
    /// One flow coordinate was returned without state mutation.
    QueryReturned {
        /// Stable edge index.
        edge: usize,
        /// Current exact flow.
        flow: BigRational,
    },
    /// One stable detection set was returned and reset.
    DetectReturned {
        /// Current update stage.
        stage: u64,
        /// Detected edges in increasing stable order.
        edges: Vec<usize>,
    },
    /// Every requested operation completed.
    Completed,
}

/// One fully reversible dynamic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level event meaning.
    pub kind: DynamicShiftedTreeChainEventKind,
    /// State before the event.
    pub before: DynamicShiftedTreeChainSnapshot,
    /// State after the event.
    pub after: DynamicShiftedTreeChainSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainResult {
    /// Query/detection responses in request order.
    pub responses: Vec<DynamicShiftedTreeChainResponse>,
    /// Terminal dynamic state.
    pub final_snapshot: DynamicShiftedTreeChainSnapshot,
}

/// Complete reversible dynamic transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicShiftedTreeChainTraceResult {
    /// Initial chain and zero-flow state.
    pub base_snapshot: DynamicShiftedTreeChainSnapshot,
    /// All source boundaries followed by completion.
    pub events: Vec<DynamicShiftedTreeChainTraceEvent>,
    /// Exact externally visible result.
    pub result: DynamicShiftedTreeChainResult,
}

/// Explicit bounded dynamic-composition failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicShiftedTreeChainError {
    /// The graph, configuration, or operation shape is invalid.
    #[error("dynamic shifted tree chain input is invalid")]
    InvalidInput,
    /// The execution exceeds the published bounded realization.
    #[error("dynamic shifted tree chain exceeds its admission band")]
    AdmissionLimit,
    /// Every level exhausted its pass budget without a good cycle.
    #[error("dynamic shifted tree chain strategy exhausted")]
    StrategyExhausted,
    /// A component tree-chain operation failed.
    #[error(transparent)]
    TreeChain(#[from] ShiftedTreeChainError),
    /// A component `FindCycle` operation failed.
    #[error(transparent)]
    Query(#[from] ShiftedTreeChainQueryError),
    /// Checked exact work accounting overflowed.
    #[error("dynamic shifted tree chain arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact compositional replay.
    #[error("dynamic shifted tree chain trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: DynamicShiftedTreeChainSnapshot,
    events: Vec<DynamicShiftedTreeChainTraceEvent>,
    result: DynamicShiftedTreeChainResult,
}

/// Executes the bounded fixed-topology dynamic composition.
///
/// # Errors
///
/// Rejects malformed/out-of-band input, component failures, pass exhaustion,
/// or exact arithmetic overflow.
pub fn execute_dynamic_shifted_tree_chain(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
) -> Result<DynamicShiftedTreeChainResult, DynamicShiftedTreeChainError> {
    run_internal(graph, config, operations, false).map(|run| run.result)
}

/// Records every observable update, rebuild, query, shift, flow, and detect boundary.
///
/// # Errors
///
/// Returns any execution or independent compositional checker failure.
pub fn trace_dynamic_shifted_tree_chain(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
) -> Result<DynamicShiftedTreeChainTraceResult, DynamicShiftedTreeChainError> {
    let run = run_internal(graph, config, operations, true)?;
    let trace = DynamicShiftedTreeChainTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_shifted_tree_chain_trace(graph, config, operations, &trace)?;
    Ok(trace)
}

/// Reconstructs the complete transcript without invoking the dynamic runner.
///
/// Tree-chain and cycle-query components are independently traced and checked;
/// this checker separately rebuilds attribute propagation, periodic scheduling,
/// pass changes, cumulative flow, detection resets, responses, and metrics.
///
/// # Errors
///
/// Rejects invalid source input or any supplied transcript drift.
pub fn check_dynamic_shifted_tree_chain_trace(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
    trace: &DynamicShiftedTreeChainTraceResult,
) -> Result<(), DynamicShiftedTreeChainError> {
    audit_input(graph, config, operations)?;
    let base_chain = trace_shifted_tree_chain(graph, config.chain, &[])?
        .result
        .final_snapshot;
    let mut snapshot = initial_snapshot(graph, config, base_chain);
    if trace.base_snapshot != snapshot {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    let mut event_index = 0_usize;
    let mut responses = Vec::new();
    for operation in operations {
        match operation {
            DynamicShiftedTreeChainOperation::Update { coordinates, eta } => {
                audit_update_operation(
                    graph,
                    config,
                    coordinates,
                    eta,
                    trace,
                    &mut snapshot,
                    &mut event_index,
                )?;
            }
            DynamicShiftedTreeChainOperation::Query { edge } => {
                let response =
                    audit_query_operation(*edge, trace, &mut snapshot, &mut event_index)?;
                responses.push(response);
            }
            DynamicShiftedTreeChainOperation::Detect => {
                let response =
                    audit_detect_operation(config, trace, &mut snapshot, &mut event_index)?;
                responses.push(response);
            }
        }
    }
    audit_completion(trace, snapshot, event_index, &responses)
}

fn audit_update_operation(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    coordinates: &[DynamicShiftedTreeChainCoordinateUpdate],
    eta: &BigRational,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    audit_attribute_boundary(graph, config, coordinates, trace, snapshot, event_index)?;
    audit_periodic_rebuild_boundary(config, trace, snapshot, event_index)?;
    let candidate = audit_query_shift_loop(config, trace, snapshot, event_index)?;
    audit_flow_boundary(eta, &candidate, trace, snapshot, event_index)
}

fn audit_attribute_boundary(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    coordinates: &[DynamicShiftedTreeChainCoordinateUpdate],
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    let refreshed = audit_refresh(graph, config, snapshot, coordinates)?;
    let event = audit_event(trace, snapshot, *event_index)?;
    let mut after = snapshot.clone();
    after.stage = audit_increment(after.stage)?;
    after.graph = refreshed.graph;
    after.chain = refreshed.chain;
    add_level_updates_audit(&mut after, &refreshed.propagated)?;
    after.metrics.updates = audit_increment(after.metrics.updates)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::AttributesUpdated {
        stage: after.stage,
        propagated_updates: refreshed.propagated,
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(())
}

fn audit_periodic_rebuild_boundary(
    config: &DynamicShiftedTreeChainConfig,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    let Some(level) = first_rebuild_level(snapshot, config) else {
        return Ok(());
    };
    let event = audit_event(trace, snapshot, *event_index)?;
    let strict_limit = config.rebuild_after_updates[level];
    let updates_before_reset = snapshot.updates_since_rebuild[level];
    let mut operations = snapshot.chain_operations.clone();
    operations.push(ShiftedTreeChainOperation::Rebuild { level });
    let chain = trace_shifted_tree_chain(&snapshot.graph, config.chain, &operations)?
        .result
        .final_snapshot;
    let mut after = snapshot.clone();
    after.chain = chain;
    after.chain_operations = operations;
    reset_rebuild_suffix_audit(&mut after, level)?;
    after.metrics.rebuilds = audit_increment(after.metrics.rebuilds)?;
    after.metrics.level_rebuilds[level] = audit_increment(after.metrics.level_rebuilds[level])?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::PeriodicRebuilt {
        level,
        strict_limit,
        updates_before_reset,
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(())
}

fn audit_query_shift_loop(
    config: &DynamicShiftedTreeChainConfig,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<ShiftedTreeChainCycleCandidate, DynamicShiftedTreeChainError> {
    loop {
        let candidate = audit_cycle_query_boundary(config, trace, snapshot, event_index)?;
        if let Some(candidate) = candidate {
            return Ok(candidate);
        }
        audit_shift_boundary(config, trace, snapshot, event_index)?;
    }
}

fn audit_cycle_query_boundary(
    config: &DynamicShiftedTreeChainConfig,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<Option<ShiftedTreeChainCycleCandidate>, DynamicShiftedTreeChainError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let query =
        trace_shifted_tree_chain_cycle_query(&snapshot.graph, config.chain, &snapshot.chain)?;
    let candidate = query.result.best_candidate.clone();
    let accepted = candidate
        .as_ref()
        .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
    let mut after = snapshot.clone();
    add_query_metrics_audit(&mut after.metrics, query.result.final_snapshot.metrics)?;
    if accepted {
        after.last_candidate.clone_from(&candidate);
    }
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::CycleQueried {
        decision: Box::new(ShiftedTreeChainQueryDecision {
            candidate: candidate.clone(),
            threshold: config.kappa_alpha.clone(),
            accepted,
            query_metrics: query.result.final_snapshot.metrics,
        }),
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(accepted.then_some(candidate).flatten())
}

fn audit_shift_boundary(
    config: &DynamicShiftedTreeChainConfig,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    let previous = snapshot.chain.levels[level].active_branch;
    let mut operations = snapshot.chain_operations.clone();
    operations.push(ShiftedTreeChainOperation::Shift { level });
    let chain = trace_shifted_tree_chain(&snapshot.graph, config.chain, &operations)?
        .result
        .final_snapshot;
    let next = chain.levels[level].active_branch;
    let wrapped = next == 0;
    let mut after = snapshot.clone();
    after.chain = chain;
    after.chain_operations = operations;
    update_passes_audit(&mut after.passes, level, wrapped)?;
    after.metrics.shifts = audit_increment(after.metrics.shifts)?;
    after.metrics.level_shifts[level] = audit_increment(after.metrics.level_shifts[level])?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::LevelShifted {
        level,
        previous_branch: previous,
        next_branch: next,
        wrapped,
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(())
}

fn audit_flow_boundary(
    eta: &BigRational,
    candidate: &ShiftedTreeChainCycleCandidate,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let application = audit_flow_application(snapshot, eta, candidate)?;
    let mut after = snapshot.clone();
    apply_flow_state_audit(&mut after, &application)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::FlowApplied {
        application: Box::new(application),
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(())
}

fn audit_query_operation(
    edge: usize,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<DynamicShiftedTreeChainResponse, DynamicShiftedTreeChainError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let flow = snapshot
        .flow
        .get(edge)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?
        .clone();
    let mut after = snapshot.clone();
    after.metrics.flow_queries = audit_increment(after.metrics.flow_queries)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::QueryReturned {
        edge,
        flow: flow.clone(),
    };
    audit_match(event, &kind, &after)?;
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(DynamicShiftedTreeChainResponse::Query { edge, flow })
}

fn audit_detect_operation(
    config: &DynamicShiftedTreeChainConfig,
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    event_index: &mut usize,
) -> Result<DynamicShiftedTreeChainResponse, DynamicShiftedTreeChainError> {
    let event = audit_event(trace, snapshot, *event_index)?;
    let edges = audit_detection_edges(snapshot, &config.epsilon)?;
    let mut after = snapshot.clone();
    for &edge in &edges {
        after.undetected_absolute_update[edge] = BigRational::zero();
        after.last_detected_stage[edge] = Some(after.stage);
    }
    let edge_count = u64::try_from(after.graph.edges.len())
        .map_err(|_| DynamicShiftedTreeChainError::TraceVerification)?;
    let detected =
        u64::try_from(edges.len()).map_err(|_| DynamicShiftedTreeChainError::TraceVerification)?;
    after.metrics.detect_calls = audit_increment(after.metrics.detect_calls)?;
    after.metrics.detection_edge_scans = after
        .metrics
        .detection_edge_scans
        .checked_add(edge_count)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    after.metrics.detected_edges = after
        .metrics
        .detected_edges
        .checked_add(detected)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
    let kind = DynamicShiftedTreeChainEventKind::DetectReturned {
        stage: after.stage,
        edges: edges.clone(),
    };
    audit_match(event, &kind, &after)?;
    let response = DynamicShiftedTreeChainResponse::Detect {
        stage: after.stage,
        edges,
    };
    *snapshot = after;
    *event_index = audit_next(*event_index)?;
    Ok(response)
}

fn audit_completion(
    trace: &DynamicShiftedTreeChainTraceResult,
    snapshot: DynamicShiftedTreeChainSnapshot,
    event_index: usize,
    responses: &[DynamicShiftedTreeChainResponse],
) -> Result<(), DynamicShiftedTreeChainError> {
    let completion = audit_event(trace, &snapshot, event_index)?;
    let mut final_snapshot = snapshot;
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    audit_match(
        completion,
        &DynamicShiftedTreeChainEventKind::Completed,
        &final_snapshot,
    )?;
    if audit_next(event_index)? != trace.events.len()
        || trace.result.responses != responses
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
    record: bool,
) -> Result<InternalRun, DynamicShiftedTreeChainError> {
    validate_input(graph, config, operations)?;
    let chain = execute_shifted_tree_chain(graph, config.chain, &[])?.final_snapshot;
    let mut snapshot = initial_snapshot(graph, config, chain);
    let base_snapshot = snapshot.clone();
    let mut events = Vec::new();
    let mut responses = Vec::new();
    for operation in operations {
        match operation {
            DynamicShiftedTreeChainOperation::Update { coordinates, eta } => {
                apply_dynamic_update(config, coordinates, eta, &mut snapshot, &mut events, record)?;
            }
            DynamicShiftedTreeChainOperation::Query { edge } => {
                let before = snapshot.clone();
                let flow = snapshot.flow[*edge].clone();
                snapshot.metrics.flow_queries = increment(snapshot.metrics.flow_queries)?;
                snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
                push_event(
                    &mut events,
                    record,
                    DynamicShiftedTreeChainEventKind::QueryReturned {
                        edge: *edge,
                        flow: flow.clone(),
                    },
                    before,
                    &snapshot,
                )?;
                responses.push(DynamicShiftedTreeChainResponse::Query { edge: *edge, flow });
            }
            DynamicShiftedTreeChainOperation::Detect => {
                let before = snapshot.clone();
                let edges = detection_edges(&snapshot, &config.epsilon);
                for &edge in &edges {
                    snapshot.undetected_absolute_update[edge] = BigRational::zero();
                    snapshot.last_detected_stage[edge] = Some(snapshot.stage);
                }
                let edge_count = u64::try_from(snapshot.graph.edges.len())
                    .map_err(|_| DynamicShiftedTreeChainError::ArithmeticOverflow)?;
                let detected = u64::try_from(edges.len())
                    .map_err(|_| DynamicShiftedTreeChainError::ArithmeticOverflow)?;
                snapshot.metrics.detect_calls = increment(snapshot.metrics.detect_calls)?;
                snapshot.metrics.detection_edge_scans = snapshot
                    .metrics
                    .detection_edge_scans
                    .checked_add(edge_count)
                    .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
                snapshot.metrics.detected_edges = snapshot
                    .metrics
                    .detected_edges
                    .checked_add(detected)
                    .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
                snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
                push_event(
                    &mut events,
                    record,
                    DynamicShiftedTreeChainEventKind::DetectReturned {
                        stage: snapshot.stage,
                        edges: edges.clone(),
                    },
                    before,
                    &snapshot,
                )?;
                responses.push(DynamicShiftedTreeChainResponse::Detect {
                    stage: snapshot.stage,
                    edges,
                });
            }
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    push_event(
        &mut events,
        record,
        DynamicShiftedTreeChainEventKind::Completed,
        before,
        &snapshot,
    )?;
    Ok(InternalRun {
        base_snapshot,
        events,
        result: DynamicShiftedTreeChainResult {
            responses,
            final_snapshot: snapshot,
        },
    })
}

fn apply_dynamic_update(
    config: &DynamicShiftedTreeChainConfig,
    coordinates: &[DynamicShiftedTreeChainCoordinateUpdate],
    eta: &BigRational,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    events: &mut Vec<DynamicShiftedTreeChainTraceEvent>,
    record: bool,
) -> Result<(), DynamicShiftedTreeChainError> {
    let before = snapshot.clone();
    let next_graph = replace_coordinates(&snapshot.graph, coordinates);
    let next_chain =
        execute_shifted_tree_chain(&next_graph, config.chain, &snapshot.chain_operations)?
            .final_snapshot;
    let propagated = derived_level_updates(&snapshot.chain, &next_chain, config.chain.depth)?;
    snapshot.stage = increment(snapshot.stage)?;
    snapshot.graph = next_graph;
    snapshot.chain = next_chain;
    add_level_updates(snapshot, &propagated)?;
    snapshot.metrics.updates = increment(snapshot.metrics.updates)?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    push_event(
        events,
        record,
        DynamicShiftedTreeChainEventKind::AttributesUpdated {
            stage: snapshot.stage,
            propagated_updates: propagated,
        },
        before,
        snapshot,
    )?;

    if let Some(level) = first_rebuild_level(snapshot, config) {
        if snapshot.chain_operations.len() >= DYNAMIC_SHIFTED_TREE_CHAIN_MAX_CHAIN_OPERATIONS {
            return Err(DynamicShiftedTreeChainError::AdmissionLimit);
        }
        let before = snapshot.clone();
        let strict_limit = config.rebuild_after_updates[level];
        let updates_before_reset = snapshot.updates_since_rebuild[level];
        snapshot
            .chain_operations
            .push(ShiftedTreeChainOperation::Rebuild { level });
        snapshot.chain =
            execute_shifted_tree_chain(&snapshot.graph, config.chain, &snapshot.chain_operations)?
                .final_snapshot;
        reset_rebuild_suffix(snapshot, level)?;
        snapshot.metrics.rebuilds = increment(snapshot.metrics.rebuilds)?;
        snapshot.metrics.level_rebuilds[level] = increment(snapshot.metrics.level_rebuilds[level])?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        push_event(
            events,
            record,
            DynamicShiftedTreeChainEventKind::PeriodicRebuilt {
                level,
                strict_limit,
                updates_before_reset,
            },
            before,
            snapshot,
        )?;
    }

    let candidate = loop {
        let before = snapshot.clone();
        let query = find_shifted_tree_chain_cycle(&snapshot.graph, config.chain, &snapshot.chain)?;
        let candidate = query.best_candidate;
        let accepted = candidate
            .as_ref()
            .is_some_and(|candidate| candidate.ratio > config.kappa_alpha);
        add_query_metrics(&mut snapshot.metrics, query.final_snapshot.metrics)?;
        if accepted {
            snapshot.last_candidate.clone_from(&candidate);
        }
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        push_event(
            events,
            record,
            DynamicShiftedTreeChainEventKind::CycleQueried {
                decision: Box::new(ShiftedTreeChainQueryDecision {
                    candidate: candidate.clone(),
                    threshold: config.kappa_alpha.clone(),
                    accepted,
                    query_metrics: query.final_snapshot.metrics,
                }),
            },
            before,
            snapshot,
        )?;
        if accepted {
            break candidate.ok_or(DynamicShiftedTreeChainError::StrategyExhausted)?;
        }
        apply_dynamic_shift(config, snapshot, events, record)?;
    };

    let before = snapshot.clone();
    let application = flow_application(snapshot, eta, &candidate)?;
    apply_flow_state(snapshot, &application)?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    push_event(
        events,
        record,
        DynamicShiftedTreeChainEventKind::FlowApplied {
            application: Box::new(application),
        },
        before,
        snapshot,
    )
}

fn apply_dynamic_shift(
    config: &DynamicShiftedTreeChainConfig,
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    events: &mut Vec<DynamicShiftedTreeChainTraceEvent>,
    record: bool,
) -> Result<(), DynamicShiftedTreeChainError> {
    if snapshot.chain_operations.len() >= DYNAMIC_SHIFTED_TREE_CHAIN_MAX_CHAIN_OPERATIONS {
        return Err(DynamicShiftedTreeChainError::AdmissionLimit);
    }
    let level = largest_eligible_level(&snapshot.passes, config.psi)
        .ok_or(DynamicShiftedTreeChainError::StrategyExhausted)?;
    let before = snapshot.clone();
    let previous = snapshot.chain.levels[level].active_branch;
    snapshot
        .chain_operations
        .push(ShiftedTreeChainOperation::Shift { level });
    snapshot.chain =
        execute_shifted_tree_chain(&snapshot.graph, config.chain, &snapshot.chain_operations)?
            .final_snapshot;
    let next = snapshot.chain.levels[level].active_branch;
    let wrapped = next == 0;
    update_passes(&mut snapshot.passes, level, wrapped)?;
    snapshot.metrics.shifts = increment(snapshot.metrics.shifts)?;
    snapshot.metrics.level_shifts[level] = increment(snapshot.metrics.level_shifts[level])?;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    push_event(
        events,
        record,
        DynamicShiftedTreeChainEventKind::LevelShifted {
            level,
            previous_branch: previous,
            next_branch: next,
            wrapped,
        },
        before,
        snapshot,
    )
}

struct RefreshedAudit {
    graph: ShiftedTreeChainGraph,
    chain: ShiftedTreeChainSnapshot,
    propagated: Vec<u64>,
}

fn audit_refresh(
    _initial_graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    snapshot: &DynamicShiftedTreeChainSnapshot,
    coordinates: &[DynamicShiftedTreeChainCoordinateUpdate],
) -> Result<RefreshedAudit, DynamicShiftedTreeChainError> {
    let graph = replace_coordinates(&snapshot.graph, coordinates);
    let chain = trace_shifted_tree_chain(&graph, config.chain, &snapshot.chain_operations)?
        .result
        .final_snapshot;
    let propagated = derived_level_updates_audit(&snapshot.chain, &chain, config.chain.depth)?;
    Ok(RefreshedAudit {
        graph,
        chain,
        propagated,
    })
}

fn replace_coordinates(
    graph: &ShiftedTreeChainGraph,
    coordinates: &[DynamicShiftedTreeChainCoordinateUpdate],
) -> ShiftedTreeChainGraph {
    let mut graph = graph.clone();
    for coordinate in coordinates {
        graph.edges[coordinate.edge].length = coordinate.length.clone();
        graph.edges[coordinate.edge].gradient = coordinate.gradient.clone();
    }
    graph
}

fn derived_level_updates(
    before: &ShiftedTreeChainSnapshot,
    after: &ShiftedTreeChainSnapshot,
    depth: usize,
) -> Result<Vec<u64>, DynamicShiftedTreeChainError> {
    (0..depth)
        .map(|level| changed_edges(&before.levels[level].graph, &after.levels[level].graph))
        .collect()
}

fn derived_level_updates_audit(
    before: &ShiftedTreeChainSnapshot,
    after: &ShiftedTreeChainSnapshot,
    depth: usize,
) -> Result<Vec<u64>, DynamicShiftedTreeChainError> {
    if before.levels.len() <= depth || after.levels.len() <= depth {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    (0..depth)
        .map(|level| changed_edges_audit(&before.levels[level].graph, &after.levels[level].graph))
        .collect()
}

fn changed_edges(
    before: &ShiftedTreeChainGraph,
    after: &ShiftedTreeChainGraph,
) -> Result<u64, DynamicShiftedTreeChainError> {
    let before = edge_map(before);
    let after = edge_map(after);
    u64::try_from(
        before
            .keys()
            .chain(after.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|edge| before.get(edge) != after.get(edge))
            .count(),
    )
    .map_err(|_| DynamicShiftedTreeChainError::ArithmeticOverflow)
}

fn changed_edges_audit(
    before: &ShiftedTreeChainGraph,
    after: &ShiftedTreeChainGraph,
) -> Result<u64, DynamicShiftedTreeChainError> {
    changed_edges(before, after).map_err(|_| DynamicShiftedTreeChainError::TraceVerification)
}

fn edge_map(
    graph: &ShiftedTreeChainGraph,
) -> BTreeMap<usize, (usize, usize, BigRational, BigRational)> {
    graph
        .edges
        .iter()
        .map(|edge| {
            (
                edge.source_edge,
                (
                    edge.from,
                    edge.to,
                    edge.length.clone(),
                    edge.gradient.clone(),
                ),
            )
        })
        .collect()
}

fn first_rebuild_level(
    snapshot: &DynamicShiftedTreeChainSnapshot,
    config: &DynamicShiftedTreeChainConfig,
) -> Option<usize> {
    snapshot
        .updates_since_rebuild
        .iter()
        .zip(&config.rebuild_after_updates)
        .position(|(updates, limit)| updates > limit)
}

fn reset_rebuild_suffix(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    level: usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    for pass in snapshot
        .passes
        .get_mut(level..)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?
    {
        *pass = 0;
    }
    for count in snapshot
        .updates_since_rebuild
        .get_mut(level..)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?
    {
        *count = 0;
    }
    Ok(())
}

fn reset_rebuild_suffix_audit(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    level: usize,
) -> Result<(), DynamicShiftedTreeChainError> {
    if level >= snapshot.passes.len() || level >= snapshot.updates_since_rebuild.len() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    snapshot.passes[level..].fill(0);
    snapshot.updates_since_rebuild[level..].fill(0);
    Ok(())
}

fn add_level_updates(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    updates: &[u64],
) -> Result<(), DynamicShiftedTreeChainError> {
    for (total, update) in snapshot.updates_since_rebuild.iter_mut().zip(updates) {
        *total = total
            .checked_add(*update)
            .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    }
    snapshot.metrics.propagated_updates = snapshot
        .metrics
        .propagated_updates
        .checked_add(updates.iter().try_fold(0_u64, |sum, &update| {
            sum.checked_add(update)
                .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)
        })?)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    Ok(())
}

fn add_level_updates_audit(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    updates: &[u64],
) -> Result<(), DynamicShiftedTreeChainError> {
    if updates.len() != snapshot.updates_since_rebuild.len() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    for (total, update) in snapshot.updates_since_rebuild.iter_mut().zip(updates) {
        *total = total
            .checked_add(*update)
            .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    }
    let increment = updates.iter().try_fold(0_u64, |sum, &update| {
        sum.checked_add(update)
            .ok_or(DynamicShiftedTreeChainError::TraceVerification)
    })?;
    snapshot.metrics.propagated_updates = snapshot
        .metrics
        .propagated_updates
        .checked_add(increment)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn largest_eligible_level(passes: &[u64], psi: u64) -> Option<usize> {
    let ceiling = psi.checked_mul(2)?;
    passes.iter().rposition(|&passes| passes < ceiling)
}

fn update_passes(
    passes: &mut [u64],
    level: usize,
    wrapped: bool,
) -> Result<(), DynamicShiftedTreeChainError> {
    if wrapped {
        passes[level] = increment(passes[level])?;
    }
    for pass in passes
        .get_mut(level + 1..)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?
    {
        *pass = 0;
    }
    Ok(())
}

fn update_passes_audit(
    passes: &mut [u64],
    level: usize,
    wrapped: bool,
) -> Result<(), DynamicShiftedTreeChainError> {
    if level >= passes.len() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    if wrapped {
        passes[level] = audit_increment(passes[level])?;
    }
    passes[level + 1..].fill(0);
    Ok(())
}

fn add_query_metrics(
    metrics: &mut DynamicShiftedTreeChainMetrics,
    query: ShiftedTreeChainQueryMetrics,
) -> Result<(), DynamicShiftedTreeChainError> {
    metrics.cycle_queries = increment(metrics.cycle_queries)?;
    metrics.intermediate_edge_inspections = metrics
        .intermediate_edge_inspections
        .checked_add(query.intermediate_edge_inspections)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    metrics.terminal_edge_inspections = metrics
        .terminal_edge_inspections
        .checked_add(query.terminal_edge_inspections)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    Ok(())
}

fn add_query_metrics_audit(
    metrics: &mut DynamicShiftedTreeChainMetrics,
    query: ShiftedTreeChainQueryMetrics,
) -> Result<(), DynamicShiftedTreeChainError> {
    metrics.cycle_queries = audit_increment(metrics.cycle_queries)?;
    metrics.intermediate_edge_inspections = metrics
        .intermediate_edge_inspections
        .checked_add(query.intermediate_edge_inspections)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    metrics.terminal_edge_inspections = metrics
        .terminal_edge_inspections
        .checked_add(query.terminal_edge_inspections)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn flow_application(
    snapshot: &DynamicShiftedTreeChainSnapshot,
    eta: &BigRational,
    candidate: &ShiftedTreeChainCycleCandidate,
) -> Result<ShiftedTreeChainFlowApplication, DynamicShiftedTreeChainError> {
    if candidate.gradient >= BigRational::zero()
        || candidate.coefficients.len() != snapshot.graph.edges.len()
    {
        return Err(DynamicShiftedTreeChainError::InvalidInput);
    }
    let beta = eta / &candidate.gradient;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    let flow = snapshot
        .flow
        .iter()
        .zip(&normalized_delta)
        .map(|(flow, delta)| flow - delta)
        .collect::<Vec<_>>();
    if normalized_delta.iter().any(rational_too_wide) || flow.iter().any(rational_too_wide) {
        return Err(DynamicShiftedTreeChainError::AdmissionLimit);
    }
    Ok(ShiftedTreeChainFlowApplication {
        gradient_dot: candidate.gradient.clone(),
        beta,
        normalized_delta,
        flow,
    })
}

fn audit_flow_application(
    snapshot: &DynamicShiftedTreeChainSnapshot,
    eta: &BigRational,
    candidate: &ShiftedTreeChainCycleCandidate,
) -> Result<ShiftedTreeChainFlowApplication, DynamicShiftedTreeChainError> {
    if candidate.coefficients.len() != snapshot.graph.edges.len() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    let gradient_dot = snapshot
        .graph
        .edges
        .iter()
        .zip(&candidate.coefficients)
        .map(|(edge, coefficient)| &edge.gradient * coefficient)
        .sum::<BigRational>();
    if gradient_dot != candidate.gradient || gradient_dot >= BigRational::zero() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    let beta = eta / &gradient_dot;
    let normalized_delta = candidate
        .coefficients
        .iter()
        .map(|coefficient| &beta * coefficient)
        .collect::<Vec<_>>();
    let flow = snapshot
        .flow
        .iter()
        .zip(&normalized_delta)
        .map(|(flow, delta)| flow - delta)
        .collect::<Vec<_>>();
    if normalized_delta.iter().any(rational_too_wide) || flow.iter().any(rational_too_wide) {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    let mut divergence = vec![BigRational::zero(); snapshot.graph.node_count];
    for (edge, coefficient) in snapshot.graph.edges.iter().zip(&normalized_delta) {
        divergence[edge.from] += coefficient;
        divergence[edge.to] -= coefficient;
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    Ok(ShiftedTreeChainFlowApplication {
        gradient_dot,
        beta,
        normalized_delta,
        flow,
    })
}

fn apply_flow_state(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    application: &ShiftedTreeChainFlowApplication,
) -> Result<(), DynamicShiftedTreeChainError> {
    snapshot.flow.clone_from(&application.flow);
    for (total, delta) in snapshot
        .undetected_absolute_update
        .iter_mut()
        .zip(&application.normalized_delta)
    {
        *total += delta.abs();
    }
    let nonzero = u64::try_from(
        application
            .normalized_delta
            .iter()
            .filter(|value| !value.is_zero())
            .count(),
    )
    .map_err(|_| DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    snapshot.metrics.flow_coordinate_updates = snapshot
        .metrics
        .flow_coordinate_updates
        .checked_add(nonzero)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)?;
    Ok(())
}

fn apply_flow_state_audit(
    snapshot: &mut DynamicShiftedTreeChainSnapshot,
    application: &ShiftedTreeChainFlowApplication,
) -> Result<(), DynamicShiftedTreeChainError> {
    let stable_edge_count = snapshot.flow.len();
    if application.flow.len() != stable_edge_count
        || application.normalized_delta.len() != stable_edge_count
    {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    snapshot.flow.clone_from(&application.flow);
    for (total, delta) in snapshot
        .undetected_absolute_update
        .iter_mut()
        .zip(&application.normalized_delta)
    {
        *total += delta.abs();
    }
    let nonzero = u64::try_from(
        application
            .normalized_delta
            .iter()
            .filter(|value| !value.is_zero())
            .count(),
    )
    .map_err(|_| DynamicShiftedTreeChainError::TraceVerification)?;
    snapshot.metrics.flow_coordinate_updates = snapshot
        .metrics
        .flow_coordinate_updates
        .checked_add(nonzero)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    Ok(())
}

fn detection_edges(
    snapshot: &DynamicShiftedTreeChainSnapshot,
    epsilon: &BigRational,
) -> Vec<usize> {
    snapshot
        .graph
        .edges
        .iter()
        .zip(&snapshot.undetected_absolute_update)
        .enumerate()
        .filter_map(|(edge, (data, movement))| {
            (&data.length * movement >= *epsilon).then_some(edge)
        })
        .collect()
}

fn audit_detection_edges(
    snapshot: &DynamicShiftedTreeChainSnapshot,
    epsilon: &BigRational,
) -> Result<Vec<usize>, DynamicShiftedTreeChainError> {
    if snapshot.graph.edges.len() != snapshot.undetected_absolute_update.len() {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    Ok(detection_edges(snapshot, epsilon))
}

fn initial_snapshot(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    chain: ShiftedTreeChainSnapshot,
) -> DynamicShiftedTreeChainSnapshot {
    let level_count = config.chain.depth;
    DynamicShiftedTreeChainSnapshot {
        stage: 0,
        graph: graph.clone(),
        chain,
        chain_operations: Vec::new(),
        passes: vec![0; level_count],
        updates_since_rebuild: vec![0; level_count],
        last_candidate: None,
        flow: vec![BigRational::zero(); graph.edges.len()],
        undetected_absolute_update: vec![BigRational::zero(); graph.edges.len()],
        last_detected_stage: vec![None; graph.edges.len()],
        complete: false,
        metrics: DynamicShiftedTreeChainMetrics {
            level_rebuilds: vec![0; level_count],
            level_shifts: vec![0; level_count],
            ..DynamicShiftedTreeChainMetrics::default()
        },
    }
}

fn validate_input(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
) -> Result<(), DynamicShiftedTreeChainError> {
    if config.chain.depth == 0
        || config.psi == 0
        || config.kappa_alpha <= BigRational::zero()
        || config.epsilon <= BigRational::zero()
        || config.rebuild_after_updates.len() != config.chain.depth
    {
        return Err(DynamicShiftedTreeChainError::InvalidInput);
    }
    if operations.len() > DYNAMIC_SHIFTED_TREE_CHAIN_MAX_OPERATIONS
        || config.psi > DYNAMIC_SHIFTED_TREE_CHAIN_MAX_PSI
        || config
            .rebuild_after_updates
            .iter()
            .any(|&limit| limit > DYNAMIC_SHIFTED_TREE_CHAIN_MAX_REBUILD_LIMIT)
        || rational_too_wide(&config.kappa_alpha)
        || rational_too_wide(&config.epsilon)
    {
        return Err(DynamicShiftedTreeChainError::AdmissionLimit);
    }
    execute_shifted_tree_chain(graph, config.chain, &[])?;
    for operation in operations {
        match operation {
            DynamicShiftedTreeChainOperation::Update { coordinates, eta } => {
                if coordinates.is_empty() || *eta <= BigRational::zero() {
                    return Err(DynamicShiftedTreeChainError::InvalidInput);
                }
                if rational_too_wide(eta) {
                    return Err(DynamicShiftedTreeChainError::AdmissionLimit);
                }
                let mut previous = None;
                for coordinate in coordinates {
                    if coordinate.edge >= graph.edges.len()
                        || previous.is_some_and(|previous| coordinate.edge <= previous)
                        || coordinate.length <= BigRational::zero()
                    {
                        return Err(DynamicShiftedTreeChainError::InvalidInput);
                    }
                    if rational_too_wide(&coordinate.length)
                        || rational_too_wide(&coordinate.gradient)
                    {
                        return Err(DynamicShiftedTreeChainError::AdmissionLimit);
                    }
                    previous = Some(coordinate.edge);
                }
            }
            DynamicShiftedTreeChainOperation::Query { edge } => {
                if *edge >= graph.edges.len() {
                    return Err(DynamicShiftedTreeChainError::InvalidInput);
                }
            }
            DynamicShiftedTreeChainOperation::Detect => {}
        }
    }
    Ok(())
}

fn audit_input(
    graph: &ShiftedTreeChainGraph,
    config: &DynamicShiftedTreeChainConfig,
    operations: &[DynamicShiftedTreeChainOperation],
) -> Result<(), DynamicShiftedTreeChainError> {
    validate_input(graph, config, operations).map_err(|error| match error {
        DynamicShiftedTreeChainError::InvalidInput
        | DynamicShiftedTreeChainError::AdmissionLimit
        | DynamicShiftedTreeChainError::TreeChain(_)
        | DynamicShiftedTreeChainError::Query(_) => error,
        DynamicShiftedTreeChainError::StrategyExhausted
        | DynamicShiftedTreeChainError::ArithmeticOverflow
        | DynamicShiftedTreeChainError::TraceVerification => {
            DynamicShiftedTreeChainError::TraceVerification
        }
    })
}

fn push_event(
    events: &mut Vec<DynamicShiftedTreeChainTraceEvent>,
    record: bool,
    kind: DynamicShiftedTreeChainEventKind,
    before: DynamicShiftedTreeChainSnapshot,
    after: &DynamicShiftedTreeChainSnapshot,
) -> Result<(), DynamicShiftedTreeChainError> {
    if !record {
        return Ok(());
    }
    if events.len() >= DYNAMIC_SHIFTED_TREE_CHAIN_MAX_TRACE_EVENTS {
        return Err(DynamicShiftedTreeChainError::AdmissionLimit);
    }
    events.push(DynamicShiftedTreeChainTraceEvent {
        catalog_id: CATALOG_ID,
        kind,
        before,
        after: after.clone(),
    });
    Ok(())
}

fn audit_event<'a>(
    trace: &'a DynamicShiftedTreeChainTraceResult,
    snapshot: &DynamicShiftedTreeChainSnapshot,
    event_index: usize,
) -> Result<&'a DynamicShiftedTreeChainTraceEvent, DynamicShiftedTreeChainError> {
    let event = trace
        .events
        .get(event_index)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)?;
    if event.catalog_id != CATALOG_ID || &event.before != snapshot {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    Ok(event)
}

fn audit_match(
    event: &DynamicShiftedTreeChainTraceEvent,
    kind: &DynamicShiftedTreeChainEventKind,
    after: &DynamicShiftedTreeChainSnapshot,
) -> Result<(), DynamicShiftedTreeChainError> {
    if &event.kind != kind || &event.after != after {
        return Err(DynamicShiftedTreeChainError::TraceVerification);
    }
    Ok(())
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_SHIFTED_TREE_CHAIN_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, DynamicShiftedTreeChainError> {
    value
        .checked_add(1)
        .ok_or(DynamicShiftedTreeChainError::ArithmeticOverflow)
}

fn audit_increment(value: u64) -> Result<u64, DynamicShiftedTreeChainError> {
    value
        .checked_add(1)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)
}

fn audit_next(event_index: usize) -> Result<usize, DynamicShiftedTreeChainError> {
    event_index
        .checked_add(1)
        .ok_or(DynamicShiftedTreeChainError::TraceVerification)
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;
    use crate::algorithms::ShiftedTreeChainEdge;

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

    fn config(limits: Vec<u64>) -> DynamicShiftedTreeChainConfig {
        DynamicShiftedTreeChainConfig {
            chain: ShiftedTreeChainConfig {
                depth: 2,
                branches: 2,
            },
            psi: 1,
            kappa_alpha: rational(1, 2),
            epsilon: rational(1, 3),
            rebuild_after_updates: limits,
        }
    }

    fn update(edge: usize, length: i64) -> DynamicShiftedTreeChainOperation {
        DynamicShiftedTreeChainOperation::Update {
            coordinates: vec![DynamicShiftedTreeChainCoordinateUpdate {
                edge,
                length: rational(length, 1),
                gradient: triangle().edges[edge].gradient.clone(),
            }],
            eta: rational(1, 1),
        }
    }

    #[test]
    fn accumulates_two_updates_queries_and_detection_exactly() {
        let operations = vec![
            update(0, 2),
            update(1, 3),
            DynamicShiftedTreeChainOperation::Query { edge: 0 },
            DynamicShiftedTreeChainOperation::Detect,
            DynamicShiftedTreeChainOperation::Detect,
        ];
        let result =
            execute_dynamic_shifted_tree_chain(&triangle(), &config(vec![100, 100]), &operations)
                .expect("dynamic");
        assert_eq!(result.final_snapshot.stage, 2);
        assert_eq!(result.final_snapshot.flow, vec![rational(-2, 5); 3]);
        assert_eq!(
            result.responses,
            vec![
                DynamicShiftedTreeChainResponse::Query {
                    edge: 0,
                    flow: rational(-2, 5),
                },
                DynamicShiftedTreeChainResponse::Detect {
                    stage: 2,
                    edges: vec![0, 1, 2],
                },
                DynamicShiftedTreeChainResponse::Detect {
                    stage: 2,
                    edges: vec![],
                },
            ]
        );
        assert_eq!(result.final_snapshot.metrics.updates, 2);
        assert_eq!(result.final_snapshot.metrics.cycle_queries, 2);
        assert_eq!(result.final_snapshot.metrics.flow_coordinate_updates, 6);
    }

    #[test]
    fn strict_integer_limit_rebuilds_smallest_level_and_resets_suffix() {
        let operations = vec![update(0, 2)];
        let trace =
            trace_dynamic_shifted_tree_chain(&triangle(), &config(vec![0, 100]), &operations)
                .expect("trace");
        assert!(matches!(
            trace.events[1].kind,
            DynamicShiftedTreeChainEventKind::PeriodicRebuilt {
                level: 0,
                strict_limit: 0,
                updates_before_reset: 1
            }
        ));
        assert_eq!(
            trace.result.final_snapshot.updates_since_rebuild,
            vec![0, 0]
        );
        assert_eq!(
            trace.result.final_snapshot.metrics.level_rebuilds,
            vec![1, 0]
        );
    }

    #[test]
    fn fast_trace_and_independent_composition_match() {
        let operations = vec![
            DynamicShiftedTreeChainOperation::Query { edge: 2 },
            update(0, 2),
            DynamicShiftedTreeChainOperation::Detect,
        ];
        let config = config(vec![1, 100]);
        let fast =
            execute_dynamic_shifted_tree_chain(&triangle(), &config, &operations).expect("fast");
        let trace =
            trace_dynamic_shifted_tree_chain(&triangle(), &config, &operations).expect("trace");
        assert_eq!(fast, trace.result);
        check_dynamic_shifted_tree_chain_trace(&triangle(), &config, &operations, &trace)
            .expect("check");
    }

    #[test]
    fn checker_rejects_rebuild_flow_and_detection_tampering() {
        let operations = vec![update(0, 2), DynamicShiftedTreeChainOperation::Detect];
        let config = config(vec![0, 100]);
        let mut trace =
            trace_dynamic_shifted_tree_chain(&triangle(), &config, &operations).expect("trace");
        let DynamicShiftedTreeChainEventKind::PeriodicRebuilt { strict_limit, .. } =
            &mut trace.events[1].kind
        else {
            panic!("rebuild");
        };
        *strict_limit = 1;
        assert_eq!(
            check_dynamic_shifted_tree_chain_trace(&triangle(), &config, &operations, &trace),
            Err(DynamicShiftedTreeChainError::TraceVerification)
        );

        let mut trace =
            trace_dynamic_shifted_tree_chain(&triangle(), &config, &operations).expect("trace");
        let flow_index = trace
            .events
            .iter()
            .position(|event| {
                matches!(
                    event.kind,
                    DynamicShiftedTreeChainEventKind::FlowApplied { .. }
                )
            })
            .expect("flow");
        let DynamicShiftedTreeChainEventKind::FlowApplied { application } =
            &mut trace.events[flow_index].kind
        else {
            panic!("flow");
        };
        application.beta = rational(-1, 4);
        assert_eq!(
            check_dynamic_shifted_tree_chain_trace(&triangle(), &config, &operations, &trace),
            Err(DynamicShiftedTreeChainError::TraceVerification)
        );

        let mut trace =
            trace_dynamic_shifted_tree_chain(&triangle(), &config, &operations).expect("trace");
        let detect_index = trace
            .events
            .iter()
            .position(|event| {
                matches!(
                    event.kind,
                    DynamicShiftedTreeChainEventKind::DetectReturned { .. }
                )
            })
            .expect("detect");
        let DynamicShiftedTreeChainEventKind::DetectReturned { edges, .. } =
            &mut trace.events[detect_index].kind
        else {
            panic!("detect");
        };
        edges.clear();
        assert_eq!(
            check_dynamic_shifted_tree_chain_trace(&triangle(), &config, &operations, &trace),
            Err(DynamicShiftedTreeChainError::TraceVerification)
        );
    }

    #[test]
    fn malformed_batches_and_concrete_limits_fail_closed() {
        let duplicate = DynamicShiftedTreeChainOperation::Update {
            coordinates: vec![
                DynamicShiftedTreeChainCoordinateUpdate {
                    edge: 0,
                    length: rational(2, 1),
                    gradient: rational(2, 1),
                },
                DynamicShiftedTreeChainCoordinateUpdate {
                    edge: 0,
                    length: rational(3, 1),
                    gradient: rational(2, 1),
                },
            ],
            eta: rational(1, 1),
        };
        assert_eq!(
            execute_dynamic_shifted_tree_chain(&triangle(), &config(vec![1, 1]), &[duplicate]),
            Err(DynamicShiftedTreeChainError::InvalidInput)
        );
        let mut invalid = config(vec![1]);
        assert_eq!(
            execute_dynamic_shifted_tree_chain(&triangle(), &invalid, &[]),
            Err(DynamicShiftedTreeChainError::InvalidInput)
        );
        invalid.rebuild_after_updates = vec![DYNAMIC_SHIFTED_TREE_CHAIN_MAX_REBUILD_LIMIT + 1, 1];
        assert_eq!(
            execute_dynamic_shifted_tree_chain(&triangle(), &invalid, &[]),
            Err(DynamicShiftedTreeChainError::AdmissionLimit)
        );
    }

    #[test]
    fn pass_exhaustion_remains_explicit_across_dynamic_stage() {
        let mut config = config(vec![100, 100]);
        config.kappa_alpha = rational(1, 1);
        assert_eq!(
            execute_dynamic_shifted_tree_chain(&triangle(), &config, &[update(0, 2)]),
            Err(DynamicShiftedTreeChainError::StrategyExhausted)
        );
    }
}
