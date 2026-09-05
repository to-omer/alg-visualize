//! Bertsekas--Tseng ordinary-network relaxation for exact minimum-cost flow.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for the integer relaxation method.
pub const RELAXATION_MAX_NODES: usize = 256;
/// Conservative interactive edge limit for the integer relaxation method.
pub const RELAXATION_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling on root iterations.
pub const RELAXATION_MAX_ITERATIONS: u64 = 100_000;
/// Deterministic ceiling on incident-label and cut scans.
pub const RELAXATION_MAX_ARC_SCANS: u128 = 10_000_000;
/// Maximum recorded events for one eager interactive trace.
pub const RELAXATION_MAX_TRACE_EVENTS: usize = 10_000;
/// Maximum aggregate scene-entity units admitted by the eager WASM projection.
pub const RELAXATION_MAX_TRACE_PROJECTION_UNITS: usize = 250_000;
/// Initial arc inspections shown individually before geometric checkpoints.
const RELAXATION_TRACE_SCAN_PREFIX: u128 = 4;

/// Exact deterministic counters from ordinary-network relaxation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelaxationMetrics {
    /// Positive-deficit root iterations.
    pub iterations: u64,
    /// Labeled nodes scanned by the FIFO balanced-arc search.
    pub label_scans: u64,
    /// Nodes labeled across all searches, including every root.
    pub labeled_nodes: u128,
    /// Incident original arcs inspected while extending labels.
    pub label_arc_scans: u128,
    /// Original arcs inspected while computing ascent slopes and price cuts.
    pub cut_arc_scans: u128,
    /// Balanced-path flow augmentations.
    pub augmentations: u64,
    /// Dual-ascent cut price adjustments.
    pub price_adjustments: u64,
    /// Balanced cut arcs moved to a bound during price adjustment.
    pub boundary_flow_updates: u128,
    /// Total units moved by balanced-path augmentations.
    pub augmented_flow_units: u128,
}

impl RelaxationMetrics {
    /// Returns all original-arc inspections charged to this kernel.
    #[must_use]
    pub const fn checked_total_arc_scans(self) -> Option<u128> {
        self.label_arc_scans.checked_add(self.cut_arc_scans)
    }

    /// Projects source-specific counters into the stable scene metric catalog.
    #[must_use]
    pub const fn projected_trace_metrics(self) -> Option<FlowTraceMetrics> {
        let Some(residual_arc_scans) = self.checked_total_arc_scans() else {
            return None;
        };
        Some(FlowTraceMetrics {
            bfs_runs: self.label_scans as u128,
            relaxation_passes: 0,
            residual_arc_scans,
            augmentations: self.augmentations as u128,
            path_searches: self.iterations as u128,
            scaling_phases: 0,
            blocking_flow_phases: 0,
            relabels: self.price_adjustments as u128,
            retreats: 0,
            reverse_bfs_runs: self.augmented_flow_units,
            gap_terminations: 0,
            pushes: self.boundary_flow_updates,
            saturating_pushes: 0,
            nonsaturating_pushes: 0,
            discharges: self.labeled_nodes,
            active_vertex_selections: self.iterations as u128,
        })
    }
}

/// Certified result of Bertsekas--Tseng ordinary-network relaxation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxationResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Final source-convention prices, where tension is `p(from) - p(to)`.
    pub prices: Vec<i128>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: RelaxationMetrics,
}

/// Certified relaxation result with reversible pedagogical events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxationTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: RelaxationResult,
    /// Replay boundary at the source-defined complementary-slack state.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent minimum-cost certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Relaxation construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RelaxationError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds relaxation admission limits")]
    AdmissionLimit,
    /// A deterministic iteration or scan ceiling was reached.
    #[error("relaxation work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and original bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Divergence reconstruction or the final independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("relaxation arithmetic overflow")]
    ArithmeticOverflow,
    /// A state failed ordinary complementary slackness.
    #[error("relaxation complementary-slackness invariant failed")]
    ComplementarySlackness,
    /// FIFO labels, an ascent cut, or a balanced path were inconsistent.
    #[error("relaxation labeling invariant failed")]
    LabelInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by ordinary-network relaxation.
///
/// The source-defined initialization uses zero prices, lower flow on every
/// nonnegative-cost arc, and upper flow on every negative-cost arc. This
/// satisfies ordinary complementary slackness but generally violates node
/// balances. A feasibility construction is run only as a precheck; its flow is
/// deliberately discarded and is not an optimization fallback.
///
/// # Errors
///
/// Rejects admission, infeasibility, checked arithmetic, residual mutation,
/// deterministic work limits, invariant, or final certificate failures.
pub fn solve_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RelaxationResult, RelaxationError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RelaxationResult, RelaxationError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every root selection, FIFO scan, augmentation, and price adjustment.
///
/// # Errors
///
/// Returns the same failures as [`solve_relaxation`] plus trace failures.
pub fn trace_relaxation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RelaxationTraceResult, RelaxationError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(RelaxationError::LabelInvariant)?;
    Ok(RelaxationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces relaxation while explicitly publishing its feasibility precheck to
/// the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_relaxation_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RelaxationTraceResult, RelaxationError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(RelaxationError::LabelInvariant)?;
    Ok(RelaxationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: RelaxationResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn record_initialization(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    deficits: &[i128],
    metrics: RelaxationMetrics,
) -> Result<(), RelaxationError> {
    record_trace(
        recorder,
        graph,
        state,
        prices,
        deficits,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "relaxation.initialize-complementary-slack-state",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "relaxation:set-zero-prices-and-cost-sign-bound-flows",
        },
        TraceView::empty(),
        Some(("positive-deficit", total_positive_deficit(deficits)?)),
    )
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
) -> Result<InternalRun, RelaxationError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_events, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, RelaxationError> {
    validate_admission(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let initial_flows = graph
        .edges()
        .iter()
        .map(|edge| {
            if edge.cost() < 0 {
                edge.capacity()
            } else {
                edge.lower()
            }
        })
        .collect::<Vec<_>>();
    let mut state = ResidualState::from_flows(graph, &initial_flows)?;
    let mut prices = vec![0_i128; graph.nodes().len()];
    let mut deficits = deficits(graph, required_divergence, state.flows())?;
    let mut metrics = RelaxationMetrics::default();
    validate_state(graph, required_divergence, &state, &prices, &deficits)?;
    let mut recorder = start_trace_recorder(graph, &state, &deficits, record_events)?;

    record_initialization(
        recorder.as_mut(),
        graph,
        &state,
        &prices,
        &deficits,
        metrics,
    )?;

    while let Some(root) = first_positive_deficit(graph, &deficits) {
        if metrics.iterations >= RELAXATION_MAX_ITERATIONS {
            return Err(RelaxationError::WorkLimit);
        }
        metrics.iterations = metrics
            .iterations
            .checked_add(1)
            .ok_or(RelaxationError::ArithmeticOverflow)?;
        let root_deficit = deficits[root.as_usize()];
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            &prices,
            &deficits,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "relaxation.select-positive-deficit",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "relaxation:choose-positive-deficit-root",
            },
            TraceView {
                search_order: vec![root],
                active_path: Vec::new(),
                focus: vec![FlowTraceEntityRef::Node(
                    graph.nodes()[root.as_usize()].id().clone(),
                )],
            },
            Some(("deficit", root_deficit)),
        )?;

        run_iteration(
            graph,
            required_divergence,
            &mut state,
            &mut prices,
            &mut deficits,
            root,
            &mut metrics,
            recorder.as_mut(),
        )?;
    }

    validate_state(graph, required_divergence, &state, &prices, &deficits)?;
    record_trace(
        recorder.as_mut(),
        graph,
        &state,
        &prices,
        &deficits,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: "relaxation.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "relaxation:return-when-all-deficits-are-zero",
        },
        TraceView::empty(),
        Some(("positive-deficit", 0)),
    )?;

    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    let result = RelaxationResult {
        flows,
        prices,
        certificate,
        metrics,
    };
    Ok(InternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

struct LabelSearch {
    labeled: Vec<bool>,
    scanned: Vec<bool>,
    predecessor: Vec<Option<(NodeIndex, ResidualArcId)>>,
    label_order: Vec<NodeIndex>,
    queue: VecDeque<NodeIndex>,
}

impl LabelSearch {
    fn new(
        node_count: usize,
        root: NodeIndex,
        metrics: &mut RelaxationMetrics,
    ) -> Result<Self, RelaxationError> {
        let mut labeled = vec![false; node_count];
        labeled[root.as_usize()] = true;
        metrics.labeled_nodes = metrics
            .labeled_nodes
            .checked_add(1)
            .ok_or(RelaxationError::ArithmeticOverflow)?;
        Ok(Self {
            labeled,
            scanned: vec![false; node_count],
            predecessor: vec![None; node_count],
            label_order: vec![root],
            queue: VecDeque::from([root]),
        })
    }
}

// This loop is the relaxation method's single FIFO search transaction. Keeping
// selection, source-time scan publication, and its terminal action together
// preserves their exact order in both fast and trace execution.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_iteration(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &mut Vec<i128>,
    deficits: &mut Vec<i128>,
    root: NodeIndex,
    metrics: &mut RelaxationMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), RelaxationError> {
    let mut search = LabelSearch::new(graph.nodes().len(), root, metrics)?;

    while let Some(node) = search.queue.pop_front() {
        if search.scanned[node.as_usize()] {
            return Err(RelaxationError::LabelInvariant);
        }
        search.scanned[node.as_usize()] = true;
        metrics.label_scans = metrics
            .label_scans
            .checked_add(1)
            .ok_or(RelaxationError::ArithmeticOverflow)?;
        let mut scan_checkpoints = Vec::new();
        let mut label_scan_trace = ArcScanTrace {
            search_order: &search.label_order,
            metrics,
            enabled: recorder.is_some(),
            checkpoints: &mut scan_checkpoints,
        };
        let newly_labeled = extend_labels(
            graph,
            state,
            prices,
            node,
            &mut search.labeled,
            &mut search.predecessor,
            &mut label_scan_trace,
        )?;
        let mut frontier = Vec::with_capacity(newly_labeled.len());
        let mut negative_target = None;
        for step in newly_labeled {
            frontier.push(step.update_residual.clone());
            search.label_order.push(step.target);
            search.queue.push_back(step.target);
            metrics.labeled_nodes = metrics
                .labeled_nodes
                .checked_add(1)
                .ok_or(RelaxationError::ArithmeticOverflow)?;
            if deficits[step.target.as_usize()] < 0 && negative_target.is_none() {
                negative_target = Some(step.target);
            }
        }
        let mut slope_scan_trace = ArcScanTrace {
            search_order: &search.label_order,
            metrics,
            enabled: recorder.is_some(),
            checkpoints: &mut scan_checkpoints,
        };
        let slope = ascent_slope(
            graph,
            state,
            deficits,
            prices,
            &search.scanned,
            &mut slope_scan_trace,
        )?;
        publish_scan_checkpoints(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            deficits,
            scan_checkpoints,
            "relaxation.scan-balanced-arcs",
            "relaxation:inspect-one-balanced-search-or-slope-arc",
        )?;
        let root_id = graph.nodes()[root.as_usize()].id().clone();
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            deficits,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "relaxation.evaluate-ascent-slope",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "relaxation:test-balanced-search-ascent-slope",
            },
            TraceView {
                search_order: search.label_order.clone(),
                active_path: frontier.last().cloned().into_iter().collect(),
                focus: vec![FlowTraceEntityRef::Node(root_id)],
            },
            Some(("ascent-slope", slope)),
        )?;

        if slope > 0 {
            apply_price_adjustment(
                graph,
                required_divergence,
                state,
                prices,
                deficits,
                &search.labeled,
                &search.scanned,
                search.label_order,
                metrics,
                recorder.as_deref_mut(),
            )?;
            return Ok(());
        }
        if let Some(target) = negative_target {
            apply_balanced_path_augmentation(
                graph,
                required_divergence,
                state,
                prices,
                deficits,
                root,
                target,
                &search.predecessor,
                search.label_order,
                metrics,
                recorder.as_deref_mut(),
            )?;
            return Ok(());
        }
    }
    Err(RelaxationError::LabelInvariant)
}

#[derive(Clone)]
struct LabelStep {
    target: NodeIndex,
    update_residual: ResidualArcId,
}

struct ArcScanTrace<'a> {
    search_order: &'a [NodeIndex],
    metrics: &'a mut RelaxationMetrics,
    enabled: bool,
    checkpoints: &'a mut Vec<ArcScanCheckpoint>,
}

fn extend_labels(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    node: NodeIndex,
    labeled: &mut [bool],
    predecessor: &mut [Option<(NodeIndex, ResidualArcId)>],
    trace: &mut ArcScanTrace<'_>,
) -> Result<Vec<LabelStep>, RelaxationError> {
    let mut result = Vec::new();
    let mut search_order = trace.search_order.to_vec();
    for edge_index in graph.edge_indices() {
        let edge = graph
            .edge(edge_index)
            .ok_or(RelaxationError::LabelInvariant)?;
        if edge.from() != node && edge.to() != node {
            continue;
        }
        record_label_arc_scan(trace.metrics)?;
        let mut active_path = Vec::new();
        let mut focus = FlowTraceEntityRef::ResidualArc(ResidualArcId::new(
            edge.id().clone(),
            ResidualDirection::Forward,
        ));
        if edge.from() != edge.to()
            && tension(graph, prices, edge_index)? == i128::from(edge.cost())
        {
            let flow = state
                .flows()
                .get(edge_index.as_usize())
                .copied()
                .ok_or(RelaxationError::LabelInvariant)?;
            let candidate = if edge.from() == node && flow > edge.lower() {
                Some((
                    edge.to(),
                    ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse),
                ))
            } else if edge.to() == node && flow < edge.capacity() {
                Some((
                    edge.from(),
                    ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward),
                ))
            } else {
                None
            };
            if let Some((target, update_residual)) = candidate
                && !labeled[target.as_usize()]
            {
                state
                    .arc(&update_residual)
                    .filter(|arc| arc.capacity > 0 && arc.from == target && arc.to == node)
                    .ok_or(RelaxationError::LabelInvariant)?;
                labeled[target.as_usize()] = true;
                predecessor[target.as_usize()] = Some((node, update_residual.clone()));
                search_order.push(target);
                active_path.push(update_residual.clone());
                focus = FlowTraceEntityRef::ResidualArc(update_residual.clone());
                result.push(LabelStep {
                    target,
                    update_residual,
                });
            }
        }
        push_arc_scan_checkpoint(
            trace.enabled,
            trace.metrics,
            &search_order,
            active_path,
            focus,
            trace.checkpoints,
        )?;
    }
    Ok(result)
}

fn ascent_slope(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    deficits: &[i128],
    prices: &[i128],
    scanned: &[bool],
    trace: &mut ArcScanTrace<'_>,
) -> Result<i128, RelaxationError> {
    let mut slope = 0_i128;
    for node in graph.node_indices() {
        if scanned[node.as_usize()] {
            slope = slope
                .checked_add(deficits[node.as_usize()])
                .ok_or(RelaxationError::ArithmeticOverflow)?;
        }
    }
    let mut last_focus = None;
    for edge_index in graph.edge_indices() {
        record_cut_arc_scan(trace.metrics)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(RelaxationError::LabelInvariant)?;
        let from_scanned = scanned[edge.from().as_usize()];
        let to_scanned = scanned[edge.to().as_usize()];
        let focus = cut_scan_focus(edge, from_scanned, to_scanned);
        last_focus = Some(focus.clone());
        push_arc_scan_checkpoint(
            trace.enabled,
            trace.metrics,
            trace.search_order,
            Vec::new(),
            focus,
            trace.checkpoints,
        )?;
        if from_scanned == to_scanned
            || tension(graph, prices, edge_index)? != i128::from(edge.cost())
        {
            continue;
        }
        let flow = state.flows()[edge_index.as_usize()];
        let transferable = if from_scanned {
            flow - edge.lower()
        } else {
            edge.capacity() - flow
        };
        slope = slope
            .checked_sub(i128::from(transferable))
            .ok_or(RelaxationError::ArithmeticOverflow)?;
    }
    finish_arc_scan_checkpoints(
        trace.enabled,
        trace.metrics,
        trace.search_order,
        Vec::new(),
        last_focus,
        trace.checkpoints,
    )?;
    Ok(slope)
}

#[allow(clippy::too_many_arguments)]
fn apply_balanced_path_augmentation(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &[i128],
    deficits: &mut Vec<i128>,
    root: NodeIndex,
    target: NodeIndex,
    predecessor: &[Option<(NodeIndex, ResidualArcId)>],
    search_order: Vec<NodeIndex>,
    metrics: &mut RelaxationMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), RelaxationError> {
    let path = reconstruct_update_path(state, predecessor, root, target)?;
    let bottleneck = path
        .iter()
        .map(|id| {
            state
                .arc(id)
                .filter(|arc| arc.capacity > 0)
                .map(|arc| arc.capacity)
                .ok_or(RelaxationError::LabelInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(RelaxationError::LabelInvariant)?;
    let root_deficit = deficits[root.as_usize()];
    let target_deficit = deficits[target.as_usize()]
        .checked_neg()
        .ok_or(RelaxationError::LabelInvariant)?;
    let delta = root_deficit.min(target_deficit).min(i128::from(bottleneck));
    if delta <= 0 {
        return Err(RelaxationError::LabelInvariant);
    }
    let delta = u64::try_from(delta).map_err(|_| RelaxationError::LabelInvariant)?;
    state.augment(&path, delta)?;
    *deficits = deficits_from_state(graph, required_divergence, state)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    metrics.augmented_flow_units = metrics
        .augmented_flow_units
        .checked_add(u128::from(delta))
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    validate_state(graph, required_divergence, state, prices, deficits)?;
    let focus = bottleneck_focus(state, &path);
    record_trace(
        recorder,
        graph,
        state,
        prices,
        deficits,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "relaxation.augment-balanced-path",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "relaxation:augment-balanced-deficit-path",
        },
        TraceView {
            search_order,
            active_path: path,
            focus,
        },
        Some(("delta", i128::from(delta))),
    )?;
    Ok(())
}

fn reconstruct_update_path(
    state: &ResidualState<'_>,
    predecessor: &[Option<(NodeIndex, ResidualArcId)>],
    root: NodeIndex,
    target: NodeIndex,
) -> Result<Vec<ResidualArcId>, RelaxationError> {
    if root == target {
        return Err(RelaxationError::LabelInvariant);
    }
    let mut path = Vec::new();
    let mut cursor = target;
    while cursor != root {
        let (parent, residual_id) = predecessor
            .get(cursor.as_usize())
            .and_then(Clone::clone)
            .ok_or(RelaxationError::LabelInvariant)?;
        let residual = state
            .arc(&residual_id)
            .filter(|arc| arc.capacity > 0 && arc.from == cursor && arc.to == parent)
            .ok_or(RelaxationError::LabelInvariant)?;
        let _ = residual;
        path.push(residual_id);
        cursor = parent;
        if path.len() > state.graph().nodes().len() {
            return Err(RelaxationError::LabelInvariant);
        }
    }
    Ok(path)
}

// The candidate flow, dual prices, and deficits are committed atomically only
// after their joint invariant check; keeping the transaction in one routine
// makes a partial update impossible.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn apply_price_adjustment(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &mut ResidualState<'_>,
    prices: &mut Vec<i128>,
    deficits: &mut Vec<i128>,
    labeled: &[bool],
    scanned: &[bool],
    search_order: Vec<NodeIndex>,
    metrics: &mut RelaxationMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), RelaxationError> {
    let mut price_checkpoints = Vec::new();
    let delta = price_delta(
        graph,
        prices,
        scanned,
        &search_order,
        metrics,
        recorder.is_some(),
        &mut price_checkpoints,
    )?;
    publish_scan_checkpoints(
        recorder.as_deref_mut(),
        graph,
        state,
        prices,
        deficits,
        price_checkpoints,
        "relaxation.scan-price-cut-arc",
        "relaxation:inspect-one-price-breakpoint-cut-arc",
    )?;
    let mut candidate_state = state.clone();
    let mut changed_residuals = Vec::new();
    let mut boundary_checkpoints = Vec::new();
    let mut last_boundary_focus = None;
    for edge_index in graph.edge_indices() {
        record_cut_arc_scan(metrics)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(RelaxationError::LabelInvariant)?;
        let from_scanned = scanned[edge.from().as_usize()];
        let to_scanned = scanned[edge.to().as_usize()];
        let focus = cut_scan_focus(edge, from_scanned, to_scanned);
        last_boundary_focus = Some(focus.clone());
        push_arc_scan_checkpoint(
            recorder.is_some(),
            metrics,
            &search_order,
            Vec::new(),
            focus,
            &mut boundary_checkpoints,
        )?;
        if from_scanned == to_scanned
            || tension(graph, prices, edge_index)? != i128::from(edge.cost())
        {
            continue;
        }
        let flow = candidate_state.flows()[edge_index.as_usize()];
        let (outside, amount, residual_id) = if from_scanned {
            (
                edge.to(),
                flow - edge.lower(),
                ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse),
            )
        } else {
            (
                edge.from(),
                edge.capacity() - flow,
                ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward),
            )
        };
        if amount == 0 {
            continue;
        }
        if !labeled[outside.as_usize()] {
            return Err(RelaxationError::LabelInvariant);
        }
        candidate_state.augment(std::slice::from_ref(&residual_id), amount)?;
        changed_residuals.push(residual_id);
    }
    finish_arc_scan_checkpoints(
        recorder.is_some(),
        metrics,
        &search_order,
        Vec::new(),
        last_boundary_focus,
        &mut boundary_checkpoints,
    )?;
    publish_scan_checkpoints(
        recorder.as_deref_mut(),
        graph,
        state,
        prices,
        deficits,
        boundary_checkpoints,
        "relaxation.scan-boundary-flow-arc",
        "relaxation:inspect-one-balanced-boundary-flow-arc",
    )?;
    let mut candidate_prices = prices.clone();
    for node in graph.node_indices() {
        if scanned[node.as_usize()] {
            candidate_prices[node.as_usize()] = candidate_prices[node.as_usize()]
                .checked_sub(delta)
                .ok_or(RelaxationError::ArithmeticOverflow)?;
        }
    }
    let candidate_deficits = deficits_from_state(graph, required_divergence, &candidate_state)?;
    validate_state(
        graph,
        required_divergence,
        &candidate_state,
        &candidate_prices,
        &candidate_deficits,
    )?;
    *state = candidate_state;
    *prices = candidate_prices;
    *deficits = candidate_deficits;
    metrics.price_adjustments = metrics
        .price_adjustments
        .checked_add(1)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    metrics.boundary_flow_updates = metrics
        .boundary_flow_updates
        .checked_add(
            u128::try_from(changed_residuals.len())
                .map_err(|_| RelaxationError::ArithmeticOverflow)?,
        )
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    let focus = changed_residuals
        .first()
        .cloned()
        .map(FlowTraceEntityRef::ResidualArc)
        .into_iter()
        .collect();
    record_trace(
        recorder,
        graph,
        state,
        prices,
        deficits,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: "relaxation.adjust-prices",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "relaxation:move-balanced-cut-flows-and-lower-scanned-prices",
        },
        TraceView {
            search_order,
            active_path: changed_residuals,
            focus,
        },
        Some(("delta", delta)),
    )?;
    Ok(())
}

fn price_delta(
    graph: &FlowNetwork,
    prices: &[i128],
    scanned: &[bool],
    search_order: &[NodeIndex],
    metrics: &mut RelaxationMetrics,
    record_trace: bool,
    checkpoints: &mut Vec<ArcScanCheckpoint>,
) -> Result<i128, RelaxationError> {
    let mut delta = None;
    let mut last_focus = None;
    for edge_index in graph.edge_indices() {
        record_cut_arc_scan(metrics)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(RelaxationError::LabelInvariant)?;
        let from_scanned = scanned[edge.from().as_usize()];
        let to_scanned = scanned[edge.to().as_usize()];
        let focus = cut_scan_focus(edge, from_scanned, to_scanned);
        last_focus = Some(focus.clone());
        push_arc_scan_checkpoint(
            record_trace,
            metrics,
            search_order,
            Vec::new(),
            focus,
            checkpoints,
        )?;
        if from_scanned == to_scanned {
            continue;
        }
        let tension = tension(graph, prices, edge_index)?;
        let cost = i128::from(edge.cost());
        let candidate = if from_scanned && tension > cost {
            Some(
                tension
                    .checked_sub(cost)
                    .ok_or(RelaxationError::ArithmeticOverflow)?,
            )
        } else if !from_scanned && to_scanned && tension < cost {
            Some(
                cost.checked_sub(tension)
                    .ok_or(RelaxationError::ArithmeticOverflow)?,
            )
        } else {
            None
        };
        if let Some(candidate) = candidate {
            delta = Some(delta.map_or(candidate, |current: i128| current.min(candidate)));
        }
    }
    finish_arc_scan_checkpoints(
        record_trace,
        metrics,
        search_order,
        Vec::new(),
        last_focus,
        checkpoints,
    )?;
    delta
        .filter(|value| *value > 0)
        .ok_or(RelaxationError::LabelInvariant)
}

struct ArcScanCheckpoint {
    metrics: RelaxationMetrics,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    focus: FlowTraceEntityRef,
}

fn push_arc_scan_checkpoint(
    record_trace: bool,
    metrics: &RelaxationMetrics,
    search_order: &[NodeIndex],
    active_path: Vec<ResidualArcId>,
    focus: FlowTraceEntityRef,
    checkpoints: &mut Vec<ArcScanCheckpoint>,
) -> Result<(), RelaxationError> {
    if record_trace && should_publish_arc_scan(total_arc_scans(*metrics)?) {
        checkpoints.push(ArcScanCheckpoint {
            metrics: *metrics,
            search_order: search_order.to_vec(),
            active_path,
            focus,
        });
    }
    Ok(())
}

fn finish_arc_scan_checkpoints(
    record_trace: bool,
    metrics: &RelaxationMetrics,
    search_order: &[NodeIndex],
    active_path: Vec<ResidualArcId>,
    last_focus: Option<FlowTraceEntityRef>,
    checkpoints: &mut Vec<ArcScanCheckpoint>,
) -> Result<(), RelaxationError> {
    let Some(focus) = record_trace.then_some(last_focus).flatten() else {
        return Ok(());
    };
    let total = total_arc_scans(*metrics)?;
    let last_matches = checkpoints
        .last()
        .map(|checkpoint| total_arc_scans(checkpoint.metrics))
        .transpose()?
        == Some(total);
    if last_matches {
        let last = checkpoints
            .last_mut()
            .expect("checked final relaxation arc-scan checkpoint");
        last.metrics = *metrics;
        last.search_order = search_order.to_vec();
        last.active_path = active_path;
        last.focus = focus;
    } else {
        checkpoints.push(ArcScanCheckpoint {
            metrics: *metrics,
            search_order: search_order.to_vec(),
            active_path,
            focus,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn publish_scan_checkpoints(
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    deficits: &[i128],
    checkpoints: Vec<ArcScanCheckpoint>,
    catalog_id: &'static str,
    pseudocode_line: &'static str,
) -> Result<(), RelaxationError> {
    for checkpoint in checkpoints {
        let ordinal = i128::try_from(total_arc_scans(checkpoint.metrics)?)
            .map_err(|_| RelaxationError::ArithmeticOverflow)?;
        record_trace(
            recorder.as_deref_mut(),
            graph,
            state,
            prices,
            deficits,
            checkpoint.metrics,
            FlowTraceEventMetadata {
                catalog_id,
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line,
            },
            TraceView {
                search_order: checkpoint.search_order,
                active_path: checkpoint.active_path,
                focus: vec![checkpoint.focus],
            },
            Some(("scan-ordinal", ordinal)),
        )?;
    }
    Ok(())
}

const fn should_publish_arc_scan(scan: u128) -> bool {
    scan <= RELAXATION_TRACE_SCAN_PREFIX || scan.is_power_of_two()
}

fn total_arc_scans(metrics: RelaxationMetrics) -> Result<u128, RelaxationError> {
    metrics
        .checked_total_arc_scans()
        .ok_or(RelaxationError::ArithmeticOverflow)
}

fn bottleneck_focus(state: &ResidualState<'_>, path: &[ResidualArcId]) -> Vec<FlowTraceEntityRef> {
    path.iter()
        .filter(|arc| state.arc(arc).is_some_and(|state| state.capacity == 0))
        .min()
        .or_else(|| path.first())
        .cloned()
        .map(FlowTraceEntityRef::ResidualArc)
        .into_iter()
        .collect()
}

fn cut_scan_focus(
    edge: &crate::model::FlowEdge,
    from_scanned: bool,
    to_scanned: bool,
) -> FlowTraceEntityRef {
    let direction = if from_scanned && !to_scanned {
        ResidualDirection::Reverse
    } else {
        ResidualDirection::Forward
    };
    FlowTraceEntityRef::ResidualArc(ResidualArcId::new(edge.id().clone(), direction))
}

fn deficits_from_state(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
) -> Result<Vec<i128>, RelaxationError> {
    deficits(graph, required_divergence, state.flows())
}

fn deficits(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
) -> Result<Vec<i128>, RelaxationError> {
    if required_divergence.len() != graph.nodes().len() {
        return Err(RelaxationError::Feasibility(
            FeasibilityError::InvalidDivergence,
        ));
    }
    divergences(graph, flows)?
        .into_iter()
        .zip(required_divergence)
        .map(|(actual, &required)| {
            actual
                .checked_sub(required)
                .ok_or(RelaxationError::ArithmeticOverflow)
        })
        .collect()
}

fn validate_state(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    state: &ResidualState<'_>,
    prices: &[i128],
    recorded_deficits: &[i128],
) -> Result<(), RelaxationError> {
    if prices.len() != graph.nodes().len()
        || recorded_deficits != deficits_from_state(graph, required_divergence, state)?
    {
        return Err(RelaxationError::LabelInvariant);
    }
    let deficit_sum = recorded_deficits.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(RelaxationError::ArithmeticOverflow)
    })?;
    if deficit_sum != 0 {
        return Err(RelaxationError::LabelInvariant);
    }
    for edge_index in graph.edge_indices() {
        let edge = graph
            .edge(edge_index)
            .ok_or(RelaxationError::ComplementarySlackness)?;
        let flow = state.flows()[edge_index.as_usize()];
        let tension = tension(graph, prices, edge_index)?;
        let cost = i128::from(edge.cost());
        if (tension < cost && flow != edge.lower()) || (tension > cost && flow != edge.capacity()) {
            return Err(RelaxationError::ComplementarySlackness);
        }
    }
    Ok(())
}

fn tension(
    graph: &FlowNetwork,
    prices: &[i128],
    edge_index: crate::model::EdgeIndex,
) -> Result<i128, RelaxationError> {
    let edge = graph
        .edge(edge_index)
        .ok_or(RelaxationError::ComplementarySlackness)?;
    prices
        .get(edge.from().as_usize())
        .copied()
        .and_then(|from| {
            prices
                .get(edge.to().as_usize())
                .and_then(|to| from.checked_sub(*to))
        })
        .ok_or(RelaxationError::ArithmeticOverflow)
}

fn first_positive_deficit(graph: &FlowNetwork, deficits: &[i128]) -> Option<NodeIndex> {
    graph
        .node_indices()
        .find(|node| deficits[node.as_usize()] > 0)
}

fn total_positive_deficit(deficits: &[i128]) -> Result<i128, RelaxationError> {
    deficits.iter().try_fold(0_i128, |sum, &value| {
        if value > 0 {
            sum.checked_add(value)
                .ok_or(RelaxationError::ArithmeticOverflow)
        } else {
            Ok(sum)
        }
    })
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), RelaxationError> {
    if graph.nodes().len() > RELAXATION_MAX_NODES || graph.edges().len() > RELAXATION_MAX_EDGES {
        return Err(RelaxationError::AdmissionLimit);
    }
    Ok(())
}

fn record_label_arc_scan(metrics: &mut RelaxationMetrics) -> Result<(), RelaxationError> {
    metrics.label_arc_scans = metrics
        .label_arc_scans
        .checked_add(1)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    validate_scan_limit(*metrics)
}

fn record_cut_arc_scan(metrics: &mut RelaxationMetrics) -> Result<(), RelaxationError> {
    metrics.cut_arc_scans = metrics
        .cut_arc_scans
        .checked_add(1)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    validate_scan_limit(*metrics)
}

fn validate_scan_limit(metrics: RelaxationMetrics) -> Result<(), RelaxationError> {
    if metrics
        .checked_total_arc_scans()
        .ok_or(RelaxationError::ArithmeticOverflow)?
        > RELAXATION_MAX_ARC_SCANS
    {
        return Err(RelaxationError::WorkLimit);
    }
    Ok(())
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    deficits: &[i128],
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
        deficits.to_vec(),
        FlowTraceMetrics::default(),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct TraceView {
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    focus: Vec<FlowTraceEntityRef>,
}

impl TraceView {
    fn empty() -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
            focus: Vec::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    prices: &[i128],
    deficits: &[i128],
    metrics: RelaxationMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), RelaxationError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let next_event_count = recorder
        .event_count()
        .checked_add(1)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    let entities_per_scene = graph
        .nodes()
        .len()
        .checked_mul(2)
        .and_then(|nodes| {
            graph
                .edges()
                .len()
                .checked_mul(4)
                .and_then(|edges| nodes.checked_add(edges))
        })
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    let projection_units = next_event_count
        .checked_mul(entities_per_scene)
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    if next_event_count > RELAXATION_MAX_TRACE_EVENTS
        || projection_units > RELAXATION_MAX_TRACE_PROJECTION_UNITS
    {
        return Err(RelaxationError::Trace(FlowTraceError::EventLimit));
    }
    let projected_metrics = metrics
        .projected_trace_metrics()
        .ok_or(RelaxationError::ArithmeticOverflow)?;
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        prices.iter().copied().map(Some).collect(),
        view.search_order,
        view.active_path,
        deficits.to_vec(),
        projected_metrics,
    );
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, view.focus)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::certificate::supply_divergences;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTracePatch, apply_trace_event};

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
        .expect("valid graph")
    }

    #[test]
    fn lowers_a_price_then_augments_a_positive_cost_supply_arc() {
        let graph = network(&[("s", 2), ("t", -2)], &[("st", "s", "t", 0, 3, 5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [2]);
        assert_eq!(result.certificate.total_cost, 10);
        assert_eq!(result.prices, [0, -5]);
        assert_eq!(result.metrics.iterations, 2);
        assert_eq!(result.metrics.price_adjustments, 1);
        assert_eq!(result.metrics.augmentations, 1);
        assert_eq!(result.metrics.augmented_flow_units, 2);
    }

    #[test]
    fn uses_an_incoming_balanced_arc_to_fix_a_negative_cycle() {
        let graph = network(
            &[("x", 0), ("y", 0)],
            &[("xy", "x", "y", 0, 3, -4), ("yx", "y", "x", 0, 3, 1)],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [3, 3]);
        assert_eq!(result.certificate.total_cost, -9);
        assert_eq!(result.prices, [-1, 0]);
        assert_eq!(result.metrics.price_adjustments, 1);
        assert_eq!(result.metrics.augmentations, 1);
    }

    #[test]
    fn caps_each_augmentation_after_comparing_full_width_parallel_deficits() {
        let graph = network(
            &[("a", 0), ("b", 0)],
            &[
                ("first", "a", "b", 0, u64::MAX, -1),
                ("second", "a", "b", 0, u64::MAX, -1),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [0, 0]);
        assert_eq!(result.certificate.total_cost, 0);
        assert_eq!(result.prices, [-1, 0]);
        assert_eq!(result.metrics.augmentations, 2);
        assert_eq!(
            result.metrics.augmented_flow_units,
            u128::from(u64::MAX) * 2
        );
    }

    #[test]
    fn eager_trace_budget_rejects_a_long_price_ladder_without_limiting_fast_solve() {
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("node id"), 160),
                FlowNode::new(NodeId::parse("t").expect("node id"), -160),
            ],
            (1_i64..=160)
                .map(|cost| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("edge-{cost:03}")).expect("edge id"),
                    from: NodeId::parse("s").expect("node id"),
                    to: NodeId::parse("t").expect("node id"),
                    lower: 0,
                    capacity: 1,
                    cost,
                })
                .collect(),
        )
        .expect("valid graph");
        let target = supply_divergences(&graph).expect("target");

        let fast = solve_relaxation(&graph, &target).expect("fast minimum cost");
        assert_eq!(fast.flows, vec![1; 160]);
        assert_eq!(fast.certificate.total_cost, 12_880);
        let trace = trace_relaxation(&graph, &target);
        assert!(
            matches!(
                trace,
                Err(RelaxationError::Trace(FlowTraceError::EventLimit))
            ),
            "unexpected trace result: {trace:?}"
        );
    }

    #[test]
    fn moves_a_labeled_cut_arc_to_its_bound_before_lowering_prices() {
        let graph = network(
            &[("m", 0), ("s", 0), ("t", 0)],
            &[
                ("negative", "s", "t", 0, 3, -5),
                ("zero", "m", "s", 0, 1, 0),
            ],
        );
        let target = supply_divergences(&graph).expect("target");
        let traced = trace_relaxation(&graph, &target).expect("minimum cost trace");

        assert_eq!(traced.result.flows, [0, 0]);
        assert_eq!(traced.result.certificate.total_cost, 0);
        assert_eq!(traced.result.metrics.boundary_flow_updates, 1);
        let price = traced
            .events
            .iter()
            .find(|event| {
                event.catalog_id == "relaxation.adjust-prices"
                    && event.patches.iter().any(|patch| {
                        matches!(
                            patch,
                            FlowTracePatch::EdgeFlow {
                                edge,
                                before: 0,
                                after: 1,
                            } if edge.as_str() == "zero"
                        )
                    })
            })
            .expect("flow-changing price adjustment");
        assert_eq!(
            price
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 5))
        );
    }

    #[test]
    fn keeps_negative_self_loops_at_the_upper_bound() {
        let graph = network(&[("x", 0)], &[("loop", "x", "x", 0, 2, -5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [2]);
        assert_eq!(result.certificate.total_cost, -10);
        assert_eq!(result.metrics.iterations, 0);
    }

    #[test]
    fn preserves_lower_bounds_and_transshipment_balances() {
        let graph = network(
            &[("a", 0), ("s", 2), ("t", -2)],
            &[
                ("at", "a", "t", 0, 2, 1),
                ("direct", "s", "t", 1, 2, 5),
                ("sa", "s", "a", 0, 2, 1),
            ],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [1, 1, 1]);
        assert_eq!(result.certificate.total_cost, 7);
        check_min_cost_flow(&graph, &target, &result.flows).expect("certificate");
    }

    #[test]
    fn resolves_parallel_arcs_by_stable_edge_identity() {
        let graph = network(
            &[("s", 2), ("t", -2)],
            &[("cheap", "s", "t", 0, 1, 1), ("costly", "s", "t", 0, 2, 5)],
        );
        let target = supply_divergences(&graph).expect("target");

        let result = solve_relaxation(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, [1, 1]);
        assert_eq!(result.certificate.total_cost, 6);
    }

    #[test]
    fn trace_replays_price_and_flow_changes_in_both_directions() {
        let graph = network(&[("s", 2), ("t", -2)], &[("st", "s", "t", 0, 3, 5)]);
        let target = supply_divergences(&graph).expect("target");
        let fast = solve_relaxation(&graph, &target).expect("fast result");
        let traced = trace_relaxation(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        let price = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "relaxation.adjust-prices")
            .expect("price event");
        assert_eq!(
            price
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 5))
        );
        assert!(price.patches.iter().any(|patch| matches!(
            patch,
            FlowTracePatch::NodeLabel {
                before: Some(0),
                after: Some(-5),
                ..
            }
        )));
        let augment = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "relaxation.augment-balanced-path")
            .expect("augment event");
        assert_eq!(
            augment
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 2))
        );
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if event.event_id == augment.event_id {
                assert_eq!(
                    replay.active_path,
                    [ResidualArcId::new(
                        EdgeId::parse("st").expect("edge id"),
                        ResidualDirection::Forward,
                    )]
                );
            }
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        assert_eq!(
            replay.node_labels,
            fast.prices.iter().copied().map(Some).collect::<Vec<_>>()
        );
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn agrees_with_cycle_canceling_on_small_deterministic_circulations() {
        let mut seed = 0xd1b5_4a32_d192_ed03_u64;
        for case in 0..48 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small count");
            let nodes = (0..node_count)
                .map(|index| FlowNode::new(NodeId::parse(&format!("v{index}")).expect("node"), 0))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if from == to || next(&mut seed).is_multiple_of(3) {
                        continue;
                    }
                    edges.push(UnresolvedFlowEdge {
                        id: EdgeId::parse(&format!("e{from}_{to}")).expect("edge"),
                        from: NodeId::parse(&format!("v{from}")).expect("tail"),
                        to: NodeId::parse(&format!("v{to}")).expect("head"),
                        lower: 0,
                        capacity: 1 + next(&mut seed) % 4,
                        cost: i64::try_from(next(&mut seed) % 11).expect("cost") - 5,
                    });
                }
            }
            if edges.is_empty() {
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse("fallback").expect("edge"),
                    from: NodeId::parse("v0").expect("tail"),
                    to: NodeId::parse("v1").expect("head"),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                });
            }
            let graph = FlowNetwork::new(nodes, edges).expect("valid graph");
            let target = vec![0_i128; node_count];
            let expected = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("reference case {case}: {error}"));
            let actual = solve_relaxation(&graph, &target)
                .unwrap_or_else(|error| panic!("relaxation case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
        }
    }

    #[test]
    fn agrees_with_cycle_canceling_on_planted_lower_bound_transshipments() {
        let mut seed = 0x8f46_2a91_b037_c5dd_u64;
        for case in 0..32 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small count");
            let nodes = (0..node_count)
                .map(|index| FlowNode::new(NodeId::parse(&format!("v{index}")).expect("node"), 0))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            let mut planted = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if from == to || next(&mut seed).is_multiple_of(2) {
                        continue;
                    }
                    let lower = next(&mut seed) % 3;
                    let capacity = lower + next(&mut seed) % 4;
                    let flow = lower + next(&mut seed) % (capacity - lower + 1);
                    edges.push(UnresolvedFlowEdge {
                        id: EdgeId::parse(&format!("e{from}_{to}")).expect("edge"),
                        from: NodeId::parse(&format!("v{from}")).expect("tail"),
                        to: NodeId::parse(&format!("v{to}")).expect("head"),
                        lower,
                        capacity,
                        cost: i64::try_from(next(&mut seed) % 11).expect("cost") - 5,
                    });
                    planted.push((from, to, flow));
                }
            }
            let lower = next(&mut seed) % 2;
            let capacity = lower + 1 + next(&mut seed) % 3;
            let flow = lower + next(&mut seed) % (capacity - lower + 1);
            edges.push(UnresolvedFlowEdge {
                id: EdgeId::parse("parallel").expect("edge"),
                from: NodeId::parse("v0").expect("tail"),
                to: NodeId::parse("v1").expect("head"),
                lower,
                capacity,
                cost: i64::try_from(next(&mut seed) % 11).expect("cost") - 5,
            });
            planted.push((0, 1, flow));
            let graph = FlowNetwork::new(nodes, edges).expect("valid graph");
            let mut target = vec![0_i128; node_count];
            for (from, to, flow) in planted {
                target[from] += i128::from(flow);
                target[to] -= i128::from(flow);
            }

            let expected = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("reference case {case}: {error}"));
            let actual = solve_relaxation(&graph, &target)
                .unwrap_or_else(|error| panic!("relaxation case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
            check_min_cost_flow(&graph, &target, &actual.flows)
                .unwrap_or_else(|error| panic!("certificate case {case}: {error}"));
        }
    }

    #[test]
    fn rejects_infeasible_balances_before_relaxation_work() {
        let graph = network(&[("s", 1), ("t", -1)], &[]);
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_relaxation(&graph, &target),
            Err(RelaxationError::Feasibility(FeasibilityError::Infeasible(
                _
            )))
        ));
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed
    }
}
