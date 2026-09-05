//! Lower-bound and balance feasibility construction with explicit witnesses.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, divergences};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeId, NodeIndex};

const FEASIBILITY_MAX_TRACE_EVENTS: usize = 2_000_000;

/// Stable identity of an original or artificial feasibility node.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FeasibilityNodeId {
    /// A node from the input network.
    Original(NodeId),
    /// Artificial source used to route positive residual imbalance.
    SuperSource,
    /// Artificial sink used to receive negative residual imbalance.
    SuperSink,
}

/// Stable identity of one logical auxiliary edge.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FeasibilityArcId {
    /// Shifted residual width of an original edge.
    Original(EdgeId),
    /// Temporary sink-to-source edge used by lower-bounded maximum flow.
    LowerBoundReturn {
        /// Original node at the tail of the temporary return edge.
        from: NodeId,
        /// Original node at the head of the temporary return edge.
        to: NodeId,
    },
    /// Artificial edge from the super-source to an original node.
    FromSuperSource(NodeId),
    /// Artificial edge from an original node to the super-sink.
    ToSuperSink(NodeId),
}

/// Direction of a residual adjacency entry around a logical auxiliary edge.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FeasibilityResidualDirection {
    /// Adds flow on the logical edge.
    Forward,
    /// Cancels flow on the logical edge.
    Reverse,
}

/// Stable identity of one inspected auxiliary residual adjacency entry.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FeasibilityResidualArcId {
    /// Logical auxiliary edge shared by both residual directions.
    pub arc: FeasibilityArcId,
    /// Residual direction inspected or pushed.
    pub direction: FeasibilityResidualDirection,
}

/// Exact work counters for the hidden feasibility Push--Relabel kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FeasibilityTraceMetrics {
    /// Original edges inspected while shifting lower bounds.
    pub original_edge_inspections: u128,
    /// Original nodes inspected while constructing imbalance edges.
    pub original_node_inspections: u128,
    /// Auxiliary residual adjacency entries inspected, including zero capacity.
    pub auxiliary_adjacency_inspections: u128,
    /// Local residual pushes.
    pub pushes: u128,
    /// Strict height increases.
    pub relabels: u128,
    /// FIFO active-node selections.
    pub active_node_selections: u128,
    /// Completed active-node discharges.
    pub discharges: u128,
    /// Residual adjacency entries inspected by the infeasibility-cut BFS.
    pub cut_adjacency_inspections: u128,
    /// Original flows extracted from shifted auxiliary edges.
    pub extracted_original_edges: u128,
}

/// Source-defined operation emitted by the feasibility constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeasibilityTraceEventKind {
    /// Inspects and materializes one lower-shifted original edge.
    AddOriginalArc,
    /// Materializes the temporary sink-to-source circulation edge.
    AddReturnArc,
    /// Inspects one original node's lower-shifted imbalance.
    InspectNodeImbalance,
    /// Materializes one super-terminal imbalance edge.
    AddImbalanceArc,
    /// Assigns the super-source its initial Push--Relabel height.
    InitializeSourceHeight,
    /// Inspects one adjacency entry while saturating the super-source.
    InspectSourceArc,
    /// Appends one newly active original node to the FIFO queue.
    ActivateNode,
    /// Removes one active original node from the FIFO queue.
    SelectActiveNode,
    /// Inspects one current adjacency entry during a discharge.
    InspectDischargeArc,
    /// Inspects one adjacency entry while computing a new height.
    InspectRelabelArc,
    /// Pushes flow through one auxiliary residual adjacency entry.
    Push,
    /// Advances one node's current-arc cursor.
    AdvanceCurrentArc,
    /// Raises one node and resets its current-arc cursor.
    Relabel,
    /// Completes one active-node discharge.
    CompleteDischarge,
    /// Publishes the total amount routed to the super-sink.
    CompleteRouting,
    /// Inspects one adjacency entry during the infeasibility-cut BFS.
    InspectCutArc,
    /// Marks one auxiliary node reachable in the cut BFS.
    MarkReachable,
    /// Checks one projected original flow during result extraction.
    ExtractOriginalFlow,
    /// Declares the transformed circulation feasible.
    Feasible,
    /// Declares the transformed circulation infeasible.
    Infeasible,
}

/// One auxiliary node at a replay boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityNodeState {
    /// Stable node identity used by events and renderers.
    pub id: FeasibilityNodeId,
    /// Current Push--Relabel height.
    pub height: usize,
    /// Current nonnegative auxiliary excess.
    pub excess: u128,
    /// Current adjacency cursor used during discharge.
    pub current_arc: usize,
    /// Whether the node is currently present in the active FIFO queue.
    pub active: bool,
    /// Whether the infeasibility-cut BFS has reached the node.
    pub reachable: bool,
}

/// One logical auxiliary edge at a replay boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityArcState {
    /// Stable logical-edge identity shared by both residual directions.
    pub id: FeasibilityArcId,
    /// Stable tail-node identity of the logical forward edge.
    pub from: FeasibilityNodeId,
    /// Stable head-node identity of the logical forward edge.
    pub to: FeasibilityNodeId,
    /// Immutable logical capacity after lower-bound shifting.
    pub capacity: u128,
    /// Current logical forward flow.
    pub flow: u128,
}

/// Original-edge flow projected while the auxiliary kernel runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityOriginalFlowState {
    /// Original input-edge identity.
    pub edge: EdgeId,
    /// Current original flow, including its lower bound.
    pub flow: u64,
}

/// Complete replay state for the dedicated feasibility overlay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityTraceSnapshot {
    /// Original and artificial node states in canonical order.
    pub nodes: Vec<FeasibilityNodeState>,
    /// Logical auxiliary edges in construction order.
    pub arcs: Vec<FeasibilityArcState>,
    /// FIFO active-node queue from front to back.
    pub active_queue: Vec<FeasibilityNodeId>,
    /// Projection of auxiliary pushes onto original-edge flows.
    pub original_flows: Vec<FeasibilityOriginalFlowState>,
    /// Total capacity leaving the artificial super-source.
    pub total_required: u128,
    /// Amount ultimately delivered to the artificial super-sink.
    pub routed: u128,
    /// Exact cumulative source-work counters.
    pub metrics: FeasibilityTraceMetrics,
}

/// One reversible mutation in a feasibility trace event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeasibilityTracePatch {
    /// Appends a newly materialized logical auxiliary edge.
    AddArc {
        /// Edge state introduced at this boundary.
        arc: FeasibilityArcState,
    },
    /// Changes one node's Push--Relabel height.
    NodeHeight {
        /// Node whose height changes.
        node: FeasibilityNodeId,
        /// Height before the event.
        before: usize,
        /// Height after the event.
        after: usize,
    },
    /// Changes one node's auxiliary excess.
    NodeExcess {
        /// Node whose excess changes.
        node: FeasibilityNodeId,
        /// Excess before the event.
        before: u128,
        /// Excess after the event.
        after: u128,
    },
    /// Changes one node's current-arc cursor.
    NodeCurrentArc {
        /// Node whose cursor changes.
        node: FeasibilityNodeId,
        /// Cursor before the event.
        before: usize,
        /// Cursor after the event.
        after: usize,
    },
    /// Changes one node's active-queue membership bit.
    NodeActive {
        /// Node whose activity changes.
        node: FeasibilityNodeId,
        /// Activity before the event.
        before: bool,
        /// Activity after the event.
        after: bool,
    },
    /// Changes one node's cut-reachability bit.
    NodeReachable {
        /// Node whose reachability changes.
        node: FeasibilityNodeId,
        /// Reachability before the event.
        before: bool,
        /// Reachability after the event.
        after: bool,
    },
    /// Appends one node to the active FIFO queue.
    QueuePushBack {
        /// Appended node.
        node: FeasibilityNodeId,
    },
    /// Removes one node from the front of the active FIFO queue.
    QueuePopFront {
        /// Removed node.
        node: FeasibilityNodeId,
    },
    /// Changes one logical auxiliary edge's forward flow.
    ArcFlow {
        /// Logical edge whose flow changes.
        arc: FeasibilityArcId,
        /// Forward flow before the event.
        before: u128,
        /// Forward flow after the event.
        after: u128,
    },
    /// Changes one projected original-edge flow.
    OriginalFlow {
        /// Original edge whose projected flow changes.
        edge: EdgeId,
        /// Original flow before the event.
        before: u64,
        /// Original flow after the event.
        after: u64,
    },
    /// Changes the constructed total imbalance requirement.
    TotalRequired {
        /// Required amount before the event.
        before: u128,
        /// Required amount after the event.
        after: u128,
    },
    /// Publishes or reverses the routed amount.
    Routed {
        /// Routed amount before the event.
        before: u128,
        /// Routed amount after the event.
        after: u128,
    },
}

/// One locally focused, reversible feasibility operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityTraceEvent {
    /// Source-defined semantic boundary kind.
    pub kind: FeasibilityTraceEventKind,
    /// Locally inspected or changed node, when applicable.
    pub focus_node: Option<FeasibilityNodeId>,
    /// Locally inspected or changed residual adjacency entry, when applicable.
    pub focus_arc: Option<FeasibilityResidualArcId>,
    /// Ordered reversible state mutations owned by this boundary.
    pub patches: Vec<FeasibilityTracePatch>,
    /// Exact cumulative work before this boundary.
    pub metrics_before: FeasibilityTraceMetrics,
    /// Exact cumulative work after this boundary.
    pub metrics: FeasibilityTraceMetrics,
}

/// Complete source trace of one feasibility construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityTrace {
    /// Replay state before the first source operation.
    pub base_snapshot: FeasibilityTraceSnapshot,
    /// Source operations in their actual execution order.
    pub events: Vec<FeasibilityTraceEvent>,
    /// Replay state immediately after the final source operation.
    pub final_snapshot: FeasibilityTraceSnapshot,
}

/// Mathematical outcome retained even when the feasibility cut is negative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeasibilityTraceOutcome {
    /// A canonical feasible original-edge flow was constructed.
    Feasible(FeasibleFlow),
    /// A residual cut proves that the transformed problem is infeasible.
    Infeasible(InfeasibilityWitness),
}

/// Same-execution outcome and dedicated auxiliary trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibilityTraceResult {
    /// Mathematical result of the traced execution.
    pub outcome: FeasibilityTraceOutcome,
    /// Complete reversible source trace from the same execution.
    pub trace: FeasibilityTrace,
}

/// Input contract retained with one feasibility trace captured from an
/// algorithm's actual source execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapturedFeasibilityRequest {
    /// Exact node divergences passed to the circulation constructor.
    Balance {
        /// One required divergence per node in canonical node order.
        required_divergence: Vec<i128>,
    },
    /// Lower-bounded maximum-flow initialization with its temporary return arc.
    MaxFlowInitial {
        /// Original source terminal.
        source: NodeIndex,
        /// Original sink terminal.
        sink: NodeIndex,
    },
}

/// Stable source boundary that owns one transformed feasibility execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedFeasibilityAnchor {
    /// Catalog identity of the enclosing source event.
    pub catalog_id: &'static str,
    /// One-based occurrence of that catalog identity in source order.
    pub occurrence: u64,
}

/// Declared semantic role of one source feasibility invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeasibilityUse {
    /// The constructed original-edge flow becomes the enclosing algorithm's
    /// initial public flow state.
    InitialFlow,
    /// The auxiliary run proves feasibility only; its flow is not adopted by
    /// the enclosing algorithm.
    PrecheckOnly,
    /// A transformed or recovery run occurs immediately before one exact
    /// enclosing source boundary and does not replace public graph state.
    BeforeEvent {
        /// Stable source event position that owns the auxiliary work.
        anchor: CapturedFeasibilityAnchor,
    },
}

/// One same-execution feasibility subroutine retained for timeline composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedFeasibilityTrace {
    /// Exact network passed to the subroutine. Recovery kernels may use a
    /// transformed network rather than the public input graph.
    pub graph: FlowNetwork,
    /// Explicit role and placement contract supplied by the source call site.
    pub use_kind: FeasibilityUse,
    /// Exact input contract used by the captured call.
    pub request: CapturedFeasibilityRequest,
    /// Independently verified reversible trace and mathematical outcome.
    pub result: FeasibilityTraceResult,
}

/// Direction used by the independent feasibility trace replay checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeasibilityTraceDirection {
    /// Applies an event from its declared before-state to its after-state.
    Forward,
    /// Reverses an event from its declared after-state to its before-state.
    Reverse,
}

/// A canonical feasible original-edge flow vector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeasibleFlow {
    /// One flow per original edge in canonical edge-ID order.
    pub flows: Vec<u64>,
}

/// Residual auxiliary cut proving that all required imbalance cannot be routed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InfeasibilityWitness {
    /// Total super-source capacity that could not reach the super-sink.
    pub unsatisfied: u128,
    /// Original nodes reachable from the super-source after maximum flow.
    pub reachable_original_nodes: Vec<NodeId>,
}

/// Feasibility construction failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FeasibilityError {
    /// The required divergence vector has a wrong length or nonzero sum.
    #[error("invalid required-divergence vector")]
    InvalidDivergence,
    /// Source and sink are equal or absent from the graph.
    #[error("invalid source and sink")]
    InvalidTerminals,
    /// No flow satisfying all original bounds and balances exists.
    #[error("flow constraints are infeasible")]
    Infeasible(InfeasibilityWitness),
    /// Checked exact arithmetic exceeded the declared numeric domain.
    #[error("feasibility arithmetic overflow")]
    ArithmeticOverflow,
    /// A trace exceeded the bounded eager event budget.
    #[error("feasibility trace event limit reached")]
    TraceWorkLimit,
    /// A recorded patch or source/replay state disagreed.
    #[error("feasibility trace invariant failed")]
    TraceInvariant,
}

struct FeasibilityTraceRecorder {
    base: FeasibilityTraceSnapshot,
    current: FeasibilityTraceSnapshot,
    events: Vec<FeasibilityTraceEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FeasibilityCaptureMode {
    Untracked,
    Trace,
    Metrics,
}

/// Explicit owner of feasibility work recorded by one enclosing algorithm run.
///
/// Source algorithms must receive this value as an ordinary argument and call
/// its construction methods at the exact point where the auxiliary kernel is
/// executed. Ordinary solver and checker entry points never discover or mutate
/// an ambient capture scope.
pub struct FeasibilityExecution {
    mode: FeasibilityCaptureMode,
    traces: Vec<CapturedFeasibilityTrace>,
    metrics: FeasibilityTraceMetrics,
    invocations: u64,
}

/// Allocation-free aggregate retained by one fast-profile source execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FeasibilityMetricSummary {
    /// Sum of exact counters across all explicitly recorded invocations.
    pub total: FeasibilityTraceMetrics,
    /// Number of auxiliary feasibility kernels that actually ran.
    pub invocations: u64,
}

impl FeasibilityExecution {
    fn new(mode: FeasibilityCaptureMode) -> Self {
        Self {
            mode,
            traces: Vec::new(),
            metrics: FeasibilityTraceMetrics::default(),
            invocations: 0,
        }
    }

    /// Creates an explicit execution context that performs no trace or metric
    /// retention. Public solver/checker wrappers use this mode.
    pub fn untracked() -> Self {
        Self::new(FeasibilityCaptureMode::Untracked)
    }

    /// Constructs a feasible circulation while recording this exact source
    /// invocation according to the enclosing run profile.
    /// # Errors
    ///
    /// Returns the algorithm-specific error when the input contract, checked
    /// execution, replay, or certificate validation fails.
    pub fn find_feasible_flow(
        &mut self,
        graph: &FlowNetwork,
        required_divergence: &[i128],
        use_kind: FeasibilityUse,
    ) -> Result<FeasibleFlow, FeasibilityError> {
        match self.mode {
            FeasibilityCaptureMode::Untracked => {
                construct_untracked(graph, required_divergence, None)
            }
            FeasibilityCaptureMode::Trace => {
                let traced = trace_construct(graph, required_divergence, None)?;
                self.traces.push(CapturedFeasibilityTrace {
                    graph: graph.clone(),
                    use_kind,
                    request: CapturedFeasibilityRequest::Balance {
                        required_divergence: required_divergence.to_vec(),
                    },
                    result: traced.clone(),
                });
                self.invocations = self
                    .invocations
                    .checked_add(1)
                    .ok_or(FeasibilityError::ArithmeticOverflow)?;
                outcome_as_result(traced.outcome)
            }
            FeasibilityCaptureMode::Metrics => {
                self.construct_with_metrics(graph, required_divergence, None)
            }
        }
    }

    /// Constructs the lower-bounded maximum-flow initial state while recording
    /// this exact source invocation according to the enclosing run profile.
    /// # Errors
    ///
    /// Returns the algorithm-specific error when the input contract, checked
    /// execution, replay, or certificate validation fails.
    pub fn find_max_flow_initial(
        &mut self,
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        use_kind: FeasibilityUse,
    ) -> Result<FeasibleFlow, FeasibilityError> {
        validate_max_flow_initial_input(graph, source, sink)?;
        if graph.edges().iter().all(|edge| edge.lower() == 0) {
            return Ok(FeasibleFlow {
                flows: vec![0; graph.edges().len()],
            });
        }
        let required_divergence = vec![0_i128; graph.nodes().len()];
        let circulation_edge = Some((sink, source, graph.capacity_sum()));
        match self.mode {
            FeasibilityCaptureMode::Untracked => {
                construct_untracked(graph, &required_divergence, circulation_edge)
            }
            FeasibilityCaptureMode::Trace => {
                let traced = trace_construct(graph, &required_divergence, circulation_edge)?;
                self.traces.push(CapturedFeasibilityTrace {
                    graph: graph.clone(),
                    use_kind,
                    request: CapturedFeasibilityRequest::MaxFlowInitial { source, sink },
                    result: traced.clone(),
                });
                self.invocations = self
                    .invocations
                    .checked_add(1)
                    .ok_or(FeasibilityError::ArithmeticOverflow)?;
                outcome_as_result(traced.outcome)
            }
            FeasibilityCaptureMode::Metrics => {
                self.construct_with_metrics(graph, &required_divergence, circulation_edge)
            }
        }
    }

    fn construct_with_metrics(
        &mut self,
        graph: &FlowNetwork,
        required_divergence: &[i128],
        circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
    ) -> Result<FeasibleFlow, FeasibilityError> {
        let mut metrics = FeasibilityTraceMetrics::default();
        let result = construct(
            graph,
            required_divergence,
            circulation_edge,
            None,
            &mut metrics,
        );
        self.metrics = checked_add_metrics(self.metrics, metrics)?;
        self.invocations = self
            .invocations
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        result
    }
}

fn outcome_as_result(outcome: FeasibilityTraceOutcome) -> Result<FeasibleFlow, FeasibilityError> {
    match outcome {
        FeasibilityTraceOutcome::Feasible(feasible) => Ok(feasible),
        FeasibilityTraceOutcome::Infeasible(witness) => Err(FeasibilityError::Infeasible(witness)),
    }
}

fn checked_add_metrics(
    left: FeasibilityTraceMetrics,
    right: FeasibilityTraceMetrics,
) -> Result<FeasibilityTraceMetrics, FeasibilityError> {
    Ok(FeasibilityTraceMetrics {
        original_edge_inspections: left
            .original_edge_inspections
            .checked_add(right.original_edge_inspections)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        original_node_inspections: left
            .original_node_inspections
            .checked_add(right.original_node_inspections)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        auxiliary_adjacency_inspections: left
            .auxiliary_adjacency_inspections
            .checked_add(right.auxiliary_adjacency_inspections)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        pushes: left
            .pushes
            .checked_add(right.pushes)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        relabels: left
            .relabels
            .checked_add(right.relabels)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        active_node_selections: left
            .active_node_selections
            .checked_add(right.active_node_selections)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        discharges: left
            .discharges
            .checked_add(right.discharges)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        cut_adjacency_inspections: left
            .cut_adjacency_inspections
            .checked_add(right.cut_adjacency_inspections)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
        extracted_original_edges: left
            .extracted_original_edges
            .checked_add(right.extracted_original_edges)
            .ok_or(FeasibilityError::ArithmeticOverflow)?,
    })
}

/// Runs one trace-producing algorithm call while retaining every explicitly
/// recorded feasibility subroutine from that same source execution.
///
/// # Panics
///
/// Panics if the capture-mode contract is violated by emitting metric-only
/// output or by reporting an invocation count different from the retained
/// source traces.
#[must_use]
pub fn capture_feasibility_traces<T>(
    operation: impl FnOnce(&mut FeasibilityExecution) -> T,
) -> (T, Vec<CapturedFeasibilityTrace>) {
    let mut execution = FeasibilityExecution::new(FeasibilityCaptureMode::Trace);
    let output = operation(&mut execution);
    assert!(
        execution.metrics == FeasibilityTraceMetrics::default(),
        "trace capture received metric-only output"
    );
    assert_eq!(
        execution.invocations,
        u64::try_from(execution.traces.len()).expect("retained trace count exceeds u64")
    );
    (output, execution.traces)
}

/// Runs a fast-profile algorithm call while retaining only the exact aggregate
/// work counters of each feasibility subroutine invoked by that same source
/// execution. No reversible event stream is allocated.
///
/// # Panics
///
/// Panics if the capture-mode contract is violated by retaining a reversible
/// trace in metrics-only mode.
#[must_use]
pub fn capture_feasibility_metrics<T>(
    operation: impl FnOnce(&mut FeasibilityExecution) -> T,
) -> (T, FeasibilityMetricSummary) {
    let mut execution = FeasibilityExecution::new(FeasibilityCaptureMode::Metrics);
    let output = operation(&mut execution);
    assert!(
        execution.traces.is_empty(),
        "metric capture received retained traces"
    );
    (
        output,
        FeasibilityMetricSummary {
            total: execution.metrics,
            invocations: execution.invocations,
        },
    )
}

/// Rechecks a captured auxiliary trace against its retained graph and request.
///
/// # Errors
///
/// Rejects any request, event, outcome, or graph mismatch.
pub fn check_captured_feasibility_trace(
    captured: &CapturedFeasibilityTrace,
) -> Result<(), FeasibilityError> {
    match &captured.request {
        CapturedFeasibilityRequest::Balance {
            required_divergence,
        } => check_feasibility_trace(&captured.graph, required_divergence, &captured.result),
        CapturedFeasibilityRequest::MaxFlowInitial { source, sink } => {
            check_max_flow_initial_trace(&captured.graph, *source, *sink, &captured.result)
        }
    }
}

impl FeasibilityTraceRecorder {
    fn new(graph: &FlowNetwork) -> Self {
        let mut nodes = graph
            .nodes()
            .iter()
            .map(|node| FeasibilityNodeState {
                id: FeasibilityNodeId::Original(node.id().clone()),
                height: 0,
                excess: 0,
                current_arc: 0,
                active: false,
                reachable: false,
            })
            .collect::<Vec<_>>();
        nodes.extend([
            FeasibilityNodeState {
                id: FeasibilityNodeId::SuperSource,
                height: 0,
                excess: 0,
                current_arc: 0,
                active: false,
                reachable: false,
            },
            FeasibilityNodeState {
                id: FeasibilityNodeId::SuperSink,
                height: 0,
                excess: 0,
                current_arc: 0,
                active: false,
                reachable: false,
            },
        ]);
        let snapshot = FeasibilityTraceSnapshot {
            nodes,
            arcs: Vec::new(),
            active_queue: Vec::new(),
            original_flows: graph
                .edges()
                .iter()
                .map(|edge| FeasibilityOriginalFlowState {
                    edge: edge.id().clone(),
                    flow: edge.lower(),
                })
                .collect(),
            total_required: 0,
            routed: 0,
            metrics: FeasibilityTraceMetrics::default(),
        };
        Self {
            base: snapshot.clone(),
            current: snapshot,
            events: Vec::new(),
        }
    }

    fn record(
        &mut self,
        kind: FeasibilityTraceEventKind,
        focus_node: Option<FeasibilityNodeId>,
        focus_arc: Option<FeasibilityResidualArcId>,
        patches: Vec<FeasibilityTracePatch>,
        metrics: FeasibilityTraceMetrics,
    ) -> Result<(), FeasibilityError> {
        if self.events.len() >= FEASIBILITY_MAX_TRACE_EVENTS {
            return Err(FeasibilityError::TraceWorkLimit);
        }
        let metrics_before = self.current.metrics;
        for patch in &patches {
            apply_feasibility_trace_patch(
                &mut self.current,
                patch,
                FeasibilityTraceDirection::Forward,
            )?;
        }
        self.current.metrics = metrics;
        self.events.push(FeasibilityTraceEvent {
            kind,
            focus_node,
            focus_arc,
            patches,
            metrics_before,
            metrics,
        });
        Ok(())
    }

    fn finish(self) -> FeasibilityTrace {
        FeasibilityTrace {
            base_snapshot: self.base,
            events: self.events,
            final_snapshot: self.current,
        }
    }
}

/// Applies or reverses one exact feasibility trace event.
///
/// # Errors
///
/// Rejects a boundary whose before-state, reversible patch, or metric state
/// does not match the supplied snapshot.
pub fn apply_feasibility_trace_event(
    snapshot: &mut FeasibilityTraceSnapshot,
    event: &FeasibilityTraceEvent,
    direction: FeasibilityTraceDirection,
) -> Result<(), FeasibilityError> {
    match direction {
        FeasibilityTraceDirection::Forward => {
            if snapshot.metrics != event.metrics_before {
                return Err(FeasibilityError::TraceInvariant);
            }
            for patch in &event.patches {
                apply_feasibility_trace_patch(snapshot, patch, direction)?;
            }
            snapshot.metrics = event.metrics;
        }
        FeasibilityTraceDirection::Reverse => {
            if snapshot.metrics != event.metrics {
                return Err(FeasibilityError::TraceInvariant);
            }
            for patch in event.patches.iter().rev() {
                apply_feasibility_trace_patch(snapshot, patch, direction)?;
            }
            snapshot.metrics = event.metrics_before;
        }
    }
    Ok(())
}

/// Independently checks a balance-feasibility trace and its retained outcome.
///
/// The checker reconstructs the canonical auxiliary topology from the input,
/// validates every exact work delta and reversible patch, and verifies the
/// final feasible flow or infeasibility cut without trusting trace snapshots.
///
/// # Errors
///
/// Rejects malformed inputs, missing, reordered, or corrupt trace events,
/// arithmetic overflow, and outcomes not certified by the original network.
pub fn check_feasibility_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    result: &FeasibilityTraceResult,
) -> Result<(), FeasibilityError> {
    check_feasibility_trace_inner(graph, required_divergence, None, result)
}

/// Independently checks a lower-bounded maximum-flow initialization trace.
///
/// # Errors
///
/// Rejects invalid terminals or supplies, missing, reordered, or corrupt trace
/// events, arithmetic overflow, and uncertified retained outcomes.
pub fn check_max_flow_initial_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    result: &FeasibilityTraceResult,
) -> Result<(), FeasibilityError> {
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(FeasibilityError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(FeasibilityError::InvalidDivergence);
    }
    check_feasibility_trace_inner(
        graph,
        &vec![0_i128; graph.nodes().len()],
        Some((sink, source, graph.capacity_sum())),
        result,
    )
}

fn check_feasibility_trace_inner(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
    result: &FeasibilityTraceResult,
) -> Result<(), FeasibilityError> {
    validate_divergence(graph, required_divergence)?;
    // Focus is evidence of the concrete kernel operand, not user-authored
    // presentation metadata. Re-run the private deterministic kernel from the
    // public input and require the retained event sequence (including every
    // focused node/residual arc) to be exactly canonical before replaying its
    // patches below. This rejects a forged but still incident focus pair.
    let canonical = trace_construct(graph, required_divergence, circulation_edge)?;
    if canonical != *result {
        return Err(FeasibilityError::TraceInvariant);
    }
    let canonical_base = FeasibilityTraceRecorder::new(graph).base;
    if result.trace.base_snapshot != canonical_base {
        return Err(FeasibilityError::TraceInvariant);
    }

    let (expected_arcs, expected_total_required) =
        expected_auxiliary_topology(graph, required_divergence, circulation_edge)?;
    let mut replay = result.trace.base_snapshot.clone();
    let mut expected_metrics = FeasibilityTraceMetrics::default();
    for (ordinal, event) in result.trace.events.iter().enumerate() {
        if event.metrics_before != expected_metrics {
            return Err(FeasibilityError::TraceInvariant);
        }
        validate_feasibility_event_shape(event)?;
        expected_metrics = metrics_after_event(expected_metrics, event.kind)?;
        if event.metrics != expected_metrics {
            return Err(FeasibilityError::TraceInvariant);
        }
        apply_feasibility_trace_event(&mut replay, event, FeasibilityTraceDirection::Forward)?;
        validate_feasibility_event_focus(&replay, event)?;
        let is_terminal = matches!(
            event.kind,
            FeasibilityTraceEventKind::Feasible | FeasibilityTraceEventKind::Infeasible
        );
        if is_terminal != (ordinal + 1 == result.trace.events.len()) {
            return Err(FeasibilityError::TraceInvariant);
        }
    }
    if replay != result.trace.final_snapshot
        || replay.metrics != expected_metrics
        || replay.total_required != expected_total_required
        || replay.arcs.len() != expected_arcs.len()
    {
        return Err(FeasibilityError::TraceInvariant);
    }
    for (actual, expected) in replay.arcs.iter().zip(&expected_arcs) {
        if actual.id != expected.id
            || actual.from != expected.from
            || actual.to != expected.to
            || actual.capacity != expected.capacity
            || actual.flow > actual.capacity
        {
            return Err(FeasibilityError::TraceInvariant);
        }
    }
    validate_original_flow_projection(graph, &replay)?;

    let terminal = result
        .trace
        .events
        .last()
        .map(|event| event.kind)
        .ok_or(FeasibilityError::TraceInvariant)?;
    match &result.outcome {
        FeasibilityTraceOutcome::Feasible(feasible) => {
            if terminal != FeasibilityTraceEventKind::Feasible
                || replay.routed != replay.total_required
            {
                return Err(FeasibilityError::TraceInvariant);
            }
            check_feasible_original_flow(
                graph,
                required_divergence,
                circulation_edge,
                feasible,
                &replay,
            )?;
        }
        FeasibilityTraceOutcome::Infeasible(witness) => {
            if terminal != FeasibilityTraceEventKind::Infeasible
                || replay.routed >= replay.total_required
                || replay
                    .routed
                    .checked_add(witness.unsatisfied)
                    .ok_or(FeasibilityError::ArithmeticOverflow)?
                    != replay.total_required
            {
                return Err(FeasibilityError::TraceInvariant);
            }
            check_infeasibility(graph, required_divergence, circulation_edge, witness)?;
            let reachable_original_nodes = replay
                .nodes
                .iter()
                .filter_map(|node| match (&node.id, node.reachable) {
                    (FeasibilityNodeId::Original(id), true) => Some(id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if reachable_original_nodes != witness.reachable_original_nodes {
                return Err(FeasibilityError::TraceInvariant);
            }
        }
    }
    Ok(())
}

fn expected_auxiliary_topology(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
) -> Result<(Vec<FeasibilityArcState>, u128), FeasibilityError> {
    let mut lower_divergence = vec![0_i128; graph.nodes().len()];
    let mut arcs = Vec::with_capacity(graph.edges().len() + graph.nodes().len() + 1);
    for edge in graph.edges() {
        let lower = i128::from(edge.lower());
        lower_divergence[edge.from().as_usize()] = lower_divergence[edge.from().as_usize()]
            .checked_add(lower)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        lower_divergence[edge.to().as_usize()] = lower_divergence[edge.to().as_usize()]
            .checked_sub(lower)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        arcs.push(FeasibilityArcState {
            id: FeasibilityArcId::Original(edge.id().clone()),
            from: FeasibilityNodeId::Original(
                graph
                    .node(edge.from())
                    .ok_or(FeasibilityError::TraceInvariant)?
                    .id()
                    .clone(),
            ),
            to: FeasibilityNodeId::Original(
                graph
                    .node(edge.to())
                    .ok_or(FeasibilityError::TraceInvariant)?
                    .id()
                    .clone(),
            ),
            capacity: u128::from(edge.capacity() - edge.lower()),
            flow: 0,
        });
    }
    if let Some((from, to, capacity)) = circulation_edge {
        let from = graph
            .node(from)
            .ok_or(FeasibilityError::InvalidTerminals)?
            .id()
            .clone();
        let to = graph
            .node(to)
            .ok_or(FeasibilityError::InvalidTerminals)?
            .id()
            .clone();
        arcs.push(FeasibilityArcState {
            id: FeasibilityArcId::LowerBoundReturn {
                from: from.clone(),
                to: to.clone(),
            },
            from: FeasibilityNodeId::Original(from),
            to: FeasibilityNodeId::Original(to),
            capacity,
            flow: 0,
        });
    }
    let mut total_required = 0_u128;
    for node in graph.node_indices() {
        let id = graph
            .node(node)
            .ok_or(FeasibilityError::TraceInvariant)?
            .id()
            .clone();
        let residual = required_divergence[node.as_usize()]
            .checked_sub(lower_divergence[node.as_usize()])
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        if residual > 0 {
            let amount =
                u128::try_from(residual).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
            total_required = total_required
                .checked_add(amount)
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            arcs.push(FeasibilityArcState {
                id: FeasibilityArcId::FromSuperSource(id.clone()),
                from: FeasibilityNodeId::SuperSource,
                to: FeasibilityNodeId::Original(id),
                capacity: amount,
                flow: 0,
            });
        } else if residual < 0 {
            let amount = residual
                .checked_neg()
                .and_then(|value| u128::try_from(value).ok())
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            arcs.push(FeasibilityArcState {
                id: FeasibilityArcId::ToSuperSink(id.clone()),
                from: FeasibilityNodeId::Original(id),
                to: FeasibilityNodeId::SuperSink,
                capacity: amount,
                flow: 0,
            });
        }
    }
    Ok((arcs, total_required))
}

#[allow(
    clippy::too_many_lines,
    reason = "the closed event grammar is intentionally visible in one exhaustive match"
)]
fn validate_feasibility_event_shape(event: &FeasibilityTraceEvent) -> Result<(), FeasibilityError> {
    use FeasibilityTraceEventKind as Kind;
    let focus_shape_is_valid = match event.kind {
        Kind::AddOriginalArc
        | Kind::AddReturnArc
        | Kind::AddImbalanceArc
        | Kind::Push
        | Kind::ExtractOriginalFlow => event.focus_node.is_none() && event.focus_arc.is_some(),
        Kind::InspectNodeImbalance
        | Kind::ActivateNode
        | Kind::SelectActiveNode
        | Kind::Relabel
        | Kind::CompleteDischarge => event.focus_node.is_some() && event.focus_arc.is_none(),
        Kind::InitializeSourceHeight => {
            event.focus_node == Some(FeasibilityNodeId::SuperSource) && event.focus_arc.is_none()
        }
        Kind::InspectSourceArc => {
            event.focus_node == Some(FeasibilityNodeId::SuperSource) && event.focus_arc.is_some()
        }
        Kind::InspectDischargeArc
        | Kind::InspectRelabelArc
        | Kind::AdvanceCurrentArc
        | Kind::InspectCutArc => event.focus_node.is_some() && event.focus_arc.is_some(),
        Kind::MarkReachable => event.focus_node.is_some(),
        Kind::CompleteRouting | Kind::Feasible | Kind::Infeasible => {
            event.focus_node.is_none() && event.focus_arc.is_none()
        }
    };
    if !focus_shape_is_valid {
        return Err(FeasibilityError::TraceInvariant);
    }
    let patches_are_valid = match event.kind {
        Kind::AddOriginalArc | Kind::AddReturnArc => {
            matches!(
                event.patches.as_slice(),
                [FeasibilityTracePatch::AddArc { .. }]
            )
        }
        Kind::AddImbalanceArc => {
            matches!(
                event.patches.as_slice(),
                [FeasibilityTracePatch::AddArc { .. }]
                    | [
                        FeasibilityTracePatch::AddArc { .. },
                        FeasibilityTracePatch::TotalRequired { .. }
                    ]
            )
        }
        Kind::InspectNodeImbalance
        | Kind::InspectSourceArc
        | Kind::InspectDischargeArc
        | Kind::InspectRelabelArc
        | Kind::CompleteDischarge
        | Kind::InspectCutArc
        | Kind::ExtractOriginalFlow
        | Kind::Feasible
        | Kind::Infeasible => event.patches.is_empty(),
        Kind::InitializeSourceHeight => matches!(
            event.patches.as_slice(),
            [FeasibilityTracePatch::NodeHeight { .. }]
        ),
        Kind::ActivateNode => matches!(
            event.patches.as_slice(),
            [
                FeasibilityTracePatch::NodeActive { .. },
                FeasibilityTracePatch::QueuePushBack { .. }
            ]
        ),
        Kind::SelectActiveNode => matches!(
            event.patches.as_slice(),
            [
                FeasibilityTracePatch::QueuePopFront { .. },
                FeasibilityTracePatch::NodeActive { .. }
            ]
        ),
        Kind::Push => matches!(
            event.patches.first(),
            Some(FeasibilityTracePatch::ArcFlow { .. })
        ),
        Kind::AdvanceCurrentArc => matches!(
            event.patches.as_slice(),
            [FeasibilityTracePatch::NodeCurrentArc { .. }]
        ),
        Kind::Relabel => matches!(
            event.patches.as_slice(),
            [
                FeasibilityTracePatch::NodeHeight { .. },
                FeasibilityTracePatch::NodeCurrentArc { .. }
            ]
        ),
        Kind::CompleteRouting => matches!(
            event.patches.as_slice(),
            [FeasibilityTracePatch::Routed { .. }]
        ),
        Kind::MarkReachable => matches!(
            event.patches.as_slice(),
            [FeasibilityTracePatch::NodeReachable { .. }]
        ),
    };
    if patches_are_valid {
        Ok(())
    } else {
        Err(FeasibilityError::TraceInvariant)
    }
}

fn metrics_after_event(
    mut metrics: FeasibilityTraceMetrics,
    kind: FeasibilityTraceEventKind,
) -> Result<FeasibilityTraceMetrics, FeasibilityError> {
    let counter = match kind {
        FeasibilityTraceEventKind::AddOriginalArc => &mut metrics.original_edge_inspections,
        FeasibilityTraceEventKind::InspectNodeImbalance => &mut metrics.original_node_inspections,
        FeasibilityTraceEventKind::InspectSourceArc
        | FeasibilityTraceEventKind::InspectDischargeArc
        | FeasibilityTraceEventKind::InspectRelabelArc => {
            &mut metrics.auxiliary_adjacency_inspections
        }
        FeasibilityTraceEventKind::Push => &mut metrics.pushes,
        FeasibilityTraceEventKind::Relabel => &mut metrics.relabels,
        FeasibilityTraceEventKind::SelectActiveNode => &mut metrics.active_node_selections,
        FeasibilityTraceEventKind::CompleteDischarge => &mut metrics.discharges,
        FeasibilityTraceEventKind::InspectCutArc => &mut metrics.cut_adjacency_inspections,
        FeasibilityTraceEventKind::ExtractOriginalFlow => &mut metrics.extracted_original_edges,
        FeasibilityTraceEventKind::AddReturnArc
        | FeasibilityTraceEventKind::AddImbalanceArc
        | FeasibilityTraceEventKind::InitializeSourceHeight
        | FeasibilityTraceEventKind::ActivateNode
        | FeasibilityTraceEventKind::AdvanceCurrentArc
        | FeasibilityTraceEventKind::CompleteRouting
        | FeasibilityTraceEventKind::MarkReachable
        | FeasibilityTraceEventKind::Feasible
        | FeasibilityTraceEventKind::Infeasible => return Ok(metrics),
    };
    *counter = counter
        .checked_add(1)
        .ok_or(FeasibilityError::ArithmeticOverflow)?;
    Ok(metrics)
}

fn validate_feasibility_event_focus(
    snapshot: &FeasibilityTraceSnapshot,
    event: &FeasibilityTraceEvent,
) -> Result<(), FeasibilityError> {
    if let Some(node) = &event.focus_node
        && !snapshot.nodes.iter().any(|state| state.id == *node)
    {
        return Err(FeasibilityError::TraceInvariant);
    }
    if let Some(residual) = &event.focus_arc {
        let Some(arc) = snapshot.arcs.iter().find(|state| state.id == residual.arc) else {
            return Err(FeasibilityError::TraceInvariant);
        };
        if let Some(node) = &event.focus_node {
            let (tail, head) = match residual.direction {
                FeasibilityResidualDirection::Forward => (&arc.from, &arc.to),
                FeasibilityResidualDirection::Reverse => (&arc.to, &arc.from),
            };
            let expected = if event.kind == FeasibilityTraceEventKind::MarkReachable {
                head
            } else {
                tail
            };
            if node != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
        }
    }
    Ok(())
}

fn validate_original_flow_projection(
    graph: &FlowNetwork,
    snapshot: &FeasibilityTraceSnapshot,
) -> Result<(), FeasibilityError> {
    if snapshot.original_flows.len() != graph.edges().len() {
        return Err(FeasibilityError::TraceInvariant);
    }
    for (edge, projected) in graph.edges().iter().zip(&snapshot.original_flows) {
        let auxiliary = snapshot
            .arcs
            .iter()
            .find(|arc| arc.id == FeasibilityArcId::Original(edge.id().clone()))
            .ok_or(FeasibilityError::TraceInvariant)?;
        let shifted =
            u64::try_from(auxiliary.flow).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
        let expected = edge
            .lower()
            .checked_add(shifted)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        if projected.edge != *edge.id() || projected.flow != expected {
            return Err(FeasibilityError::TraceInvariant);
        }
    }
    Ok(())
}

fn check_feasible_original_flow(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
    feasible: &FeasibleFlow,
    snapshot: &FeasibilityTraceSnapshot,
) -> Result<(), FeasibilityError> {
    if feasible.flows.len() != graph.edges().len()
        || feasible
            .flows
            .iter()
            .zip(graph.edges())
            .any(|(&flow, edge)| flow < edge.lower() || flow > edge.capacity())
        || snapshot
            .original_flows
            .iter()
            .map(|flow| flow.flow)
            .ne(feasible.flows.iter().copied())
    {
        return Err(FeasibilityError::TraceInvariant);
    }
    let mut actual =
        divergences(graph, &feasible.flows).map_err(|error| map_certificate_error(&error))?;
    if let Some((from, to, _)) = circulation_edge {
        let return_flow = snapshot
            .arcs
            .iter()
            .find_map(|arc| {
                matches!(arc.id, FeasibilityArcId::LowerBoundReturn { .. }).then_some(arc.flow)
            })
            .ok_or(FeasibilityError::TraceInvariant)?;
        let return_flow =
            i128::try_from(return_flow).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
        actual[from.as_usize()] = actual[from.as_usize()]
            .checked_add(return_flow)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        actual[to.as_usize()] = actual[to.as_usize()]
            .checked_sub(return_flow)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
    }
    if actual != required_divergence {
        return Err(FeasibilityError::TraceInvariant);
    }
    Ok(())
}

#[allow(
    clippy::too_many_lines,
    reason = "all reversible patch variants share one fail-closed before/after-state validator"
)]
fn apply_feasibility_trace_patch(
    snapshot: &mut FeasibilityTraceSnapshot,
    patch: &FeasibilityTracePatch,
    direction: FeasibilityTraceDirection,
) -> Result<(), FeasibilityError> {
    match patch {
        FeasibilityTracePatch::AddArc { arc } => match direction {
            FeasibilityTraceDirection::Forward => {
                if snapshot.arcs.iter().any(|candidate| candidate.id == arc.id) {
                    return Err(FeasibilityError::TraceInvariant);
                }
                snapshot.arcs.push(arc.clone());
            }
            FeasibilityTraceDirection::Reverse => {
                if snapshot.arcs.last() != Some(arc) {
                    return Err(FeasibilityError::TraceInvariant);
                }
                snapshot.arcs.pop();
            }
        },
        FeasibilityTracePatch::NodeHeight {
            node,
            before,
            after,
        } => {
            let state = snapshot
                .nodes
                .iter_mut()
                .find(|state| state.id == *node)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.height != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.height = replacement;
        }
        FeasibilityTracePatch::NodeExcess {
            node,
            before,
            after,
        } => {
            let state = snapshot
                .nodes
                .iter_mut()
                .find(|state| state.id == *node)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.excess != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.excess = replacement;
        }
        FeasibilityTracePatch::NodeCurrentArc {
            node,
            before,
            after,
        } => {
            let state = snapshot
                .nodes
                .iter_mut()
                .find(|state| state.id == *node)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.current_arc != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.current_arc = replacement;
        }
        FeasibilityTracePatch::NodeActive {
            node,
            before,
            after,
        } => {
            let state = snapshot
                .nodes
                .iter_mut()
                .find(|state| state.id == *node)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.active != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.active = replacement;
        }
        FeasibilityTracePatch::NodeReachable {
            node,
            before,
            after,
        } => {
            let state = snapshot
                .nodes
                .iter_mut()
                .find(|state| state.id == *node)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.reachable != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.reachable = replacement;
        }
        FeasibilityTracePatch::QueuePushBack { node } => match direction {
            FeasibilityTraceDirection::Forward => snapshot.active_queue.push(node.clone()),
            FeasibilityTraceDirection::Reverse => {
                if snapshot.active_queue.last() != Some(node) {
                    return Err(FeasibilityError::TraceInvariant);
                }
                snapshot.active_queue.pop();
            }
        },
        FeasibilityTracePatch::QueuePopFront { node } => match direction {
            FeasibilityTraceDirection::Forward => {
                if snapshot.active_queue.first() != Some(node) {
                    return Err(FeasibilityError::TraceInvariant);
                }
                snapshot.active_queue.remove(0);
            }
            FeasibilityTraceDirection::Reverse => snapshot.active_queue.insert(0, node.clone()),
        },
        FeasibilityTracePatch::ArcFlow { arc, before, after } => {
            let state = snapshot
                .arcs
                .iter_mut()
                .find(|state| state.id == *arc)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.flow != expected || replacement > state.capacity {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.flow = replacement;
        }
        FeasibilityTracePatch::OriginalFlow {
            edge,
            before,
            after,
        } => {
            let state = snapshot
                .original_flows
                .iter_mut()
                .find(|state| state.edge == *edge)
                .ok_or(FeasibilityError::TraceInvariant)?;
            let (expected, replacement) = directional_values(direction, *before, *after);
            if state.flow != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            state.flow = replacement;
        }
        FeasibilityTracePatch::TotalRequired { before, after } => {
            let (expected, replacement) = directional_values(direction, *before, *after);
            if snapshot.total_required != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            snapshot.total_required = replacement;
        }
        FeasibilityTracePatch::Routed { before, after } => {
            let (expected, replacement) = directional_values(direction, *before, *after);
            if snapshot.routed != expected {
                return Err(FeasibilityError::TraceInvariant);
            }
            snapshot.routed = replacement;
        }
    }
    Ok(())
}

fn directional_values<T: Copy>(
    direction: FeasibilityTraceDirection,
    before: T,
    after: T,
) -> (T, T) {
    match direction {
        FeasibilityTraceDirection::Forward => (before, after),
        FeasibilityTraceDirection::Reverse => (after, before),
    }
}

/// Constructs any original-edge flow satisfying exact node divergences.
///
/// Lower bounds are shifted into node imbalance. An internal super-source and
/// super-sink are connected only to imbalanced original nodes, then an auxiliary
/// maximum flow must saturate every super-source edge. Auxiliary edges are never
/// returned as part of the candidate result.
///
/// # Errors
///
/// Rejects invalid target vectors, arithmetic overflow, or infeasibility with a
/// residual-cut witness.
pub fn find_feasible_flow(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<FeasibleFlow, FeasibilityError> {
    construct_untracked(graph, required_divergence, None)
}

/// Traces the same feasibility construction that produces the returned outcome.
///
/// Infeasibility is a certified mathematical outcome rather than a trace
/// construction failure, so the residual-cut witness is retained in the result.
///
/// # Errors
///
/// Rejects invalid divergence vectors, arithmetic overflow, trace exhaustion,
/// or a source/replay invariant disagreement.
pub fn trace_feasible_flow(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<FeasibilityTraceResult, FeasibilityError> {
    trace_construct(graph, required_divergence, None)
}

/// Constructs a feasible initial state for lower-bounded maximum flow.
///
/// A temporary `sink -> source` edge converts the unknown terminal value into a
/// circulation. Its capacity is bounded by the checked sum of original upper
/// capacities. The returned vector contains original edges only.
///
/// # Errors
///
/// Rejects invalid terminals, arithmetic overflow, or an infeasible transformed
/// circulation.
pub fn find_max_flow_initial(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FeasibleFlow, FeasibilityError> {
    validate_max_flow_initial_input(graph, source, sink)?;
    if graph.edges().iter().all(|edge| edge.lower() == 0) {
        return Ok(FeasibleFlow {
            flows: vec![0; graph.edges().len()],
        });
    }
    construct_untracked(
        graph,
        &vec![0_i128; graph.nodes().len()],
        Some((sink, source, graph.capacity_sum())),
    )
}

/// Traces the lower-bounded maximum-flow feasibility transformation.
///
/// The dedicated overlay includes the real temporary sink-to-source edge and
/// both artificial imbalance terminals; none is projected as an input edge.
///
/// # Errors
///
/// Rejects invalid terminals or supplies, arithmetic overflow, trace
/// exhaustion, or a source/replay invariant disagreement.
pub fn trace_max_flow_initial(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<FeasibilityTraceResult, FeasibilityError> {
    validate_max_flow_initial_input(graph, source, sink)?;
    trace_construct(
        graph,
        &vec![0_i128; graph.nodes().len()],
        Some((sink, source, graph.capacity_sum())),
    )
}

fn validate_max_flow_initial_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), FeasibilityError> {
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(FeasibilityError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(FeasibilityError::InvalidDivergence);
    }
    Ok(())
}

/// Independently verifies a balance-feasibility cut witness on original edges.
///
/// # Errors
///
/// Rejects malformed node sets, an invalid target, arithmetic overflow, or a
/// witness whose declared deficit is not exactly the violated cut inequality.
pub fn check_balance_infeasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    witness: &InfeasibilityWitness,
) -> Result<(), FeasibilityError> {
    check_infeasibility(graph, required_divergence, None, witness)
}

/// Independently verifies a lower-bounded max-flow infeasibility cut.
///
/// # Errors
///
/// Rejects invalid terminals, malformed node sets, arithmetic overflow, or a
/// witness that does not prove the transformed circulation infeasible.
pub fn check_max_flow_infeasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    witness: &InfeasibilityWitness,
) -> Result<(), FeasibilityError> {
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(FeasibilityError::InvalidTerminals);
    }
    check_infeasibility(
        graph,
        &vec![0_i128; graph.nodes().len()],
        Some((sink, source, graph.capacity_sum())),
        witness,
    )
}

fn check_infeasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
    witness: &InfeasibilityWitness,
) -> Result<(), FeasibilityError> {
    validate_divergence(graph, required_divergence)?;
    if witness.unsatisfied == 0 {
        return Err(FeasibilityError::InvalidDivergence);
    }
    let mut reachable = vec![false; graph.nodes().len()];
    for node_id in &witness.reachable_original_nodes {
        let node = graph
            .node_index(node_id)
            .ok_or(FeasibilityError::InvalidDivergence)?;
        if std::mem::replace(&mut reachable[node.as_usize()], true) {
            return Err(FeasibilityError::InvalidDivergence);
        }
    }
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence =
        divergences(graph, &lower_flows).map_err(|error| map_certificate_error(&error))?;
    let residual_requirement = graph.node_indices().try_fold(0_i128, |sum, node| {
        if !reachable[node.as_usize()] {
            return Ok(sum);
        }
        let requirement = required_divergence[node.as_usize()]
            .checked_sub(lower_divergence[node.as_usize()])
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        sum.checked_add(requirement)
            .ok_or(FeasibilityError::ArithmeticOverflow)
    })?;
    let mut outgoing_capacity = 0_u128;
    for edge in graph.edges() {
        if reachable[edge.from().as_usize()] && !reachable[edge.to().as_usize()] {
            outgoing_capacity = outgoing_capacity
                .checked_add(u128::from(edge.capacity() - edge.lower()))
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
        }
    }
    if let Some((from, to, capacity)) = circulation_edge
        && reachable[from.as_usize()]
        && !reachable[to.as_usize()]
    {
        outgoing_capacity = outgoing_capacity
            .checked_add(capacity)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
    }
    let outgoing_capacity =
        i128::try_from(outgoing_capacity).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
    let violation = residual_requirement
        .checked_sub(outgoing_capacity)
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(FeasibilityError::InvalidDivergence)?;
    if violation != witness.unsatisfied {
        return Err(FeasibilityError::InvalidDivergence);
    }
    Ok(())
}

fn construct_untracked(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
) -> Result<FeasibleFlow, FeasibilityError> {
    let mut metrics = FeasibilityTraceMetrics::default();
    construct(
        graph,
        required_divergence,
        circulation_edge,
        None,
        &mut metrics,
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "the feasibility transform publishes one atomic auxiliary-network construction and certificate"
)]
fn construct(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
    mut recorder: Option<&mut FeasibilityTraceRecorder>,
    metrics: &mut FeasibilityTraceMetrics,
) -> Result<FeasibleFlow, FeasibilityError> {
    validate_divergence(graph, required_divergence)?;
    let original_node_count = graph.nodes().len();
    let super_source = original_node_count;
    let super_sink = original_node_count + 1;
    let mut auxiliary_node_ids = graph
        .nodes()
        .iter()
        .map(|node| FeasibilityNodeId::Original(node.id().clone()))
        .collect::<Vec<_>>();
    auxiliary_node_ids.extend([FeasibilityNodeId::SuperSource, FeasibilityNodeId::SuperSink]);
    let mut auxiliary = AuxiliaryGraph::new(auxiliary_node_ids);
    let mut lower_divergence = vec![0_i128; original_node_count];
    let mut original_handles = Vec::with_capacity(graph.edges().len());
    for edge in graph.edges() {
        let lower = i128::from(edge.lower());
        lower_divergence[edge.from().as_usize()] = lower_divergence[edge.from().as_usize()]
            .checked_add(lower)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        lower_divergence[edge.to().as_usize()] = lower_divergence[edge.to().as_usize()]
            .checked_sub(lower)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        let handle = auxiliary.add_edge(
            edge.from().as_usize(),
            edge.to().as_usize(),
            u128::from(edge.capacity() - edge.lower()),
            FeasibilityArcId::Original(edge.id().clone()),
        );
        metrics.original_edge_inspections = metrics
            .original_edge_inspections
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        if let Some(recorder) = recorder.as_deref_mut() {
            record_added_arc(
                recorder,
                &auxiliary,
                &handle,
                FeasibilityTraceEventKind::AddOriginalArc,
                *metrics,
            )?;
        }
        original_handles.push(handle);
    }
    if let Some((from, to, capacity)) = circulation_edge {
        let from_id = graph
            .node(from)
            .ok_or(FeasibilityError::InvalidTerminals)?
            .id()
            .clone();
        let to_id = graph
            .node(to)
            .ok_or(FeasibilityError::InvalidTerminals)?
            .id()
            .clone();
        let handle = auxiliary.add_edge(
            from.as_usize(),
            to.as_usize(),
            capacity,
            FeasibilityArcId::LowerBoundReturn {
                from: from_id,
                to: to_id,
            },
        );
        if let Some(recorder) = recorder.as_deref_mut() {
            record_added_arc(
                recorder,
                &auxiliary,
                &handle,
                FeasibilityTraceEventKind::AddReturnArc,
                *metrics,
            )?;
        }
    }

    let mut total_required = 0_u128;
    for node in graph.node_indices() {
        let node_id = graph
            .node(node)
            .ok_or(FeasibilityError::TraceInvariant)?
            .id()
            .clone();
        let residual_divergence = required_divergence[node.as_usize()]
            .checked_sub(lower_divergence[node.as_usize()])
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        metrics.original_node_inspections = metrics
            .original_node_inspections
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        if let Some(recorder) = recorder.as_deref_mut() {
            recorder.record(
                FeasibilityTraceEventKind::InspectNodeImbalance,
                Some(FeasibilityNodeId::Original(node_id.clone())),
                None,
                Vec::new(),
                *metrics,
            )?;
        }
        if residual_divergence > 0 {
            let amount = u128::try_from(residual_divergence)
                .map_err(|_| FeasibilityError::ArithmeticOverflow)?;
            let handle = auxiliary.add_edge(
                super_source,
                node.as_usize(),
                amount,
                FeasibilityArcId::FromSuperSource(node_id),
            );
            let before = total_required;
            total_required = total_required
                .checked_add(amount)
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            if let Some(recorder) = recorder.as_deref_mut() {
                record_added_arc_with_patches(
                    recorder,
                    &auxiliary,
                    &handle,
                    FeasibilityTraceEventKind::AddImbalanceArc,
                    vec![FeasibilityTracePatch::TotalRequired {
                        before,
                        after: total_required,
                    }],
                    *metrics,
                )?;
            }
        } else if residual_divergence < 0 {
            let amount = residual_divergence
                .checked_neg()
                .and_then(|value| u128::try_from(value).ok())
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            let handle = auxiliary.add_edge(
                node.as_usize(),
                super_sink,
                amount,
                FeasibilityArcId::ToSuperSink(node_id),
            );
            if let Some(recorder) = recorder.as_deref_mut() {
                record_added_arc(
                    recorder,
                    &auxiliary,
                    &handle,
                    FeasibilityTraceEventKind::AddImbalanceArc,
                    *metrics,
                )?;
            }
        }
    }

    let routed = auxiliary.max_flow(super_source, super_sink, recorder.as_deref_mut(), metrics)?;
    if let Some(recorder) = recorder.as_deref_mut() {
        recorder.record(
            FeasibilityTraceEventKind::CompleteRouting,
            None,
            None,
            vec![FeasibilityTracePatch::Routed {
                before: recorder.current.routed,
                after: routed,
            }],
            *metrics,
        )?;
    }
    if routed != total_required {
        let reachable = auxiliary.reachable_from(super_source, recorder.as_deref_mut(), metrics)?;
        let reachable_original_nodes = graph
            .node_indices()
            .filter(|node| reachable[node.as_usize()])
            .filter_map(|node| graph.node(node).map(|item| item.id().clone()))
            .collect();
        let witness = InfeasibilityWitness {
            unsatisfied: total_required
                .checked_sub(routed)
                .ok_or(FeasibilityError::ArithmeticOverflow)?,
            reachable_original_nodes,
        };
        if let Some(recorder) = recorder.as_deref_mut() {
            recorder.record(
                FeasibilityTraceEventKind::Infeasible,
                None,
                None,
                Vec::new(),
                *metrics,
            )?;
        }
        return Err(FeasibilityError::Infeasible(witness));
    }

    let flows = graph
        .edges()
        .iter()
        .zip(&original_handles)
        .map(|(edge, handle)| {
            let shifted = auxiliary.flow_on(handle)?;
            let shifted =
                u64::try_from(shifted).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
            edge.lower()
                .checked_add(shifted)
                .ok_or(FeasibilityError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, FeasibilityError>>()?;
    for (edge, &flow) in graph.edges().iter().zip(&flows) {
        metrics.extracted_original_edges = metrics
            .extracted_original_edges
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        if let Some(recorder) = recorder.as_deref_mut() {
            recorder.record(
                FeasibilityTraceEventKind::ExtractOriginalFlow,
                None,
                Some(FeasibilityResidualArcId {
                    arc: FeasibilityArcId::Original(edge.id().clone()),
                    direction: FeasibilityResidualDirection::Forward,
                }),
                Vec::new(),
                *metrics,
            )?;
            let projected = recorder
                .current
                .original_flows
                .iter()
                .find(|state| state.edge == *edge.id())
                .ok_or(FeasibilityError::TraceInvariant)?
                .flow;
            if projected != flow {
                return Err(FeasibilityError::TraceInvariant);
            }
        }
    }
    if let Some(recorder) = recorder {
        recorder.record(
            FeasibilityTraceEventKind::Feasible,
            None,
            None,
            Vec::new(),
            *metrics,
        )?;
    }
    Ok(FeasibleFlow { flows })
}

fn record_added_arc(
    recorder: &mut FeasibilityTraceRecorder,
    auxiliary: &AuxiliaryGraph,
    handle: &EdgeHandle,
    kind: FeasibilityTraceEventKind,
    metrics: FeasibilityTraceMetrics,
) -> Result<(), FeasibilityError> {
    record_added_arc_with_patches(recorder, auxiliary, handle, kind, Vec::new(), metrics)
}

fn record_added_arc_with_patches(
    recorder: &mut FeasibilityTraceRecorder,
    auxiliary: &AuxiliaryGraph,
    handle: &EdgeHandle,
    kind: FeasibilityTraceEventKind,
    mut patches: Vec<FeasibilityTracePatch>,
    metrics: FeasibilityTraceMetrics,
) -> Result<(), FeasibilityError> {
    let arc = auxiliary.arc_state(handle)?;
    let focus_arc = FeasibilityResidualArcId {
        arc: arc.id.clone(),
        direction: FeasibilityResidualDirection::Forward,
    };
    patches.insert(0, FeasibilityTracePatch::AddArc { arc });
    recorder.record(kind, None, Some(focus_arc), patches, metrics)
}

fn trace_construct(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    circulation_edge: Option<(NodeIndex, NodeIndex, u128)>,
) -> Result<FeasibilityTraceResult, FeasibilityError> {
    let mut recorder = FeasibilityTraceRecorder::new(graph);
    let mut metrics = FeasibilityTraceMetrics::default();
    let outcome = match construct(
        graph,
        required_divergence,
        circulation_edge,
        Some(&mut recorder),
        &mut metrics,
    ) {
        Ok(feasible) => FeasibilityTraceOutcome::Feasible(feasible),
        Err(FeasibilityError::Infeasible(witness)) => FeasibilityTraceOutcome::Infeasible(witness),
        Err(error) => return Err(error),
    };
    if recorder.current.metrics != metrics {
        return Err(FeasibilityError::TraceInvariant);
    }
    Ok(FeasibilityTraceResult {
        outcome,
        trace: recorder.finish(),
    })
}

fn validate_divergence(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<(), FeasibilityError> {
    if required_divergence.len() != graph.nodes().len() {
        return Err(FeasibilityError::InvalidDivergence);
    }
    let sum = required_divergence.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(FeasibilityError::ArithmeticOverflow)
    })?;
    if sum != 0 {
        return Err(FeasibilityError::InvalidDivergence);
    }
    Ok(())
}

fn map_certificate_error(error: &CertificateError) -> FeasibilityError {
    match error {
        CertificateError::ArithmeticOverflow => FeasibilityError::ArithmeticOverflow,
        _ => FeasibilityError::InvalidDivergence,
    }
}

#[derive(Clone, Debug)]
struct EdgeHandle {
    from: usize,
    index: usize,
    initial_capacity: u128,
    arc: FeasibilityArcId,
}

#[derive(Clone, Debug)]
struct AuxiliaryPush {
    to: usize,
    residual_arc: FeasibilityResidualArcId,
    flow_before: u128,
    flow_after: u128,
}

#[derive(Clone, Debug)]
struct AuxiliaryEdge {
    to: usize,
    reverse: usize,
    capacity: u128,
    arc: FeasibilityArcId,
    direction: FeasibilityResidualDirection,
}

#[derive(Clone, Debug)]
struct AuxiliaryGraph {
    adjacency: Vec<Vec<AuxiliaryEdge>>,
    node_ids: Vec<FeasibilityNodeId>,
}

impl AuxiliaryGraph {
    fn new(node_ids: Vec<FeasibilityNodeId>) -> Self {
        Self {
            adjacency: vec![Vec::new(); node_ids.len()],
            node_ids,
        }
    }

    fn node_id(&self, index: usize) -> Result<FeasibilityNodeId, FeasibilityError> {
        self.node_ids
            .get(index)
            .cloned()
            .ok_or(FeasibilityError::TraceInvariant)
    }

    fn arc_state(&self, handle: &EdgeHandle) -> Result<FeasibilityArcState, FeasibilityError> {
        let edge = self
            .adjacency
            .get(handle.from)
            .and_then(|edges| edges.get(handle.index))
            .ok_or(FeasibilityError::TraceInvariant)?;
        Ok(FeasibilityArcState {
            id: handle.arc.clone(),
            from: self.node_id(handle.from)?,
            to: self.node_id(edge.to)?,
            capacity: handle.initial_capacity,
            flow: 0,
        })
    }

    fn add_edge(
        &mut self,
        from: usize,
        to: usize,
        capacity: u128,
        arc: FeasibilityArcId,
    ) -> EdgeHandle {
        let forward_index = self.adjacency[from].len();
        let reverse_index = if from == to {
            forward_index + 1
        } else {
            self.adjacency[to].len()
        };
        self.adjacency[from].push(AuxiliaryEdge {
            to,
            reverse: reverse_index,
            capacity,
            arc: arc.clone(),
            direction: FeasibilityResidualDirection::Forward,
        });
        self.adjacency[to].push(AuxiliaryEdge {
            to: from,
            reverse: forward_index,
            capacity: 0,
            arc: arc.clone(),
            direction: FeasibilityResidualDirection::Reverse,
        });
        EdgeHandle {
            from,
            index: forward_index,
            initial_capacity: capacity,
            arc,
        }
    }

    fn flow_on(&self, handle: &EdgeHandle) -> Result<u128, FeasibilityError> {
        handle
            .initial_capacity
            .checked_sub(self.adjacency[handle.from][handle.index].capacity)
            .ok_or(FeasibilityError::ArithmeticOverflow)
    }

    fn residual_arc_id(
        &self,
        from: usize,
        edge_index: usize,
    ) -> Result<FeasibilityResidualArcId, FeasibilityError> {
        let edge = self
            .adjacency
            .get(from)
            .and_then(|edges| edges.get(edge_index))
            .ok_or(FeasibilityError::TraceInvariant)?;
        Ok(FeasibilityResidualArcId {
            arc: edge.arc.clone(),
            direction: edge.direction,
        })
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the FIFO push-relabel loop records each real inspection, push, relabel, and discharge inline"
    )]
    fn max_flow(
        &mut self,
        source: usize,
        sink: usize,
        mut recorder: Option<&mut FeasibilityTraceRecorder>,
        metrics: &mut FeasibilityTraceMetrics,
    ) -> Result<u128, FeasibilityError> {
        let node_count = self.adjacency.len();
        let mut excess = vec![0_u128; node_count];
        let mut height = vec![0_usize; node_count];
        let mut current = vec![0_usize; node_count];
        let mut active = vec![false; node_count];
        let mut queue = VecDeque::new();
        height[source] = node_count;
        if let Some(recorder) = recorder.as_deref_mut() {
            recorder.record(
                FeasibilityTraceEventKind::InitializeSourceHeight,
                Some(self.node_id(source)?),
                None,
                vec![FeasibilityTracePatch::NodeHeight {
                    node: self.node_id(source)?,
                    before: 0,
                    after: node_count,
                }],
                *metrics,
            )?;
        }

        let source_degree = self.adjacency[source].len();
        for edge_index in 0..source_degree {
            record_auxiliary_inspection(
                recorder.as_deref_mut(),
                self,
                source,
                edge_index,
                FeasibilityTraceEventKind::InspectSourceArc,
                false,
                metrics,
            )?;
            let capacity = self.adjacency[source][edge_index].capacity;
            if capacity == 0 {
                continue;
            }
            let pushed = self.push_arc(source, edge_index, capacity)?;
            let to_before = excess[pushed.to];
            if pushed.to != source {
                excess[pushed.to] = excess[pushed.to]
                    .checked_add(capacity)
                    .ok_or(FeasibilityError::ArithmeticOverflow)?;
            }
            record_auxiliary_push(
                recorder.as_deref_mut(),
                self,
                &pushed,
                (pushed.to != source)
                    .then_some((pushed.to, to_before, excess[pushed.to]))
                    .into_iter(),
                metrics,
            )?;
            if activate(pushed.to, source, sink, &excess, &mut active, &mut queue) {
                record_activation(recorder.as_deref_mut(), self, pushed.to)?;
            }
        }

        while let Some(node) = queue.pop_front() {
            active[node] = false;
            metrics.active_node_selections = metrics
                .active_node_selections
                .checked_add(1)
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            if let Some(recorder) = recorder.as_deref_mut() {
                let node_id = self.node_id(node)?;
                recorder.record(
                    FeasibilityTraceEventKind::SelectActiveNode,
                    Some(node_id.clone()),
                    None,
                    vec![
                        FeasibilityTracePatch::QueuePopFront {
                            node: node_id.clone(),
                        },
                        FeasibilityTracePatch::NodeActive {
                            node: node_id,
                            before: true,
                            after: false,
                        },
                    ],
                    *metrics,
                )?;
            }
            while excess[node] > 0 {
                if current[node] == self.adjacency[node].len() {
                    let mut minimum_height = None;
                    for edge_index in 0..self.adjacency[node].len() {
                        record_auxiliary_inspection(
                            recorder.as_deref_mut(),
                            self,
                            node,
                            edge_index,
                            FeasibilityTraceEventKind::InspectRelabelArc,
                            false,
                            metrics,
                        )?;
                        let edge = &self.adjacency[node][edge_index];
                        if edge.capacity > 0 {
                            minimum_height =
                                Some(minimum_height.map_or(height[edge.to], |old: usize| {
                                    old.min(height[edge.to])
                                }));
                        }
                    }
                    let minimum_height =
                        minimum_height.ok_or(FeasibilityError::ArithmeticOverflow)?;
                    let before_height = height[node];
                    height[node] = minimum_height
                        .checked_add(1)
                        .ok_or(FeasibilityError::ArithmeticOverflow)?;
                    let before_current = current[node];
                    current[node] = 0;
                    metrics.relabels = metrics
                        .relabels
                        .checked_add(1)
                        .ok_or(FeasibilityError::ArithmeticOverflow)?;
                    if let Some(recorder) = recorder.as_deref_mut() {
                        let node_id = self.node_id(node)?;
                        recorder.record(
                            FeasibilityTraceEventKind::Relabel,
                            Some(node_id.clone()),
                            None,
                            vec![
                                FeasibilityTracePatch::NodeHeight {
                                    node: node_id.clone(),
                                    before: before_height,
                                    after: height[node],
                                },
                                FeasibilityTracePatch::NodeCurrentArc {
                                    node: node_id,
                                    before: before_current,
                                    after: 0,
                                },
                            ],
                            *metrics,
                        )?;
                    }
                    continue;
                }
                let edge_index = current[node];
                record_auxiliary_inspection(
                    recorder.as_deref_mut(),
                    self,
                    node,
                    edge_index,
                    FeasibilityTraceEventKind::InspectDischargeArc,
                    false,
                    metrics,
                )?;
                let to = self.adjacency[node][edge_index].to;
                if self.adjacency[node][edge_index].capacity > 0
                    && height[node] == height[to].saturating_add(1)
                {
                    let amount = excess[node].min(self.adjacency[node][edge_index].capacity);
                    let was_zero = excess[to] == 0;
                    let node_before = excess[node];
                    let to_before = excess[to];
                    let pushed = self.push_arc(node, edge_index, amount)?;
                    excess[node] -= amount;
                    if pushed.to != source {
                        excess[pushed.to] = excess[pushed.to]
                            .checked_add(amount)
                            .ok_or(FeasibilityError::ArithmeticOverflow)?;
                    }
                    record_auxiliary_push(
                        recorder.as_deref_mut(),
                        self,
                        &pushed,
                        std::iter::once((node, node_before, excess[node])).chain(
                            (pushed.to != source).then_some((
                                pushed.to,
                                to_before,
                                excess[pushed.to],
                            )),
                        ),
                        metrics,
                    )?;
                    if was_zero
                        && activate(pushed.to, source, sink, &excess, &mut active, &mut queue)
                    {
                        record_activation(recorder.as_deref_mut(), self, pushed.to)?;
                    }
                } else {
                    let before = current[node];
                    current[node] += 1;
                    if let Some(recorder) = recorder.as_deref_mut() {
                        let node_id = self.node_id(node)?;
                        recorder.record(
                            FeasibilityTraceEventKind::AdvanceCurrentArc,
                            Some(node_id.clone()),
                            Some(self.residual_arc_id(node, edge_index)?),
                            vec![FeasibilityTracePatch::NodeCurrentArc {
                                node: node_id,
                                before,
                                after: current[node],
                            }],
                            *metrics,
                        )?;
                    }
                }
            }
            metrics.discharges = metrics
                .discharges
                .checked_add(1)
                .ok_or(FeasibilityError::ArithmeticOverflow)?;
            if let Some(recorder) = recorder.as_deref_mut() {
                recorder.record(
                    FeasibilityTraceEventKind::CompleteDischarge,
                    Some(self.node_id(node)?),
                    None,
                    Vec::new(),
                    *metrics,
                )?;
            }
        }
        Ok(excess[sink])
    }

    fn push_arc(
        &mut self,
        from: usize,
        edge_index: usize,
        amount: u128,
    ) -> Result<AuxiliaryPush, FeasibilityError> {
        let edge = self
            .adjacency
            .get(from)
            .and_then(|edges| edges.get(edge_index))
            .ok_or(FeasibilityError::TraceInvariant)?
            .clone();
        let to = edge.to;
        let reverse = edge.reverse;
        let flow_before = match edge.direction {
            FeasibilityResidualDirection::Forward => self.adjacency[to][reverse].capacity,
            FeasibilityResidualDirection::Reverse => edge.capacity,
        };
        let flow_after = match edge.direction {
            FeasibilityResidualDirection::Forward => flow_before
                .checked_add(amount)
                .ok_or(FeasibilityError::ArithmeticOverflow)?,
            FeasibilityResidualDirection::Reverse => flow_before
                .checked_sub(amount)
                .ok_or(FeasibilityError::ArithmeticOverflow)?,
        };
        self.adjacency[from][edge_index].capacity = self.adjacency[from][edge_index]
            .capacity
            .checked_sub(amount)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        self.adjacency[to][reverse].capacity = self.adjacency[to][reverse]
            .capacity
            .checked_add(amount)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        Ok(AuxiliaryPush {
            to,
            residual_arc: FeasibilityResidualArcId {
                arc: edge.arc,
                direction: edge.direction,
            },
            flow_before,
            flow_after,
        })
    }

    fn reachable_from(
        &self,
        source: usize,
        mut recorder: Option<&mut FeasibilityTraceRecorder>,
        metrics: &mut FeasibilityTraceMetrics,
    ) -> Result<Vec<bool>, FeasibilityError> {
        let mut reachable = vec![false; self.adjacency.len()];
        let mut queue = VecDeque::from([source]);
        reachable[source] = true;
        if let Some(recorder) = recorder.as_deref_mut() {
            let node = self.node_id(source)?;
            recorder.record(
                FeasibilityTraceEventKind::MarkReachable,
                Some(node.clone()),
                None,
                vec![FeasibilityTracePatch::NodeReachable {
                    node,
                    before: false,
                    after: true,
                }],
                *metrics,
            )?;
        }
        while let Some(node) = queue.pop_front() {
            for (edge_index, edge) in self.adjacency[node].iter().enumerate() {
                record_auxiliary_inspection(
                    recorder.as_deref_mut(),
                    self,
                    node,
                    edge_index,
                    FeasibilityTraceEventKind::InspectCutArc,
                    true,
                    metrics,
                )?;
                if edge.capacity > 0 && !reachable[edge.to] {
                    reachable[edge.to] = true;
                    queue.push_back(edge.to);
                    if let Some(recorder) = recorder.as_deref_mut() {
                        let target = self.node_id(edge.to)?;
                        recorder.record(
                            FeasibilityTraceEventKind::MarkReachable,
                            Some(target.clone()),
                            Some(self.residual_arc_id(node, edge_index)?),
                            vec![FeasibilityTracePatch::NodeReachable {
                                node: target,
                                before: false,
                                after: true,
                            }],
                            *metrics,
                        )?;
                    }
                }
            }
        }
        Ok(reachable)
    }
}

fn record_auxiliary_inspection(
    recorder: Option<&mut FeasibilityTraceRecorder>,
    auxiliary: &AuxiliaryGraph,
    node: usize,
    edge_index: usize,
    kind: FeasibilityTraceEventKind,
    cut: bool,
    metrics: &mut FeasibilityTraceMetrics,
) -> Result<(), FeasibilityError> {
    if cut {
        metrics.cut_adjacency_inspections = metrics
            .cut_adjacency_inspections
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
    } else {
        metrics.auxiliary_adjacency_inspections = metrics
            .auxiliary_adjacency_inspections
            .checked_add(1)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
    }
    let Some(recorder) = recorder else {
        return Ok(());
    };
    recorder.record(
        kind,
        Some(auxiliary.node_id(node)?),
        Some(auxiliary.residual_arc_id(node, edge_index)?),
        Vec::new(),
        *metrics,
    )
}

fn record_activation(
    recorder: Option<&mut FeasibilityTraceRecorder>,
    auxiliary: &AuxiliaryGraph,
    node: usize,
) -> Result<(), FeasibilityError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let node = auxiliary.node_id(node)?;
    recorder.record(
        FeasibilityTraceEventKind::ActivateNode,
        Some(node.clone()),
        None,
        vec![
            FeasibilityTracePatch::NodeActive {
                node: node.clone(),
                before: false,
                after: true,
            },
            FeasibilityTracePatch::QueuePushBack { node },
        ],
        recorder.current.metrics,
    )
}

fn record_auxiliary_push<I>(
    recorder: Option<&mut FeasibilityTraceRecorder>,
    auxiliary: &AuxiliaryGraph,
    pushed: &AuxiliaryPush,
    excess_changes: I,
    metrics: &mut FeasibilityTraceMetrics,
) -> Result<(), FeasibilityError>
where
    I: IntoIterator<Item = (usize, u128, u128)>,
{
    metrics.pushes = metrics
        .pushes
        .checked_add(1)
        .ok_or(FeasibilityError::ArithmeticOverflow)?;
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let mut patches = vec![FeasibilityTracePatch::ArcFlow {
        arc: pushed.residual_arc.arc.clone(),
        before: pushed.flow_before,
        after: pushed.flow_after,
    }];
    if let FeasibilityArcId::Original(edge) = &pushed.residual_arc.arc {
        let original = recorder
            .current
            .original_flows
            .iter()
            .find(|state| state.edge == *edge)
            .ok_or(FeasibilityError::TraceInvariant)?;
        let before_shifted =
            u64::try_from(pushed.flow_before).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
        let after_shifted =
            u64::try_from(pushed.flow_after).map_err(|_| FeasibilityError::ArithmeticOverflow)?;
        let lower = original
            .flow
            .checked_sub(before_shifted)
            .ok_or(FeasibilityError::TraceInvariant)?;
        let after = lower
            .checked_add(after_shifted)
            .ok_or(FeasibilityError::ArithmeticOverflow)?;
        patches.push(FeasibilityTracePatch::OriginalFlow {
            edge: edge.clone(),
            before: original.flow,
            after,
        });
    }
    for (node, before, after) in excess_changes {
        patches.push(FeasibilityTracePatch::NodeExcess {
            node: auxiliary.node_id(node)?,
            before,
            after,
        });
    }
    recorder.record(
        FeasibilityTraceEventKind::Push,
        None,
        Some(pushed.residual_arc.clone()),
        patches,
        *metrics,
    )
}

fn activate(
    node: usize,
    source: usize,
    sink: usize,
    excess: &[u128],
    active: &mut [bool],
    queue: &mut VecDeque<usize>,
) -> bool {
    if node != source && node != sink && excess[node] > 0 && !active[node] {
        active[node] = true;
        queue.push_back(node);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::certificate::{check_min_cost_flow, fixed_flow_divergences};
    use crate::model::{EdgeId, FlowNode, UnresolvedFlowEdge};

    use super::*;

    fn graph(capacity: u64, lower: u64) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let source_id = NodeId::parse("s").expect("source id");
        let sink_id = NodeId::parse("t").expect("sink id");
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(source_id.clone(), 0),
                FlowNode::new(sink_id.clone(), 0),
            ],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("edge").expect("edge id"),
                from: source_id.clone(),
                to: sink_id.clone(),
                lower,
                capacity,
                cost: 1,
            }],
        )
        .expect("valid graph");
        let source = graph.node_index(&source_id).expect("source");
        let sink = graph.node_index(&sink_id).expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn exact_fixed_flow_respects_lower_bound() {
        let (graph, source, sink) = graph(5, 2);
        let target = fixed_flow_divergences(&graph, source, sink, 3).expect("target");
        let feasible = find_feasible_flow(&graph, &target).expect("feasible");

        assert_eq!(feasible.flows, vec![3]);
        check_min_cost_flow(&graph, &target, &feasible.flows).expect("valid flow");
    }

    fn assert_trace_replays(trace: &FeasibilityTrace) {
        let mut snapshot = trace.base_snapshot.clone();
        for event in &trace.events {
            apply_feasibility_trace_event(&mut snapshot, event, FeasibilityTraceDirection::Forward)
                .expect("forward feasibility replay");
        }
        assert_eq!(snapshot, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_feasibility_trace_event(&mut snapshot, event, FeasibilityTraceDirection::Reverse)
                .expect("reverse feasibility replay");
        }
        assert_eq!(snapshot, trace.base_snapshot);
    }

    #[test]
    fn traced_feasibility_is_same_execution_reversible_and_work_exact() {
        let (graph, source, sink) = graph(5, 2);
        let target = fixed_flow_divergences(&graph, source, sink, 3).expect("target");
        let fast = find_feasible_flow(&graph, &target).expect("fast feasible");
        let traced = trace_feasible_flow(&graph, &target).expect("traced feasible");
        assert_eq!(
            traced.outcome,
            FeasibilityTraceOutcome::Feasible(fast.clone())
        );
        assert_eq!(
            traced.trace.events.last().map(|event| event.kind),
            Some(FeasibilityTraceEventKind::Feasible)
        );
        assert_eq!(
            traced.trace.final_snapshot.original_flows,
            vec![FeasibilityOriginalFlowState {
                edge: EdgeId::parse("edge").expect("edge id"),
                flow: fast.flows[0],
            }]
        );
        let metrics = traced.trace.final_snapshot.metrics;
        assert_eq!(
            traced
                .trace
                .events
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        FeasibilityTraceEventKind::InspectSourceArc
                            | FeasibilityTraceEventKind::InspectDischargeArc
                            | FeasibilityTraceEventKind::InspectRelabelArc
                    )
                })
                .count() as u128,
            metrics.auxiliary_adjacency_inspections
        );
        assert_eq!(
            traced
                .trace
                .events
                .iter()
                .filter(|event| event.kind == FeasibilityTraceEventKind::Push)
                .count() as u128,
            metrics.pushes
        );
        assert!(traced.trace.events.iter().all(|event| {
            !matches!(
                event.kind,
                FeasibilityTraceEventKind::InspectSourceArc
                    | FeasibilityTraceEventKind::InspectDischargeArc
                    | FeasibilityTraceEventKind::InspectRelabelArc
                    | FeasibilityTraceEventKind::InspectCutArc
            ) || event.focus_arc.is_some()
        }));
        check_feasibility_trace(&graph, &target, &traced).expect("trace checker");
        assert_trace_replays(&traced.trace);
    }

    #[test]
    fn infeasible_target_returns_unsatisfied_cut_witness() {
        let (graph, source, sink) = graph(1, 0);
        let target = fixed_flow_divergences(&graph, source, sink, 2).expect("target");
        let error = find_feasible_flow(&graph, &target).expect_err("infeasible");

        let FeasibilityError::Infeasible(witness) = error else {
            panic!("expected infeasibility witness");
        };
        assert_eq!(witness.unsatisfied, 1);
        check_balance_infeasibility(&graph, &target, &witness).expect("witness verifies");

        let mut corrupt = witness;
        corrupt.unsatisfied = 2;
        assert_eq!(
            check_balance_infeasibility(&graph, &target, &corrupt),
            Err(FeasibilityError::InvalidDivergence)
        );
    }

    #[test]
    fn traced_infeasibility_retains_and_replays_the_cut_bfs() {
        let (graph, source, sink) = graph(1, 0);
        let target = fixed_flow_divergences(&graph, source, sink, 2).expect("target");
        let traced = trace_feasible_flow(&graph, &target).expect("traced outcome");
        let FeasibilityTraceOutcome::Infeasible(witness) = &traced.outcome else {
            panic!("expected infeasibility outcome");
        };
        check_balance_infeasibility(&graph, &target, witness).expect("witness verifies");
        assert_eq!(
            traced.trace.events.last().map(|event| event.kind),
            Some(FeasibilityTraceEventKind::Infeasible)
        );
        let cut_inspections = traced
            .trace
            .events
            .iter()
            .filter(|event| event.kind == FeasibilityTraceEventKind::InspectCutArc)
            .count() as u128;
        assert!(cut_inspections > 0);
        assert_eq!(
            cut_inspections,
            traced
                .trace
                .final_snapshot
                .metrics
                .cut_adjacency_inspections
        );
        check_feasibility_trace(&graph, &target, &traced).expect("trace checker");
        assert_trace_replays(&traced.trace);
    }

    #[test]
    fn temporary_sink_to_source_edge_builds_lower_bound_initial_state() {
        let (graph, source, sink) = graph(5, 2);
        let feasible = find_max_flow_initial(&graph, source, sink).expect("feasible");

        assert_eq!(feasible.flows, vec![2]);
    }

    #[test]
    fn traced_lower_bound_initialization_exposes_the_return_and_super_arcs() {
        let (graph, source, sink) = graph(5, 2);
        let fast = find_max_flow_initial(&graph, source, sink).expect("fast feasible");
        let traced = trace_max_flow_initial(&graph, source, sink).expect("traced feasible");
        assert_eq!(
            traced.outcome,
            FeasibilityTraceOutcome::Feasible(fast.clone())
        );
        assert!(
            traced
                .trace
                .final_snapshot
                .arcs
                .iter()
                .any(|arc| matches!(arc.id, FeasibilityArcId::LowerBoundReturn { .. }))
        );
        assert!(
            traced
                .trace
                .final_snapshot
                .arcs
                .iter()
                .any(|arc| matches!(arc.id, FeasibilityArcId::FromSuperSource(_)))
        );
        assert!(
            traced
                .trace
                .final_snapshot
                .arcs
                .iter()
                .any(|arc| matches!(arc.id, FeasibilityArcId::ToSuperSink(_)))
        );
        assert_eq!(
            traced.trace.final_snapshot.original_flows[0].flow,
            fast.flows[0]
        );
        check_max_flow_initial_trace(&graph, source, sink, &traced).expect("trace checker");
        assert_trace_replays(&traced.trace);
    }

    #[test]
    fn trace_checker_rejects_missing_reordered_and_corrupt_source_events() {
        let (graph, source, sink) = graph(5, 2);
        let target = fixed_flow_divergences(&graph, source, sink, 3).expect("target");
        let traced = trace_feasible_flow(&graph, &target).expect("traced feasible");

        let mut missing = traced.clone();
        let inspection = missing
            .trace
            .events
            .iter()
            .position(|event| event.kind == FeasibilityTraceEventKind::InspectSourceArc)
            .expect("source inspection");
        missing.trace.events.remove(inspection);
        assert_eq!(
            check_feasibility_trace(&graph, &target, &missing),
            Err(FeasibilityError::TraceInvariant)
        );

        let mut reordered = traced.clone();
        reordered.trace.events.swap(0, 1);
        assert_eq!(
            check_feasibility_trace(&graph, &target, &reordered),
            Err(FeasibilityError::TraceInvariant)
        );

        let mut corrupt = traced;
        let event = corrupt
            .trace
            .events
            .iter_mut()
            .find(|event| event.kind == FeasibilityTraceEventKind::InspectSourceArc)
            .expect("source inspection");
        event.focus_node = Some(FeasibilityNodeId::Original(
            NodeId::parse("ghost").expect("ghost node id"),
        ));
        assert_eq!(
            check_feasibility_trace(&graph, &target, &corrupt),
            Err(FeasibilityError::TraceInvariant)
        );

        let a = NodeId::parse("a").expect("node id");
        let b = NodeId::parse("b").expect("node id");
        let t = NodeId::parse("t").expect("node id");
        let multi_source_graph = FlowNetwork::new(
            vec![
                FlowNode::new(a.clone(), 0),
                FlowNode::new(b.clone(), 0),
                FlowNode::new(t.clone(), 0),
            ],
            vec![
                UnresolvedFlowEdge {
                    id: EdgeId::parse("a-t").expect("edge id"),
                    from: a,
                    to: t.clone(),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                },
                UnresolvedFlowEdge {
                    id: EdgeId::parse("b-t").expect("edge id"),
                    from: b,
                    to: t,
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                },
            ],
        )
        .expect("multi-source feasibility graph");
        let multi_source_target = vec![1, 1, -2];
        let traced = trace_feasible_flow(&multi_source_graph, &multi_source_target)
            .expect("multi-source traced feasible");
        let distinct_source_arcs = traced
            .trace
            .events
            .iter()
            .filter(|event| event.kind == FeasibilityTraceEventKind::InspectSourceArc)
            .filter_map(|event| event.focus_arc.clone())
            .collect::<BTreeSet<_>>();
        let mut alternatives = distinct_source_arcs.into_iter();
        let original = alternatives.next().expect("first source operand");
        let replacement = alternatives.next().expect("second source operand");
        let mut forged_incident_focus = traced;
        let event = forged_incident_focus
            .trace
            .events
            .iter_mut()
            .find(|event| {
                event.kind == FeasibilityTraceEventKind::InspectSourceArc
                    && event.focus_arc.as_ref() == Some(&original)
            })
            .expect("source event to forge");
        event.focus_arc = Some(replacement);
        assert_eq!(
            check_feasibility_trace(
                &multi_source_graph,
                &multi_source_target,
                &forged_incident_focus,
            ),
            Err(FeasibilityError::TraceInvariant),
            "an incident but noncanonical kernel operand must not be accepted as typed focus"
        );
    }

    #[test]
    fn auxiliary_self_loop_pairing_does_not_corrupt_flow_extraction() {
        let node_id = NodeId::parse("v").expect("node id");
        let graph = FlowNetwork::new(
            vec![FlowNode::new(node_id.clone(), 0)],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("loop").expect("edge id"),
                from: node_id.clone(),
                to: node_id,
                lower: 1,
                capacity: 3,
                cost: -1,
            }],
        )
        .expect("valid graph");

        assert_eq!(
            find_feasible_flow(&graph, &[0]).expect("feasible").flows,
            vec![1]
        );
    }

    #[test]
    fn explicit_source_capture_retains_only_the_recorded_feasibility_call() {
        let (graph, source, sink) = graph(5, 2);
        let target = fixed_flow_divergences(&graph, source, sink, 3).expect("target");
        let (flow, captured) = capture_feasibility_traces(|execution| {
            let flow = execution
                .find_feasible_flow(&graph, &target, FeasibilityUse::InitialFlow)
                .expect("source feasibility");
            find_feasible_flow(&graph, &target).expect("independent replay");
            flow
        });

        assert_eq!(captured.len(), 1);
        assert_eq!(
            captured[0].request,
            CapturedFeasibilityRequest::Balance {
                required_divergence: target,
            }
        );
        assert_eq!(
            captured[0].result.outcome,
            FeasibilityTraceOutcome::Feasible(flow)
        );
        check_captured_feasibility_trace(&captured[0]).expect("captured trace");
    }

    #[test]
    fn explicit_metric_capture_matches_trace_without_retaining_checker_work() {
        let (graph, source, sink) = graph(5, 2);
        let target = fixed_flow_divergences(&graph, source, sink, 3).expect("target");
        let traced = trace_feasible_flow(&graph, &target).expect("trace");
        let (flow, captured) = capture_feasibility_metrics(|execution| {
            let flow = execution
                .find_feasible_flow(&graph, &target, FeasibilityUse::InitialFlow)
                .expect("source feasibility");
            find_feasible_flow(&graph, &target).expect("independent replay");
            flow
        });

        assert_eq!(
            FeasibilityTraceOutcome::Feasible(flow),
            traced.outcome,
            "metric-only execution must retain the same mathematical result"
        );
        assert_eq!(
            captured,
            FeasibilityMetricSummary {
                total: traced.trace.final_snapshot.metrics,
                invocations: 1,
            }
        );
    }

    #[test]
    fn source_capture_preserves_max_flow_return_edge_identity() {
        let (graph, source, sink) = graph(5, 2);
        let (flow, captured) = capture_feasibility_traces(|execution| {
            execution
                .find_max_flow_initial(&graph, source, sink, FeasibilityUse::InitialFlow)
                .expect("source initialization")
        });

        assert_eq!(captured.len(), 1);
        assert_eq!(
            captured[0].request,
            CapturedFeasibilityRequest::MaxFlowInitial { source, sink }
        );
        assert_eq!(
            captured[0].result.outcome,
            FeasibilityTraceOutcome::Feasible(flow)
        );
        assert!(
            captured[0]
                .result
                .trace
                .final_snapshot
                .arcs
                .iter()
                .any(|arc| matches!(arc.id, FeasibilityArcId::LowerBoundReturn { .. }))
        );
        check_captured_feasibility_trace(&captured[0]).expect("captured max-flow trace");
    }

    #[test]
    fn zero_lower_bound_max_flow_uses_the_canonical_zero_initial_state() {
        let (graph, source, sink) = graph(5, 0);
        let (flow, captured) = capture_feasibility_traces(|execution| {
            execution
                .find_max_flow_initial(&graph, source, sink, FeasibilityUse::InitialFlow)
                .expect("zero initial state")
        });

        assert_eq!(flow.flows, vec![0]);
        assert!(
            captured.is_empty(),
            "a zero-lower-bound max-flow instance must not run an auxiliary feasibility solver"
        );
    }
}
