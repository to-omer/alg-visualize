//! Exact fixed-topology flow tracker from Definition 4.5.
//!
//! This bounded primitive implements the flow-maintenance part of Dynamic
//! Min-Ratio Cycle with Hidden Stability in van den Brand et al., "A
//! Deterministic Almost-Linear Time Algorithm for Minimum-Cost Flow"
//! (arXiv:2309.16629v1). An update applies sparse gradient/length changes,
//! checks a supplied circulation `Delta`, computes
//! `beta = eta / <g, Delta>`, and performs `f <- f - beta Delta`. `Query(e)`
//! returns the exact maintained coordinate. `Detect()` returns precisely the
//! edges whose current length times accumulated absolute normalized updates
//! since their previous detection reaches `epsilon`, then resets those
//! accumulators.
//!
//! The source data structure chooses `Delta` through Algorithm 2 and supports
//! dynamic topology with link-cut trees. This primitive deliberately accepts
//! `Delta` as an input and keeps topology fixed; it does not claim hidden
//! stability, approximation quality, or the paper's running time.

use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

/// Maximum vertices in the fixed-topology bounded primitive.
pub const DYNAMIC_FLOW_TRACKER_MAX_NODES: usize = 8;
/// Maximum edges in the fixed-topology bounded primitive.
pub const DYNAMIC_FLOW_TRACKER_MAX_EDGES: usize = 12;
/// Maximum requested operations in one execution.
pub const DYNAMIC_FLOW_TRACKER_MAX_OPERATIONS: usize = 256;
/// Maximum trace boundaries, including completion.
pub const DYNAMIC_FLOW_TRACKER_MAX_TRACE_EVENTS: usize = 257;
/// Maximum numerator or denominator width for every exact scalar.
pub const DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS: u64 = 512;

const CATALOG_ID: &str = "dynamic-flow-tracker";

/// One directed edge with the currently observable optimization attributes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerEdge {
    /// Tail vertex.
    pub from: usize,
    /// Head vertex.
    pub to: usize,
    /// Exact positive length.
    pub length: BigRational,
    /// Exact signed gradient.
    pub gradient: BigRational,
}

/// Fixed graph accepted by the bounded tracker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerGraph {
    /// Number of stable vertices.
    pub node_count: usize,
    /// Directed edges in stable query/detection order.
    pub edges: Vec<DynamicFlowTrackerEdge>,
}

/// One sparse observable-coordinate replacement in an update batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerCoordinateUpdate {
    /// Stable edge index. Updates must be strictly increasing by this field.
    pub edge: usize,
    /// Replacement exact positive length.
    pub length: BigRational,
    /// Replacement exact signed gradient.
    pub gradient: BigRational,
}

/// One Definition 4.5 operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicFlowTrackerOperation {
    /// Apply attributes, normalize the supplied circulation, and update flow.
    Update {
        /// Sparse replacements for the current attributes.
        coordinates: Vec<DynamicFlowTrackerCoordinateUpdate>,
        /// Supplied min-ratio circulation in stable edge order.
        delta: Vec<BigRational>,
        /// Exact positive target progress parameter.
        eta: BigRational,
    },
    /// Return one maintained flow coordinate.
    Query {
        /// Stable edge index.
        edge: usize,
    },
    /// Return and reset all currently detectable edges.
    Detect,
}

/// Exact externally visible response from a query or detection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicFlowTrackerResponse {
    /// Exact flow-coordinate query response.
    Query {
        /// Stable edge index.
        edge: usize,
        /// Current exact maintained flow.
        flow: BigRational,
    },
    /// Stable detection set at the current update stage.
    Detect {
        /// Number of completed updates.
        stage: u64,
        /// Detected edges in increasing stable order.
        edges: Vec<usize>,
    },
}

/// Exact work counters for the fixed-topology realization.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicFlowTrackerMetrics {
    /// Completed update calls.
    pub updates: u64,
    /// Completed coordinate queries.
    pub queries: u64,
    /// Completed detection calls.
    pub detect_calls: u64,
    /// Nonzero normalized coordinates applied to the maintained flow.
    pub flow_coordinate_updates: u64,
    /// Edge threshold checks performed by `Detect`.
    pub detection_edge_scans: u64,
    /// Total edges returned by `Detect`.
    pub detected_edges: u64,
    /// Reversible public state transitions.
    pub state_transitions: u64,
}

/// Complete state at one operation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerSnapshot {
    /// Number of applied update stages.
    pub stage: u64,
    /// Current exact lengths in stable edge order.
    pub lengths: Vec<BigRational>,
    /// Current exact gradients in stable edge order.
    pub gradients: Vec<BigRational>,
    /// Maintained flow, initialized at zero.
    pub flow: Vec<BigRational>,
    /// Sum of absolute normalized updates since each edge's last detection.
    pub undetected_absolute_update: Vec<BigRational>,
    /// Last update stage at which each edge was returned, if any.
    pub last_detected_stage: Vec<Option<u64>>,
    /// Whether the completion boundary was emitted.
    pub complete: bool,
    /// Exact work counters.
    pub metrics: DynamicFlowTrackerMetrics,
}

/// Source meaning of one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DynamicFlowTrackerEventKind {
    /// One normalized circulation was applied.
    UpdateApplied {
        /// New update stage.
        stage: u64,
        /// Exact `<g, Delta>` after coordinate replacements.
        gradient_dot: BigRational,
        /// Exact `eta / <g, Delta>`.
        beta: BigRational,
        /// Exact vector accumulated by `Detect`; flow subtracts this vector.
        normalized_delta: Vec<BigRational>,
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

/// One fully reversible event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerTraceEvent {
    /// Stable component identity.
    pub catalog_id: &'static str,
    /// Source-level event meaning.
    pub kind: DynamicFlowTrackerEventKind,
    /// State before the event.
    pub before: DynamicFlowTrackerSnapshot,
    /// State after the event.
    pub after: DynamicFlowTrackerSnapshot,
}

/// Exact fast result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerResult {
    /// Query/detect responses in request order.
    pub responses: Vec<DynamicFlowTrackerResponse>,
    /// Terminal state.
    pub final_snapshot: DynamicFlowTrackerSnapshot,
}

/// Complete reversible execution transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicFlowTrackerTraceResult {
    /// Initial zero-flow state.
    pub base_snapshot: DynamicFlowTrackerSnapshot,
    /// One event per operation, then completion.
    pub events: Vec<DynamicFlowTrackerTraceEvent>,
    /// Exact externally visible result.
    pub result: DynamicFlowTrackerResult,
}

/// Explicit bounded-tracker failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicFlowTrackerError {
    /// The graph, epsilon, or operation shape is invalid.
    #[error("dynamic flow tracker input is invalid")]
    InvalidInput,
    /// The graph, operation count, or exact scalar exceeds the published band.
    #[error("dynamic flow tracker exceeds its admission band")]
    AdmissionLimit,
    /// A supplied update is not a nonzero negative-gradient circulation.
    #[error("dynamic flow tracker update invariant failed")]
    UpdateInvariant,
    /// Checked work accounting overflowed.
    #[error("dynamic flow tracker arithmetic overflow")]
    ArithmeticOverflow,
    /// A supplied transcript is not the exact stable replay.
    #[error("dynamic flow tracker trace verification failed")]
    TraceVerification,
}

struct InternalRun {
    base_snapshot: DynamicFlowTrackerSnapshot,
    events: Vec<DynamicFlowTrackerTraceEvent>,
    result: DynamicFlowTrackerResult,
}

type OperationOutcome = (
    DynamicFlowTrackerEventKind,
    Option<DynamicFlowTrackerResponse>,
);

/// Executes Definition 4.5 flow tracking without recording trace events.
///
/// # Errors
///
/// Rejects invalid/admission-exceeding input, a malformed circulation update,
/// or checked work overflow.
pub fn execute_dynamic_flow_tracker(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
) -> Result<DynamicFlowTrackerResult, DynamicFlowTrackerError> {
    run_internal(graph, epsilon, operations, false).map(|run| run.result)
}

/// Records every update, query, detection, and completion boundary.
///
/// # Errors
///
/// Returns an execution failure or independent replay-checker failure.
pub fn trace_dynamic_flow_tracker(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
) -> Result<DynamicFlowTrackerTraceResult, DynamicFlowTrackerError> {
    let run = run_internal(graph, epsilon, operations, true)?;
    let trace = DynamicFlowTrackerTraceResult {
        base_snapshot: run.base_snapshot,
        events: run.events,
        result: run.result,
    };
    check_dynamic_flow_tracker_trace(graph, epsilon, operations, &trace)?;
    Ok(trace)
}

/// Independently reconstructs exact state, responses, metrics, and events.
///
/// This checker does not call the production runner or its transition helpers.
///
/// # Errors
///
/// Rejects invalid source input or any supplied transcript drift.
pub fn check_dynamic_flow_tracker_trace(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
    trace: &DynamicFlowTrackerTraceResult,
) -> Result<(), DynamicFlowTrackerError> {
    audit_input(graph, epsilon, operations)?;
    let mut snapshot = audit_base_snapshot(graph);
    if trace.base_snapshot != snapshot
        || trace.events.len()
            != operations
                .len()
                .checked_add(1)
                .ok_or(DynamicFlowTrackerError::TraceVerification)?
    {
        return Err(DynamicFlowTrackerError::TraceVerification);
    }
    let mut responses = Vec::new();
    for (operation, event) in operations.iter().zip(&trace.events) {
        if event.catalog_id != CATALOG_ID || event.before != snapshot {
            return Err(DynamicFlowTrackerError::TraceVerification);
        }
        let mut after = snapshot.clone();
        let (kind, response) = audit_apply(graph, epsilon, operation, &mut after)?;
        after.metrics.state_transitions = audit_increment(after.metrics.state_transitions)?;
        if event.kind != kind || event.after != after {
            return Err(DynamicFlowTrackerError::TraceVerification);
        }
        if let Some(response) = response {
            responses.push(response);
        }
        snapshot = after;
    }
    let completion = trace
        .events
        .last()
        .ok_or(DynamicFlowTrackerError::TraceVerification)?;
    let mut final_snapshot = snapshot.clone();
    final_snapshot.complete = true;
    final_snapshot.metrics.state_transitions =
        audit_increment(final_snapshot.metrics.state_transitions)?;
    if completion.catalog_id != CATALOG_ID
        || completion.before != snapshot
        || completion.kind != DynamicFlowTrackerEventKind::Completed
        || completion.after != final_snapshot
        || trace.result.responses != responses
        || trace.result.final_snapshot != final_snapshot
    {
        return Err(DynamicFlowTrackerError::TraceVerification);
    }
    Ok(())
}

fn run_internal(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
    record: bool,
) -> Result<InternalRun, DynamicFlowTrackerError> {
    validate_input(graph, epsilon, operations)?;
    let mut snapshot = base_snapshot(graph);
    let base_snapshot = snapshot.clone();
    let mut responses = Vec::new();
    let event_capacity = operations
        .len()
        .checked_add(1)
        .ok_or(DynamicFlowTrackerError::ArithmeticOverflow)?;
    let mut events = Vec::with_capacity(if record { event_capacity } else { 0 });
    for operation in operations {
        let before = snapshot.clone();
        let (kind, response) = apply_operation(graph, epsilon, operation, &mut snapshot)?;
        snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
        if let Some(response) = response {
            responses.push(response);
        }
        if record {
            events.push(DynamicFlowTrackerTraceEvent {
                catalog_id: CATALOG_ID,
                kind,
                before,
                after: snapshot.clone(),
            });
        }
    }
    let before = snapshot.clone();
    snapshot.complete = true;
    snapshot.metrics.state_transitions = increment(snapshot.metrics.state_transitions)?;
    if record {
        events.push(DynamicFlowTrackerTraceEvent {
            catalog_id: CATALOG_ID,
            kind: DynamicFlowTrackerEventKind::Completed,
            before,
            after: snapshot.clone(),
        });
    }
    Ok(InternalRun {
        base_snapshot,
        events,
        result: DynamicFlowTrackerResult {
            responses,
            final_snapshot: snapshot,
        },
    })
}

fn base_snapshot(graph: &DynamicFlowTrackerGraph) -> DynamicFlowTrackerSnapshot {
    let edge_count = graph.edges.len();
    DynamicFlowTrackerSnapshot {
        stage: 0,
        lengths: graph.edges.iter().map(|edge| edge.length.clone()).collect(),
        gradients: graph
            .edges
            .iter()
            .map(|edge| edge.gradient.clone())
            .collect(),
        flow: vec![BigRational::zero(); edge_count],
        undetected_absolute_update: vec![BigRational::zero(); edge_count],
        last_detected_stage: vec![None; edge_count],
        complete: false,
        metrics: DynamicFlowTrackerMetrics::default(),
    }
}

fn apply_operation(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operation: &DynamicFlowTrackerOperation,
    snapshot: &mut DynamicFlowTrackerSnapshot,
) -> Result<OperationOutcome, DynamicFlowTrackerError> {
    match operation {
        DynamicFlowTrackerOperation::Update {
            coordinates,
            delta,
            eta,
        } => apply_update(graph, coordinates, delta, eta, snapshot).map(|kind| (kind, None)),
        DynamicFlowTrackerOperation::Query { edge } => {
            let flow = snapshot
                .flow
                .get(*edge)
                .ok_or(DynamicFlowTrackerError::InvalidInput)?
                .clone();
            snapshot.metrics.queries = increment(snapshot.metrics.queries)?;
            Ok((
                DynamicFlowTrackerEventKind::QueryReturned {
                    edge: *edge,
                    flow: flow.clone(),
                },
                Some(DynamicFlowTrackerResponse::Query { edge: *edge, flow }),
            ))
        }
        DynamicFlowTrackerOperation::Detect => {
            let edges = detect_edges(epsilon, snapshot)?;
            let response = DynamicFlowTrackerResponse::Detect {
                stage: snapshot.stage,
                edges: edges.clone(),
            };
            Ok((
                DynamicFlowTrackerEventKind::DetectReturned {
                    stage: snapshot.stage,
                    edges,
                },
                Some(response),
            ))
        }
    }
}

fn apply_update(
    graph: &DynamicFlowTrackerGraph,
    coordinates: &[DynamicFlowTrackerCoordinateUpdate],
    delta: &[BigRational],
    eta: &BigRational,
    snapshot: &mut DynamicFlowTrackerSnapshot,
) -> Result<DynamicFlowTrackerEventKind, DynamicFlowTrackerError> {
    for update in coordinates {
        snapshot.lengths[update.edge] = update.length.clone();
        snapshot.gradients[update.edge] = update.gradient.clone();
    }
    ensure_circulation(graph, delta)?;
    let gradient_dot = snapshot
        .gradients
        .iter()
        .zip(delta)
        .fold(BigRational::zero(), |sum, (gradient, value)| {
            sum + gradient * value
        });
    if gradient_dot >= BigRational::zero() || delta.iter().all(BigRational::is_zero) {
        return Err(DynamicFlowTrackerError::UpdateInvariant);
    }
    let beta = eta / &gradient_dot;
    let normalized_delta = delta.iter().map(|value| &beta * value).collect::<Vec<_>>();
    ensure_scalar_band(normalized_delta.iter())?;
    for ((flow, accumulator), value) in snapshot
        .flow
        .iter_mut()
        .zip(&mut snapshot.undetected_absolute_update)
        .zip(&normalized_delta)
    {
        *flow -= value;
        *accumulator += value.abs();
        if !value.is_zero() {
            snapshot.metrics.flow_coordinate_updates =
                increment(snapshot.metrics.flow_coordinate_updates)?;
        }
    }
    ensure_scalar_band(snapshot.flow.iter())?;
    ensure_scalar_band(snapshot.undetected_absolute_update.iter())?;
    snapshot.stage = increment(snapshot.stage)?;
    snapshot.metrics.updates = increment(snapshot.metrics.updates)?;
    Ok(DynamicFlowTrackerEventKind::UpdateApplied {
        stage: snapshot.stage,
        gradient_dot,
        beta,
        normalized_delta,
    })
}

fn detect_edges(
    epsilon: &BigRational,
    snapshot: &mut DynamicFlowTrackerSnapshot,
) -> Result<Vec<usize>, DynamicFlowTrackerError> {
    let mut detected = Vec::new();
    for edge in 0..snapshot.lengths.len() {
        snapshot.metrics.detection_edge_scans = increment(snapshot.metrics.detection_edge_scans)?;
        if &snapshot.lengths[edge] * &snapshot.undetected_absolute_update[edge] >= *epsilon {
            detected.push(edge);
        }
    }
    for &edge in &detected {
        snapshot.undetected_absolute_update[edge] = BigRational::zero();
        snapshot.last_detected_stage[edge] = Some(snapshot.stage);
        snapshot.metrics.detected_edges = increment(snapshot.metrics.detected_edges)?;
    }
    snapshot.metrics.detect_calls = increment(snapshot.metrics.detect_calls)?;
    Ok(detected)
}

fn validate_input(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
) -> Result<(), DynamicFlowTrackerError> {
    if graph.node_count == 0 || graph.edges.is_empty() || epsilon <= &BigRational::zero() {
        return Err(DynamicFlowTrackerError::InvalidInput);
    }
    if graph.node_count > DYNAMIC_FLOW_TRACKER_MAX_NODES
        || graph.edges.len() > DYNAMIC_FLOW_TRACKER_MAX_EDGES
        || operations.len() > DYNAMIC_FLOW_TRACKER_MAX_OPERATIONS
        || rational_too_wide(epsilon)
    {
        return Err(DynamicFlowTrackerError::AdmissionLimit);
    }
    for edge in &graph.edges {
        if edge.from >= graph.node_count
            || edge.to >= graph.node_count
            || edge.length <= BigRational::zero()
        {
            return Err(DynamicFlowTrackerError::InvalidInput);
        }
        if rational_too_wide(&edge.length) || rational_too_wide(&edge.gradient) {
            return Err(DynamicFlowTrackerError::AdmissionLimit);
        }
    }
    for operation in operations {
        validate_operation(graph.edges.len(), operation)?;
    }
    Ok(())
}

fn validate_operation(
    edge_count: usize,
    operation: &DynamicFlowTrackerOperation,
) -> Result<(), DynamicFlowTrackerError> {
    match operation {
        DynamicFlowTrackerOperation::Update {
            coordinates,
            delta,
            eta,
        } => {
            if delta.len() != edge_count || eta <= &BigRational::zero() {
                return Err(DynamicFlowTrackerError::InvalidInput);
            }
            let mut previous = None;
            for update in coordinates {
                if update.edge >= edge_count
                    || previous.is_some_and(|edge| edge >= update.edge)
                    || update.length <= BigRational::zero()
                {
                    return Err(DynamicFlowTrackerError::InvalidInput);
                }
                previous = Some(update.edge);
                if rational_too_wide(&update.length) || rational_too_wide(&update.gradient) {
                    return Err(DynamicFlowTrackerError::AdmissionLimit);
                }
            }
            ensure_scalar_band(delta.iter())?;
            ensure_scalar_band(std::iter::once(eta))
        }
        DynamicFlowTrackerOperation::Query { edge } if *edge >= edge_count => {
            Err(DynamicFlowTrackerError::InvalidInput)
        }
        DynamicFlowTrackerOperation::Query { .. } | DynamicFlowTrackerOperation::Detect => Ok(()),
    }
}

fn ensure_circulation(
    graph: &DynamicFlowTrackerGraph,
    delta: &[BigRational],
) -> Result<(), DynamicFlowTrackerError> {
    if delta.len() != graph.edges.len() {
        return Err(DynamicFlowTrackerError::UpdateInvariant);
    }
    let mut divergence = vec![BigRational::zero(); graph.node_count];
    for (edge, value) in graph.edges.iter().zip(delta) {
        divergence[edge.from] += value;
        divergence[edge.to] -= value;
    }
    if divergence.iter().any(|value| !value.is_zero()) {
        return Err(DynamicFlowTrackerError::UpdateInvariant);
    }
    Ok(())
}

fn ensure_scalar_band<'a>(
    values: impl IntoIterator<Item = &'a BigRational>,
) -> Result<(), DynamicFlowTrackerError> {
    if values.into_iter().any(rational_too_wide) {
        return Err(DynamicFlowTrackerError::AdmissionLimit);
    }
    Ok(())
}

fn rational_too_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS
}

fn increment(value: u64) -> Result<u64, DynamicFlowTrackerError> {
    value
        .checked_add(1)
        .ok_or(DynamicFlowTrackerError::ArithmeticOverflow)
}

// The checker intentionally duplicates input validation and every transition.
fn audit_input(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operations: &[DynamicFlowTrackerOperation],
) -> Result<(), DynamicFlowTrackerError> {
    let valid_shape = graph.node_count > 0
        && graph.node_count <= DYNAMIC_FLOW_TRACKER_MAX_NODES
        && !graph.edges.is_empty()
        && graph.edges.len() <= DYNAMIC_FLOW_TRACKER_MAX_EDGES
        && operations.len() <= DYNAMIC_FLOW_TRACKER_MAX_OPERATIONS
        && operations.len() < DYNAMIC_FLOW_TRACKER_MAX_TRACE_EVENTS
        && epsilon > &BigRational::zero()
        && !audit_wide(epsilon);
    if !valid_shape {
        return Err(DynamicFlowTrackerError::TraceVerification);
    }
    for edge in &graph.edges {
        if edge.from >= graph.node_count
            || edge.to >= graph.node_count
            || edge.length <= BigRational::zero()
            || audit_wide(&edge.length)
            || audit_wide(&edge.gradient)
        {
            return Err(DynamicFlowTrackerError::TraceVerification);
        }
    }
    for operation in operations {
        match operation {
            DynamicFlowTrackerOperation::Update {
                coordinates,
                delta,
                eta,
            } => {
                if delta.len() != graph.edges.len()
                    || eta <= &BigRational::zero()
                    || audit_wide(eta)
                    || delta.iter().any(audit_wide)
                {
                    return Err(DynamicFlowTrackerError::TraceVerification);
                }
                let mut previous = None;
                for update in coordinates {
                    if update.edge >= graph.edges.len()
                        || previous.is_some_and(|edge| edge >= update.edge)
                        || update.length <= BigRational::zero()
                        || audit_wide(&update.length)
                        || audit_wide(&update.gradient)
                    {
                        return Err(DynamicFlowTrackerError::TraceVerification);
                    }
                    previous = Some(update.edge);
                }
            }
            DynamicFlowTrackerOperation::Query { edge } if *edge >= graph.edges.len() => {
                return Err(DynamicFlowTrackerError::TraceVerification);
            }
            DynamicFlowTrackerOperation::Query { .. } | DynamicFlowTrackerOperation::Detect => {}
        }
    }
    Ok(())
}

fn audit_base_snapshot(graph: &DynamicFlowTrackerGraph) -> DynamicFlowTrackerSnapshot {
    DynamicFlowTrackerSnapshot {
        stage: 0,
        lengths: graph.edges.iter().map(|edge| edge.length.clone()).collect(),
        gradients: graph
            .edges
            .iter()
            .map(|edge| edge.gradient.clone())
            .collect(),
        flow: vec![BigRational::zero(); graph.edges.len()],
        undetected_absolute_update: vec![BigRational::zero(); graph.edges.len()],
        last_detected_stage: vec![None; graph.edges.len()],
        complete: false,
        metrics: DynamicFlowTrackerMetrics::default(),
    }
}

fn audit_apply(
    graph: &DynamicFlowTrackerGraph,
    epsilon: &BigRational,
    operation: &DynamicFlowTrackerOperation,
    snapshot: &mut DynamicFlowTrackerSnapshot,
) -> Result<OperationOutcome, DynamicFlowTrackerError> {
    match operation {
        DynamicFlowTrackerOperation::Update {
            coordinates,
            delta,
            eta,
        } => {
            for update in coordinates {
                snapshot.lengths[update.edge] = update.length.clone();
                snapshot.gradients[update.edge] = update.gradient.clone();
            }
            let mut divergence = vec![BigRational::zero(); graph.node_count];
            for (edge, value) in graph.edges.iter().zip(delta) {
                divergence[edge.from] += value;
                divergence[edge.to] -= value;
            }
            let dot = snapshot
                .gradients
                .iter()
                .zip(delta)
                .map(|(gradient, value)| gradient * value)
                .sum::<BigRational>();
            if divergence.iter().any(|value| !value.is_zero())
                || delta.iter().all(BigRational::is_zero)
                || dot >= BigRational::zero()
            {
                return Err(DynamicFlowTrackerError::TraceVerification);
            }
            let beta = eta / &dot;
            let normalized = delta.iter().map(|value| &beta * value).collect::<Vec<_>>();
            if normalized.iter().any(audit_wide) {
                return Err(DynamicFlowTrackerError::TraceVerification);
            }
            for (edge, value) in normalized.iter().enumerate() {
                snapshot.flow[edge] -= value;
                snapshot.undetected_absolute_update[edge] += value.abs();
                if !value.is_zero() {
                    snapshot.metrics.flow_coordinate_updates =
                        audit_increment(snapshot.metrics.flow_coordinate_updates)?;
                }
            }
            if snapshot.flow.iter().any(audit_wide)
                || snapshot.undetected_absolute_update.iter().any(audit_wide)
            {
                return Err(DynamicFlowTrackerError::TraceVerification);
            }
            snapshot.stage = audit_increment(snapshot.stage)?;
            snapshot.metrics.updates = audit_increment(snapshot.metrics.updates)?;
            Ok((
                DynamicFlowTrackerEventKind::UpdateApplied {
                    stage: snapshot.stage,
                    gradient_dot: dot,
                    beta,
                    normalized_delta: normalized,
                },
                None,
            ))
        }
        DynamicFlowTrackerOperation::Query { edge } => {
            let flow = snapshot
                .flow
                .get(*edge)
                .ok_or(DynamicFlowTrackerError::TraceVerification)?
                .clone();
            snapshot.metrics.queries = audit_increment(snapshot.metrics.queries)?;
            Ok((
                DynamicFlowTrackerEventKind::QueryReturned {
                    edge: *edge,
                    flow: flow.clone(),
                },
                Some(DynamicFlowTrackerResponse::Query { edge: *edge, flow }),
            ))
        }
        DynamicFlowTrackerOperation::Detect => {
            let mut edges = Vec::new();
            for edge in 0..snapshot.lengths.len() {
                snapshot.metrics.detection_edge_scans =
                    audit_increment(snapshot.metrics.detection_edge_scans)?;
                if &snapshot.lengths[edge] * &snapshot.undetected_absolute_update[edge] >= *epsilon
                {
                    edges.push(edge);
                }
            }
            for &edge in &edges {
                snapshot.undetected_absolute_update[edge] = BigRational::zero();
                snapshot.last_detected_stage[edge] = Some(snapshot.stage);
                snapshot.metrics.detected_edges = audit_increment(snapshot.metrics.detected_edges)?;
            }
            snapshot.metrics.detect_calls = audit_increment(snapshot.metrics.detect_calls)?;
            Ok((
                DynamicFlowTrackerEventKind::DetectReturned {
                    stage: snapshot.stage,
                    edges: edges.clone(),
                },
                Some(DynamicFlowTrackerResponse::Detect {
                    stage: snapshot.stage,
                    edges,
                }),
            ))
        }
    }
}

fn audit_increment(value: u64) -> Result<u64, DynamicFlowTrackerError> {
    value
        .checked_add(1)
        .ok_or(DynamicFlowTrackerError::TraceVerification)
}

fn audit_wide(value: &BigRational) -> bool {
    value.numer().bits() > DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS
        || value.denom().bits() > DYNAMIC_FLOW_TRACKER_MAX_RATIONAL_BITS
}

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;

    fn rational(numerator: i64, denominator: i64) -> BigRational {
        BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
    }

    fn triangle() -> DynamicFlowTrackerGraph {
        DynamicFlowTrackerGraph {
            node_count: 3,
            edges: vec![
                DynamicFlowTrackerEdge {
                    from: 0,
                    to: 1,
                    length: rational(1, 1),
                    gradient: rational(-2, 1),
                },
                DynamicFlowTrackerEdge {
                    from: 1,
                    to: 2,
                    length: rational(1, 1),
                    gradient: rational(0, 1),
                },
                DynamicFlowTrackerEdge {
                    from: 2,
                    to: 0,
                    length: rational(1, 1),
                    gradient: rational(0, 1),
                },
            ],
        }
    }

    fn update() -> DynamicFlowTrackerOperation {
        DynamicFlowTrackerOperation::Update {
            coordinates: Vec::new(),
            delta: vec![rational(1, 1); 3],
            eta: rational(1, 1),
        }
    }

    #[test]
    fn applies_exact_beta_queries_and_resets_detection_accumulators() {
        let operations = vec![
            update(),
            DynamicFlowTrackerOperation::Detect,
            update(),
            DynamicFlowTrackerOperation::Query { edge: 1 },
            DynamicFlowTrackerOperation::Detect,
            DynamicFlowTrackerOperation::Detect,
        ];
        let result = execute_dynamic_flow_tracker(&triangle(), &rational(1, 1), &operations)
            .expect("tracker");
        assert_eq!(result.final_snapshot.flow, vec![rational(1, 1); 3]);
        assert_eq!(
            result.responses,
            vec![
                DynamicFlowTrackerResponse::Detect {
                    stage: 1,
                    edges: Vec::new(),
                },
                DynamicFlowTrackerResponse::Query {
                    edge: 1,
                    flow: rational(1, 1),
                },
                DynamicFlowTrackerResponse::Detect {
                    stage: 2,
                    edges: vec![0, 1, 2],
                },
                DynamicFlowTrackerResponse::Detect {
                    stage: 2,
                    edges: Vec::new(),
                },
            ]
        );
        assert_eq!(result.final_snapshot.last_detected_stage, vec![Some(2); 3]);
        assert_eq!(result.final_snapshot.metrics.flow_coordinate_updates, 6);
        assert_eq!(result.final_snapshot.metrics.detection_edge_scans, 9);
    }

    #[test]
    fn current_length_and_gradient_replacements_apply_before_the_step() {
        let operation = DynamicFlowTrackerOperation::Update {
            coordinates: vec![DynamicFlowTrackerCoordinateUpdate {
                edge: 0,
                length: rational(2, 1),
                gradient: rational(-4, 1),
            }],
            delta: vec![rational(1, 1); 3],
            eta: rational(1, 1),
        };
        let trace = trace_dynamic_flow_tracker(
            &triangle(),
            &rational(1, 2),
            &[operation, DynamicFlowTrackerOperation::Detect],
        )
        .expect("trace");
        let DynamicFlowTrackerEventKind::UpdateApplied {
            gradient_dot, beta, ..
        } = &trace.events[0].kind
        else {
            panic!("update");
        };
        assert_eq!(gradient_dot, &rational(-4, 1));
        assert_eq!(beta, &rational(-1, 4));
        assert_eq!(
            trace.result.responses,
            vec![DynamicFlowTrackerResponse::Detect {
                stage: 1,
                edges: vec![0],
            }]
        );
    }

    #[test]
    fn fast_trace_and_independent_checker_match() {
        let operations = vec![
            update(),
            DynamicFlowTrackerOperation::Query { edge: 0 },
            DynamicFlowTrackerOperation::Detect,
        ];
        let fast =
            execute_dynamic_flow_tracker(&triangle(), &rational(1, 2), &operations).expect("fast");
        let trace =
            trace_dynamic_flow_tracker(&triangle(), &rational(1, 2), &operations).expect("trace");
        assert_eq!(fast, trace.result);
        assert_eq!(trace.events.len(), operations.len() + 1);
        check_dynamic_flow_tracker_trace(&triangle(), &rational(1, 2), &operations, &trace)
            .expect("check");
    }

    #[test]
    fn checker_rejects_normalization_detection_and_snapshot_tampering() {
        let operations = vec![update(), DynamicFlowTrackerOperation::Detect];
        let mut trace =
            trace_dynamic_flow_tracker(&triangle(), &rational(1, 2), &operations).expect("trace");
        let DynamicFlowTrackerEventKind::UpdateApplied { beta, .. } = &mut trace.events[0].kind
        else {
            panic!("update");
        };
        *beta = rational(-1, 3);
        assert_eq!(
            check_dynamic_flow_tracker_trace(&triangle(), &rational(1, 2), &operations, &trace),
            Err(DynamicFlowTrackerError::TraceVerification)
        );

        let mut trace =
            trace_dynamic_flow_tracker(&triangle(), &rational(1, 2), &operations).expect("trace");
        trace.events[1].after.last_detected_stage[0] = None;
        assert_eq!(
            check_dynamic_flow_tracker_trace(&triangle(), &rational(1, 2), &operations, &trace),
            Err(DynamicFlowTrackerError::TraceVerification)
        );
    }

    #[test]
    fn rejects_noncirculation_nonnegative_objective_and_bad_sparse_order() {
        let noncirculation = DynamicFlowTrackerOperation::Update {
            coordinates: Vec::new(),
            delta: vec![rational(1, 1), rational(0, 1), rational(0, 1)],
            eta: rational(1, 1),
        };
        assert_eq!(
            execute_dynamic_flow_tracker(&triangle(), &rational(1, 1), &[noncirculation]),
            Err(DynamicFlowTrackerError::UpdateInvariant)
        );

        let nonnegative = DynamicFlowTrackerOperation::Update {
            coordinates: vec![DynamicFlowTrackerCoordinateUpdate {
                edge: 0,
                length: rational(1, 1),
                gradient: rational(2, 1),
            }],
            delta: vec![rational(1, 1); 3],
            eta: rational(1, 1),
        };
        assert_eq!(
            execute_dynamic_flow_tracker(&triangle(), &rational(1, 1), &[nonnegative]),
            Err(DynamicFlowTrackerError::UpdateInvariant)
        );

        let duplicate = DynamicFlowTrackerOperation::Update {
            coordinates: vec![
                DynamicFlowTrackerCoordinateUpdate {
                    edge: 1,
                    length: rational(1, 1),
                    gradient: rational(0, 1),
                },
                DynamicFlowTrackerCoordinateUpdate {
                    edge: 1,
                    length: rational(1, 1),
                    gradient: rational(0, 1),
                },
            ],
            delta: vec![rational(1, 1); 3],
            eta: rational(1, 1),
        };
        assert_eq!(
            execute_dynamic_flow_tracker(&triangle(), &rational(1, 1), &[duplicate]),
            Err(DynamicFlowTrackerError::InvalidInput)
        );
    }

    #[test]
    fn admission_limits_are_closed() {
        let mut graph = triangle();
        graph.node_count = DYNAMIC_FLOW_TRACKER_MAX_NODES + 1;
        assert_eq!(
            execute_dynamic_flow_tracker(&graph, &rational(1, 1), &[]),
            Err(DynamicFlowTrackerError::AdmissionLimit)
        );
        assert_eq!(
            execute_dynamic_flow_tracker(&triangle(), &BigRational::zero(), &[]),
            Err(DynamicFlowTrackerError::InvalidInput)
        );
    }
}
