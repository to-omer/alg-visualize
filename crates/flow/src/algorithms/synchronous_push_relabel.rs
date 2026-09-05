//! Baumstark–Blelloch–Shun synchronous parallel push–relabel.
//!
//! The browser build deterministically simulates the paper's parallel rounds:
//! every discharge reads the labels and excesses from the same round boundary,
//! active-active residual edges use the Section 3.1 ownership rule, and excess
//! deltas become visible only at the round barrier. This preserves the source
//! state machine even when the enclosing WebAssembly worker has one CPU thread.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceDirection, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot, apply_trace_event,
};

/// Conservative node band for complete synchronous-round visualization.
pub const SYNCHRONOUS_PUSH_RELABEL_MAX_NODES: usize = 256;
/// Conservative edge band for complete synchronous-round visualization.
pub const SYNCHRONOUS_PUSH_RELABEL_MAX_EDGES: usize = 2_048;
/// Hard positive-residual scan ceiling.
pub const SYNCHRONOUS_PUSH_RELABEL_MAX_ARC_SCANS: u128 = 20_000_000;
/// Hard round/push/relabel/recovery transition ceiling.
pub const SYNCHRONOUS_PUSH_RELABEL_MAX_TRANSITIONS: u64 = 200_000;
/// Preserve every small discharge scan and geometric progress on larger inputs.
const SYNCHRONOUS_PUSH_RELABEL_TRACE_SCAN_PREFIX: u128 = 512;

/// Exact counters for the deterministic logical-superstep execution.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SynchronousPushRelabelMetrics {
    /// Completed synchronous discharge rounds.
    pub rounds: u64,
    /// Sink-rooted exact global relabels, including the initial run.
    pub global_relabels: u64,
    /// Active vertices presented to logical workers across all rounds.
    pub active_vertex_visits: u64,
    /// Residual arcs inspected by discharge, global relabel, and recovery.
    pub residual_arc_scans: u128,
    /// Successful source-initialization and round pushes.
    pub pushes: u64,
    /// Temporary-label increases committed at round barriers.
    pub relabels: u64,
    /// Active-active admissible arcs deferred to their owning endpoint.
    pub ownership_conflicts: u64,
    /// Reverse positive-flow paths used to recover a feasible flow.
    pub recovery_paths: u64,
}

/// Certified synchronous parallel push–relabel result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynchronousPushRelabelResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Solver-independent maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact logical-work counters.
    pub metrics: SynchronousPushRelabelMetrics,
}

/// Certified result with source-specific reversible superstep events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SynchronousPushRelabelTraceResult {
    /// Same canonical result as the fast profile.
    pub result: SynchronousPushRelabelResult,
    /// Zero-flow boundary before source saturation.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible global-relabel, proposal, barrier, and recovery events.
    pub events: Vec<FlowTraceEvent>,
    /// Boundary after independent certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SynchronousPushRelabelError {
    /// Input exceeds the bounded complete-trace band.
    #[error("graph exceeds synchronous push-relabel admission limits")]
    AdmissionLimit,
    /// Source assumptions require zero lower bounds and zero supplies.
    #[error("synchronous push-relabel requires zero lower bounds and zero node supplies")]
    InputContract,
    /// Source and sink are missing or equal.
    #[error("invalid synchronous push-relabel terminals")]
    InvalidTerminals,
    /// A deterministic work ceiling was reached.
    #[error("synchronous push-relabel work limit reached")]
    WorkLimit,
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Exact flow arithmetic or the independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked state arithmetic overflowed.
    #[error("synchronous push-relabel arithmetic overflow")]
    ArithmeticOverflow,
    /// A source-defined active-label, excess, ownership, or recovery invariant failed.
    #[error("synchronous push-relabel invariant failed: {0}")]
    Invariant(&'static str),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Independent source-trace contract failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SynchronousPushRelabelTraceCheckError {
    /// Event patch replay failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Flow/excess/label state contradicted the synchronous preflow invariant.
    #[error("synchronous push-relabel trace invariant failed")]
    Invariant,
    /// An event did not belong to the source-specific vocabulary.
    #[error("synchronous push-relabel trace contains an unexpected event")]
    UnexpectedEvent,
    /// Final result or reverse replay did not match its declared boundary.
    #[error("synchronous push-relabel trace boundary mismatch")]
    BoundaryMismatch,
    /// Final independent maximum-flow certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
}

/// Solves maximum flow with deterministic simulation of the paper's
/// synchronous parallel discharge rounds.
///
/// # Errors
///
/// Rejects incompatible inputs, work-limit exhaustion, invariant failures, or
/// a candidate rejected by the independent max-flow checker.
pub fn solve_synchronous_parallel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SynchronousPushRelabelResult, SynchronousPushRelabelError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Runs synchronous parallel push–relabel with reversible round events.
///
/// # Errors
///
/// Returns the fast-profile failures plus trace transaction failures.
pub fn trace_synchronous_parallel_push_relabel(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<SynchronousPushRelabelTraceResult, SynchronousPushRelabelError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(SynchronousPushRelabelError::Invariant("missing-trace"))?;
    Ok(SynchronousPushRelabelTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Replays the trace independently and checks the preflow/label contract at
/// every stable source boundary.
///
/// # Errors
///
/// Rejects event-identity drift, invalid patches, invalid stable preflows,
/// final-certificate mismatch, or reverse-replay mismatch.
pub fn check_synchronous_push_relabel_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    run: &SynchronousPushRelabelTraceResult,
) -> Result<(), SynchronousPushRelabelTraceCheckError> {
    validate_base_snapshot(graph, &run.base_snapshot)?;
    let mut replay = run.base_snapshot.clone();
    let mut initialized = false;
    let mut globally_relabelled = false;
    let mut pending_commit = false;
    let mut recovering = false;
    let mut optimal = false;
    for (index, event) in run.events.iter().enumerate() {
        let stable = matches!(
            event.catalog_id.as_str(),
            "synchronous-parallel-push-relabel.initialize"
                | "synchronous-parallel-push-relabel.global-relabel"
                | "synchronous-parallel-push-relabel.commit-round"
                | "synchronous-parallel-push-relabel.recover-flow"
                | "synchronous-parallel-push-relabel.optimal"
        );
        let proposal = event.catalog_id == "synchronous-parallel-push-relabel.propose-round";
        let inspection =
            event.catalog_id == "synchronous-parallel-push-relabel.inspect-residual-arc";
        if !(stable || proposal || inspection) {
            return Err(SynchronousPushRelabelTraceCheckError::UnexpectedEvent);
        }
        match event.catalog_id.as_str() {
            "synchronous-parallel-push-relabel.initialize"
                if index == 0 && !initialized && !pending_commit =>
            {
                initialized = true;
            }
            "synchronous-parallel-push-relabel.global-relabel"
                if initialized && !pending_commit && !recovering && !optimal =>
            {
                globally_relabelled = true;
            }
            "synchronous-parallel-push-relabel.propose-round"
                if initialized
                    && globally_relabelled
                    && !pending_commit
                    && !recovering
                    && !optimal =>
            {
                pending_commit = true;
            }
            "synchronous-parallel-push-relabel.inspect-residual-arc"
                if initialized
                    && globally_relabelled
                    && !pending_commit
                    && !recovering
                    && !optimal => {}
            "synchronous-parallel-push-relabel.commit-round"
                if pending_commit && !recovering && !optimal =>
            {
                pending_commit = false;
            }
            "synchronous-parallel-push-relabel.recover-flow"
                if initialized && globally_relabelled && !pending_commit && !optimal =>
            {
                recovering = true;
            }
            "synchronous-parallel-push-relabel.optimal"
                if initialized && globally_relabelled && !pending_commit && !optimal =>
            {
                optimal = true;
            }
            _ => return Err(SynchronousPushRelabelTraceCheckError::UnexpectedEvent),
        }
        let before = replay.clone();
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)?;
        if (proposal || inspection)
            && (replay.edge_capacities != before.edge_capacities
                || replay.flows != before.flows
                || replay.residual_capacities != before.residual_capacities
                || replay.node_labels != before.node_labels
                || replay.remaining_divergence != before.remaining_divergence)
        {
            return Err(SynchronousPushRelabelTraceCheckError::Invariant);
        }
        validate_snapshot_preflow(graph, source, sink, &replay)?;
    }
    if !optimal
        || pending_commit
        || replay != run.final_snapshot
        || replay.flows != run.result.flows
    {
        return Err(SynchronousPushRelabelTraceCheckError::BoundaryMismatch);
    }
    let certificate = check_max_flow(graph, source, sink, &replay.flows)?;
    if certificate != run.result.certificate {
        return Err(SynchronousPushRelabelTraceCheckError::BoundaryMismatch);
    }
    for event in run.events.iter().rev() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Reverse)?;
    }
    if replay != run.base_snapshot {
        return Err(SynchronousPushRelabelTraceCheckError::BoundaryMismatch);
    }
    Ok(())
}

fn validate_base_snapshot(
    graph: &FlowNetwork,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), SynchronousPushRelabelTraceCheckError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)
        .map_err(|_| SynchronousPushRelabelTraceCheckError::Invariant)?;
    if snapshot.flows.iter().any(|&flow| flow != 0)
        || snapshot.node_labels.len() != graph.nodes().len()
        || snapshot.node_labels.iter().any(Option::is_some)
        || snapshot.remaining_divergence != vec![0; graph.nodes().len()]
        || snapshot.residual_capacities
            != FlowTraceSnapshot::capture(
                graph,
                &state,
                vec![None; graph.nodes().len()],
                Vec::new(),
                Vec::new(),
                vec![0; graph.nodes().len()],
                FlowTraceMetrics::default(),
            )
            .residual_capacities
    {
        return Err(SynchronousPushRelabelTraceCheckError::Invariant);
    }
    Ok(())
}

struct InternalRun {
    result: SynchronousPushRelabelResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone)]
struct KernelState {
    heights: Vec<usize>,
    excess: Vec<i128>,
}

struct RoundOutcome<'graph> {
    state: ResidualState<'graph>,
    kernel: KernelState,
    proposed_arcs: Vec<ResidualArcId>,
    scan_checkpoints: Vec<RoundScanCheckpoint>,
}

struct RoundScanCheckpoint {
    node: NodeIndex,
    arc: ResidualArcId,
    metrics: SynchronousPushRelabelMetrics,
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_trace: bool,
) -> Result<InternalRun, SynchronousPushRelabelError> {
    validate_input(graph, source, sink)?;
    let mut state = ResidualState::at_lower_bounds(graph);
    let mut kernel = KernelState {
        heights: vec![0; graph.nodes().len()],
        excess: excess_from_flows(graph, state.flows())?,
    };
    // The trace begins at the public zero-flow Ready state. Heights and
    // excesses first become visible in the recorded source initialization.
    let base_snapshot = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        vec![0; graph.nodes().len()],
        FlowTraceMetrics::default(),
    );
    let mut recorder = if record_trace {
        Some(FlowTraceRecorder::new(graph, base_snapshot)?)
    } else {
        None
    };
    let mut metrics = SynchronousPushRelabelMetrics::default();
    let mut transitions = 0_u64;

    initialize_source_preflow(
        graph,
        &mut state,
        source,
        &mut kernel,
        &mut metrics,
        &mut transitions,
    )?;
    record(
        recorder.as_mut(),
        graph,
        &state,
        &kernel,
        metrics,
        "synchronous-parallel-push-relabel.initialize",
        "synchronous-parallel-push-relabel:saturate-source-arcs",
        TraceGranularityV1::Phase,
        active_nodes(graph, &kernel, source, sink, true),
        Vec::new(),
        Some(("pushes", i128::from(metrics.pushes))),
    )?;

    global_relabel(graph, &state, source, sink, &mut kernel, &mut metrics)?;
    record_global_relabel(
        recorder.as_mut(),
        graph,
        &state,
        &kernel,
        metrics,
        source,
        sink,
    )?;
    let threshold = 12_u128
        .checked_mul(graph.nodes().len() as u128)
        .and_then(|value| value.checked_add(2_u128 * graph.edges().len() as u128))
        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
    let mut scans_at_last_global = metrics.residual_arc_scans;

    loop {
        let working = active_nodes(graph, &kernel, source, sink, true);
        if working.is_empty() {
            break;
        }
        count_transition(&mut transitions)?;
        metrics.rounds = checked_add_u64(metrics.rounds, 1)?;
        metrics.active_vertex_visits = metrics
            .active_vertex_visits
            .checked_add(working.len() as u64)
            .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
        let before = (
            state.flows().to_vec(),
            kernel.heights.clone(),
            kernel.excess.clone(),
        );
        let outcome = simulate_round(
            graph,
            &state,
            &kernel,
            &working,
            &mut metrics,
            &mut transitions,
            record_trace,
        )?;
        for checkpoint in &outcome.scan_checkpoints {
            record(
                recorder.as_mut(),
                graph,
                &state,
                &kernel,
                checkpoint.metrics,
                "synchronous-parallel-push-relabel.inspect-residual-arc",
                "synchronous-parallel-push-relabel:inspect-owned-residual-arc",
                TraceGranularityV1::Micro,
                vec![checkpoint.node],
                vec![checkpoint.arc.clone()],
                Some((
                    "scan",
                    i128::try_from(checkpoint.metrics.residual_arc_scans)
                        .map_err(|_| SynchronousPushRelabelError::ArithmeticOverflow)?,
                )),
            )?;
        }
        record(
            recorder.as_mut(),
            graph,
            &state,
            &kernel,
            metrics,
            "synchronous-parallel-push-relabel.propose-round",
            "synchronous-parallel-push-relabel:compute-owned-pushes-from-old-labels",
            TraceGranularityV1::Operation,
            working.clone(),
            outcome.proposed_arcs.clone(),
            Some(("round", i128::from(metrics.rounds))),
        )?;
        state = outcome.state;
        kernel = outcome.kernel;
        if before
            == (
                state.flows().to_vec(),
                kernel.heights.clone(),
                kernel.excess.clone(),
            )
        {
            return Err(SynchronousPushRelabelError::Invariant(
                "round-made-no-progress",
            ));
        }
        validate_kernel_preflow(graph, &state, source, sink, &kernel)?;
        record(
            recorder.as_mut(),
            graph,
            &state,
            &kernel,
            metrics,
            "synchronous-parallel-push-relabel.commit-round",
            "synchronous-parallel-push-relabel:apply-labels-and-excess-deltas-at-barrier",
            TraceGranularityV1::Operation,
            working,
            outcome.proposed_arcs,
            Some(("round", i128::from(metrics.rounds))),
        )?;

        if metrics
            .residual_arc_scans
            .saturating_sub(scans_at_last_global)
            >= threshold
        {
            global_relabel(graph, &state, source, sink, &mut kernel, &mut metrics)?;
            scans_at_last_global = metrics.residual_arc_scans;
            record_global_relabel(
                recorder.as_mut(),
                graph,
                &state,
                &kernel,
                metrics,
                source,
                sink,
            )?;
        }
    }

    recover_feasible_flow(
        graph,
        &mut state,
        source,
        sink,
        &mut kernel,
        &mut metrics,
        &mut transitions,
        &mut recorder,
    )?;
    validate_kernel_preflow(graph, &state, source, sink, &kernel)?;
    if graph
        .node_indices()
        .any(|node| node != source && node != sink && kernel.excess[node.as_usize()] != 0)
    {
        return Err(SynchronousPushRelabelError::Invariant(
            "recovery-left-intermediate-excess",
        ));
    }
    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record(
        recorder.as_mut(),
        graph,
        &state,
        &kernel,
        metrics,
        "synchronous-parallel-push-relabel.optimal",
        "synchronous-parallel-push-relabel:return-certified-flow",
        TraceGranularityV1::Phase,
        Vec::new(),
        Vec::new(),
        Some(("value", certificate.value)),
    )?;
    Ok(InternalRun {
        result: SynchronousPushRelabelResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn validate_input(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), SynchronousPushRelabelError> {
    if graph.nodes().len() > SYNCHRONOUS_PUSH_RELABEL_MAX_NODES
        || graph.edges().len() > SYNCHRONOUS_PUSH_RELABEL_MAX_EDGES
    {
        return Err(SynchronousPushRelabelError::AdmissionLimit);
    }
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(SynchronousPushRelabelError::InvalidTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(SynchronousPushRelabelError::InputContract);
    }
    Ok(())
}

fn initialize_source_preflow(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    kernel: &mut KernelState,
    metrics: &mut SynchronousPushRelabelMetrics,
    transitions: &mut u64,
) -> Result<(), SynchronousPushRelabelError> {
    kernel.heights[source.as_usize()] = graph.nodes().len();
    for arc in state.outgoing_arcs(source) {
        if arc.to == source || arc.id.direction() != ResidualDirection::Forward {
            continue;
        }
        count_transition(transitions)?;
        state.augment(std::slice::from_ref(&arc.id), arc.capacity)?;
        metrics.pushes = checked_add_u64(metrics.pushes, 1)?;
    }
    kernel.excess = excess_from_flows(graph, state.flows())?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn simulate_round<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'graph>,
    kernel: &KernelState,
    working: &[NodeIndex],
    metrics: &mut SynchronousPushRelabelMetrics,
    transitions: &mut u64,
    record_trace: bool,
) -> Result<RoundOutcome<'graph>, SynchronousPushRelabelError> {
    let node_count = graph.nodes().len();
    let mut next_state = state.clone();
    let mut next_kernel = kernel.clone();
    let old_heights = kernel.heights.clone();
    let old_excess = kernel.excess.clone();
    let mut delta = vec![0_i128; node_count];
    let mut old_active = vec![false; node_count];
    for &node in working {
        old_active[node.as_usize()] = true;
    }
    let mut proposed_arcs = Vec::new();
    let mut scan_checkpoints = Vec::new();

    for &node in working {
        let index = node.as_usize();
        let mut local_excess = old_excess[index];
        let mut local_height = old_heights[index];
        let mut completed = false;
        while local_excess > 0 && local_height < node_count {
            let arcs = next_state.outgoing_arcs(node);
            let mut minimum_label = node_count;
            let mut deferred_admissible = false;
            for arc in arcs {
                count_scan(metrics)?;
                if record_trace
                    && (metrics.residual_arc_scans <= SYNCHRONOUS_PUSH_RELABEL_TRACE_SCAN_PREFIX
                        || metrics.residual_arc_scans.is_power_of_two())
                {
                    scan_checkpoints.push(RoundScanCheckpoint {
                        node,
                        arc: arc.id.clone(),
                        metrics: *metrics,
                    });
                }
                let head = arc.to;
                if head == node {
                    continue;
                }
                let head_height = old_heights[head.as_usize()];
                let admissible = local_height == head_height.saturating_add(1);
                let owned =
                    !old_active[head.as_usize()] || wins_active_edge(node, head, &old_heights);
                if admissible && !owned {
                    metrics.ownership_conflicts = checked_add_u64(metrics.ownership_conflicts, 1)?;
                    deferred_admissible = true;
                    continue;
                }
                if admissible {
                    let amount = u64::try_from(local_excess.min(i128::from(arc.capacity)))
                        .map_err(|_| SynchronousPushRelabelError::ArithmeticOverflow)?;
                    if amount == 0 {
                        return Err(SynchronousPushRelabelError::Invariant("zero-push"));
                    }
                    count_transition(transitions)?;
                    next_state.augment(std::slice::from_ref(&arc.id), amount)?;
                    metrics.pushes = checked_add_u64(metrics.pushes, 1)?;
                    proposed_arcs.push(arc.id.clone());
                    local_excess = local_excess
                        .checked_sub(i128::from(amount))
                        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
                    delta[index] = delta[index]
                        .checked_sub(i128::from(amount))
                        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
                    delta[head.as_usize()] = delta[head.as_usize()]
                        .checked_add(i128::from(amount))
                        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
                    if local_excess == 0 {
                        completed = true;
                        break;
                    }
                }
                let remaining_capacity = next_state.arc(&arc.id).map_or(0, |next| next.capacity);
                if remaining_capacity > 0 && head_height >= local_height {
                    minimum_label = minimum_label.min(head_height.saturating_add(1));
                }
            }
            if completed || local_excess == 0 {
                break;
            }
            if deferred_admissible {
                break;
            }
            let new_height = minimum_label.min(node_count);
            if new_height <= local_height {
                return Err(SynchronousPushRelabelError::Invariant(
                    "relabel-did-not-increase",
                ));
            }
            count_transition(transitions)?;
            local_height = new_height;
            next_kernel.heights[index] = new_height;
            metrics.relabels = checked_add_u64(metrics.relabels, 1)?;
        }
    }
    next_kernel.excess = old_excess
        .iter()
        .zip(delta)
        .map(|(&value, change)| {
            value
                .checked_add(change)
                .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let reconstructed = excess_from_flows(graph, next_state.flows())?;
    if next_kernel.excess != reconstructed {
        return Err(SynchronousPushRelabelError::Invariant(
            "round-excess-does-not-match-flow",
        ));
    }
    Ok(RoundOutcome {
        state: next_state,
        kernel: next_kernel,
        proposed_arcs,
        scan_checkpoints,
    })
}

fn wins_active_edge(node: NodeIndex, head: NodeIndex, old_heights: &[usize]) -> bool {
    let node_height = old_heights[node.as_usize()];
    let head_height = old_heights[head.as_usize()];
    node_height.saturating_add(1) < head_height
        || node_height == head_height.saturating_add(1)
        || (node_height == head_height && node < head)
}

fn global_relabel(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut KernelState,
    metrics: &mut SynchronousPushRelabelMetrics,
) -> Result<(), SynchronousPushRelabelError> {
    let node_count = graph.nodes().len();
    let mut distances = vec![None; node_count];
    distances[sink.as_usize()] = Some(0_usize);
    let mut queue = VecDeque::from([sink]);
    while let Some(head) = queue.pop_front() {
        let next_distance = distances[head.as_usize()]
            .ok_or(SynchronousPushRelabelError::Invariant(
                "global-relabel-queue-without-distance",
            ))?
            .saturating_add(1);
        for tail in graph.node_indices() {
            for arc in state.outgoing_arcs(tail) {
                count_scan(metrics)?;
                if arc.to == head && distances[tail.as_usize()].is_none() {
                    distances[tail.as_usize()] = Some(next_distance);
                    queue.push_back(tail);
                    break;
                }
            }
        }
    }
    for node in graph.node_indices() {
        kernel.heights[node.as_usize()] = distances[node.as_usize()].unwrap_or(node_count);
    }
    kernel.heights[source.as_usize()] = node_count;
    metrics.global_relabels = checked_add_u64(metrics.global_relabels, 1)?;
    validate_kernel_preflow(graph, state, source, sink, kernel)
}

#[allow(clippy::too_many_arguments)]
fn recover_feasible_flow(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &mut KernelState,
    metrics: &mut SynchronousPushRelabelMetrics,
    transitions: &mut u64,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), SynchronousPushRelabelError> {
    loop {
        let Some(start) = graph
            .node_indices()
            .find(|&node| node != source && node != sink && kernel.excess[node.as_usize()] > 0)
        else {
            break;
        };
        let path = reverse_positive_flow_path(graph, state, start, source, metrics)?;
        let bottleneck = path
            .iter()
            .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
            .min()
            .ok_or(SynchronousPushRelabelError::Invariant(
                "recovery-path-has-no-residual-capacity",
            ))?;
        let amount = u64::try_from(kernel.excess[start.as_usize()].min(i128::from(bottleneck)))
            .map_err(|_| SynchronousPushRelabelError::ArithmeticOverflow)?;
        if amount == 0 {
            return Err(SynchronousPushRelabelError::Invariant(
                "recovery-computed-zero-amount",
            ));
        }
        count_transition(transitions)?;
        state.augment(&path, amount)?;
        metrics.recovery_paths = checked_add_u64(metrics.recovery_paths, 1)?;
        kernel.excess = excess_from_flows(graph, state.flows())?;
        record(
            recorder.as_mut(),
            graph,
            state,
            kernel,
            *metrics,
            "synchronous-parallel-push-relabel.recover-flow",
            "synchronous-parallel-push-relabel:cancel-excess-to-source",
            TraceGranularityV1::Operation,
            vec![start, source],
            path,
            Some(("delta", i128::from(amount))),
        )?;
    }
    Ok(())
}

fn reverse_positive_flow_path(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    start: NodeIndex,
    source: NodeIndex,
    metrics: &mut SynchronousPushRelabelMetrics,
) -> Result<Vec<ResidualArcId>, SynchronousPushRelabelError> {
    let mut predecessor = vec![None::<ResidualArcId>; graph.nodes().len()];
    let mut seen = vec![false; graph.nodes().len()];
    seen[start.as_usize()] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if node == source {
            break;
        }
        for arc in state.outgoing_arcs(node) {
            count_scan(metrics)?;
            if arc.id.direction() != ResidualDirection::Reverse || seen[arc.to.as_usize()] {
                continue;
            }
            seen[arc.to.as_usize()] = true;
            predecessor[arc.to.as_usize()] = Some(arc.id);
            queue.push_back(arc.to);
        }
    }
    if !seen[source.as_usize()] {
        return Err(SynchronousPushRelabelError::Invariant(
            "recovery-cannot-reach-source",
        ));
    }
    let mut reversed = Vec::new();
    let mut current = source;
    while current != start {
        let arc_id = predecessor[current.as_usize()].clone().ok_or(
            SynchronousPushRelabelError::Invariant("recovery-predecessor-missing"),
        )?;
        let arc = state
            .arc(&arc_id)
            .ok_or(SynchronousPushRelabelError::Invariant(
                "recovery-residual-arc-missing",
            ))?;
        reversed.push(arc_id);
        current = arc.from;
    }
    reversed.reverse();
    Ok(reversed)
}

fn active_nodes(
    graph: &FlowNetwork,
    kernel: &KernelState,
    source: NodeIndex,
    sink: NodeIndex,
    reachable_only: bool,
) -> Vec<NodeIndex> {
    let node_count = graph.nodes().len();
    graph
        .node_indices()
        .filter(|&node| {
            node != source
                && node != sink
                && kernel.excess[node.as_usize()] > 0
                && (!reachable_only || kernel.heights[node.as_usize()] < node_count)
        })
        .collect()
}

fn excess_from_flows(
    graph: &FlowNetwork,
    flows: &[u64],
) -> Result<Vec<i128>, SynchronousPushRelabelError> {
    divergences(graph, flows)?
        .into_iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)
        })
        .collect()
}

fn validate_kernel_preflow(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    kernel: &KernelState,
) -> Result<(), SynchronousPushRelabelError> {
    if kernel.heights.len() != graph.nodes().len()
        || kernel.excess != excess_from_flows(graph, state.flows())?
        || kernel.heights[source.as_usize()] != graph.nodes().len()
        || kernel.heights[sink.as_usize()] != 0
    {
        return Err(SynchronousPushRelabelError::Invariant(
            "preflow-shape-or-terminal-label",
        ));
    }
    for node in graph.node_indices() {
        if node != source && node != sink && kernel.excess[node.as_usize()] < 0 {
            return Err(SynchronousPushRelabelError::Invariant(
                "negative-intermediate-excess",
            ));
        }
        if kernel.heights[node.as_usize()] < graph.nodes().len() {
            for arc in state.outgoing_arcs(node) {
                if kernel.heights[node.as_usize()]
                    > kernel.heights[arc.to.as_usize()].saturating_add(1)
                {
                    return Err(SynchronousPushRelabelError::Invariant(
                        "invalid-active-residual-label",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_snapshot_preflow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    snapshot: &FlowTraceSnapshot,
) -> Result<(), SynchronousPushRelabelTraceCheckError> {
    let state = ResidualState::from_flows(graph, &snapshot.flows)
        .map_err(|_| SynchronousPushRelabelTraceCheckError::Invariant)?;
    let heights = snapshot
        .node_labels
        .iter()
        .map(|value| {
            value
                .and_then(|label| usize::try_from(label).ok())
                .ok_or(SynchronousPushRelabelTraceCheckError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let excess = divergences(graph, &snapshot.flows)?
        .into_iter()
        .map(i128::checked_neg)
        .collect::<Option<Vec<_>>>()
        .ok_or(SynchronousPushRelabelTraceCheckError::Invariant)?;
    if excess != snapshot.remaining_divergence
        || heights[source.as_usize()] != graph.nodes().len()
        || heights[sink.as_usize()] != 0
    {
        return Err(SynchronousPushRelabelTraceCheckError::Invariant);
    }
    for node in graph.node_indices() {
        if node != source && node != sink && excess[node.as_usize()] < 0 {
            return Err(SynchronousPushRelabelTraceCheckError::Invariant);
        }
        if heights[node.as_usize()] < graph.nodes().len() {
            for arc in state.outgoing_arcs(node) {
                if heights[node.as_usize()] > heights[arc.to.as_usize()].saturating_add(1) {
                    return Err(SynchronousPushRelabelTraceCheckError::Invariant);
                }
            }
        }
    }
    Ok(())
}

fn snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    kernel: &KernelState,
    metrics: SynchronousPushRelabelMetrics,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
) -> Result<FlowTraceSnapshot, SynchronousPushRelabelError> {
    let distances = kernel
        .heights
        .iter()
        .map(|&height| {
            i128::try_from(height)
                .map(Some)
                .map_err(|_| SynchronousPushRelabelError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FlowTraceSnapshot::capture(
        graph,
        state,
        distances,
        search_order,
        active_path,
        kernel.excess.clone(),
        trace_metrics(metrics),
    ))
}

#[allow(clippy::too_many_arguments)]
fn record(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    kernel: &KernelState,
    metrics: SynchronousPushRelabelMetrics,
    catalog_id: &'static str,
    pseudocode_line: &'static str,
    granularity: TraceGranularityV1,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    detail: Option<(&'static str, i128)>,
) -> Result<(), SynchronousPushRelabelError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let next = snapshot(graph, state, kernel, metrics, search_order, active_path)?;
    recorder.record_transition_with_detail(
        FlowTraceEventMetadata {
            catalog_id,
            minimum_granularity: granularity,
            pseudocode_line,
        },
        &next,
        detail,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_global_relabel(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    kernel: &KernelState,
    metrics: SynchronousPushRelabelMetrics,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), SynchronousPushRelabelError> {
    record(
        recorder,
        graph,
        state,
        kernel,
        metrics,
        "synchronous-parallel-push-relabel.global-relabel",
        "synchronous-parallel-push-relabel:reverse-bfs-between-rounds",
        TraceGranularityV1::Phase,
        active_nodes(graph, kernel, source, sink, true),
        Vec::new(),
        Some(("global-relabels", i128::from(metrics.global_relabels))),
    )
}

const fn trace_metrics(metrics: SynchronousPushRelabelMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.global_relabels as u128,
        relaxation_passes: metrics.rounds as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.recovery_paths as u128,
        path_searches: metrics.recovery_paths as u128,
        scaling_phases: 0,
        blocking_flow_phases: metrics.rounds as u128,
        relabels: metrics.relabels as u128,
        retreats: metrics.ownership_conflicts as u128,
        reverse_bfs_runs: metrics.global_relabels as u128,
        gap_terminations: 0,
        pushes: metrics.pushes as u128,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: metrics.active_vertex_visits as u128,
        active_vertex_selections: metrics.active_vertex_visits as u128,
    }
}

fn count_scan(
    metrics: &mut SynchronousPushRelabelMetrics,
) -> Result<(), SynchronousPushRelabelError> {
    if metrics.residual_arc_scans >= SYNCHRONOUS_PUSH_RELABEL_MAX_ARC_SCANS {
        return Err(SynchronousPushRelabelError::WorkLimit);
    }
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
    Ok(())
}

fn count_transition(transitions: &mut u64) -> Result<(), SynchronousPushRelabelError> {
    if *transitions >= SYNCHRONOUS_PUSH_RELABEL_MAX_TRANSITIONS {
        return Err(SynchronousPushRelabelError::WorkLimit);
    }
    *transitions = transitions
        .checked_add(1)
        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)?;
    Ok(())
}

fn checked_add_u64(left: u64, right: u64) -> Result<u64, SynchronousPushRelabelError> {
    left.checked_add(right)
        .ok_or(SynchronousPushRelabelError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn network(nodes: &[&str], edges: &[(&str, &str, &str, u64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower: 0,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("valid graph")
    }

    fn terminals(graph: &FlowNetwork) -> (NodeIndex, NodeIndex) {
        (
            graph
                .node_index(&NodeId::parse("s").expect("source id"))
                .expect("source"),
            graph
                .node_index(&NodeId::parse("t").expect("sink id"))
                .expect("sink"),
        )
    }

    #[test]
    fn synchronous_rounds_match_independent_max_flow_and_replay() {
        let graph = network(
            &["a", "b", "c", "s", "t"],
            &[
                ("sa", "s", "a", 7),
                ("sb", "s", "b", 6),
                ("ab", "a", "b", 3),
                ("ac", "a", "c", 5),
                ("bc", "b", "c", 5),
                ("at", "a", "t", 2),
                ("bt", "b", "t", 2),
                ("ct", "c", "t", 7),
            ],
        );
        let (source, sink) = terminals(&graph);
        let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        let fast = solve_synchronous_parallel_push_relabel(&graph, source, sink)
            .expect("synchronous fast");
        let traced = trace_synchronous_parallel_push_relabel(&graph, source, sink)
            .expect("synchronous trace");

        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate, oracle.certificate);
        assert!(fast.metrics.rounds > 0);
        assert!(fast.metrics.global_relabels > 0);
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "synchronous-parallel-push-relabel.propose-round"
        }));
        assert!(
            traced.events.iter().any(|event| {
                event.catalog_id == "synchronous-parallel-push-relabel.commit-round"
            })
        );
        check_synchronous_push_relabel_trace(&graph, source, sink, &traced)
            .expect("source trace checker");
    }

    #[test]
    fn ownership_rule_is_antisymmetric_for_equal_active_labels() {
        let heights = vec![4, 4];
        let left = NodeIndex::try_from_usize(0).expect("left");
        let right = NodeIndex::try_from_usize(1).expect("right");
        assert!(wins_active_edge(left, right, &heights));
        assert!(!wins_active_edge(right, left, &heights));
    }

    #[test]
    fn rejects_nonzero_lower_bounds_before_mutation() {
        let graph = FlowNetwork::new(
            vec![
                FlowNode::new(NodeId::parse("s").expect("source"), 0),
                FlowNode::new(NodeId::parse("t").expect("sink"), 0),
            ],
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("edge"),
                from: NodeId::parse("s").expect("source"),
                to: NodeId::parse("t").expect("sink"),
                lower: 1,
                capacity: 2,
                cost: 0,
            }],
        )
        .expect("graph");
        let (source, sink) = terminals(&graph);
        assert_eq!(
            solve_synchronous_parallel_push_relabel(&graph, source, sink),
            Err(SynchronousPushRelabelError::InputContract)
        );
    }

    #[test]
    fn exhaustive_small_capacity_family_matches_edmonds_karp() {
        const ARCS: [(&str, &str, &str); 6] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("ab", "a", "b"),
            ("ba", "b", "a"),
            ("at", "a", "t"),
            ("bt", "b", "t"),
        ];
        for encoded in 0_u64..3_u64.pow(u32::try_from(ARCS.len()).expect("small fixed arc count")) {
            let mut digits = encoded;
            let edges = ARCS
                .iter()
                .map(|&(id, from, to)| {
                    let capacity = digits % 3;
                    digits /= 3;
                    (id, from, to, capacity)
                })
                .collect::<Vec<_>>();
            let graph = network(&["s", "a", "b", "t"], &edges);
            let (source, sink) = terminals(&graph);
            let oracle = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            let actual = solve_synchronous_parallel_push_relabel(&graph, source, sink)
                .unwrap_or_else(|error| panic!("capacity code {encoded}: {error}"));
            assert_eq!(
                actual.certificate, oracle.certificate,
                "capacity code {encoded}"
            );
        }
    }

    #[test]
    fn source_checker_rejects_event_identity_and_final_boundary_corruption() {
        let graph = network(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 5),
                ("sb", "s", "b", 4),
                ("ab", "a", "b", 2),
                ("at", "a", "t", 3),
                ("bt", "b", "t", 6),
            ],
        );
        let (source, sink) = terminals(&graph);
        let run = trace_synchronous_parallel_push_relabel(&graph, source, sink).expect("trace");

        let mut wrong_identity = run.clone();
        wrong_identity.events[0].catalog_id =
            "synchronous-parallel-push-relabel.optimal".to_owned();
        assert_eq!(
            check_synchronous_push_relabel_trace(&graph, source, sink, &wrong_identity),
            Err(SynchronousPushRelabelTraceCheckError::UnexpectedEvent)
        );

        let mut wrong_boundary = run;
        wrong_boundary.final_snapshot.flows[0] = wrong_boundary.final_snapshot.flows[0]
            .checked_sub(1)
            .expect("first flow is positive");
        assert_eq!(
            check_synchronous_push_relabel_trace(&graph, source, sink, &wrong_boundary),
            Err(SynchronousPushRelabelTraceCheckError::BoundaryMismatch)
        );
    }
}
