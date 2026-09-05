//! Hochbaum's labeling pseudoflow with explicit normalized-forest work.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeIndex, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot, apply_trace_event,
};

/// Conservative interactive node admission for the explicit-tree implementation.
pub const PSEUDOFLOW_MAX_NODES: usize = 256;
/// Conservative interactive edge admission for the explicit-tree implementation.
pub const PSEUDOFLOW_MAX_EDGES: usize = 2_048;
/// Hard ceiling for materialized residual-arc inspections.
pub const PSEUDOFLOW_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Hard ceiling for relabel, merger, normalization, and recovery transitions.
pub const PSEUDOFLOW_MAX_STATE_TRANSITIONS: u64 = 100_000;
const PSEUDOFLOW_TRACE_DENSE_SCAN_PREFIX: u128 = 16;
const PSEUDOFLOW_TRACE_SCAN_BLOCK_MAX: u128 = 256;

/// Exact counters for the deterministic labeling-pseudoflow kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PseudoflowMetrics {
    /// Positive transformed or recovery residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Strong-to-weak branch mergers.
    pub mergers: u64,
    /// Basic-labeling updates over the current strong set.
    pub relabels: u64,
    /// Residual-arc pushes performed while renormalizing a merged branch.
    pub normalization_pushes: u64,
    /// Saturated tree arcs removed into new branches.
    pub splits: u64,
    /// Real residual arcs traversed by pseudoflow-simplex pivot cycles.
    pub pivot_cycle_arcs: u128,
    /// Zero-delta pseudoflow-simplex basis pivots.
    pub degenerate_pivots: u64,
    /// Pseudoflow-simplex pivots leaving through an internal basis arc.
    pub internal_leaves: u64,
    /// Degenerate exchanges in which the entering arc immediately leaves.
    pub entering_leaves: u64,
    /// Pseudoflow-simplex pivots leaving through a virtual strong-root arc.
    pub strong_root_leaves: u64,
    /// Pseudoflow-simplex pivots leaving through a virtual weak-root arc.
    pub weak_root_leaves: u64,
    /// Residual paths used to recover a feasible maximum flow.
    pub recovery_paths: u64,
    /// Residual-arc mutations across all recovery paths.
    pub recovery_arc_pushes: u64,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that stop before exhausting the selected residual arc.
    pub nonsaturating_pushes: u64,
}

/// Certified maximum flow recovered from an optimal normalized tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PseudoflowResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Solver-independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact explicit-tree operation counters.
    pub metrics: PseudoflowMetrics,
}

/// Certified result with reversible labeling, merger, split, and recovery events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PseudoflowTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: PseudoflowResult,
    /// Boundary before source/sink-adjacent residual arcs are saturated.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete deterministic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Boundary after feasible-flow recovery and independent certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Exact counters for Hochbaum's one-enter/one-leave pseudoflow simplex.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PseudoflowSimplexMetrics {
    /// Positive residual arcs inspected by entering-arc selection and recovery.
    pub residual_arc_scans: u128,
    /// One-enter/one-leave basis pivots.
    pub pivots: u64,
    /// Real residual arcs on all pivot cycles, excluding the two virtual root arcs.
    pub pivot_cycle_arcs: u128,
    /// Pivots whose first bottleneck has zero residual capacity.
    pub degenerate_pivots: u64,
    /// Strong-set label updates.
    pub relabels: u64,
    /// Pivots that remove an internal basis arc.
    pub internal_leaves: u64,
    /// Pivots in which the entering arc is also the first bottleneck.
    pub entering_leaves: u64,
    /// Pivots that remove the strong branch's virtual excess arc.
    pub strong_root_leaves: u64,
    /// Pivots that remove the weak branch's virtual deficit arc.
    pub weak_root_leaves: u64,
    /// Residual-arc mutations on positive-delta pivot cycles.
    pub pivot_arc_pushes: u64,
    /// Residual paths used to recover a feasible maximum flow.
    pub recovery_paths: u64,
    /// Residual-arc mutations across all recovery paths.
    pub recovery_arc_pushes: u64,
    /// Pushes that exhaust the selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that leave positive capacity on the selected residual arc.
    pub nonsaturating_pushes: u64,
}

/// Certified maximum flow recovered from an optimal pseudoflow-simplex basis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PseudoflowSimplexResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Solver-independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Source-defined pivot, basis, and recovery counters.
    pub metrics: PseudoflowSimplexMetrics,
}

/// Certified pseudoflow-simplex result with reversible basis pivots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PseudoflowSimplexTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: PseudoflowSimplexResult,
    /// Boundary before the simple normalized tree is initialized.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete deterministic source-defined event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Boundary after recovery and independent certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Pseudoflow construction, normalization, recovery, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PseudoflowError {
    /// Input exceeds the bounded explicit-tree execution band.
    #[error("graph exceeds pseudoflow admission limits")]
    AdmissionLimit,
    /// A deterministic execution ceiling was reached.
    #[error("pseudoflow work limit reached")]
    WorkLimit,
    /// Lower-bound feasibility construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The recovered result failed independent certification.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact arithmetic exceeded the supported domain.
    #[error("pseudoflow arithmetic overflow")]
    ArithmeticOverflow,
    /// A normalized-tree, label, or recovery invariant failed.
    #[error("pseudoflow invariant failed")]
    Invariant,
    /// Reversible trace construction contradicted the algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Runs Hochbaum's basic labeling pseudoflow and recovers a feasible maximum flow.
///
/// The paper's simple normalized-tree construction is applied to the positive
/// residual network of a feasible lower-bounded flow. This preserves the exact
/// original model while keeping the merger and normalization algorithm genuine.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible bounds, work-limit exhaustion,
/// normalized-tree violations, recovery failure, or certificate failure.
pub fn solve_hochbaum_pseudoflow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PseudoflowResult, PseudoflowError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Runs labeling pseudoflow while recording every visible forest operation.
///
/// # Errors
///
/// Returns the same failures as [`solve_hochbaum_pseudoflow`] plus reversible
/// trace construction failures.
pub fn trace_hochbaum_pseudoflow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PseudoflowTraceResult, PseudoflowError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PseudoflowError::Invariant)?;
    Ok(PseudoflowTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Runs labeling pseudoflow while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_hochbaum_pseudoflow_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<PseudoflowResult, PseudoflowError> {
    solve_internal_with_feasibility(graph, source, sink, false, feasibility).map(|run| run.result)
}

/// Traces labeling pseudoflow while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_hochbaum_pseudoflow_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<PseudoflowTraceResult, PseudoflowError> {
    let run = solve_internal_with_feasibility(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PseudoflowError::Invariant)?;
    Ok(PseudoflowTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Runs Hochbaum's pseudoflow-simplex with one entering and one leaving arc
/// per normalized-tree basis pivot.
///
/// The bounded implementation uses an explicit forest and therefore does not
/// claim the paper's dynamic-tree end-to-end bound.
///
/// # Errors
///
/// Rejects out-of-band graphs, infeasible bounds, work-limit exhaustion,
/// normalized-basis violations, recovery failure, or certificate failure.
pub fn solve_pseudoflow_simplex(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PseudoflowSimplexResult, PseudoflowError> {
    solve_simplex_internal(graph, source, sink, false).map(|run| run.result)
}

/// Runs pseudoflow-simplex while recording entering arcs, complete real cycle
/// sections, the first bottleneck leaving arc, and each resulting basis.
///
/// # Errors
///
/// Returns the same failures as [`solve_pseudoflow_simplex`] plus reversible
/// trace construction or source-conformance failures.
pub fn trace_pseudoflow_simplex(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<PseudoflowSimplexTraceResult, PseudoflowError> {
    let run = solve_simplex_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PseudoflowError::Invariant)?;
    let traced = PseudoflowSimplexTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    };
    check_pseudoflow_simplex_trace(graph, source, sink, &traced)?;
    Ok(traced)
}

/// Runs pseudoflow simplex while explicitly publishing auxiliary feasibility
/// work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_pseudoflow_simplex_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<PseudoflowSimplexResult, PseudoflowError> {
    solve_simplex_internal_with_feasibility(graph, source, sink, false, feasibility)
        .map(|run| run.result)
}

/// Traces pseudoflow simplex while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_pseudoflow_simplex_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<PseudoflowSimplexTraceResult, PseudoflowError> {
    let run = solve_simplex_internal_with_feasibility(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(PseudoflowError::Invariant)?;
    let traced = PseudoflowSimplexTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    };
    check_pseudoflow_simplex_trace(graph, source, sink, &traced)?;
    Ok(traced)
}

struct InternalRun {
    result: PseudoflowResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct SimplexInternalRun {
    result: PseudoflowSimplexResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PseudoDirection {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PseudoResidualId {
    arc: usize,
    direction: PseudoDirection,
}

#[derive(Clone, Debug)]
struct PseudoArc {
    public_forward: ResidualArcId,
    original_edge: EdgeIndex,
    from: NodeIndex,
    to: NodeIndex,
    capacity: u64,
    flow: u64,
}

#[derive(Clone, Debug)]
struct PseudoResidualArc {
    id: PseudoResidualId,
    public_id: ResidualArcId,
    from: NodeIndex,
    to: NodeIndex,
    capacity: u64,
}

struct PseudoflowKernel<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    base_flows: Vec<u64>,
    arcs: Vec<PseudoArc>,
    parent: Vec<Option<NodeIndex>>,
    down_arc: Vec<Option<PseudoResidualId>>,
    labels: Vec<Option<i128>>,
    metrics: PseudoflowMetrics,
    transitions: u64,
    last_published_scan: u128,
}

struct RecoveryTraceView {
    metadata: FlowTraceEventMetadata,
    active_path: Vec<ResidualArcId>,
    search_order: Vec<NodeIndex>,
    detail: Option<(&'static str, i128)>,
    exact_focus: Option<Vec<FlowTraceEntityRef>>,
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
) -> Result<InternalRun, PseudoflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, source, sink, record_trace, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, PseudoflowError> {
    if graph.nodes().len() > PSEUDOFLOW_MAX_NODES || graph.edges().len() > PSEUDOFLOW_MAX_EDGES {
        return Err(PseudoflowError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let initial_state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut kernel = PseudoflowKernel::new(graph, source, sink, initial.flows)?;
    let mut recorder = if record_trace {
        let base = FlowTraceSnapshot::capture(
            graph,
            &initial_state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            vec![0; graph.nodes().len()],
            FlowTraceMetrics::default(),
        );
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };

    kernel.initialize_boundary_arcs()?;
    kernel.validate_normalized()?;
    kernel.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "hochbaum-pseudoflow.initialize",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "pseudoflow:initialize-simple-normalized-tree",
        },
        Vec::new(),
        kernel.strong_nodes()?,
        None,
        None,
    )?;

    let strong_nodes = kernel.run_labeling(recorder.as_mut())?;

    let mut recovered = ResidualState::from_flows(graph, &kernel.project_flows()?)?;
    recover_feasible_flow(
        &mut recovered,
        source,
        sink,
        &mut kernel,
        recorder.as_mut(),
        &strong_nodes,
        LABELING_RECOVERY_EVENTS,
    )?;
    let flows = recovered.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    kernel.record_recovery_state(
        recorder.as_mut(),
        &recovered,
        RecoveryTraceView {
            metadata: FlowTraceEventMetadata {
                catalog_id: "hochbaum-pseudoflow.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "pseudoflow:certify-recovered-maximum-flow",
            },
            active_path: Vec::new(),
            search_order: strong_nodes,
            detail: Some(("cut", certificate.cut_bound)),
            exact_focus: None,
        },
    )?;
    let result = PseudoflowResult {
        flows,
        certificate,
        metrics: kernel.metrics,
    };
    Ok(InternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimplexLeaving {
    StrongRoot,
    WeakRoot,
    Real(PseudoResidualId),
}

struct SimplexPivotPlan {
    entering: PseudoResidualArc,
    real_cycle: Vec<PseudoResidualId>,
    strong_root: NodeIndex,
    weak_root: NodeIndex,
    leaving: SimplexLeaving,
    delta: u64,
}

#[derive(Clone, Copy)]
struct ScanEvents {
    catalog_id: &'static str,
    pseudocode_line: &'static str,
}

const LABELING_SCAN_EVENTS: ScanEvents = ScanEvents {
    catalog_id: "hochbaum-pseudoflow.inspect-residual-arc",
    pseudocode_line: "pseudoflow:inspect-residual-arc",
};

const SIMPLEX_SCAN_EVENTS: ScanEvents = ScanEvents {
    catalog_id: "pseudoflow-simplex.inspect-residual-arc",
    pseudocode_line: "pseudoflow-simplex:inspect-residual-arc",
};

#[allow(clippy::too_many_lines)]
fn solve_simplex_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
) -> Result<SimplexInternalRun, PseudoflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_simplex_internal_with_feasibility(graph, source, sink, record_trace, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn solve_simplex_internal_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<SimplexInternalRun, PseudoflowError> {
    if graph.nodes().len() > PSEUDOFLOW_MAX_NODES || graph.edges().len() > PSEUDOFLOW_MAX_EDGES {
        return Err(PseudoflowError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let initial_state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut kernel = PseudoflowKernel::new(graph, source, sink, initial.flows)?;
    let mut recorder = if record_trace {
        let base = FlowTraceSnapshot::capture(
            graph,
            &initial_state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            vec![0; graph.nodes().len()],
            FlowTraceMetrics::default(),
        );
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };

    kernel.initialize_boundary_arcs()?;
    kernel.validate_normalized()?;
    kernel.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "pseudoflow-simplex.initialize",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "pseudoflow-simplex:initialize-simple-normalized-basis",
        },
        Vec::new(),
        kernel.strong_nodes()?,
        None,
        None,
    )?;

    while let Some(entering) = kernel.select_merger(recorder.as_mut(), SIMPLEX_SCAN_EVENTS)? {
        let strong_nodes = kernel.strong_nodes()?;
        kernel.relabel_strong_set(
            &entering,
            &strong_nodes,
            recorder.as_mut(),
            "pseudoflow-simplex.relabel-strong-set",
            "pseudoflow-simplex:raise-strong-labels",
        )?;
        let plan = kernel.plan_simplex_pivot(entering)?;
        let public_cycle = kernel.public_path(&plan.real_cycle);
        kernel.record(
            recorder.as_mut(),
            FlowTraceEventMetadata {
                catalog_id: "pseudoflow-simplex.select-entering",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "pseudoflow-simplex:select-strong-to-weak-entering-arc",
            },
            public_cycle.clone(),
            vec![
                plan.strong_root,
                plan.entering.from,
                plan.entering.to,
                plan.weak_root,
            ],
            Some(("delta", i128::from(plan.delta))),
            Some(vec![FlowTraceEntityRef::ResidualArc(
                plan.entering.public_id.clone(),
            )]),
        )?;
        let event = kernel.apply_simplex_pivot(&plan)?;
        kernel.record(
            recorder.as_mut(),
            event,
            public_cycle,
            match plan.leaving {
                SimplexLeaving::StrongRoot => vec![plan.strong_root],
                SimplexLeaving::WeakRoot => vec![plan.weak_root],
                SimplexLeaving::Real(id) => {
                    let leaving = kernel.residual(id)?;
                    vec![leaving.from, leaving.to]
                }
            },
            Some(("delta", i128::from(plan.delta))),
            Some(vec![FlowTraceEntityRef::ResidualArc(
                plan.entering.public_id.clone(),
            )]),
        )?;
        kernel.validate_normalized()?;
    }

    let strong_nodes = kernel.strong_nodes()?;
    kernel.validate_optimal_partition(&strong_nodes, recorder.as_mut(), SIMPLEX_SCAN_EVENTS)?;
    kernel.record(
        recorder.as_mut(),
        FlowTraceEventMetadata {
            catalog_id: "pseudoflow-simplex.blocking-cut",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "pseudoflow-simplex:return-maximum-blocking-cut",
        },
        Vec::new(),
        strong_nodes.clone(),
        Some((
            "strong",
            i128::try_from(strong_nodes.len()).map_err(|_| PseudoflowError::ArithmeticOverflow)?,
        )),
        None,
    )?;

    let mut recovered = ResidualState::from_flows(graph, &kernel.project_flows()?)?;
    recover_feasible_flow(
        &mut recovered,
        source,
        sink,
        &mut kernel,
        recorder.as_mut(),
        &strong_nodes,
        SIMPLEX_RECOVERY_EVENTS,
    )?;
    let flows = recovered.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    kernel.record_recovery_state(
        recorder.as_mut(),
        &recovered,
        RecoveryTraceView {
            metadata: FlowTraceEventMetadata {
                catalog_id: "pseudoflow-simplex.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "pseudoflow-simplex:certify-recovered-maximum-flow",
            },
            active_path: Vec::new(),
            search_order: strong_nodes,
            detail: Some(("cut", certificate.cut_bound)),
            exact_focus: None,
        },
    )?;
    let metrics = PseudoflowSimplexMetrics {
        residual_arc_scans: kernel.metrics.residual_arc_scans,
        pivots: kernel.metrics.mergers,
        pivot_cycle_arcs: kernel.metrics.pivot_cycle_arcs,
        degenerate_pivots: kernel.metrics.degenerate_pivots,
        relabels: kernel.metrics.relabels,
        internal_leaves: kernel.metrics.internal_leaves,
        entering_leaves: kernel.metrics.entering_leaves,
        strong_root_leaves: kernel.metrics.strong_root_leaves,
        weak_root_leaves: kernel.metrics.weak_root_leaves,
        pivot_arc_pushes: kernel.metrics.normalization_pushes,
        recovery_paths: kernel.metrics.recovery_paths,
        recovery_arc_pushes: kernel.metrics.recovery_arc_pushes,
        saturating_pushes: kernel.metrics.saturating_pushes,
        nonsaturating_pushes: kernel.metrics.nonsaturating_pushes,
    };
    Ok(SimplexInternalRun {
        result: PseudoflowSimplexResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

impl<'graph> PseudoflowKernel<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        base_flows: Vec<u64>,
    ) -> Result<Self, PseudoflowError> {
        let state = ResidualState::from_flows(graph, &base_flows)?;
        let mut arcs = Vec::with_capacity(graph.edges().len() * 2);
        for edge in graph.edges() {
            let original_edge = graph
                .edge_index(edge.id())
                .ok_or(PseudoflowError::Invariant)?;
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                let public_forward = ResidualArcId::new(edge.id().clone(), direction);
                let residual = state
                    .arc(&public_forward)
                    .ok_or(PseudoflowError::Invariant)?;
                if residual.capacity > 0 {
                    arcs.push(PseudoArc {
                        public_forward,
                        original_edge,
                        from: residual.from,
                        to: residual.to,
                        capacity: residual.capacity,
                        flow: 0,
                    });
                }
            }
        }
        arcs.sort_unstable_by(|left, right| left.public_forward.cmp(&right.public_forward));
        let mut labels = vec![Some(1); graph.nodes().len()];
        labels[source.as_usize()] = None;
        labels[sink.as_usize()] = None;
        Ok(Self {
            graph,
            source,
            sink,
            base_flows,
            arcs,
            parent: vec![None; graph.nodes().len()],
            down_arc: vec![None; graph.nodes().len()],
            labels,
            metrics: PseudoflowMetrics::default(),
            transitions: 0,
            last_published_scan: 0,
        })
    }

    fn initialize_boundary_arcs(&mut self) -> Result<(), PseudoflowError> {
        for arc in &mut self.arcs {
            if (arc.from == self.source && arc.to != self.source)
                || (arc.to == self.sink && arc.from != self.sink)
            {
                arc.flow = arc.capacity;
            }
        }
        self.count_transition()
    }

    fn run_labeling(
        &mut self,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    ) -> Result<Vec<NodeIndex>, PseudoflowError> {
        while let Some(merger) =
            self.select_merger(recorder.as_deref_mut(), LABELING_SCAN_EVENTS)?
        {
            let strong_nodes = self.strong_nodes()?;
            self.relabel_strong_set(
                &merger,
                &strong_nodes,
                recorder.as_deref_mut(),
                "hochbaum-pseudoflow.relabel-strong-set",
                "pseudoflow:raise-strong-labels",
            )?;
            let normalization_path = self.merge_path(&merger)?;
            let root = self.root(merger.from)?;
            let excess = self
                .excesses()?
                .get(root.as_usize())
                .copied()
                .ok_or(PseudoflowError::Invariant)?;
            if excess <= 0 {
                return Err(PseudoflowError::Invariant);
            }
            let amount = u64::try_from(excess).map_err(|_| PseudoflowError::ArithmeticOverflow)?;
            self.invert_and_attach(&merger)?;
            self.metrics.mergers = self
                .metrics
                .mergers
                .checked_add(1)
                .ok_or(PseudoflowError::ArithmeticOverflow)?;
            self.count_transition()?;
            self.record(
                recorder.as_deref_mut(),
                FlowTraceEventMetadata {
                    catalog_id: "hochbaum-pseudoflow.merge",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "pseudoflow:merge-strong-into-weak",
                },
                self.public_path(&normalization_path),
                vec![merger.from, merger.to],
                Some(("excess", excess)),
                Some(vec![FlowTraceEntityRef::ResidualArc(
                    merger.public_id.clone(),
                )]),
            )?;
            self.normalize(
                &normalization_path,
                amount,
                recorder.as_deref_mut(),
                &strong_nodes,
            )?;
            self.validate_normalized()?;
        }
        let strong_nodes = self.strong_nodes()?;
        self.validate_optimal_partition(
            &strong_nodes,
            recorder.as_deref_mut(),
            LABELING_SCAN_EVENTS,
        )?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id: "hochbaum-pseudoflow.blocking-cut",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "pseudoflow:return-maximum-blocking-cut",
            },
            Vec::new(),
            strong_nodes.clone(),
            Some((
                "strong",
                i128::try_from(strong_nodes.len())
                    .map_err(|_| PseudoflowError::ArithmeticOverflow)?,
            )),
            None,
        )?;
        Ok(strong_nodes)
    }

    fn relabel_strong_set(
        &mut self,
        merger: &PseudoResidualArc,
        strong_nodes: &[NodeIndex],
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        catalog_id: &'static str,
        pseudocode_line: &'static str,
    ) -> Result<(), PseudoflowError> {
        let new_label = self
            .labels
            .get(merger.to.as_usize())
            .copied()
            .flatten()
            .and_then(|label| label.checked_add(1))
            .ok_or(PseudoflowError::Invariant)?;
        let mut changed = false;
        for node in strong_nodes {
            let label = self
                .labels
                .get_mut(node.as_usize())
                .ok_or(PseudoflowError::Invariant)?;
            if label.is_some_and(|current| current < new_label) {
                *label = Some(new_label);
                changed = true;
            }
        }
        if !changed {
            return Ok(());
        }
        self.metrics.relabels = self
            .metrics
            .relabels
            .checked_add(1)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        self.count_transition()?;
        self.record(
            recorder,
            FlowTraceEventMetadata {
                catalog_id,
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line,
            },
            vec![merger.public_id.clone()],
            strong_nodes.to_vec(),
            Some(("label", new_label)),
            Some(vec![FlowTraceEntityRef::ResidualArc(
                merger.public_id.clone(),
            )]),
        )
    }

    fn select_merger(
        &mut self,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
        scan_events: ScanEvents,
    ) -> Result<Option<PseudoResidualArc>, PseudoflowError> {
        let excesses = self.excesses()?;
        let roots = self.roots()?;
        let mut candidates = Vec::new();
        let mut pending_scan = None;
        let internal_nodes = self.internal_nodes().collect::<Vec<_>>();
        for node in internal_nodes {
            let root = roots[node.as_usize()];
            if excesses[root.as_usize()] <= 0 {
                continue;
            }
            for arc in self.outgoing_residuals(node)? {
                self.count_scan()?;
                pending_scan = Some(arc.clone());
                if self.should_publish_scan() {
                    self.publish_scan(
                        recorder.as_deref_mut(),
                        None,
                        &arc.public_id,
                        arc.from,
                        arc.to,
                        scan_events,
                    )?;
                    pending_scan = None;
                }
                if !self.is_internal(arc.to) {
                    continue;
                }
                let target_root = roots[arc.to.as_usize()];
                if excesses[target_root.as_usize()] > 0 {
                    continue;
                }
                let weak_label = self
                    .labels
                    .get(arc.to.as_usize())
                    .copied()
                    .flatten()
                    .ok_or(PseudoflowError::Invariant)?;
                candidates.push((
                    weak_label,
                    arc.to.as_usize(),
                    arc.from.as_usize(),
                    arc.public_id.clone(),
                    arc.id,
                    arc,
                ));
            }
        }
        if let Some(arc) = pending_scan {
            self.publish_scan(
                recorder,
                None,
                &arc.public_id,
                arc.from,
                arc.to,
                scan_events,
            )?;
        }
        candidates.sort_unstable_by(|left, right| {
            (&left.0, &left.1, &left.2, &left.3, &left.4)
                .cmp(&(&right.0, &right.1, &right.2, &right.3, &right.4))
        });
        Ok(candidates.into_iter().next().map(|item| item.5))
    }

    fn merge_path(
        &self,
        merger: &PseudoResidualArc,
    ) -> Result<Vec<PseudoResidualId>, PseudoflowError> {
        let strong_root = self.root(merger.from)?;
        let mut strong_reversed = Vec::new();
        let mut cursor = merger.from;
        while cursor != strong_root {
            strong_reversed.push(
                self.down_arc
                    .get(cursor.as_usize())
                    .copied()
                    .flatten()
                    .ok_or(PseudoflowError::Invariant)?,
            );
            cursor = self
                .parent
                .get(cursor.as_usize())
                .copied()
                .flatten()
                .ok_or(PseudoflowError::Invariant)?;
        }
        strong_reversed.reverse();
        let mut path = strong_reversed;
        path.push(merger.id);
        cursor = merger.to;
        while let Some(parent) = self.parent.get(cursor.as_usize()).copied().flatten() {
            let down = self
                .down_arc
                .get(cursor.as_usize())
                .copied()
                .flatten()
                .ok_or(PseudoflowError::Invariant)?;
            let upward = Self::reverse_residual(down);
            let arc = self.residual(upward)?;
            if arc.from != cursor || arc.to != parent {
                return Err(PseudoflowError::Invariant);
            }
            path.push(upward);
            cursor = parent;
        }
        Ok(path)
    }

    fn plan_simplex_pivot(
        &self,
        entering: PseudoResidualArc,
    ) -> Result<SimplexPivotPlan, PseudoflowError> {
        let strong_root = self.root(entering.from)?;
        let weak_root = self.root(entering.to)?;
        if strong_root == weak_root {
            return Err(PseudoflowError::Invariant);
        }
        let excesses = self.excesses()?;
        let strong_excess = *excesses
            .get(strong_root.as_usize())
            .ok_or(PseudoflowError::Invariant)?;
        let weak_excess = *excesses
            .get(weak_root.as_usize())
            .ok_or(PseudoflowError::Invariant)?;
        if strong_excess <= 0 || weak_excess > 0 {
            return Err(PseudoflowError::Invariant);
        }
        let mut delta =
            u64::try_from(strong_excess).map_err(|_| PseudoflowError::ArithmeticOverflow)?;
        let mut leaving = SimplexLeaving::StrongRoot;
        let real_cycle = self.merge_path(&entering)?;
        for id in &real_cycle {
            let capacity = self.residual(*id)?.capacity;
            if capacity < delta {
                delta = capacity;
                leaving = SimplexLeaving::Real(*id);
            }
        }
        let weak_deficit = u64::try_from(
            weak_excess
                .checked_neg()
                .ok_or(PseudoflowError::ArithmeticOverflow)?,
        )
        .map_err(|_| PseudoflowError::ArithmeticOverflow)?;
        if weak_deficit < delta {
            delta = weak_deficit;
            leaving = SimplexLeaving::WeakRoot;
        }
        Ok(SimplexPivotPlan {
            entering,
            real_cycle,
            strong_root,
            weak_root,
            leaving,
            delta,
        })
    }

    #[allow(clippy::too_many_lines)]
    fn apply_simplex_pivot(
        &mut self,
        plan: &SimplexPivotPlan,
    ) -> Result<FlowTraceEventMetadata, PseudoflowError> {
        let cycle_arcs = u128::try_from(plan.real_cycle.len())
            .map_err(|_| PseudoflowError::ArithmeticOverflow)?;
        self.metrics.pivot_cycle_arcs = self
            .metrics
            .pivot_cycle_arcs
            .checked_add(cycle_arcs)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;

        let mut basis = self
            .internal_nodes()
            .filter_map(|node| self.down_arc[node.as_usize()].map(|id| id.arc))
            .collect::<BTreeSet<_>>();
        basis.insert(plan.entering.id.arc);
        let mut roots = self
            .internal_nodes()
            .filter(|node| self.parent[node.as_usize()].is_none())
            .collect::<BTreeSet<_>>();

        let metadata = match plan.leaving {
            SimplexLeaving::StrongRoot => {
                if !roots.remove(&plan.strong_root) {
                    return Err(PseudoflowError::Invariant);
                }
                self.metrics.strong_root_leaves = self
                    .metrics
                    .strong_root_leaves
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                FlowTraceEventMetadata {
                    catalog_id: "pseudoflow-simplex.pivot-leave-strong-root",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "pseudoflow-simplex:exchange-entering-for-strong-root-arc",
                }
            }
            SimplexLeaving::WeakRoot => {
                if !roots.remove(&plan.weak_root) {
                    return Err(PseudoflowError::Invariant);
                }
                self.metrics.weak_root_leaves = self
                    .metrics
                    .weak_root_leaves
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                FlowTraceEventMetadata {
                    catalog_id: "pseudoflow-simplex.pivot-leave-weak-root",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "pseudoflow-simplex:exchange-entering-for-weak-root-arc",
                }
            }
            SimplexLeaving::Real(id) if id.arc == plan.entering.id.arc => {
                if !basis.remove(&id.arc) {
                    return Err(PseudoflowError::Invariant);
                }
                self.metrics.entering_leaves = self
                    .metrics
                    .entering_leaves
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                FlowTraceEventMetadata {
                    catalog_id: "pseudoflow-simplex.pivot-entering-leaves",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "pseudoflow-simplex:remove-entering-first-bottleneck",
                }
            }
            SimplexLeaving::Real(id) => {
                if !basis.remove(&id.arc) {
                    return Err(PseudoflowError::Invariant);
                }
                self.metrics.internal_leaves = self
                    .metrics
                    .internal_leaves
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                FlowTraceEventMetadata {
                    catalog_id: "pseudoflow-simplex.pivot-leave-internal",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "pseudoflow-simplex:exchange-entering-for-first-bottleneck",
                }
            }
        };

        if plan.delta == 0 {
            self.metrics.degenerate_pivots = self
                .metrics
                .degenerate_pivots
                .checked_add(1)
                .ok_or(PseudoflowError::ArithmeticOverflow)?;
        } else {
            for id in &plan.real_cycle {
                let capacity = self.residual(*id)?.capacity;
                self.augment(*id, plan.delta)?;
                self.metrics.normalization_pushes = self
                    .metrics
                    .normalization_pushes
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                self.count_push_partition(capacity == plan.delta)?;
            }
        }
        self.rebuild_basis(&basis, &roots)?;
        self.metrics.mergers = self
            .metrics
            .mergers
            .checked_add(1)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        self.count_transition()?;
        Ok(metadata)
    }

    fn rebuild_basis(
        &mut self,
        basis: &BTreeSet<usize>,
        roots: &BTreeSet<NodeIndex>,
    ) -> Result<(), PseudoflowError> {
        let internal_count = self.internal_nodes().count();
        if basis
            .len()
            .checked_add(roots.len())
            .ok_or(PseudoflowError::ArithmeticOverflow)?
            != internal_count
        {
            return Err(PseudoflowError::Invariant);
        }
        let mut adjacency = vec![Vec::<(NodeIndex, usize)>::new(); self.graph.nodes().len()];
        for &arc_index in basis {
            let arc = self.arcs.get(arc_index).ok_or(PseudoflowError::Invariant)?;
            if !self.is_internal(arc.from) || !self.is_internal(arc.to) || arc.from == arc.to {
                return Err(PseudoflowError::Invariant);
            }
            adjacency[arc.from.as_usize()].push((arc.to, arc_index));
            adjacency[arc.to.as_usize()].push((arc.from, arc_index));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable_by(|left, right| {
                let left_id = &self.arcs[left.1].public_forward;
                let right_id = &self.arcs[right.1].public_forward;
                (left.0, left_id, left.1).cmp(&(right.0, right_id, right.1))
            });
        }

        self.parent.fill(None);
        self.down_arc.fill(None);
        let mut visited = vec![false; self.graph.nodes().len()];
        let mut traversed_edges = BTreeSet::new();
        for &root in roots {
            if !self.is_internal(root) || visited[root.as_usize()] {
                return Err(PseudoflowError::Invariant);
            }
            visited[root.as_usize()] = true;
            let mut queue = VecDeque::from([root]);
            while let Some(parent) = queue.pop_front() {
                for &(child, arc_index) in &adjacency[parent.as_usize()] {
                    if !traversed_edges.insert(arc_index) {
                        continue;
                    }
                    if visited[child.as_usize()] {
                        return Err(PseudoflowError::Invariant);
                    }
                    let arc = self.arcs.get(arc_index).ok_or(PseudoflowError::Invariant)?;
                    let direction = if arc.from == parent && arc.to == child {
                        PseudoDirection::Forward
                    } else if arc.to == parent && arc.from == child {
                        PseudoDirection::Reverse
                    } else {
                        return Err(PseudoflowError::Invariant);
                    };
                    let down = PseudoResidualId {
                        arc: arc_index,
                        direction,
                    };
                    if self.residual(down)?.capacity == 0 {
                        return Err(PseudoflowError::Invariant);
                    }
                    self.parent[child.as_usize()] = Some(parent);
                    self.down_arc[child.as_usize()] = Some(down);
                    visited[child.as_usize()] = true;
                    queue.push_back(child);
                }
            }
        }
        if self.internal_nodes().any(|node| !visited[node.as_usize()])
            || traversed_edges.len() != basis.len()
        {
            return Err(PseudoflowError::Invariant);
        }
        Ok(())
    }

    fn invert_and_attach(&mut self, merger: &PseudoResidualArc) -> Result<(), PseudoflowError> {
        let mut chain = Vec::new();
        let mut cursor = merger.from;
        while let Some(parent) = self.parent.get(cursor.as_usize()).copied().flatten() {
            let down = self
                .down_arc
                .get(cursor.as_usize())
                .copied()
                .flatten()
                .ok_or(PseudoflowError::Invariant)?;
            chain.push((cursor, parent, down));
            cursor = parent;
            if chain.len() > self.graph.nodes().len() {
                return Err(PseudoflowError::Invariant);
            }
        }
        for (child, parent, down) in chain {
            self.parent[parent.as_usize()] = Some(child);
            self.down_arc[parent.as_usize()] = Some(Self::reverse_residual(down));
        }
        self.parent[merger.from.as_usize()] = Some(merger.to);
        self.down_arc[merger.from.as_usize()] = Some(Self::reverse_residual(merger.id));
        Ok(())
    }

    fn normalize(
        &mut self,
        path: &[PseudoResidualId],
        initial_amount: u64,
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
        selected_nodes: &[NodeIndex],
    ) -> Result<(), PseudoflowError> {
        let mut amount = initial_amount;
        for &id in path {
            if amount == 0 {
                break;
            }
            let arc = self.residual(id)?;
            let pushed = amount.min(arc.capacity);
            if pushed > 0 {
                self.augment(id, pushed)?;
                self.metrics.normalization_pushes = self
                    .metrics
                    .normalization_pushes
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                self.count_push_partition(arc.capacity == pushed)?;
            }
            let split = arc.capacity < amount;
            if split {
                if self.parent.get(arc.from.as_usize()).copied().flatten() != Some(arc.to) {
                    return Err(PseudoflowError::Invariant);
                }
                self.parent[arc.from.as_usize()] = None;
                self.down_arc[arc.from.as_usize()] = None;
                self.metrics.splits = self
                    .metrics
                    .splits
                    .checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?;
                amount = arc.capacity;
            }
            self.count_transition()?;
            self.record(
                recorder.as_deref_mut(),
                FlowTraceEventMetadata {
                    catalog_id: if split {
                        "hochbaum-pseudoflow.split"
                    } else {
                        "hochbaum-pseudoflow.normalize-push"
                    },
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: if split {
                        "pseudoflow:split-blocking-tree-arc"
                    } else {
                        "pseudoflow:push-excess-on-merger-path"
                    },
                },
                vec![arc.public_id.clone()],
                selected_nodes.to_vec(),
                Some(("delta", i128::from(pushed))),
                Some(vec![FlowTraceEntityRef::ResidualArc(arc.public_id)]),
            )?;
        }
        Ok(())
    }

    fn validate_normalized(&self) -> Result<(), PseudoflowError> {
        let excesses = self.excesses()?;
        let mut in_tree = vec![false; self.arcs.len()];
        for node in self.internal_nodes() {
            if let Some(parent) = self.parent[node.as_usize()] {
                if !self.is_internal(parent) || excesses[node.as_usize()] != 0 {
                    return Err(PseudoflowError::Invariant);
                }
                let down = self.down_arc[node.as_usize()].ok_or(PseudoflowError::Invariant)?;
                let arc = self.residual(down)?;
                if arc.from != parent || arc.to != node || arc.capacity == 0 {
                    return Err(PseudoflowError::Invariant);
                }
                in_tree[down.arc] = true;
            } else if self.down_arc[node.as_usize()].is_some() {
                return Err(PseudoflowError::Invariant);
            }
            self.root(node)?;
        }
        for (index, arc) in self.arcs.iter().enumerate() {
            if arc.flow > 0 && arc.flow < arc.capacity && !in_tree[index] {
                return Err(PseudoflowError::Invariant);
            }
            if ((arc.from == self.source && arc.to != self.source)
                || (arc.to == self.sink && arc.from != self.sink))
                && arc.flow != arc.capacity
            {
                return Err(PseudoflowError::Invariant);
            }
        }
        Ok(())
    }

    fn validate_optimal_partition(
        &mut self,
        strong_nodes: &[NodeIndex],
        mut recorder: Option<&mut FlowTraceRecorder<'_>>,
        scan_events: ScanEvents,
    ) -> Result<(), PseudoflowError> {
        let strong =
            strong_nodes
                .iter()
                .fold(vec![false; self.graph.nodes().len()], |mut flags, node| {
                    flags[node.as_usize()] = true;
                    flags
                });
        let mut pending_scan = None;
        for node in strong_nodes {
            for arc in self.outgoing_residuals(*node)? {
                self.count_scan()?;
                pending_scan = Some(arc.clone());
                if self.should_publish_scan() {
                    self.publish_scan(
                        recorder.as_deref_mut(),
                        None,
                        &arc.public_id,
                        arc.from,
                        arc.to,
                        scan_events,
                    )?;
                    pending_scan = None;
                }
                if self.is_internal(arc.to) && !strong[arc.to.as_usize()] {
                    return Err(PseudoflowError::Invariant);
                }
            }
        }
        if let Some(arc) = pending_scan {
            self.publish_scan(
                recorder,
                None,
                &arc.public_id,
                arc.from,
                arc.to,
                scan_events,
            )?;
        }
        Ok(())
    }

    fn outgoing_residuals(
        &self,
        node: NodeIndex,
    ) -> Result<Vec<PseudoResidualArc>, PseudoflowError> {
        let mut result = Vec::new();
        for (index, _) in self.arcs.iter().enumerate() {
            for direction in [PseudoDirection::Forward, PseudoDirection::Reverse] {
                let arc = self.residual(PseudoResidualId {
                    arc: index,
                    direction,
                })?;
                if arc.from == node && arc.capacity > 0 {
                    result.push(arc);
                }
            }
        }
        result.sort_unstable_by(|left, right| {
            (&left.public_id, left.id).cmp(&(&right.public_id, right.id))
        });
        Ok(result)
    }

    fn residual(&self, id: PseudoResidualId) -> Result<PseudoResidualArc, PseudoflowError> {
        let arc = self.arcs.get(id.arc).ok_or(PseudoflowError::Invariant)?;
        Ok(match id.direction {
            PseudoDirection::Forward => PseudoResidualArc {
                id,
                public_id: arc.public_forward.clone(),
                from: arc.from,
                to: arc.to,
                capacity: arc.capacity - arc.flow,
            },
            PseudoDirection::Reverse => PseudoResidualArc {
                id,
                public_id: reverse_public(&arc.public_forward),
                from: arc.to,
                to: arc.from,
                capacity: arc.flow,
            },
        })
    }

    fn augment(&mut self, id: PseudoResidualId, amount: u64) -> Result<(), PseudoflowError> {
        if amount == 0 {
            return Err(PseudoflowError::Invariant);
        }
        let residual = self.residual(id)?;
        if residual.capacity < amount {
            return Err(PseudoflowError::Invariant);
        }
        let arc = self
            .arcs
            .get_mut(id.arc)
            .ok_or(PseudoflowError::Invariant)?;
        arc.flow = match id.direction {
            PseudoDirection::Forward => arc.flow.checked_add(amount),
            PseudoDirection::Reverse => arc.flow.checked_sub(amount),
        }
        .ok_or(PseudoflowError::ArithmeticOverflow)?;
        Ok(())
    }

    fn reverse_residual(id: PseudoResidualId) -> PseudoResidualId {
        PseudoResidualId {
            arc: id.arc,
            direction: match id.direction {
                PseudoDirection::Forward => PseudoDirection::Reverse,
                PseudoDirection::Reverse => PseudoDirection::Forward,
            },
        }
    }

    fn root(&self, node: NodeIndex) -> Result<NodeIndex, PseudoflowError> {
        let mut cursor = node;
        for _ in 0..=self.graph.nodes().len() {
            match self.parent.get(cursor.as_usize()).copied().flatten() {
                Some(parent) => cursor = parent,
                None => return Ok(cursor),
            }
        }
        Err(PseudoflowError::Invariant)
    }

    fn roots(&self) -> Result<Vec<NodeIndex>, PseudoflowError> {
        self.graph
            .node_indices()
            .map(|node| {
                if self.is_internal(node) {
                    self.root(node)
                } else {
                    Ok(node)
                }
            })
            .collect()
    }

    fn strong_nodes(&self) -> Result<Vec<NodeIndex>, PseudoflowError> {
        let excesses = self.excesses()?;
        Ok(self
            .internal_nodes()
            .filter(|node| {
                self.root(*node)
                    .is_ok_and(|root| excesses[root.as_usize()] > 0)
            })
            .collect())
    }

    fn excesses(&self) -> Result<Vec<i128>, PseudoflowError> {
        let mut excesses = vec![0_i128; self.graph.nodes().len()];
        for arc in &self.arcs {
            let amount = i128::from(arc.flow);
            excesses[arc.from.as_usize()] = excesses[arc.from.as_usize()]
                .checked_sub(amount)
                .ok_or(PseudoflowError::ArithmeticOverflow)?;
            excesses[arc.to.as_usize()] = excesses[arc.to.as_usize()]
                .checked_add(amount)
                .ok_or(PseudoflowError::ArithmeticOverflow)?;
        }
        Ok(excesses)
    }

    fn project_flows(&self) -> Result<Vec<u64>, PseudoflowError> {
        let mut flows = self
            .base_flows
            .iter()
            .copied()
            .map(i128::from)
            .collect::<Vec<_>>();
        for arc in &self.arcs {
            let target = flows
                .get_mut(arc.original_edge.as_usize())
                .ok_or(PseudoflowError::Invariant)?;
            let amount = i128::from(arc.flow);
            *target = match arc.public_forward.direction() {
                ResidualDirection::Forward => target.checked_add(amount),
                ResidualDirection::Reverse => target.checked_sub(amount),
            }
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        }
        flows
            .into_iter()
            .map(|flow| u64::try_from(flow).map_err(|_| PseudoflowError::Invariant))
            .collect()
    }

    fn public_path(&self, path: &[PseudoResidualId]) -> Vec<ResidualArcId> {
        path.iter()
            .filter_map(|id| self.residual(*id).ok().map(|arc| arc.public_id))
            .collect()
    }

    fn record(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        metadata: FlowTraceEventMetadata,
        active_path: Vec<ResidualArcId>,
        search_order: Vec<NodeIndex>,
        detail: Option<(&'static str, i128)>,
        exact_focus: Option<Vec<FlowTraceEntityRef>>,
    ) -> Result<(), PseudoflowError> {
        let Some(recorder) = recorder else {
            return Ok(());
        };
        let flows = self.project_flows()?;
        let state = ResidualState::from_flows(self.graph, &flows)?;
        let snapshot = FlowTraceSnapshot::capture(
            self.graph,
            &state,
            self.labels.clone(),
            search_order,
            active_path,
            self.excesses()?,
            trace_metrics(self.metrics),
        )
        .with_forest_overlay(self.graph, self.forest_public_arcs()?, self.strong_nodes()?);
        if let Some(focus) = exact_focus {
            recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
        } else {
            recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
        }
        Ok(())
    }

    fn record_recovery_state(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        state: &ResidualState<'_>,
        view: RecoveryTraceView,
    ) -> Result<(), PseudoflowError> {
        let Some(recorder) = recorder else {
            return Ok(());
        };
        let divergence = divergences(self.graph, state.flows())?;
        let excesses = divergence
            .into_iter()
            .map(|value| {
                value
                    .checked_neg()
                    .ok_or(PseudoflowError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let snapshot = FlowTraceSnapshot::capture(
            self.graph,
            state,
            self.labels.clone(),
            view.search_order,
            view.active_path,
            excesses,
            trace_metrics(self.metrics),
        )
        .with_forest_overlay(self.graph, self.forest_public_arcs()?, self.strong_nodes()?);
        if let Some(focus) = view.exact_focus {
            recorder.record_transition_with_detail_and_focus(
                view.metadata,
                &snapshot,
                view.detail,
                focus,
            )?;
        } else {
            recorder.record_transition_with_detail(view.metadata, &snapshot, view.detail)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_scan(
        &self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        recovery_state: Option<&ResidualState<'_>>,
        public_id: &ResidualArcId,
        from: NodeIndex,
        to: NodeIndex,
        events: ScanEvents,
    ) -> Result<(), PseudoflowError> {
        let metadata = FlowTraceEventMetadata {
            catalog_id: events.catalog_id,
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: events.pseudocode_line,
        };
        let detail = Some((
            "scan",
            i128::try_from(self.metrics.residual_arc_scans)
                .map_err(|_| PseudoflowError::ArithmeticOverflow)?,
        ));
        if let Some(state) = recovery_state {
            return self.record_recovery_state(
                recorder,
                state,
                RecoveryTraceView {
                    metadata,
                    active_path: vec![public_id.clone()],
                    search_order: vec![from, to],
                    detail,
                    exact_focus: Some(vec![FlowTraceEntityRef::ResidualArc(public_id.clone())]),
                },
            );
        }
        self.record(
            recorder,
            metadata,
            vec![public_id.clone()],
            vec![from, to],
            detail,
            Some(vec![FlowTraceEntityRef::ResidualArc(public_id.clone())]),
        )
    }

    fn count_scan(&mut self) -> Result<(), PseudoflowError> {
        if self.metrics.residual_arc_scans >= PSEUDOFLOW_MAX_RESIDUAL_ARC_SCANS {
            return Err(PseudoflowError::WorkLimit);
        }
        self.metrics.residual_arc_scans = self
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        Ok(())
    }

    fn should_publish_scan(&self) -> bool {
        self.metrics.residual_arc_scans <= PSEUDOFLOW_TRACE_DENSE_SCAN_PREFIX
            || self
                .metrics
                .residual_arc_scans
                .saturating_sub(self.last_published_scan)
                >= PSEUDOFLOW_TRACE_SCAN_BLOCK_MAX
    }

    fn publish_scan(
        &mut self,
        recorder: Option<&mut FlowTraceRecorder<'_>>,
        recovery_state: Option<&ResidualState<'_>>,
        public_id: &ResidualArcId,
        from: NodeIndex,
        to: NodeIndex,
        events: ScanEvents,
    ) -> Result<(), PseudoflowError> {
        self.record_scan(recorder, recovery_state, public_id, from, to, events)?;
        self.last_published_scan = self.metrics.residual_arc_scans;
        Ok(())
    }

    fn count_transition(&mut self) -> Result<(), PseudoflowError> {
        if self.transitions >= PSEUDOFLOW_MAX_STATE_TRANSITIONS {
            return Err(PseudoflowError::WorkLimit);
        }
        self.transitions = self
            .transitions
            .checked_add(1)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        Ok(())
    }

    fn count_push_partition(&mut self, saturating: bool) -> Result<(), PseudoflowError> {
        let target = if saturating {
            &mut self.metrics.saturating_pushes
        } else {
            &mut self.metrics.nonsaturating_pushes
        };
        *target = target
            .checked_add(1)
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        Ok(())
    }

    fn is_internal(&self, node: NodeIndex) -> bool {
        node != self.source && node != self.sink
    }

    fn internal_nodes(&self) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph
            .node_indices()
            .filter(|node| self.is_internal(*node))
    }

    fn forest_public_arcs(&self) -> Result<Vec<ResidualArcId>, PseudoflowError> {
        self.internal_nodes()
            .filter_map(|node| self.down_arc[node.as_usize()])
            .map(|id| self.residual(id).map(|arc| arc.public_id))
            .collect()
    }
}

#[derive(Clone, Copy)]
struct RecoveryEvents {
    excess_catalog_id: &'static str,
    excess_pseudocode_line: &'static str,
    deficit_catalog_id: &'static str,
    deficit_pseudocode_line: &'static str,
    scan: ScanEvents,
}

const LABELING_RECOVERY_EVENTS: RecoveryEvents = RecoveryEvents {
    excess_catalog_id: "hochbaum-pseudoflow.recover-excess",
    excess_pseudocode_line: "pseudoflow:send-root-excess-to-source-or-deficit",
    deficit_catalog_id: "hochbaum-pseudoflow.recover-deficit",
    deficit_pseudocode_line: "pseudoflow:send-sink-excess-to-deficit-root",
    scan: LABELING_SCAN_EVENTS,
};

const SIMPLEX_RECOVERY_EVENTS: RecoveryEvents = RecoveryEvents {
    excess_catalog_id: "pseudoflow-simplex.recover-excess",
    excess_pseudocode_line: "pseudoflow-simplex:send-root-excess-to-source-or-deficit",
    deficit_catalog_id: "pseudoflow-simplex.recover-deficit",
    deficit_pseudocode_line: "pseudoflow-simplex:send-sink-excess-to-deficit-root",
    scan: SIMPLEX_SCAN_EVENTS,
};

fn recover_feasible_flow(
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut PseudoflowKernel<'_>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    strong_nodes: &[NodeIndex],
    events: RecoveryEvents,
) -> Result<(), PseudoflowError> {
    loop {
        let excesses = flow_excesses(state.graph(), state.flows())?;
        let Some(start) = state
            .graph()
            .node_indices()
            .filter(|node| *node != source && *node != sink)
            .find(|node| excesses[node.as_usize()] > 0)
        else {
            break;
        };
        let search = recovery_bfs(
            state,
            start,
            |node| {
                node == source || (node != sink && node != start && excesses[node.as_usize()] < 0)
            },
            kernel,
            recorder.as_deref_mut(),
            events.scan,
        )?;
        let target = search.target.ok_or(PseudoflowError::Invariant)?;
        let mut amount = excesses[start.as_usize()];
        if target != source {
            amount = amount.min(
                excesses[target.as_usize()]
                    .checked_neg()
                    .ok_or(PseudoflowError::ArithmeticOverflow)?,
            );
        }
        recover_on_path(
            state,
            &search.path,
            amount,
            kernel,
            recorder.as_deref_mut(),
            events.excess_catalog_id,
            events.excess_pseudocode_line,
            vec![start, target],
            strong_nodes,
        )?;
    }
    loop {
        let excesses = flow_excesses(state.graph(), state.flows())?;
        if !state
            .graph()
            .node_indices()
            .filter(|node| *node != source && *node != sink)
            .any(|node| excesses[node.as_usize()] < 0)
        {
            break;
        }
        let search = recovery_bfs(
            state,
            sink,
            |node| node != source && node != sink && excesses[node.as_usize()] < 0,
            kernel,
            recorder.as_deref_mut(),
            events.scan,
        )?;
        let target = search.target.ok_or(PseudoflowError::Invariant)?;
        let amount = excesses[target.as_usize()]
            .checked_neg()
            .ok_or(PseudoflowError::ArithmeticOverflow)?;
        recover_on_path(
            state,
            &search.path,
            amount,
            kernel,
            recorder.as_deref_mut(),
            events.deficit_catalog_id,
            events.deficit_pseudocode_line,
            vec![sink, target],
            strong_nodes,
        )?;
    }
    Ok(())
}

struct RecoverySearch {
    target: Option<NodeIndex>,
    path: Vec<ResidualArcId>,
}

fn recovery_bfs(
    state: &ResidualState<'_>,
    start: NodeIndex,
    mut is_target: impl FnMut(NodeIndex) -> bool,
    kernel: &mut PseudoflowKernel<'_>,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    scan_events: ScanEvents,
) -> Result<RecoverySearch, PseudoflowError> {
    let mut predecessor = vec![None; state.graph().nodes().len()];
    let mut visited = vec![false; state.graph().nodes().len()];
    let mut queue = VecDeque::from([start]);
    visited[start.as_usize()] = true;
    let mut target = None;
    let mut pending_scan = None;
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            kernel.count_scan()?;
            pending_scan = Some(arc.clone());
            if kernel.should_publish_scan() {
                kernel.publish_scan(
                    recorder.as_deref_mut(),
                    Some(state),
                    &arc.id,
                    arc.from,
                    arc.to,
                    scan_events,
                )?;
                pending_scan = None;
            }
            if visited[arc.to.as_usize()] {
                continue;
            }
            visited[arc.to.as_usize()] = true;
            predecessor[arc.to.as_usize()] = Some(arc.id);
            if is_target(arc.to) {
                target = Some(arc.to);
                queue.clear();
                break;
            }
            queue.push_back(arc.to);
        }
    }
    if let Some(arc) = pending_scan {
        kernel.publish_scan(
            recorder,
            Some(state),
            &arc.id,
            arc.from,
            arc.to,
            scan_events,
        )?;
    }
    let path = target.map_or_else(Vec::new, |target| {
        reconstruct_recovery_path(state, start, target, &predecessor).unwrap_or_default()
    });
    if target.is_some() && path.is_empty() {
        return Err(PseudoflowError::Invariant);
    }
    Ok(RecoverySearch { target, path })
}

fn reconstruct_recovery_path(
    state: &ResidualState<'_>,
    start: NodeIndex,
    target: NodeIndex,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, PseudoflowError> {
    let mut reversed = Vec::new();
    let mut cursor = target;
    while cursor != start {
        let id = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(PseudoflowError::Invariant)?;
        let arc = state.arc(&id).ok_or(PseudoflowError::Invariant)?;
        if arc.to != cursor {
            return Err(PseudoflowError::Invariant);
        }
        reversed.push(id);
        cursor = arc.from;
        if reversed.len() > state.graph().nodes().len() {
            return Err(PseudoflowError::Invariant);
        }
    }
    reversed.reverse();
    Ok(reversed)
}

#[allow(clippy::too_many_arguments)]
fn recover_on_path(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    requested: i128,
    kernel: &mut PseudoflowKernel<'_>,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    catalog_id: &'static str,
    pseudocode_line: &'static str,
    search_order: Vec<NodeIndex>,
    strong_nodes: &[NodeIndex],
) -> Result<(), PseudoflowError> {
    let bottleneck = path
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(PseudoflowError::Invariant)?;
    let requested = u64::try_from(requested).map_err(|_| PseudoflowError::ArithmeticOverflow)?;
    let amount = requested.min(bottleneck);
    state.augment(path, amount)?;
    kernel.metrics.recovery_paths = kernel
        .metrics
        .recovery_paths
        .checked_add(1)
        .ok_or(PseudoflowError::ArithmeticOverflow)?;
    let arc_count = u64::try_from(path.len()).map_err(|_| PseudoflowError::ArithmeticOverflow)?;
    kernel.metrics.recovery_arc_pushes = kernel
        .metrics
        .recovery_arc_pushes
        .checked_add(arc_count)
        .ok_or(PseudoflowError::ArithmeticOverflow)?;
    for id in path {
        let capacity_after = state.arc(id).ok_or(PseudoflowError::Invariant)?.capacity;
        kernel.count_push_partition(capacity_after == 0)?;
    }
    kernel.count_transition()?;
    kernel.record_recovery_state(
        recorder,
        state,
        RecoveryTraceView {
            metadata: FlowTraceEventMetadata {
                catalog_id,
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line,
            },
            active_path: path.to_vec(),
            search_order: if search_order.is_empty() {
                strong_nodes.to_vec()
            } else {
                search_order
            },
            detail: Some(("delta", i128::from(amount))),
            exact_focus: Some(
                path.iter()
                    .cloned()
                    .map(FlowTraceEntityRef::ResidualArc)
                    .collect(),
            ),
        },
    )?;
    Ok(())
}

fn flow_excesses(graph: &FlowNetwork, flows: &[u64]) -> Result<Vec<i128>, PseudoflowError> {
    divergences(graph, flows)?
        .into_iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(PseudoflowError::ArithmeticOverflow)
        })
        .collect()
}

fn reverse_public(id: &ResidualArcId) -> ResidualArcId {
    ResidualArcId::new(
        id.original_edge().clone(),
        match id.direction() {
            ResidualDirection::Forward => ResidualDirection::Reverse,
            ResidualDirection::Reverse => ResidualDirection::Forward,
        },
    )
}

const fn trace_metrics(metrics: PseudoflowMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.recovery_paths as u128,
        path_searches: metrics.pivot_cycle_arcs,
        scaling_phases: metrics.strong_root_leaves as u128,
        blocking_flow_phases: metrics.weak_root_leaves as u128,
        relabels: metrics.relabels as u128,
        retreats: metrics.internal_leaves as u128,
        reverse_bfs_runs: metrics.entering_leaves as u128,
        gap_terminations: 0,
        pushes: metrics.normalization_pushes as u128 + metrics.recovery_arc_pushes as u128,
        saturating_pushes: metrics.saturating_pushes as u128,
        nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
        discharges: metrics.degenerate_pivots as u128,
        active_vertex_selections: metrics.mergers as u128,
    }
}

/// Independently checks the pseudoflow-simplex event grammar, every committed
/// normalized basis, reversible replay, and the recovered maximum-flow result.
///
/// # Errors
///
/// Returns an invariant, replay, or certificate error when any source-defined
/// pivot or normalized-tree condition is contradicted.
pub fn check_pseudoflow_simplex_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    run: &PseudoflowSimplexTraceResult,
) -> Result<(), PseudoflowError> {
    if run.events.is_empty() {
        return Err(PseudoflowError::Invariant);
    }
    let mut snapshot = run.base_snapshot.clone();
    let mut initialized = false;
    let mut entering_selected = false;
    let mut blocking_cut_seen = false;
    let mut optimal_seen = false;
    for (index, event) in run.events.iter().enumerate() {
        let expected_line =
            simplex_pseudocode_line(&event.catalog_id).ok_or(PseudoflowError::Invariant)?;
        if event.pseudocode_line != expected_line || optimal_seen {
            return Err(PseudoflowError::Invariant);
        }
        match event.catalog_id.as_str() {
            "pseudoflow-simplex.initialize" if index == 0 && !initialized => {
                initialized = true;
            }
            "pseudoflow-simplex.relabel-strong-set"
                if initialized && !entering_selected && !blocking_cut_seen => {}
            "pseudoflow-simplex.inspect-residual-arc" if initialized && !entering_selected => {}
            "pseudoflow-simplex.select-entering"
                if initialized && !entering_selected && !blocking_cut_seen =>
            {
                entering_selected = true;
            }
            "pseudoflow-simplex.pivot-leave-strong-root"
            | "pseudoflow-simplex.pivot-leave-weak-root"
            | "pseudoflow-simplex.pivot-entering-leaves"
            | "pseudoflow-simplex.pivot-leave-internal"
                if entering_selected && !blocking_cut_seen =>
            {
                entering_selected = false;
            }
            "pseudoflow-simplex.blocking-cut"
                if initialized && !entering_selected && !blocking_cut_seen =>
            {
                blocking_cut_seen = true;
            }
            "pseudoflow-simplex.recover-excess" | "pseudoflow-simplex.recover-deficit"
                if blocking_cut_seen => {}
            "pseudoflow-simplex.optimal" if blocking_cut_seen && index + 1 == run.events.len() => {
                optimal_seen = true;
            }
            _ => return Err(PseudoflowError::Invariant),
        }
        apply_trace_event(graph, &mut snapshot, event, FlowTraceDirection::Forward)?;
        if !blocking_cut_seen || event.catalog_id == "pseudoflow-simplex.blocking-cut" {
            check_simplex_normalized_snapshot(graph, source, sink, &snapshot)?;
        }
        if event.catalog_id == "pseudoflow-simplex.blocking-cut" {
            check_simplex_blocking_cut(graph, source, sink, &snapshot)?;
        }
    }
    if entering_selected || !blocking_cut_seen || !optimal_seen || snapshot != run.final_snapshot {
        return Err(PseudoflowError::Invariant);
    }
    let certificate = check_max_flow(graph, source, sink, &run.result.flows)?;
    if certificate != run.result.certificate || snapshot.flows != run.result.flows {
        return Err(PseudoflowError::Invariant);
    }
    for event in run.events.iter().rev() {
        apply_trace_event(graph, &mut snapshot, event, FlowTraceDirection::Reverse)?;
    }
    if snapshot != run.base_snapshot {
        return Err(PseudoflowError::Invariant);
    }
    Ok(())
}

fn simplex_pseudocode_line(catalog_id: &str) -> Option<&'static str> {
    Some(match catalog_id {
        "pseudoflow-simplex.initialize" => "pseudoflow-simplex:initialize-simple-normalized-basis",
        "pseudoflow-simplex.relabel-strong-set" => "pseudoflow-simplex:raise-strong-labels",
        "pseudoflow-simplex.inspect-residual-arc" => "pseudoflow-simplex:inspect-residual-arc",
        "pseudoflow-simplex.select-entering" => {
            "pseudoflow-simplex:select-strong-to-weak-entering-arc"
        }
        "pseudoflow-simplex.pivot-leave-strong-root" => {
            "pseudoflow-simplex:exchange-entering-for-strong-root-arc"
        }
        "pseudoflow-simplex.pivot-leave-weak-root" => {
            "pseudoflow-simplex:exchange-entering-for-weak-root-arc"
        }
        "pseudoflow-simplex.pivot-entering-leaves" => {
            "pseudoflow-simplex:remove-entering-first-bottleneck"
        }
        "pseudoflow-simplex.pivot-leave-internal" => {
            "pseudoflow-simplex:exchange-entering-for-first-bottleneck"
        }
        "pseudoflow-simplex.blocking-cut" => "pseudoflow-simplex:return-maximum-blocking-cut",
        "pseudoflow-simplex.recover-excess" => {
            "pseudoflow-simplex:send-root-excess-to-source-or-deficit"
        }
        "pseudoflow-simplex.recover-deficit" => {
            "pseudoflow-simplex:send-sink-excess-to-deficit-root"
        }
        "pseudoflow-simplex.optimal" => "pseudoflow-simplex:certify-recovered-maximum-flow",
        _ => return None,
    })
}

fn check_simplex_normalized_snapshot(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), PseudoflowError> {
    if snapshot.remaining_divergence.len() != graph.nodes().len()
        || snapshot.node_labels.len() != graph.nodes().len()
    {
        return Err(PseudoflowError::Invariant);
    }
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    let mut parent = vec![None; graph.nodes().len()];
    for id in &snapshot.forest_arcs {
        let arc = state.arc(id).ok_or(PseudoflowError::Invariant)?;
        if arc.capacity == 0
            || arc.from == source
            || arc.from == sink
            || arc.to == source
            || arc.to == sink
            || arc.from == arc.to
            || parent[arc.to.as_usize()].replace(arc.from).is_some()
        {
            return Err(PseudoflowError::Invariant);
        }
    }
    let mut roots = BTreeSet::new();
    for node in graph
        .node_indices()
        .filter(|node| *node != source && *node != sink)
    {
        let mut cursor = node;
        let mut steps = 0;
        while let Some(next) = parent[cursor.as_usize()] {
            cursor = next;
            steps += 1;
            if steps > graph.nodes().len() {
                return Err(PseudoflowError::Invariant);
            }
        }
        roots.insert(cursor);
        if node != cursor && snapshot.remaining_divergence[node.as_usize()] != 0 {
            return Err(PseudoflowError::Invariant);
        }
    }
    let internal_count = graph.nodes().len().saturating_sub(2);
    if snapshot
        .forest_arcs
        .len()
        .checked_add(roots.len())
        .ok_or(PseudoflowError::ArithmeticOverflow)?
        != internal_count
    {
        return Err(PseudoflowError::Invariant);
    }

    let declared_strong = snapshot
        .strong_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut actual_strong = BTreeSet::new();
    for node in graph
        .node_indices()
        .filter(|node| *node != source && *node != sink)
    {
        let mut root = node;
        while let Some(next) = parent[root.as_usize()] {
            root = next;
        }
        if snapshot.remaining_divergence[root.as_usize()] > 0 {
            actual_strong.insert(
                graph
                    .node(node)
                    .ok_or(PseudoflowError::Invariant)?
                    .id()
                    .clone(),
            );
        }
    }
    if actual_strong != declared_strong {
        return Err(PseudoflowError::Invariant);
    }

    for edge in graph.edges() {
        for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
            let id = ResidualArcId::new(edge.id().clone(), direction);
            let Some(arc) = state.arc(&id).filter(|arc| arc.capacity > 0) else {
                continue;
            };
            if arc.from == source || arc.from == sink || arc.to == source || arc.to == sink {
                continue;
            }
            let from =
                snapshot.node_labels[arc.from.as_usize()].ok_or(PseudoflowError::Invariant)?;
            let to = snapshot.node_labels[arc.to.as_usize()].ok_or(PseudoflowError::Invariant)?;
            if from
                > to.checked_add(1)
                    .ok_or(PseudoflowError::ArithmeticOverflow)?
            {
                return Err(PseudoflowError::Invariant);
            }
        }
    }
    Ok(())
}

fn check_simplex_blocking_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), PseudoflowError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    let strong = snapshot
        .strong_nodes
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for edge in graph.edges() {
        for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
            let id = ResidualArcId::new(edge.id().clone(), direction);
            let Some(arc) = state.arc(&id).filter(|arc| arc.capacity > 0) else {
                continue;
            };
            if arc.from == source || arc.from == sink || arc.to == source || arc.to == sink {
                continue;
            }
            let from = graph.node(arc.from).ok_or(PseudoflowError::Invariant)?.id();
            let to = graph.node(arc.to).ok_or(PseudoflowError::Invariant)?.id();
            if strong.contains(from) && !strong.contains(to) {
                return Err(PseudoflowError::Invariant);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn network(
        nodes: &[&str],
        edges: &[(&str, &str, &str, u64, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let graph = FlowNetwork::new(
            nodes
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, lower, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    #[test]
    fn merges_splits_recovers_and_replays_both_directions() {
        let (graph, source, sink) = network(
            &["s", "a", "b", "c", "t"],
            &[
                ("sa", "s", "a", 0, 9),
                ("sb", "s", "b", 0, 4),
                ("ac", "a", "c", 0, 3),
                ("bc", "b", "c", 0, 7),
                ("ct", "c", "t", 0, 8),
            ],
        );
        let fast = solve_hochbaum_pseudoflow(&graph, source, sink).expect("fast");
        let traced = trace_hochbaum_pseudoflow(&graph, source, sink).expect("trace");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.value, 7);
        assert!(fast.metrics.mergers > 0);
        assert!(fast.metrics.splits > 0);
        assert!(fast.metrics.recovery_paths > 0);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "hochbaum-pseudoflow.blocking-cut")
        );
        let mut snapshot = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("forward");
        }
        assert_eq!(snapshot, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Reverse)
                .expect("reverse");
        }
        assert_eq!(snapshot, traced.base_snapshot);
    }

    #[test]
    fn preserves_lower_bounds_parallel_opposites_self_loops_and_terminal_incidence() {
        let (graph, source, sink) = network(
            &["s", "a", "b", "t"],
            &[
                ("as", "a", "s", 1, 3),
                ("sa0", "s", "a", 2, 7),
                ("sa1", "s", "a", 0, 4),
                ("ab", "a", "b", 1, 6),
                ("ba", "b", "a", 0, 3),
                ("bb", "b", "b", 1, 2),
                ("bt0", "b", "t", 2, 8),
                ("bt1", "b", "t", 0, 5),
                ("ta", "t", "a", 1, 2),
            ],
        );
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        let actual = solve_hochbaum_pseudoflow(&graph, source, sink).expect("pseudoflow");
        assert_eq!(actual.certificate.value, expected.certificate.value);
        check_max_flow(&graph, source, sink, &actual.flows).expect("certificate");
        let simplex = solve_pseudoflow_simplex(&graph, source, sink).expect("pseudoflow simplex");
        assert_eq!(simplex.certificate.value, expected.certificate.value);
        check_max_flow(&graph, source, sink, &simplex.flows).expect("simplex certificate");
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_edmonds_karp() {
        for seed in 0_u64..64 {
            let mut edges = vec![
                ("sa", "s", "a", 0, 1 + seed % 11),
                ("sb", "s", "b", 0, 1 + seed.rotate_left(3) % 13),
                ("at", "a", "t", 0, 1 + seed.rotate_left(7) % 9),
                ("bt", "b", "t", 0, 1 + seed.rotate_left(11) % 15),
                ("ab", "a", "b", 0, seed.rotate_left(17) % 8),
                ("ba", "b", "a", 0, seed.rotate_left(23) % 8),
            ];
            if seed % 2 == 0 {
                edges.push(("st", "s", "t", 0, seed % 5));
            }
            let (graph, source, sink) = network(&["s", "a", "b", "t"], &edges);
            let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            let actual = solve_hochbaum_pseudoflow(&graph, source, sink).expect("pseudoflow");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "seed {seed}"
            );
        }
    }

    #[test]
    fn simplex_uses_one_enter_one_leave_pivots_and_replays() {
        let (graph, source, sink) = network(
            &["s", "a", "b", "c", "t"],
            &[
                ("sa", "s", "a", 0, 9),
                ("sb", "s", "b", 0, 4),
                ("ac", "a", "c", 0, 3),
                ("bc", "b", "c", 0, 7),
                ("ct", "c", "t", 0, 8),
            ],
        );
        let fast = solve_pseudoflow_simplex(&graph, source, sink).expect("fast simplex");
        let traced = trace_pseudoflow_simplex(&graph, source, sink).expect("traced simplex");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.value, 7);
        assert!(fast.metrics.pivots > 0);
        assert!(fast.metrics.pivot_cycle_arcs >= u128::from(fast.metrics.pivots));
        assert_eq!(
            traced
                .events
                .iter()
                .filter(|event| event.catalog_id == "pseudoflow-simplex.select-entering")
                .count(),
            usize::try_from(fast.metrics.pivots).expect("bounded pivot count")
        );
        assert_eq!(
            traced
                .events
                .iter()
                .filter(|event| event.catalog_id.starts_with("pseudoflow-simplex.pivot-"))
                .count(),
            usize::try_from(fast.metrics.pivots).expect("bounded pivot count")
        );
        check_pseudoflow_simplex_trace(&graph, source, sink, &traced).expect("trace checker");
    }

    #[test]
    fn simplex_exhaustive_five_edge_capacities_match_edmonds_karp() {
        for encoded in 0_u64..243 {
            let mut value = encoded;
            let mut capacities = [0_u64; 5];
            for capacity in &mut capacities {
                *capacity = value % 3;
                value /= 3;
            }
            let (graph, source, sink) = network(
                &["s", "a", "b", "t"],
                &[
                    ("sa", "s", "a", 0, capacities[0]),
                    ("sb", "s", "b", 0, capacities[1]),
                    ("ab", "a", "b", 0, capacities[2]),
                    ("at", "a", "t", 0, capacities[3]),
                    ("bt", "b", "t", 0, capacities[4]),
                ],
            );
            let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            let actual = solve_pseudoflow_simplex(&graph, source, sink)
                .unwrap_or_else(|error| panic!("simplex case {encoded}: {error}"));
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "capacity encoding {encoded}"
            );
            check_max_flow(&graph, source, sink, &actual.flows).expect("certificate");
        }
    }

    #[test]
    fn simplex_trace_checker_rejects_event_identity_corruption() {
        let (graph, source, sink) = network(
            &["s", "a", "t"],
            &[("sa", "s", "a", 0, 3), ("at", "a", "t", 0, 2)],
        );
        let mut traced = trace_pseudoflow_simplex(&graph, source, sink).expect("trace");
        traced.events[0].catalog_id = "pseudoflow-simplex.pivot-leave-internal".to_owned();
        assert_eq!(
            check_pseudoflow_simplex_trace(&graph, source, sink, &traced),
            Err(PseudoflowError::Invariant)
        );
    }
}
