//! Feasible-flow negative-cycle cancellation for exact minimum-cost flow.

use thiserror::Error;

use crate::certificate::{CertificateError, MinCostFlowCertificate, check_min_cost_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetricId,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for simple cycle canceling.
pub const SIMPLE_CYCLE_CANCELING_MAX_NODES: usize = 256;
/// Conservative interactive edge limit for simple cycle canceling.
pub const SIMPLE_CYCLE_CANCELING_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling on canceled negative cycles.
pub const SIMPLE_CYCLE_CANCELING_MAX_CYCLES: u64 = 10_000;
/// Deterministic ceiling on positive residual-arc scans.
pub const SIMPLE_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;

/// Exact deterministic counters from simple cycle canceling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimpleCycleCancelingMetrics {
    /// Complete all-component Bellman–Ford negative-cycle searches.
    pub cycle_searches: u64,
    /// Bellman–Ford outer relaxation passes.
    pub relaxation_passes: u64,
    /// Positive residual arcs inspected during relaxation.
    pub residual_arc_scans: u128,
    /// Negative residual cycles canceled.
    pub canceled_cycles: u64,
}

/// Certified canonical simple-cycle-canceling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleCycleCancelingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: SimpleCycleCancelingMetrics,
}

/// Certified result with reversible negative-cycle searches and cancellations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimpleCycleCancelingTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: SimpleCycleCancelingResult,
    /// Replay boundary at the arbitrary feasible initial flow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after verified minimum-cost optimality.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Simple cycle canceling construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SimpleCycleCancelingError {
    /// Input exceeds the practical admission band.
    #[error("graph exceeds simple cycle canceling admission limits")]
    AdmissionLimit,
    /// A deterministic cycle or residual-scan ceiling was reached.
    #[error("simple cycle canceling work limit reached")]
    WorkLimit,
    /// No flow satisfies the requested balances and original bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The final independent primal/dual certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("simple cycle canceling arithmetic overflow")]
    ArithmeticOverflow,
    /// Bellman–Ford predecessors did not form a negative residual cycle.
    #[error("simple cycle canceling predecessor invariant failed")]
    CycleInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Minimizes an arbitrary feasible flow by canceling residual negative cycles.
///
/// # Errors
///
/// Rejects admission, infeasibility, arithmetic, residual mutation, work-limit,
/// predecessor, or final independent-certificate failure.
pub fn solve_simple_cycle_canceling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<SimpleCycleCancelingResult, SimpleCycleCancelingError> {
    solve_simple_cycle_canceling_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_simple_cycle_canceling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<SimpleCycleCancelingResult, SimpleCycleCancelingError> {
    solve_simple_cycle_canceling_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Runs simple cycle canceling with reversible search/cancellation events.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace invariant failures.
pub fn trace_simple_cycle_canceling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<SimpleCycleCancelingTraceResult, SimpleCycleCancelingError> {
    let run = solve_simple_cycle_canceling_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(SimpleCycleCancelingError::CycleInvariant)?;
    Ok(SimpleCycleCancelingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces simple cycle canceling while explicitly publishing its initial
/// feasible-flow construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_simple_cycle_canceling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<SimpleCycleCancelingTraceResult, SimpleCycleCancelingError> {
    let run = solve_simple_cycle_canceling_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(SimpleCycleCancelingError::CycleInvariant)?;
    Ok(SimpleCycleCancelingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct SimpleCycleCancelingInternalRun {
    result: SimpleCycleCancelingResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_simple_cycle_canceling_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
) -> Result<SimpleCycleCancelingInternalRun, SimpleCycleCancelingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_simple_cycle_canceling_internal_with_feasibility(
        graph,
        required_divergence,
        record_events,
        &mut feasibility,
    )
}

fn solve_simple_cycle_canceling_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<SimpleCycleCancelingInternalRun, SimpleCycleCancelingError> {
    validate_admission(graph)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &feasible.flows)?;
    let mut metrics = SimpleCycleCancelingMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, metrics, record_events)?;

    loop {
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "simple-cycle-canceling.start-negative-cycle-search",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "simple-cycle-canceling:start-bellman-ford-search",
            },
            CycleTraceView::empty(graph),
            None,
        )?;
        let search = find_negative_cycle(&state, &mut metrics, recorder.as_mut())?;
        let Some(cycle) = search.cycle else {
            record_trace(
                recorder.as_mut(),
                graph,
                &state,
                metrics,
                FlowTraceEventMetadata {
                    catalog_id: "simple-cycle-canceling.optimal",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "simple-cycle-canceling:return-negative-cycle-free-flow",
                },
                CycleTraceView::empty(graph),
                None,
            )?;
            break;
        };
        if metrics.canceled_cycles >= SIMPLE_CYCLE_CANCELING_MAX_CYCLES {
            return Err(SimpleCycleCancelingError::WorkLimit);
        }
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "simple-cycle-canceling.find-negative-cycle",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "simple-cycle-canceling:bellman-ford-negative-cycle",
            },
            CycleTraceView {
                labels: search.distances,
                search_order: search.search_order.clone(),
                cycle: cycle.clone(),
            },
            Some(("cycle-cost", search.cycle_cost)),
        )?;
        let amount = cancel_cycle(&mut state, &cycle, &mut metrics)?;
        record_trace(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            FlowTraceEventMetadata {
                catalog_id: "simple-cycle-canceling.cancel-negative-cycle",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "simple-cycle-canceling:augment-cycle-to-bottleneck",
            },
            CycleTraceView {
                labels: vec![None; graph.nodes().len()],
                search_order: search.search_order,
                cycle,
            },
            Some(("delta", i128::from(amount))),
        )?;
    }

    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    let result = SimpleCycleCancelingResult {
        flows,
        certificate,
        metrics,
    };
    Ok(SimpleCycleCancelingInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), SimpleCycleCancelingError> {
    if graph.nodes().len() > SIMPLE_CYCLE_CANCELING_MAX_NODES
        || graph.edges().len() > SIMPLE_CYCLE_CANCELING_MAX_EDGES
    {
        return Err(SimpleCycleCancelingError::AdmissionLimit);
    }
    Ok(())
}

struct NegativeCycleSearch {
    cycle: Option<Vec<ResidualArcId>>,
    cycle_cost: i128,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

fn find_negative_cycle(
    state: &ResidualState<'_>,
    metrics: &mut SimpleCycleCancelingMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
) -> Result<NegativeCycleSearch, SimpleCycleCancelingError> {
    metrics.cycle_searches = metrics
        .cycle_searches
        .checked_add(1)
        .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
    let node_count = state.graph().nodes().len();
    if node_count == 0 {
        return Ok(NegativeCycleSearch {
            cycle: None,
            cycle_cost: 0,
            distances: Vec::new(),
            search_order: Vec::new(),
        });
    }
    let mut distances = vec![0_i128; node_count];
    let mut predecessor = vec![None; node_count];
    let mut seen = vec![false; node_count];
    let mut search_order = Vec::new();
    let mut last_updated = None;
    for _ in 0..node_count {
        metrics.relaxation_passes = metrics
            .relaxation_passes
            .checked_add(1)
            .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
        last_updated = None;
        let mut updated_nodes = 0_usize;
        for node in state.graph().node_indices() {
            for arc in state.outgoing_arcs(node) {
                record_arc_scan(metrics, recorder.as_deref_mut(), &arc.id)?;
                let candidate = distances[arc.from.as_usize()]
                    .checked_add(arc.cost)
                    .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
                if candidate < distances[arc.to.as_usize()] {
                    distances[arc.to.as_usize()] = candidate;
                    predecessor[arc.to.as_usize()] = Some(arc.id);
                    last_updated = Some(arc.to);
                    updated_nodes = updated_nodes
                        .checked_add(1)
                        .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
                    if !std::mem::replace(&mut seen[arc.to.as_usize()], true) {
                        search_order.push(arc.to);
                    }
                }
            }
        }
        record_trace(
            recorder.as_deref_mut(),
            state.graph(),
            state,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: "simple-cycle-canceling.relaxation-pass",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "simple-cycle-canceling:relax-positive-residual-arcs",
            },
            CycleTraceView {
                labels: distances.iter().copied().map(Some).collect(),
                search_order: search_order.clone(),
                cycle: Vec::new(),
            },
            Some((
                "updated-nodes",
                i128::try_from(updated_nodes)
                    .map_err(|_| SimpleCycleCancelingError::ArithmeticOverflow)?,
            )),
        )?;
        if last_updated.is_none() {
            return Ok(NegativeCycleSearch {
                cycle: None,
                cycle_cost: 0,
                distances: distances.into_iter().map(Some).collect(),
                search_order,
            });
        }
    }
    let cycle = reconstruct_cycle(
        state,
        &predecessor,
        last_updated.ok_or(SimpleCycleCancelingError::CycleInvariant)?,
    )?;
    let cycle_cost = cycle_cost(state, &cycle)?;
    if cycle_cost >= 0 {
        return Err(SimpleCycleCancelingError::CycleInvariant);
    }
    Ok(NegativeCycleSearch {
        cycle: Some(cycle),
        cycle_cost,
        distances: distances.into_iter().map(Some).collect(),
        search_order,
    })
}

fn record_arc_scan(
    metrics: &mut SimpleCycleCancelingMetrics,
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    arc: &ResidualArcId,
) -> Result<(), SimpleCycleCancelingError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > SIMPLE_CYCLE_CANCELING_MAX_RESIDUAL_ARC_SCANS {
        return Err(SimpleCycleCancelingError::WorkLimit);
    }
    if let Some(recorder) = recorder {
        recorder.record_metric_observation(
            FlowTraceEventMetadata {
                catalog_id: "simple-cycle-canceling.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "simple-cycle-canceling:inspect-residual-arc",
            },
            FlowTraceMetricId::ResidualArcScans,
            FlowTraceEntityRef::ResidualArc(arc.clone()),
        )?;
    }
    Ok(())
}

fn reconstruct_cycle(
    state: &ResidualState<'_>,
    predecessor: &[Option<ResidualArcId>],
    mut cursor: NodeIndex,
) -> Result<Vec<ResidualArcId>, SimpleCycleCancelingError> {
    let node_count = state.graph().nodes().len();
    for _ in 0..node_count {
        let id = predecessor[cursor.as_usize()]
            .as_ref()
            .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
        cursor = state
            .arc(id)
            .filter(|arc| arc.to == cursor)
            .map(|arc| arc.from)
            .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
    }
    let start = cursor;
    let mut reversed = Vec::new();
    loop {
        let id = predecessor[cursor.as_usize()]
            .clone()
            .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
        let arc = state
            .arc(&id)
            .filter(|arc| arc.to == cursor)
            .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
        cursor = arc.from;
        reversed.push(id);
        if cursor == start {
            break;
        }
        if reversed.len() > node_count {
            return Err(SimpleCycleCancelingError::CycleInvariant);
        }
    }
    reversed.reverse();
    let offset = reversed
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.cmp(right))
        .map_or(0, |(index, _)| index);
    reversed.rotate_left(offset);
    Ok(reversed)
}

fn cancel_cycle(
    state: &mut ResidualState<'_>,
    cycle: &[ResidualArcId],
    metrics: &mut SimpleCycleCancelingMetrics,
) -> Result<u64, SimpleCycleCancelingError> {
    let amount = cycle
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
    if amount == 0 {
        return Err(SimpleCycleCancelingError::CycleInvariant);
    }
    if cycle_cost(state, cycle)? >= 0 {
        return Err(SimpleCycleCancelingError::CycleInvariant);
    }
    state.augment(cycle, amount)?;
    metrics.canceled_cycles = metrics
        .canceled_cycles
        .checked_add(1)
        .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
    Ok(amount)
}

fn cycle_cost(
    state: &ResidualState<'_>,
    cycle: &[ResidualArcId],
) -> Result<i128, SimpleCycleCancelingError> {
    let mut cost = 0_i128;
    for id in cycle {
        let arc = state
            .arc(id)
            .ok_or(SimpleCycleCancelingError::CycleInvariant)?;
        cost = cost
            .checked_add(arc.cost)
            .ok_or(SimpleCycleCancelingError::ArithmeticOverflow)?;
    }
    Ok(cost)
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: SimpleCycleCancelingMetrics,
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
        Vec::new(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct CycleTraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    cycle: Vec<ResidualArcId>,
}

impl CycleTraceView {
    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            cycle: Vec::new(),
        }
    }
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: SimpleCycleCancelingMetrics,
    metadata: FlowTraceEventMetadata,
    view: CycleTraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.labels,
        view.search_order,
        view.cycle,
        Vec::new(),
        trace_metrics(metrics),
    );
    if metadata.catalog_id == "simple-cycle-canceling.relaxation-pass" {
        // Each inspected residual arc already owns a local Micro event. The
        // pass boundary publishes aggregate label changes without turning all
        // changed nodes into the currently focused primitive.
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, Vec::new())
    } else {
        recorder.record_transition_with_detail(metadata, &snapshot, detail)
    }
}

const fn trace_metrics(metrics: SimpleCycleCancelingMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: metrics.relaxation_passes as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.canceled_cycles as u128,
        path_searches: metrics.cycle_searches as u128,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::certificate::{fixed_flow_divergences, supply_divergences};
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
    fn cancels_a_disconnected_finite_negative_cycle() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("path", "s", "t", 0, 1, 2),
                ("xy", "x", "y", 0, 3, -4),
                ("yx", "y", "x", 0, 3, 1),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");

        let result = solve_simple_cycle_canceling(&graph, &target).expect("minimum cost");

        assert_eq!(result.certificate.total_cost, -7);
        assert_eq!(result.metrics.canceled_cycles, 1);
        assert_eq!(result.metrics.cycle_searches, 2);
        check_min_cost_flow(&graph, &target, &result.flows).expect("certificate");
    }

    #[test]
    fn accepts_an_already_optimal_feasible_flow_without_cancellation() {
        let graph = network(&[("a", -2), ("s", 2)], &[("send", "s", "a", 1, 3, -5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_simple_cycle_canceling(&graph, &target).expect("minimum cost");

        assert_eq!(result.certificate.total_cost, -10);
        assert_eq!(result.metrics.canceled_cycles, 0);
        assert_eq!(result.metrics.cycle_searches, 1);
    }

    #[test]
    fn treats_a_negative_self_loop_as_a_one_arc_cycle() {
        let graph = network(&[("x", 0)], &[("loop", "x", "x", 0, 2, -5)]);
        let target = supply_divergences(&graph).expect("target");

        let result = solve_simple_cycle_canceling(&graph, &target).expect("minimum cost");

        assert_eq!(result.flows, vec![2]);
        assert_eq!(result.certificate.total_cost, -10);
        assert_eq!(result.metrics.canceled_cycles, 1);
    }

    #[test]
    fn trace_replays_cycle_search_and_cancellation_in_both_directions() {
        let graph = network(
            &[("x", 0), ("y", 0)],
            &[("xy", "x", "y", 0, 2, -3), ("yx", "y", "x", 0, 2, 1)],
        );
        let target = supply_divergences(&graph).expect("target");
        let fast = solve_simple_cycle_canceling(&graph, &target).expect("fast result");
        let traced = trace_simple_cycle_canceling(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        let find = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "simple-cycle-canceling.find-negative-cycle")
            .expect("find event");
        assert_eq!(
            find.detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("cycle-cost", -2))
        );
        assert!(find.patches.iter().any(|patch| matches!(
            patch,
            FlowTracePatch::ActivePath { after, .. } if after.len() == 2
        )));
        let cancel = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "simple-cycle-canceling.cancel-negative-cycle")
            .expect("cancel event");
        assert_eq!(
            cancel
                .detail
                .as_ref()
                .map(|detail| (detail.label.as_str(), detail.value)),
            Some(("delta", 2))
        );
        let scans = traced
            .events
            .iter()
            .filter(|event| event.catalog_id == "simple-cycle-canceling.inspect-residual-arc")
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(scans.len()).expect("scan event count"),
            fast.metrics.residual_arc_scans
        );
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "simple-cycle-canceling.relaxation-pass"
                && event.minimum_granularity == TraceGranularityV1::Operation
                && event.entity_refs.is_empty()
        }));
        assert!(scans.iter().all(|event| {
            event.entity_refs.len() == 1
                && matches!(event.entity_refs[0], FlowTraceEntityRef::ResidualArc(_))
                && event.patches.iter().any(|patch| {
                    matches!(
                        patch,
                        FlowTracePatch::Metric {
                            metric: FlowTraceMetricId::ResidualArcScans,
                            before,
                            after,
                        } if *after == before.saturating_add(1)
                    )
                })
        }));

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn infeasible_balances_are_rejected_before_cycle_search() {
        let graph = network(&[("s", 1), ("t", -1)], &[]);
        let target = supply_divergences(&graph).expect("target");

        assert!(matches!(
            solve_simple_cycle_canceling(&graph, &target),
            Err(SimpleCycleCancelingError::Feasibility(
                FeasibilityError::Infeasible(_)
            ))
        ));
    }
}
