//! Davies--Vassilvitskii--Wang warm-start Push--Relabel.
//!
//! A user-supplied bounded pseudoflow is first made cut-saturating.  The two
//! auxiliary gap-relabel runs then move all excess to the source side and all
//! deficit to the sink side before conservation is restored inside the
//! certified minimum cut.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use super::push_relabel::{
    PushRelabelError, PushRelabelMetrics, solve_gap_relabel_push_relabel,
    trace_gap_relabel_push_relabel,
};
use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::model::{
    EdgeId, FlowModelError, FlowNetwork, FlowNode, NodeId, NodeIndex, UnresolvedFlowEdge,
};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot, apply_trace_event,
    residual_arc_entity_refs,
};

/// Conservative interactive node limit for the explicit auxiliary networks.
pub const WARM_START_PUSH_RELABEL_MAX_NODES: usize = 256;
/// Conservative interactive original-edge limit.
pub const WARM_START_PUSH_RELABEL_MAX_EDGES: usize = 2_048;
/// Maximum recovery path searches and pushes after the cut is certified.
pub const WARM_START_PUSH_RELABEL_MAX_RECOVERY_TRANSITIONS: u64 = 100_000;
/// Maximum positive residual arcs inspected by recovery BFS runs.
pub const WARM_START_PUSH_RELABEL_MAX_RECOVERY_ARC_SCANS: u128 = 10_000_000;
/// Preserve every early auxiliary scan before geometric sampling.
const WARM_START_PUSH_RELABEL_TRACE_SCAN_PREFIX: u128 = 512;

/// Exact prediction-sensitive and auxiliary Push--Relabel counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WarmStartPushRelabelMetrics {
    /// Original edges carrying positive predicted flow.
    pub predicted_positive_edges: u64,
    /// Sum of nonterminal predicted excess and deficit.
    pub imbalance_error: u128,
    /// Additional residual maximum-flow value needed to saturate a cut.
    pub cut_saturation_error: u128,
    /// Exact prediction error `max(imbalance_error, cut_saturation_error)`.
    pub eta: u128,
    /// Gap-relabel auxiliary maximum-flow problems solved.
    pub auxiliary_solves: u64,
    /// Nodes transferred across the maintained cut by Algorithms 3 and 6.
    pub cut_transfers: u64,
    /// Conservation-recovery residual paths.
    pub recovery_paths: u64,
    /// Positive residual arcs inspected by all auxiliary and recovery work.
    pub residual_arc_scans: u128,
    /// Valid auxiliary height increases.
    pub relabels: u64,
    /// Auxiliary local pushes.
    pub pushes: u64,
    /// Auxiliary saturating pushes.
    pub saturating_pushes: u64,
    /// Auxiliary nonsaturating pushes.
    pub nonsaturating_pushes: u64,
    /// Auxiliary completed discharges.
    pub discharges: u64,
    /// Auxiliary active-vertex selections.
    pub active_vertex_selections: u64,
    /// Auxiliary nonempty gap relabel batches.
    pub gap_relabels: u64,
}

/// Certified warm-start maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WarmStartPushRelabelResult {
    /// User prediction in canonical original-edge order.
    pub predicted_flows: Vec<u64>,
    /// Recovered feasible maximum flow.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact deterministic work counters.
    pub metrics: WarmStartPushRelabelMetrics,
}

/// Warm-start result plus complete reversible original-graph trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WarmStartPushRelabelTraceResult {
    /// Same result returned by the fast profile.
    pub result: WarmStartPushRelabelResult,
    /// Boundary containing the prediction before algorithm annotations.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible events.
    pub events: Vec<FlowTraceEvent>,
    /// Boundary after independent certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Warm-start construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WarmStartPushRelabelError {
    /// The graph exceeds the explicit auxiliary-network admission band.
    #[error("graph exceeds warm-start push-relabel admission limits")]
    AdmissionLimit,
    /// The prediction does not contain exactly one bounded value per edge.
    #[error("warm-start prediction shape or bounds are invalid")]
    InvalidPrediction,
    /// Warm-start max flow requires zero lower bounds and supplies.
    #[error("warm-start push-relabel requires zero lower bounds and supplies")]
    UnsupportedGraph,
    /// A deterministic recovery ceiling was reached.
    #[error("warm-start push-relabel work limit reached")]
    WorkLimit,
    /// Exact aggregate arithmetic exceeded its declared domain.
    #[error("warm-start push-relabel arithmetic overflow")]
    ArithmeticOverflow,
    /// A cut, imbalance, auxiliary solution, or recovery invariant failed.
    #[error("warm-start push-relabel invariant failed")]
    Invariant,
    /// Building a bounded auxiliary network failed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// An auxiliary gap-relabel solve failed.
    #[error(transparent)]
    PushRelabel(#[from] PushRelabelError),
    /// An original residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent feasibility or max-flow certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction or replay failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Runs the prediction-seeded Algorithm 3/6 warm-start pipeline.
///
/// # Errors
///
/// Rejects out-of-band or non-max-flow graphs, malformed predictions,
/// auxiliary work failures, invariant violations, and rejected certificates.
pub fn solve_warm_start_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    predicted_flows: &[u64],
) -> Result<WarmStartPushRelabelResult, WarmStartPushRelabelError> {
    solve_internal(graph, source, sink, predicted_flows, false).map(|run| run.result)
}

/// Traces cut saturation, both side-separation passes, and conservation repair.
///
/// # Errors
///
/// Returns the same failures as [`solve_warm_start_push_relabel`], plus trace
/// construction or independent replay-check failures.
pub fn trace_warm_start_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    predicted_flows: &[u64],
) -> Result<WarmStartPushRelabelTraceResult, WarmStartPushRelabelError> {
    let run = solve_internal(graph, source, sink, predicted_flows, true)?;
    let trace = run.trace.ok_or(WarmStartPushRelabelError::Invariant)?;
    let result = WarmStartPushRelabelTraceResult {
        result: run.result,
        base_snapshot: trace.0,
        events: trace.1,
        final_snapshot: trace.2,
    };
    check_warm_start_push_relabel_trace(graph, source, sink, &result)?;
    Ok(result)
}

/// Independently replays and checks a warm-start trace and its phase invariants.
///
/// # Errors
///
/// Rejects noncanonical event grammar, failed bidirectional replay, a
/// nonsaturated maintained cut, misplaced excess/deficit, or a bad optimum.
pub fn check_warm_start_push_relabel_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &WarmStartPushRelabelTraceResult,
) -> Result<(), WarmStartPushRelabelError> {
    if trace.base_snapshot.flows != trace.result.predicted_flows
        || trace.final_snapshot.flows != trace.result.flows
        || trace.events.len() < 5
        || trace.events.first().map(|event| event.catalog_id.as_str())
            != Some("warm-start-push-relabel.initialize-prediction")
        || trace.events.last().map(|event| event.catalog_id.as_str())
            != Some("warm-start-push-relabel.optimal")
    {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    let mut snapshot = trace.base_snapshot.clone();
    let mut saw_cut = false;
    let mut saw_excess_move = false;
    let mut saw_deficit_move = false;
    for event in &trace.events {
        apply_trace_event(graph, &mut snapshot, event, FlowTraceDirection::Forward)?;
        match event.catalog_id.as_str() {
            "warm-start-push-relabel.initialize-prediction"
            | "warm-start-push-relabel.inspect-cut-saturation-arc"
            | "warm-start-push-relabel.inspect-s-deficit-arc"
            | "warm-start-push-relabel.inspect-t-excess-arc" => {}
            "warm-start-push-relabel.saturate-cut" => {
                saw_cut = true;
                check_snapshot_cut(graph, source, sink, &snapshot)?;
            }
            "warm-start-push-relabel.move-t-excess" => {
                if !saw_cut {
                    return Err(WarmStartPushRelabelError::Invariant);
                }
                saw_excess_move = true;
                check_snapshot_cut(graph, source, sink, &snapshot)?;
                check_no_excess_on_sink_side(graph, source, sink, &snapshot)?;
            }
            "warm-start-push-relabel.move-s-deficit" => {
                if !saw_excess_move {
                    return Err(WarmStartPushRelabelError::Invariant);
                }
                saw_deficit_move = true;
                check_snapshot_cut(graph, source, sink, &snapshot)?;
                check_separated_imbalance(graph, source, sink, &snapshot)?;
            }
            "warm-start-push-relabel.recover-excess"
            | "warm-start-push-relabel.recover-deficit" => {
                if !saw_deficit_move {
                    return Err(WarmStartPushRelabelError::Invariant);
                }
                check_snapshot_cut(graph, source, sink, &snapshot)?;
            }
            "warm-start-push-relabel.optimal" => {
                if !saw_deficit_move {
                    return Err(WarmStartPushRelabelError::Invariant);
                }
            }
            _ => return Err(WarmStartPushRelabelError::Invariant),
        }
    }
    if snapshot != trace.final_snapshot {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    for event in trace.events.iter().rev() {
        apply_trace_event(graph, &mut snapshot, event, FlowTraceDirection::Reverse)?;
    }
    if snapshot != trace.base_snapshot {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    let certificate = check_max_flow(graph, source, sink, &trace.result.flows)?;
    if certificate != trace.result.certificate {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(())
}

struct InternalRun {
    result: WarmStartPushRelabelResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    predicted_flows: &[u64],
    record_trace: bool,
) -> Result<InternalRun, WarmStartPushRelabelError> {
    validate_input(graph, source, sink, predicted_flows)?;
    let predicted_state = ResidualState::from_flows(graph, predicted_flows)?;
    let mut state = ResidualState::from_flows(graph, predicted_flows)?;
    let initial_divergence = divergences(graph, predicted_flows)?;
    let imbalance_error = imbalance_total(&initial_divergence, source, sink)?;
    let mut metrics = WarmStartPushRelabelMetrics {
        predicted_positive_edges: u64::try_from(
            predicted_flows.iter().filter(|flow| **flow > 0).count(),
        )
        .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)?,
        imbalance_error,
        ..WarmStartPushRelabelMetrics::default()
    };
    let base = FlowTraceSnapshot::capture(
        graph,
        &predicted_state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        vec![0; graph.nodes().len()],
        FlowTraceMetrics::default(),
    );
    let mut recorder = if record_trace {
        Some(FlowTraceRecorder::new(graph, base)?)
    } else {
        None
    };
    record_state(
        graph,
        &state,
        sink,
        &BTreeSet::new(),
        &metrics,
        recorder.as_mut(),
        "warm-start-push-relabel.initialize-prediction",
        TraceGranularityV1::Phase,
        "warm-start:read-bounded-predicted-pseudoflow",
        Vec::new(),
        Some(("imbalance", to_i128(imbalance_error)?)),
    )?;

    let all_nodes = graph.node_indices().collect::<BTreeSet<_>>();
    let cut_solution = solve_auxiliary(
        graph,
        &state,
        &all_nodes,
        AuxiliaryTerminals::Original { source, sink },
        &[],
        &[],
        record_trace,
    )?;
    metrics.auxiliary_solves = 1;
    metrics.cut_saturation_error =
        u128::try_from(cut_solution.value).map_err(|_| WarmStartPushRelabelError::Invariant)?;
    metrics.eta = metrics.imbalance_error.max(metrics.cut_saturation_error);
    let cut_scan_base = metrics;
    absorb_auxiliary_metrics(&mut metrics, cut_solution.metrics)?;
    let cut_active = apply_auxiliary_flow(&mut state, &cut_solution.original_flow)?;
    let mut source_side = canonical_source_side(graph, &state, source, sink)?;
    check_cut_saturated(graph, &state, source, sink, &source_side)?;
    publish_auxiliary_scan_checkpoints(
        graph,
        &state,
        sink,
        &source_side,
        &mut metrics,
        recorder.as_mut(),
        cut_scan_base,
        &cut_solution.scan_checkpoints,
        "warm-start-push-relabel.inspect-cut-saturation-arc",
        "warm-start:inspect-cut-saturation-auxiliary-arc",
    )?;
    record_state(
        graph,
        &state,
        sink,
        &source_side,
        &metrics,
        recorder.as_mut(),
        "warm-start-push-relabel.saturate-cut",
        TraceGranularityV1::Phase,
        "warm-start:algorithm-5-gap-relabel-residual-cut",
        cut_active,
        Some(("eta", to_i128(metrics.eta)?)),
    )?;

    let big = u64::try_from(metrics.eta)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    let before_excess_cut = source_side.clone();
    let sink_side = graph
        .node_indices()
        .filter(|node| !source_side.contains(node))
        .collect::<BTreeSet<_>>();
    let divergence = divergences(graph, state.flows())?;
    let t_source_links = excess_links(&divergence, &sink_side, source, sink)?;
    let mut t_sink_links = deficit_links(&divergence, &sink_side, source, sink)?;
    t_sink_links.push((sink, big));
    let excess_solution = solve_auxiliary(
        graph,
        &state,
        &sink_side,
        AuxiliaryTerminals::Synthetic,
        &t_source_links,
        &t_sink_links,
        record_trace,
    )?;
    metrics.auxiliary_solves = metrics
        .auxiliary_solves
        .checked_add(1)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    let excess_scan_base = metrics;
    absorb_auxiliary_metrics(&mut metrics, excess_solution.metrics)?;
    let excess_active = apply_auxiliary_flow(&mut state, &excess_solution.original_flow)?;
    source_side.extend(excess_solution.original_source_side.iter().copied());
    metrics.cut_transfers = metrics
        .cut_transfers
        .checked_add(count_set_difference(&source_side, &before_excess_cut)?)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    check_cut_saturated(graph, &state, source, sink, &source_side)?;
    check_no_excess_in_sink_set(graph, &state, source, sink, &source_side)?;
    publish_auxiliary_scan_checkpoints(
        graph,
        &state,
        sink,
        &source_side,
        &mut metrics,
        recorder.as_mut(),
        excess_scan_base,
        &excess_solution.scan_checkpoints,
        "warm-start-push-relabel.inspect-t-excess-arc",
        "warm-start:inspect-t-excess-auxiliary-arc",
    )?;
    record_state(
        graph,
        &state,
        sink,
        &source_side,
        &metrics,
        recorder.as_mut(),
        "warm-start-push-relabel.move-t-excess",
        TraceGranularityV1::Phase,
        "warm-start:algorithm-3-move-excess-to-source-side",
        excess_active,
        Some(("moved", i128::from(metrics.cut_transfers))),
    )?;

    let before_deficit_cut = source_side.clone();
    let divergence = divergences(graph, state.flows())?;
    let mut s_source_links = excess_links(&divergence, &source_side, source, sink)?;
    s_source_links.push((source, big));
    let s_sink_links = deficit_links(&divergence, &source_side, source, sink)?;
    let deficit_solution = solve_auxiliary(
        graph,
        &state,
        &source_side,
        AuxiliaryTerminals::Synthetic,
        &s_source_links,
        &s_sink_links,
        record_trace,
    )?;
    metrics.auxiliary_solves = metrics
        .auxiliary_solves
        .checked_add(1)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    let deficit_scan_base = metrics;
    absorb_auxiliary_metrics(&mut metrics, deficit_solution.metrics)?;
    let deficit_active = apply_auxiliary_flow(&mut state, &deficit_solution.original_flow)?;
    source_side.retain(|node| deficit_solution.original_source_side.contains(node));
    metrics.cut_transfers = metrics
        .cut_transfers
        .checked_add(count_set_difference(&before_deficit_cut, &source_side)?)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    check_cut_saturated(graph, &state, source, sink, &source_side)?;
    check_separated_sets(graph, &state, source, sink, &source_side)?;
    publish_auxiliary_scan_checkpoints(
        graph,
        &state,
        sink,
        &source_side,
        &mut metrics,
        recorder.as_mut(),
        deficit_scan_base,
        &deficit_solution.scan_checkpoints,
        "warm-start-push-relabel.inspect-s-deficit-arc",
        "warm-start:inspect-s-deficit-auxiliary-arc",
    )?;
    record_state(
        graph,
        &state,
        sink,
        &source_side,
        &metrics,
        recorder.as_mut(),
        "warm-start-push-relabel.move-s-deficit",
        TraceGranularityV1::Phase,
        "warm-start:algorithm-6-move-deficit-to-sink-side",
        deficit_active,
        Some(("moved", i128::from(metrics.cut_transfers))),
    )?;

    recover_conservation(
        graph,
        source,
        sink,
        &source_side,
        &mut state,
        &mut metrics,
        recorder.as_mut(),
    )?;
    let certificate = check_max_flow(graph, source, sink, state.flows())?;
    record_state(
        graph,
        &state,
        sink,
        &source_side,
        &metrics,
        recorder.as_mut(),
        "warm-start-push-relabel.optimal",
        TraceGranularityV1::Phase,
        "warm-start:certify-feasible-flow-and-maintained-min-cut",
        Vec::new(),
        Some(("value", certificate.value)),
    )?;
    let result = WarmStartPushRelabelResult {
        predicted_flows: predicted_flows.to_vec(),
        flows: state.flows().to_vec(),
        certificate,
        metrics,
    };
    Ok(InternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[derive(Clone, Copy)]
enum AuxiliaryTerminals {
    Original { source: NodeIndex, sink: NodeIndex },
    Synthetic,
}

struct AuxiliarySolution {
    value: i128,
    original_flow: Vec<(ResidualArcId, u64)>,
    original_source_side: BTreeSet<NodeIndex>,
    metrics: PushRelabelMetrics,
    scan_checkpoints: Vec<AuxiliaryScanCheckpoint>,
}

#[derive(Clone)]
enum AuxiliaryFocus {
    ResidualArc(ResidualArcId),
    Node(NodeIndex),
}

#[derive(Clone)]
struct AuxiliaryScanCheckpoint {
    metrics: FlowTraceMetrics,
    focus: AuxiliaryFocus,
}

#[allow(clippy::too_many_lines)]
fn solve_auxiliary(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    included: &BTreeSet<NodeIndex>,
    terminals: AuxiliaryTerminals,
    source_links: &[(NodeIndex, u64)],
    sink_links: &[(NodeIndex, u64)],
    record_trace: bool,
) -> Result<AuxiliarySolution, WarmStartPushRelabelError> {
    let mut nodes = included
        .iter()
        .map(|node| {
            graph
                .node(*node)
                .map(|item| FlowNode::new(item.id().clone(), 0))
                .ok_or(WarmStartPushRelabelError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let occupied = nodes
        .iter()
        .map(|node| node.id().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let (aux_source_id, aux_sink_id) = match terminals {
        AuxiliaryTerminals::Original { source, sink } => (
            graph
                .node(source)
                .ok_or(WarmStartPushRelabelError::Invariant)?
                .id()
                .clone(),
            graph
                .node(sink)
                .ok_or(WarmStartPushRelabelError::Invariant)?
                .id()
                .clone(),
        ),
        AuxiliaryTerminals::Synthetic => {
            let source_id = unique_node_id("warm auxiliary source", &occupied)?;
            let mut with_source = occupied.clone();
            with_source.insert(source_id.as_str().to_owned());
            let sink_id = unique_node_id("warm auxiliary sink", &with_source)?;
            nodes.push(FlowNode::new(source_id.clone(), 0));
            nodes.push(FlowNode::new(sink_id.clone(), 0));
            (source_id, sink_id)
        }
    };

    let mut edges = Vec::new();
    let mut mapping = BTreeMap::new();
    let mut focus_mapping = BTreeMap::new();
    let mut residual_arcs = included
        .iter()
        .flat_map(|node| state.outgoing_arcs(*node))
        .filter(|arc| arc.capacity > 0 && included.contains(&arc.to))
        .map(|arc| (arc.id.clone(), arc.from, arc.to, arc.capacity))
        .collect::<Vec<_>>();
    residual_arcs.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    for (ordinal, (residual_id, from, to, capacity)) in residual_arcs.into_iter().enumerate() {
        let edge_id = EdgeId::parse(&format!("warm residual {ordinal:08}"))?;
        let from_id = graph
            .node(from)
            .ok_or(WarmStartPushRelabelError::Invariant)?
            .id()
            .clone();
        let to_id = graph
            .node(to)
            .ok_or(WarmStartPushRelabelError::Invariant)?
            .id()
            .clone();
        mapping.insert(edge_id.clone(), residual_id.clone());
        focus_mapping.insert(edge_id.clone(), AuxiliaryFocus::ResidualArc(residual_id));
        edges.push(UnresolvedFlowEdge {
            id: edge_id,
            from: from_id,
            to: to_id,
            lower: 0,
            capacity,
            cost: 0,
        });
    }
    for (ordinal, &(node, capacity)) in source_links.iter().enumerate() {
        if capacity == 0 || !included.contains(&node) {
            continue;
        }
        let edge_id = EdgeId::parse(&format!("warm source link {ordinal:08}"))?;
        focus_mapping.insert(edge_id.clone(), AuxiliaryFocus::Node(node));
        edges.push(UnresolvedFlowEdge {
            id: edge_id,
            from: aux_source_id.clone(),
            to: graph
                .node(node)
                .ok_or(WarmStartPushRelabelError::Invariant)?
                .id()
                .clone(),
            lower: 0,
            capacity,
            cost: 0,
        });
    }
    for (ordinal, &(node, capacity)) in sink_links.iter().enumerate() {
        if capacity == 0 || !included.contains(&node) {
            continue;
        }
        let edge_id = EdgeId::parse(&format!("warm sink link {ordinal:08}"))?;
        focus_mapping.insert(edge_id.clone(), AuxiliaryFocus::Node(node));
        edges.push(UnresolvedFlowEdge {
            id: edge_id,
            from: graph
                .node(node)
                .ok_or(WarmStartPushRelabelError::Invariant)?
                .id()
                .clone(),
            to: aux_sink_id.clone(),
            lower: 0,
            capacity,
            cost: 0,
        });
    }
    let auxiliary = FlowNetwork::new(nodes, edges)?;
    let aux_source = auxiliary
        .node_index(&aux_source_id)
        .ok_or(WarmStartPushRelabelError::Invariant)?;
    let aux_sink = auxiliary
        .node_index(&aux_sink_id)
        .ok_or(WarmStartPushRelabelError::Invariant)?;
    let (solved, scan_checkpoints) = if record_trace {
        let traced = trace_gap_relabel_push_relabel(&auxiliary, aux_source, aux_sink)?;
        let checkpoints = auxiliary_scan_checkpoints(
            &auxiliary,
            &traced.base_snapshot,
            &traced.events,
            &focus_mapping,
            traced.result.metrics.residual_arc_scans,
        )?;
        (traced.result, checkpoints)
    } else {
        (
            solve_gap_relabel_push_relabel(&auxiliary, aux_source, aux_sink)?,
            Vec::new(),
        )
    };
    let source_ids = solved
        .certificate
        .source_side
        .iter()
        .map(NodeId::as_str)
        .collect::<BTreeSet<_>>();
    let original_source_side = included
        .iter()
        .filter(|node| {
            graph
                .node(**node)
                .is_some_and(|item| source_ids.contains(item.id().as_str()))
        })
        .copied()
        .collect();
    let original_flow = auxiliary
        .edges()
        .iter()
        .zip(&solved.flows)
        .filter_map(|(edge, &flow)| {
            mapping
                .get(edge.id())
                .filter(|_| flow > 0)
                .map(|id| (id.clone(), flow))
        })
        .collect();
    Ok(AuxiliarySolution {
        value: solved.certificate.value,
        original_flow,
        original_source_side,
        metrics: solved.metrics,
        scan_checkpoints,
    })
}

fn auxiliary_scan_checkpoints(
    graph: &FlowNetwork,
    base: &FlowTraceSnapshot,
    events: &[FlowTraceEvent],
    focus_mapping: &BTreeMap<EdgeId, AuxiliaryFocus>,
    total: u128,
) -> Result<Vec<AuxiliaryScanCheckpoint>, WarmStartPushRelabelError> {
    let mut replay = base.clone();
    let mut last_published = 0_u128;
    let mut checkpoints = Vec::new();
    for event in events {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)?;
        let completed = replay.metrics.residual_arc_scans;
        if completed <= last_published {
            continue;
        }
        let Some(focus) = replay
            .active_path
            .first()
            .and_then(|arc| focus_mapping.get(arc.original_edge()).cloned())
        else {
            continue;
        };
        last_published = completed;
        if completed <= WARM_START_PUSH_RELABEL_TRACE_SCAN_PREFIX
            || completed.is_power_of_two()
            || completed == total
        {
            checkpoints.push(AuxiliaryScanCheckpoint {
                metrics: replay.metrics,
                focus,
            });
        }
    }
    if total > 0 && checkpoints.is_empty() {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(checkpoints)
}

fn unique_node_id(
    base: &str,
    occupied: &BTreeSet<String>,
) -> Result<NodeId, WarmStartPushRelabelError> {
    for suffix in 0_u16..=u16::MAX {
        let value = format!("{base} {suffix}");
        if !occupied.contains(&value) {
            return Ok(NodeId::parse(&value)?);
        }
    }
    Err(WarmStartPushRelabelError::Invariant)
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    predicted_flows: &[u64],
) -> Result<(), WarmStartPushRelabelError> {
    if graph.nodes().len() > WARM_START_PUSH_RELABEL_MAX_NODES
        || graph.edges().len() > WARM_START_PUSH_RELABEL_MAX_EDGES
    {
        return Err(WarmStartPushRelabelError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(WarmStartPushRelabelError::UnsupportedGraph);
    }
    if predicted_flows.len() != graph.edges().len()
        || graph
            .edges()
            .iter()
            .zip(predicted_flows)
            .any(|(edge, flow)| *flow > edge.capacity())
    {
        return Err(WarmStartPushRelabelError::InvalidPrediction);
    }
    Ok(())
}

fn imbalance_total(
    divergence: &[i128],
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<u128, WarmStartPushRelabelError> {
    divergence
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != source.as_usize() && *index != sink.as_usize())
        .try_fold(0_u128, |total, (_, value)| {
            total
                .checked_add(value.unsigned_abs())
                .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)
        })
}

fn excess_links(
    divergence: &[i128],
    included: &BTreeSet<NodeIndex>,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<Vec<(NodeIndex, u64)>, WarmStartPushRelabelError> {
    imbalance_links(divergence, included, source, sink, false)
}

fn deficit_links(
    divergence: &[i128],
    included: &BTreeSet<NodeIndex>,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<Vec<(NodeIndex, u64)>, WarmStartPushRelabelError> {
    imbalance_links(divergence, included, source, sink, true)
}

fn imbalance_links(
    divergence: &[i128],
    included: &BTreeSet<NodeIndex>,
    source: NodeIndex,
    sink: NodeIndex,
    deficit: bool,
) -> Result<Vec<(NodeIndex, u64)>, WarmStartPushRelabelError> {
    included
        .iter()
        .filter(|node| **node != source && **node != sink)
        .filter_map(|node| {
            let value = divergence[node.as_usize()];
            let amount = if deficit { value } else { -value };
            (amount > 0).then_some((*node, amount))
        })
        .map(|(node, amount)| {
            u64::try_from(amount)
                .map(|amount| (node, amount))
                .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)
        })
        .collect()
}

fn apply_auxiliary_flow(
    state: &mut ResidualState<'_>,
    flows: &[(ResidualArcId, u64)],
) -> Result<Vec<ResidualArcId>, WarmStartPushRelabelError> {
    let mut active = Vec::new();
    for (id, amount) in flows {
        state.augment(std::slice::from_ref(id), *amount)?;
        active.push(id.clone());
    }
    active.sort_unstable();
    active.dedup();
    Ok(active)
}

fn absorb_auxiliary_metrics(
    metrics: &mut WarmStartPushRelabelMetrics,
    auxiliary: PushRelabelMetrics,
) -> Result<(), WarmStartPushRelabelError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(auxiliary.residual_arc_scans)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    macro_rules! add {
        ($field:ident) => {
            metrics.$field = metrics
                .$field
                .checked_add(auxiliary.$field)
                .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
        };
    }
    add!(relabels);
    add!(pushes);
    add!(saturating_pushes);
    add!(nonsaturating_pushes);
    add!(discharges);
    add!(active_vertex_selections);
    add!(gap_relabels);
    Ok(())
}

fn auxiliary_checkpoint_metrics(
    base: WarmStartPushRelabelMetrics,
    checkpoint: FlowTraceMetrics,
) -> Result<WarmStartPushRelabelMetrics, WarmStartPushRelabelError> {
    let mut metrics = base;
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(checkpoint.residual_arc_scans)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    macro_rules! add_trace_metric {
        ($target:ident, $source:ident) => {
            metrics.$target = metrics
                .$target
                .checked_add(
                    u64::try_from(checkpoint.$source)
                        .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)?,
                )
                .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
        };
    }
    add_trace_metric!(relabels, relabels);
    add_trace_metric!(pushes, pushes);
    add_trace_metric!(saturating_pushes, saturating_pushes);
    add_trace_metric!(nonsaturating_pushes, nonsaturating_pushes);
    add_trace_metric!(discharges, discharges);
    add_trace_metric!(active_vertex_selections, active_vertex_selections);
    add_trace_metric!(gap_relabels, gap_terminations);
    Ok(metrics)
}

#[allow(clippy::too_many_arguments)]
fn publish_auxiliary_scan_checkpoints(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
    metrics: &mut WarmStartPushRelabelMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    base_metrics: WarmStartPushRelabelMetrics,
    checkpoints: &[AuxiliaryScanCheckpoint],
    catalog_id: &'static str,
    pseudocode_line: &'static str,
) -> Result<(), WarmStartPushRelabelError> {
    let final_metrics = *metrics;
    for checkpoint in checkpoints {
        *metrics = auxiliary_checkpoint_metrics(base_metrics, checkpoint.metrics)?;
        let (active_nodes, active_path, exact_focus) = match &checkpoint.focus {
            AuxiliaryFocus::ResidualArc(arc) => (
                Vec::new(),
                vec![arc.clone()],
                vec![FlowTraceEntityRef::ResidualArc(arc.clone())],
            ),
            AuxiliaryFocus::Node(node) => (
                vec![*node],
                Vec::new(),
                vec![FlowTraceEntityRef::Node(
                    graph
                        .node(*node)
                        .ok_or(FlowTraceError::MissingEntity)?
                        .id()
                        .clone(),
                )],
            ),
        };
        record_state_with_focus(
            graph,
            state,
            sink,
            source_side,
            metrics,
            recorder.as_deref_mut(),
            catalog_id,
            TraceGranularityV1::Micro,
            pseudocode_line,
            active_nodes,
            active_path,
            Some(exact_focus),
            Some(("scan", to_i128(metrics.residual_arc_scans)?)),
        )?;
    }
    *metrics = final_metrics;
    Ok(())
}

fn canonical_source_side(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BTreeSet<NodeIndex>, WarmStartPushRelabelError> {
    let mut sink_reachable = BTreeSet::from([sink]);
    let mut queue = VecDeque::from([sink]);
    while let Some(node) = queue.pop_front() {
        for arc in incoming_arcs(state, node) {
            if arc.capacity > 0 && sink_reachable.insert(arc.from) {
                queue.push_back(arc.from);
            }
        }
    }
    if sink_reachable.contains(&source) {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(graph
        .node_indices()
        .filter(|node| !sink_reachable.contains(node))
        .collect())
}

fn recover_conservation(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
    state: &mut ResidualState<'_>,
    metrics: &mut WarmStartPushRelabelMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<(), WarmStartPushRelabelError> {
    loop {
        let divergence = divergences(graph, state.flows())?;
        let Some(start) = graph.node_indices().find(|node| {
            *node != source
                && *node != sink
                && source_side.contains(node)
                && divergence[node.as_usize()] < 0
        }) else {
            break;
        };
        let path = residual_bfs_path(state, start, source, source_side, metrics)?;
        let delta = recovery_delta(state, &path, divergence[start.as_usize()].unsigned_abs())?;
        push_path(state, &path, delta)?;
        count_recovery(metrics)?;
        record_state(
            graph,
            state,
            sink,
            source_side,
            metrics,
            recorder.as_deref_mut(),
            "warm-start-push-relabel.recover-excess",
            TraceGranularityV1::Operation,
            "warm-start:return-source-side-excess-to-source",
            path,
            Some(("delta", i128::from(delta))),
        )?;
    }
    let sink_side = graph
        .node_indices()
        .filter(|node| !source_side.contains(node))
        .collect::<BTreeSet<_>>();
    loop {
        let divergence = divergences(graph, state.flows())?;
        let Some(target) = graph.node_indices().find(|node| {
            *node != source
                && *node != sink
                && sink_side.contains(node)
                && divergence[node.as_usize()] > 0
        }) else {
            break;
        };
        let path = residual_bfs_path(state, sink, target, &sink_side, metrics)?;
        let delta = recovery_delta(
            state,
            &path,
            u128::try_from(divergence[target.as_usize()])
                .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)?,
        )?;
        push_path(state, &path, delta)?;
        count_recovery(metrics)?;
        record_state(
            graph,
            state,
            sink,
            source_side,
            metrics,
            recorder.as_deref_mut(),
            "warm-start-push-relabel.recover-deficit",
            TraceGranularityV1::Operation,
            "warm-start:send-sink-flow-to-sink-side-deficit",
            path,
            Some(("delta", i128::from(delta))),
        )?;
    }
    Ok(())
}

fn residual_bfs_path(
    state: &ResidualState<'_>,
    start: NodeIndex,
    target: NodeIndex,
    allowed: &BTreeSet<NodeIndex>,
    metrics: &mut WarmStartPushRelabelMetrics,
) -> Result<Vec<ResidualArcId>, WarmStartPushRelabelError> {
    let node_count = state.graph().nodes().len();
    let mut predecessor = vec![None::<ResidualArcId>; node_count];
    let mut seen = vec![false; node_count];
    let mut queue = VecDeque::from([start]);
    seen[start.as_usize()] = true;
    while let Some(node) = queue.pop_front() {
        if node == target {
            break;
        }
        for arc in state.outgoing_arcs(node) {
            metrics.residual_arc_scans = metrics
                .residual_arc_scans
                .checked_add(1)
                .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
            if metrics.residual_arc_scans > WARM_START_PUSH_RELABEL_MAX_RECOVERY_ARC_SCANS {
                return Err(WarmStartPushRelabelError::WorkLimit);
            }
            if arc.capacity > 0 && allowed.contains(&arc.to) && !seen[arc.to.as_usize()] {
                seen[arc.to.as_usize()] = true;
                predecessor[arc.to.as_usize()] = Some(arc.id.clone());
                queue.push_back(arc.to);
            }
        }
    }
    if !seen[target.as_usize()] {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    let mut path = Vec::new();
    let mut node = target;
    while node != start {
        let id = predecessor[node.as_usize()]
            .clone()
            .ok_or(WarmStartPushRelabelError::Invariant)?;
        node = state
            .arc(&id)
            .ok_or(WarmStartPushRelabelError::Invariant)?
            .from;
        path.push(id);
    }
    path.reverse();
    Ok(path)
}

fn recovery_delta(
    state: &ResidualState<'_>,
    path: &[ResidualArcId],
    required: u128,
) -> Result<u64, WarmStartPushRelabelError> {
    let required =
        u64::try_from(required).map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)?;
    path.iter().try_fold(required, |delta, id| {
        state
            .arc(id)
            .map(|arc| delta.min(arc.capacity))
            .ok_or(WarmStartPushRelabelError::Invariant)
    })
}

fn push_path(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    delta: u64,
) -> Result<(), WarmStartPushRelabelError> {
    if delta == 0 || path.is_empty() {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    state.augment(path, delta)?;
    Ok(())
}

fn count_recovery(
    metrics: &mut WarmStartPushRelabelMetrics,
) -> Result<(), WarmStartPushRelabelError> {
    metrics.recovery_paths = metrics
        .recovery_paths
        .checked_add(1)
        .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
    if metrics.recovery_paths > WARM_START_PUSH_RELABEL_MAX_RECOVERY_TRANSITIONS {
        return Err(WarmStartPushRelabelError::WorkLimit);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_state(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
    metrics: &WarmStartPushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    catalog_id: &'static str,
    granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
    active_path: Vec<ResidualArcId>,
    detail: Option<(&'static str, i128)>,
) -> Result<(), WarmStartPushRelabelError> {
    record_state_with_focus(
        graph,
        state,
        sink,
        source_side,
        metrics,
        recorder,
        catalog_id,
        granularity,
        pseudocode_line,
        Vec::new(),
        active_path,
        None,
        detail,
    )
}

#[allow(clippy::too_many_arguments)]
fn record_state_with_focus(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
    metrics: &WarmStartPushRelabelMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    catalog_id: &'static str,
    granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
    active_nodes: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    exact_focus: Option<Vec<FlowTraceEntityRef>>,
    detail: Option<(&'static str, i128)>,
) -> Result<(), WarmStartPushRelabelError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let divergence = divergences(graph, state.flows())?;
    let excess = divergence
        .iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let labels = if source_side.is_empty() {
        vec![None; graph.nodes().len()]
    } else {
        height_labels(graph, state, sink, source_side)?
    };
    let focus = if let Some(exact_focus) = exact_focus {
        exact_focus
    } else {
        let mut focus = active_nodes
            .iter()
            .map(|&node| {
                graph
                    .node(node)
                    .map(|value| FlowTraceEntityRef::Node(value.id().clone()))
                    .ok_or(FlowTraceError::MissingEntity)
            })
            .collect::<Result<Vec<_>, _>>()?;
        focus.extend(residual_arc_entity_refs(graph, state, &active_path)?);
        focus
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        labels,
        active_nodes,
        active_path,
        excess,
        trace_metrics(*metrics),
    )
    .with_forest_overlay(graph, Vec::new(), source_side.iter().copied().collect());
    let metadata = FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity: granularity,
        pseudocode_line,
    };
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
    Ok(())
}

fn height_labels(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
) -> Result<Vec<Option<i128>>, WarmStartPushRelabelError> {
    if source_side.contains(&sink) || graph.node(sink).is_none() {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    let mut distance = vec![None::<u64>; graph.nodes().len()];
    let mut queue = VecDeque::new();
    // Algorithm 4 distances are specifically rooted at t.  The caller's cut
    // does not carry t, so use the unique sink-side node with zero outgoing
    // distance only when it is the actual terminal in `record_state` callers.
    distance[sink.as_usize()] = Some(0);
    queue.push_back(sink);
    while let Some(head) = queue.pop_front() {
        let next = distance[head.as_usize()]
            .and_then(|value| value.checked_add(1))
            .ok_or(WarmStartPushRelabelError::ArithmeticOverflow)?;
        for arc in incoming_arcs(state, head) {
            if arc.capacity > 0 && distance[arc.from.as_usize()].is_none() {
                distance[arc.from.as_usize()] = Some(next);
                queue.push_back(arc.from);
            }
        }
    }
    let n = i128::try_from(graph.nodes().len())
        .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)?;
    Ok(graph
        .node_indices()
        .map(|node| {
            if source_side.contains(&node) {
                Some(n)
            } else {
                distance[node.as_usize()].map(i128::from)
            }
        })
        .collect::<Vec<_>>())
}

fn trace_metrics(metrics: WarmStartPushRelabelMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: u128::from(metrics.auxiliary_solves),
        relaxation_passes: metrics.eta,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: u128::from(metrics.auxiliary_solves),
        path_searches: u128::from(metrics.recovery_paths),
        scaling_phases: metrics.cut_saturation_error,
        blocking_flow_phases: metrics.imbalance_error,
        relabels: u128::from(metrics.relabels),
        retreats: u128::from(metrics.cut_transfers),
        reverse_bfs_runs: u128::from(metrics.predicted_positive_edges),
        gap_terminations: u128::from(metrics.gap_relabels),
        pushes: u128::from(metrics.pushes),
        saturating_pushes: u128::from(metrics.saturating_pushes),
        nonsaturating_pushes: u128::from(metrics.nonsaturating_pushes),
        discharges: u128::from(metrics.discharges),
        active_vertex_selections: u128::from(metrics.active_vertex_selections),
    }
}

fn check_cut_saturated(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
) -> Result<(), WarmStartPushRelabelError> {
    if !source_side.contains(&source)
        || source_side.contains(&sink)
        || graph.node_indices().any(|node| {
            source_side.contains(&node)
                && state
                    .outgoing_arcs(node)
                    .into_iter()
                    .any(|arc| arc.capacity > 0 && !source_side.contains(&arc.to))
        })
    {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(())
}

fn incoming_arcs(state: &ResidualState<'_>, node: NodeIndex) -> Vec<crate::residual::ResidualArc> {
    let graph = state.graph();
    let mut arcs = graph
        .incoming_edges(node)
        .iter()
        .filter_map(|edge| graph.edge(*edge))
        .filter_map(|edge| {
            state.arc(&ResidualArcId::new(
                edge.id().clone(),
                ResidualDirection::Forward,
            ))
        })
        .chain(
            graph
                .outgoing_edges(node)
                .iter()
                .filter_map(|edge| graph.edge(*edge))
                .filter_map(|edge| {
                    state.arc(&ResidualArcId::new(
                        edge.id().clone(),
                        ResidualDirection::Reverse,
                    ))
                }),
        )
        .filter(|arc| arc.capacity > 0)
        .collect::<Vec<_>>();
    arcs.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    arcs
}

fn check_no_excess_in_sink_set(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
) -> Result<(), WarmStartPushRelabelError> {
    let divergence = divergences(graph, state.flows())?;
    if graph.node_indices().any(|node| {
        node != source
            && node != sink
            && !source_side.contains(&node)
            && divergence[node.as_usize()] < 0
    }) {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(())
}

fn check_separated_sets(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    source_side: &BTreeSet<NodeIndex>,
) -> Result<(), WarmStartPushRelabelError> {
    check_no_excess_in_sink_set(graph, state, source, sink, source_side)?;
    let divergence = divergences(graph, state.flows())?;
    if graph.node_indices().any(|node| {
        node != source
            && node != sink
            && source_side.contains(&node)
            && divergence[node.as_usize()] > 0
    }) {
        return Err(WarmStartPushRelabelError::Invariant);
    }
    Ok(())
}

fn check_snapshot_cut(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), WarmStartPushRelabelError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    let source_side = snapshot
        .strong_nodes
        .iter()
        .filter_map(|id| graph.node_index(id))
        .collect::<BTreeSet<_>>();
    check_cut_saturated(graph, &state, source, sink, &source_side)
}

fn check_no_excess_on_sink_side(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), WarmStartPushRelabelError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    let source_side = snapshot
        .strong_nodes
        .iter()
        .filter_map(|id| graph.node_index(id))
        .collect::<BTreeSet<_>>();
    check_no_excess_in_sink_set(graph, &state, source, sink, &source_side)
}

fn check_separated_imbalance(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), WarmStartPushRelabelError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)?;
    let source_side = snapshot
        .strong_nodes
        .iter()
        .filter_map(|id| graph.node_index(id))
        .collect::<BTreeSet<_>>();
    check_separated_sets(graph, &state, source, sink, &source_side)
}

fn count_set_difference(
    left: &BTreeSet<NodeIndex>,
    right: &BTreeSet<NodeIndex>,
) -> Result<u64, WarmStartPushRelabelError> {
    u64::try_from(left.difference(right).count())
        .map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)
}

fn to_i128(value: u128) -> Result<i128, WarmStartPushRelabelError> {
    i128::try_from(value).map_err(|_| WarmStartPushRelabelError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_edmonds_karp;

    fn graph(
        edges: &[(&str, &str, &str, u64)],
    ) -> Result<(FlowNetwork, NodeIndex, NodeIndex), WarmStartPushRelabelError> {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| Ok(FlowNode::new(NodeId::parse(id)?, 0)))
            .collect::<Result<Vec<_>, FlowModelError>>()?;
        let edges = edges
            .iter()
            .map(|&(id, from, to, capacity)| {
                Ok(UnresolvedFlowEdge {
                    id: EdgeId::parse(id)?,
                    from: NodeId::parse(from)?,
                    to: NodeId::parse(to)?,
                    lower: 0,
                    capacity,
                    cost: 0,
                })
            })
            .collect::<Result<Vec<_>, FlowModelError>>()?;
        let graph = FlowNetwork::new(nodes, edges)?;
        let source = graph
            .node_index(&NodeId::parse("s")?)
            .ok_or(WarmStartPushRelabelError::Invariant)?;
        let sink = graph
            .node_index(&NodeId::parse("t")?)
            .ok_or(WarmStartPushRelabelError::Invariant)?;
        Ok((graph, source, sink))
    }

    fn admission_graph(
        node_count: usize,
        edge_count: usize,
    ) -> Result<(FlowNetwork, NodeIndex, NodeIndex), WarmStartPushRelabelError> {
        let mut nodes = vec![
            FlowNode::new(NodeId::parse("s")?, 0),
            FlowNode::new(NodeId::parse("t")?, 0),
        ];
        for ordinal in 0..node_count.saturating_sub(2) {
            nodes.push(FlowNode::new(NodeId::parse(&format!("v{ordinal:04}"))?, 0));
        }
        let edges = (0..edge_count)
            .map(|ordinal| {
                Ok(UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("e{ordinal:05}"))?,
                    from: NodeId::parse("s")?,
                    to: NodeId::parse("t")?,
                    lower: 0,
                    capacity: 0,
                    cost: 0,
                })
            })
            .collect::<Result<Vec<_>, FlowModelError>>()?;
        let graph = FlowNetwork::new(nodes, edges)?;
        let source = graph
            .node_index(&NodeId::parse("s")?)
            .ok_or(WarmStartPushRelabelError::Invariant)?;
        let sink = graph
            .node_index(&NodeId::parse("t")?)
            .ok_or(WarmStartPushRelabelError::Invariant)?;
        Ok((graph, source, sink))
    }

    #[test]
    fn arbitrary_prediction_runs_all_warm_start_phases_and_replays() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 7),
            ("sb", "s", "b", 4),
            ("ab", "a", "b", 3),
            ("at", "a", "t", 4),
            ("bt", "b", "t", 7),
        ])
        .expect("graph");
        // Canonical edge order is ab, at, bt, sa, sb.
        let prediction = vec![3, 1, 1, 5, 0];
        let traced =
            trace_warm_start_push_relabel(&graph, source, sink, &prediction).expect("warm trace");
        let cold = solve_edmonds_karp(&graph, source, sink).expect("cold oracle");
        assert_eq!(traced.result.certificate.value, cold.certificate.value);
        assert_eq!(traced.result.predicted_flows, prediction);
        assert!(traced.result.metrics.imbalance_error > 0);
        assert_eq!(traced.result.metrics.auxiliary_solves, 3);
        assert!(
            traced
                .events
                .iter()
                .filter(|event| event.catalog_id.contains(".inspect-"))
                .all(|event| {
                    matches!(
                        event.entity_refs.as_slice(),
                        [FlowTraceEntityRef::ResidualArc(_) | FlowTraceEntityRef::Node(_)]
                    )
                })
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.catalog_id == "warm-start-push-relabel.move-t-excess" })
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.catalog_id == "warm-start-push-relabel.move-s-deficit" })
        );
    }

    #[test]
    fn recovery_fixture_exercises_both_cut_side_paths() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 5),
            ("at", "a", "t", 1),
            ("sb", "s", "b", 1),
            ("bt", "b", "t", 5),
        ])
        .expect("graph");
        // Canonical edge order is at, bt, sa, sb.  The prediction creates
        // excess at a and deficit at b on opposite sides of the final cut.
        let traced =
            trace_warm_start_push_relabel(&graph, source, sink, &[0, 5, 5, 0]).expect("warm trace");
        assert_eq!(traced.result.certificate.value, 2);
        assert!(traced.result.metrics.recovery_paths >= 2);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "warm-start-push-relabel.recover-excess")
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "warm-start-push-relabel.recover-deficit")
        );
    }

    #[test]
    fn admission_limits_accept_the_boundary_and_reject_the_next_value() {
        let (nodes_at_limit, source, sink) =
            admission_graph(WARM_START_PUSH_RELABEL_MAX_NODES, 0).expect("node boundary graph");
        assert_eq!(
            validate_input(
                &nodes_at_limit,
                source,
                sink,
                &vec![0; nodes_at_limit.edges().len()]
            ),
            Ok(())
        );
        let (nodes_over_limit, source, sink) =
            admission_graph(WARM_START_PUSH_RELABEL_MAX_NODES + 1, 0).expect("node overflow graph");
        assert_eq!(
            validate_input(&nodes_over_limit, source, sink, &[]),
            Err(WarmStartPushRelabelError::AdmissionLimit)
        );

        let (edges_at_limit, source, sink) =
            admission_graph(2, WARM_START_PUSH_RELABEL_MAX_EDGES).expect("edge boundary graph");
        assert_eq!(
            validate_input(
                &edges_at_limit,
                source,
                sink,
                &vec![0; edges_at_limit.edges().len()]
            ),
            Ok(())
        );
        let (edges_over_limit, source, sink) =
            admission_graph(2, WARM_START_PUSH_RELABEL_MAX_EDGES + 1).expect("edge overflow graph");
        assert_eq!(
            validate_input(
                &edges_over_limit,
                source,
                sink,
                &vec![0; edges_over_limit.edges().len()]
            ),
            Err(WarmStartPushRelabelError::AdmissionLimit)
        );
    }

    #[test]
    fn eta_plus_one_overflow_fails_closed() {
        let (graph, source, sink) = graph(&[("sa", "s", "a", u64::MAX)]).expect("full-width graph");
        assert_eq!(
            solve_warm_start_push_relabel(&graph, source, sink, &[u64::MAX]),
            Err(WarmStartPushRelabelError::ArithmeticOverflow)
        );
    }

    #[test]
    fn trace_checker_rejects_forged_phase_identity() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 5),
            ("at", "a", "t", 1),
            ("sb", "s", "b", 1),
            ("bt", "b", "t", 5),
        ])
        .expect("graph");
        let mut traced =
            trace_warm_start_push_relabel(&graph, source, sink, &[0, 5, 5, 0]).expect("warm trace");
        traced.events[1].catalog_id = "warm-start-push-relabel.forged".to_owned();
        assert!(matches!(
            check_warm_start_push_relabel_trace(&graph, source, sink, &traced),
            Err(WarmStartPushRelabelError::Invariant)
        ));
    }

    #[test]
    fn already_optimal_prediction_has_zero_error() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 3),
            ("at", "a", "t", 3),
            ("sb", "s", "b", 2),
            ("bt", "b", "t", 2),
        ])
        .expect("graph");
        // Canonical edge order is at, bt, sa, sb.
        let solved =
            solve_warm_start_push_relabel(&graph, source, sink, &[3, 2, 3, 2]).expect("warm solve");
        assert_eq!(solved.metrics.eta, 0);
        assert_eq!(solved.certificate.value, 5);
        assert_eq!(solved.flows, vec![3, 2, 3, 2]);
    }

    #[test]
    fn bounded_capacity_space_matches_cold_oracle() {
        const ARCS: [(&str, &str, &str); 5] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("ab", "a", "b"),
            ("at", "a", "t"),
            ("bt", "b", "t"),
        ];
        for encoded in 0_u64..3_u64.pow(u32::try_from(ARCS.len()).expect("small arc count")) {
            let mut value = encoded;
            let capacities = ARCS.map(|(id, from, to)| {
                let capacity = value % 3;
                value /= 3;
                (id, from, to, capacity)
            });
            let (graph, source, sink) = graph(&capacities).expect("graph");
            let prediction = graph
                .edges()
                .iter()
                .enumerate()
                .map(|(index, edge)| {
                    if index % 2 == 0 {
                        edge.capacity()
                    } else {
                        edge.capacity() / 2
                    }
                })
                .collect::<Vec<_>>();
            let warm = solve_warm_start_push_relabel(&graph, source, sink, &prediction)
                .expect("warm solve");
            let cold = solve_edmonds_karp(&graph, source, sink).expect("cold solve");
            assert_eq!(warm.certificate.value, cold.certificate.value);
        }
    }
}
