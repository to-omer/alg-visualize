//! Bounded exact Goldberg--Rao binary blocking-flow phases.
//!
//! The implementation follows Sections 3--6 of Goldberg and Rao (1998):
//! residual arcs receive binary lengths, source-to-sink distances define the
//! admissible graph, special arcs preserve the distance increase argument, and
//! zero-length strongly connected components are contracted for one blocking
//! or delta-limited augmentation.  The contracted blocking flow is lifted back
//! through explicit in/out paths inside every zero-length component.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot, residual_arc_entity_refs,
};

/// Conservative node limit for the explicit SCC/lifting implementation.
pub const GOLDBERG_RAO_MAX_NODES: usize = 256;
/// Conservative edge limit for the explicit SCC/lifting implementation.
pub const GOLDBERG_RAO_MAX_EDGES: usize = 2_048;
/// Deterministic residual-arc scan ceiling.
pub const GOLDBERG_RAO_MAX_RESIDUAL_ARC_SCANS: u128 = 20_000_000;
/// Deterministic phase/update/augmentation transition ceiling.
pub const GOLDBERG_RAO_MAX_STATE_TRANSITIONS: u64 = 250_000;
/// Eager semantic event ceiling.
pub const GOLDBERG_RAO_MAX_TRACE_EVENTS: usize = 100_000;

/// Maximum measured residual-arc inspections represented by one trace event.
///
/// The first few inspections remain one-per-event. Long scans then retain the
/// exact last inspected arc of each bounded block so trace size stays below
/// the eager event ceiling without hiding an unbounded amount of source work.
/// Maximum number of exact source scans represented by one binary primitive
/// detail boundary after the dense prefix.
pub const BINARY_BLOCKING_TRACE_SCAN_BLOCK_MAX: u128 = 256;
const BINARY_TRACE_DENSE_PREFIX: u128 = 16;

/// Exact counters for the binary-length Goldberg--Rao kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GoldbergRaoMetrics {
    /// Gap-halving phases entered.
    pub phases: u64,
    /// Binary blocking-flow update steps.
    pub update_steps: u64,
    /// Reverse 0--1 shortest-path computations.
    pub distance_searches: u64,
    /// Positive residual arcs inspected by analysis, cuts, SCCs, and paths.
    pub residual_arc_scans: u128,
    /// Canonical distance cuts evaluated.
    pub canonical_cut_evaluations: u64,
    /// Arcs assigned base binary length zero.
    pub zero_length_arc_observations: u64,
    /// Special arcs whose modified length is zero.
    pub special_arc_observations: u64,
    /// Zero-length SCC contractions performed, counting nontrivial SCCs.
    pub nontrivial_contractions: u64,
    /// Contracted admissible-path augmentations.
    pub contracted_augmentations: u64,
    /// Update steps that ended with a blocking flow below delta.
    pub blocking_updates: u64,
    /// Update steps that delivered exactly delta units.
    pub delta_limited_updates: u64,
    /// Internal SCC routing paths used while lifting contracted flow.
    pub component_routing_paths: u64,
    /// Canonical cut replacements that reduced the residual gap by at least half.
    pub cut_updates: u64,
    /// Total flow units added by update steps.
    pub augmented_units: u128,
    /// State transitions charged to the deterministic work ceiling.
    pub state_transitions: u64,
}

/// One residual augmentation operation in primitive execution order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryBlockingAugmentation {
    /// Stable residual direction selected from the step's initial admissible graph.
    pub arc: ResidualArcId,
    /// Positive integral amount routed on the arc.
    pub amount: u64,
}

/// Certified result of one source-defined binary blocking-flow primitive step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryBlockingStepResult {
    /// Starting original-edge flow vector.
    pub initial_flows: Vec<u64>,
    /// Flow vector after contracted augmentation and SCC lifting.
    pub flows: Vec<u64>,
    /// Gap upper bound used to choose the phase scale.
    pub upper_bound: u128,
    /// Integral delta cap for this update step.
    pub delta: u128,
    /// Flow value delivered by the primitive.
    pub value: u128,
    /// Whether the fixed admissible graph is blocked before reaching delta.
    pub blocking: bool,
    /// Exact binary distance-to-sink labels at the start of the step.
    pub distances: Vec<Option<u64>>,
    /// Stable base-zero residual directions.
    pub base_zero_arcs: Vec<ResidualArcId>,
    /// Stable special residual directions.
    pub special_arcs: Vec<ResidualArcId>,
    /// Stable admissible residual directions after special-length correction.
    pub admissible_arcs: Vec<ResidualArcId>,
    /// Stable zero-length admissible directions contracted into SCCs.
    pub zero_admissible_arcs: Vec<ResidualArcId>,
    /// Canonical zero-length SCC ordinal for every original node.
    pub component_of: Vec<usize>,
    /// Primitive augmentation operations, including internal SCC lifting.
    pub augmentation: Vec<BinaryBlockingAugmentation>,
    /// Exact bounded-work counters for this one primitive invocation.
    pub metrics: GoldbergRaoMetrics,
}

/// One binary blocking-flow primitive with reversible semantic boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryBlockingStepTraceResult {
    /// Independently checked primitive result.
    pub result: BinaryBlockingStepResult,
    /// Boundary before binary lengths are analyzed.
    pub base_snapshot: FlowTraceSnapshot,
    /// Analyze, contract, and atomic augment/lift events.
    pub events: Vec<FlowTraceEvent>,
    /// Boundary after the complete primitive update.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Certified exact maximum flow produced by Goldberg--Rao phases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldbergRaoResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact source-conformance counters.
    pub metrics: GoldbergRaoMetrics,
}

/// Goldberg--Rao result with reversible binary-length phase boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldbergRaoTraceResult {
    /// Same canonical result as the fast profile.
    pub result: GoldbergRaoResult,
    /// Boundary before the initial source cut is installed.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete semantic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Independently certified maximum-flow boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Binary blocking-flow or Goldberg--Rao construction failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GoldbergRaoError {
    /// Input exceeds the bounded interactive implementation band.
    #[error("graph exceeds Goldberg-Rao admission limits")]
    AdmissionLimit,
    /// The source paper's zero-feasible integral-flow domain was violated.
    #[error("Goldberg-Rao requires zero lower bounds and distinct terminals")]
    GraphRequirement,
    /// Residual mutation or reconstruction failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent certificate rejected the candidate.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Checked arithmetic or a deterministic counter exceeded its domain.
    #[error("Goldberg-Rao exact arithmetic overflow")]
    ArithmeticOverflow,
    /// Deterministic scan, transition, or trace budget was exhausted.
    #[error("Goldberg-Rao deterministic work limit exceeded")]
    WorkLimit,
    /// Binary lengths, distances, contraction, lifting, or blocking disagreed.
    #[error("Goldberg-Rao binary blocking-flow invariant failed")]
    Invariant,
}

/// Solves exact integral maximum flow with bounded Goldberg--Rao phases.
///
/// The explicit contracted-DAG blocking implementation is intended for small
/// observable graphs.  It does not claim the paper's dynamic-tree blocking-flow
/// time bound.
///
/// # Errors
///
/// Rejects nonzero lower bounds, identical terminals, out-of-band input,
/// deterministic work exhaustion, invariant failure, or failed certification.
pub fn solve_goldberg_rao(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<GoldbergRaoResult, GoldbergRaoError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every gap phase, binary analysis, contraction, lift, and cut update.
///
/// # Errors
///
/// Returns the same failures as [`solve_goldberg_rao`] plus trace failures.
pub fn trace_goldberg_rao(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<GoldbergRaoTraceResult, GoldbergRaoError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(GoldbergRaoError::Invariant)?;
    Ok(GoldbergRaoTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Executes and independently verifies one binary blocking-flow primitive step.
///
/// # Errors
///
/// Rejects an invalid initial flow, zero parameters, source-domain mismatch, or
/// any binary-length/contraction/blocking invariant failure.
pub fn solve_binary_blocking_step(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    initial_flows: &[u64],
    upper_bound: u128,
    delta: u128,
) -> Result<BinaryBlockingStepResult, GoldbergRaoError> {
    validate_graph(graph, source, sink)?;
    if upper_bound == 0 || delta == 0 || delta > upper_bound {
        return Err(GoldbergRaoError::Invariant);
    }
    let state = ResidualState::from_flows(graph, initial_flows)?;
    validate_feasible_flow(graph, source, sink, state.flows())?;
    let mut metrics = GoldbergRaoMetrics::default();
    let mut execution =
        execute_binary_step(&state, source, sink, upper_bound, delta, &mut metrics, None)?;
    increment(&mut metrics.update_steps)?;
    metrics.augmented_units = execution.result.value;
    if execution.result.blocking {
        increment(&mut metrics.blocking_updates)?;
    } else {
        increment(&mut metrics.delta_limited_updates)?;
    }
    execution.result.metrics = metrics;
    check_binary_blocking_step(graph, source, sink, &execution.result)?;
    Ok(execution.result)
}

/// Executes the deterministic first Goldberg--Rao binary blocking primitive.
///
/// The current feasible flow is taken from `initial_flows`.  Its residual
/// source cut supplies a valid gap upper bound.  A zero cut is normalized to
/// one so the positive integral phase parameter remains defined; one is still
/// a valid upper bound on the zero residual gap.
///
/// # Errors
///
/// Returns the same validation and bounded-work failures as
/// [`solve_binary_blocking_step`].
pub fn solve_binary_blocking_first_step(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    initial_flows: &[u64],
) -> Result<BinaryBlockingStepResult, GoldbergRaoError> {
    validate_graph(graph, source, sink)?;
    let state = ResidualState::from_flows(graph, initial_flows)?;
    validate_feasible_flow(graph, source, sink, state.flows())?;
    let mut metrics = GoldbergRaoMetrics::default();
    let upper_bound = residual_cut_capacity(&state, &[source], &mut metrics, None)?.max(1);
    let delta = phase_delta(upper_bound, graph.nodes().len(), graph.edges().len())?;
    let mut execution =
        execute_binary_step(&state, source, sink, upper_bound, delta, &mut metrics, None)?;
    increment(&mut metrics.update_steps)?;
    metrics.augmented_units = execution.result.value;
    if execution.result.blocking {
        increment(&mut metrics.blocking_updates)?;
    } else {
        increment(&mut metrics.delta_limited_updates)?;
    }
    execution.result.metrics = metrics;
    check_binary_blocking_step(graph, source, sink, &execution.result)?;
    Ok(execution.result)
}

/// Records analysis, zero-SCC contraction, and the atomic lifted update for
/// the deterministic first binary blocking-flow primitive.
///
/// # Errors
///
/// Returns the same failures as [`solve_binary_blocking_first_step`] plus
/// reversible trace construction failures.
#[allow(clippy::too_many_lines)]
pub fn trace_binary_blocking_first_step(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    initial_flows: &[u64],
) -> Result<BinaryBlockingStepTraceResult, GoldbergRaoError> {
    validate_graph(graph, source, sink)?;
    let initial_state = ResidualState::from_flows(graph, initial_flows)?;
    validate_feasible_flow(graph, source, sink, initial_state.flows())?;
    let mut metrics = GoldbergRaoMetrics::default();
    let base_snapshot = binary_primitive_snapshot(
        graph,
        &initial_state,
        &[],
        &[],
        GoldbergRaoMetrics::default(),
    );
    let mut recorder = FlowTraceRecorder::new(graph, base_snapshot.clone())?;
    let mut initial_cut_checkpoints = BinaryCheckpointCollector::new(metrics.residual_arc_scans);
    let upper_bound = residual_cut_capacity(
        &initial_state,
        &[source],
        &mut metrics,
        Some(&mut initial_cut_checkpoints),
    )?
    .max(1);
    for checkpoint in initial_cut_checkpoints.finish(initial_state.flows()) {
        record_binary_primitive_checkpoint(graph, &mut recorder, &checkpoint)?;
    }
    let delta = phase_delta(upper_bound, graph.nodes().len(), graph.edges().len())?;
    let mut analysis_checkpoints = BinaryCheckpointCollector::new(metrics.residual_arc_scans);
    let analysis = analyze_binary_network(
        &initial_state,
        sink,
        delta,
        &mut metrics,
        Some(&mut analysis_checkpoints),
    )?;
    for checkpoint in analysis_checkpoints.finish(initial_state.flows()) {
        record_binary_primitive_checkpoint(graph, &mut recorder, &checkpoint)?;
    }
    let distances = analysis.distances.clone();
    let admissible_arcs = analysis
        .admissible_arcs
        .iter()
        .map(|arc| arc.id.clone())
        .collect::<Vec<_>>();
    let zero_admissible_arcs = analysis
        .zero_admissible_arcs
        .iter()
        .map(|arc| arc.id.clone())
        .collect::<Vec<_>>();
    let component_count = binary_component_count(&analysis.component_of);
    let analyzed =
        binary_primitive_snapshot(graph, &initial_state, &distances, &admissible_arcs, metrics);
    recorder.record_transition_with_detail(
        BinaryPrimitiveEventKind::Analyze.metadata(),
        &analyzed,
        Some(("delta", to_i128(delta)?)),
    )?;
    let contracted = binary_primitive_snapshot(
        graph,
        &initial_state,
        &distances,
        &zero_admissible_arcs,
        metrics,
    );
    recorder.record_transition_with_detail(
        BinaryPrimitiveEventKind::Contract.metadata(),
        &contracted,
        Some((
            "components",
            i128::try_from(component_count).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
        )),
    )?;
    let mut execution_checkpoints = BinaryCheckpointCollector::new(metrics.residual_arc_scans);
    let mut execution = execute_binary_step_from_analysis(
        &initial_state,
        source,
        sink,
        upper_bound,
        delta,
        analysis,
        &mut metrics,
        Some(&mut execution_checkpoints),
    )?;
    for checkpoint in execution_checkpoints.finish(execution.state.flows()) {
        record_binary_primitive_checkpoint(graph, &mut recorder, &checkpoint)?;
    }
    increment(&mut metrics.update_steps)?;
    metrics.augmented_units = execution.result.value;
    if execution.result.blocking {
        increment(&mut metrics.blocking_updates)?;
    } else {
        increment(&mut metrics.delta_limited_updates)?;
    }
    execution.result.metrics = metrics;
    let result = execution.result;
    check_binary_blocking_step(graph, source, sink, &result)?;
    let final_state = ResidualState::from_flows(graph, &result.flows)?;
    let augmented_arcs = result
        .augmentation
        .iter()
        .map(|operation| operation.arc.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let final_snapshot = binary_primitive_snapshot(
        graph,
        &final_state,
        &result.distances,
        &augmented_arcs,
        result.metrics,
    );
    recorder.record_transition_with_detail(
        BinaryPrimitiveEventKind::Complete.metadata(),
        &final_snapshot,
        Some(("delivered", to_i128(result.value)?)),
    )?;
    let (recorded_base, events, recorded_final) = recorder.finish();
    if recorded_base != base_snapshot || recorded_final != final_snapshot {
        return Err(GoldbergRaoError::Invariant);
    }
    Ok(BinaryBlockingStepTraceResult {
        result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn record_binary_primitive_checkpoint(
    graph: &FlowNetwork,
    recorder: &mut FlowTraceRecorder<'_>,
    checkpoint: &BinaryAnalysisCheckpoint,
) -> Result<(), GoldbergRaoError> {
    if recorder.event_count() >= GOLDBERG_RAO_MAX_TRACE_EVENTS {
        return Err(GoldbergRaoError::WorkLimit);
    }
    let state = ResidualState::from_flows(graph, &checkpoint.flows)?;
    let inspected = binary_primitive_snapshot(
        graph,
        &state,
        &checkpoint.distances,
        &checkpoint.focus_arcs,
        checkpoint.metrics,
    );
    let focus = checkpoint
        .focus_arcs
        .iter()
        .cloned()
        .map(FlowTraceEntityRef::ResidualArc)
        .collect();
    recorder.record_transition_with_detail_and_focus(
        checkpoint.kind.metadata(),
        &inspected,
        Some((checkpoint.kind.detail_label(), checkpoint.detail)),
        focus,
    )?;
    Ok(())
}

/// Checks the primitive certificate and every deterministic trace boundary.
///
/// # Errors
///
/// Rejects a malformed primitive result, discontinuous event sequence, or any
/// disagreement with a fresh source-level execution.
pub fn check_binary_blocking_step_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &BinaryBlockingStepTraceResult,
) -> Result<(), GoldbergRaoError> {
    check_binary_blocking_step(graph, source, sink, &trace.result)?;
    let expected =
        trace_binary_blocking_first_step(graph, source, sink, &trace.result.initial_flows)?;
    if trace != &expected {
        return Err(GoldbergRaoError::Invariant);
    }
    Ok(())
}

/// Replays and verifies a binary blocking-flow primitive result independently.
///
/// # Errors
///
/// Rejects wrong labels, arc classes, SCCs, augmentation replay, delta value,
/// conservation, or a false blocking claim.
pub fn check_binary_blocking_step(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    result: &BinaryBlockingStepResult,
) -> Result<(), GoldbergRaoError> {
    validate_graph(graph, source, sink)?;
    if result.upper_bound == 0
        || result.delta == 0
        || result.delta > result.upper_bound
        || result.initial_flows.len() != graph.edges().len()
        || result.flows.len() != graph.edges().len()
        || result.component_of.len() != graph.nodes().len()
    {
        return Err(GoldbergRaoError::Invariant);
    }
    let start = ResidualState::from_flows(graph, &result.initial_flows)?;
    validate_feasible_flow(graph, source, sink, start.flows())?;
    let mut metrics = GoldbergRaoMetrics::default();
    let analysis = analyze_binary_network(&start, sink, result.delta, &mut metrics, None)?;
    if analysis.distances != result.distances
        || analysis.base_zero_arcs != result.base_zero_arcs
        || analysis.special_arcs != result.special_arcs
        || analysis
            .admissible_arcs
            .iter()
            .map(|arc| arc.id.clone())
            .collect::<Vec<_>>()
            != result.admissible_arcs
        || analysis
            .zero_admissible_arcs
            .iter()
            .map(|arc| arc.id.clone())
            .collect::<Vec<_>>()
            != result.zero_admissible_arcs
        || analysis.component_of != result.component_of
    {
        return Err(GoldbergRaoError::Invariant);
    }
    let admissible = analysis
        .admissible_arcs
        .iter()
        .map(|arc| arc.id.clone())
        .collect::<BTreeSet<_>>();
    let mut replay = start.clone();
    let mut used = BTreeMap::<ResidualArcId, u128>::new();
    for operation in &result.augmentation {
        if operation.amount == 0 || !admissible.contains(&operation.arc) {
            return Err(GoldbergRaoError::Invariant);
        }
        replay.augment(std::slice::from_ref(&operation.arc), operation.amount)?;
        let entry = used.entry(operation.arc.clone()).or_default();
        *entry = entry
            .checked_add(u128::from(operation.amount))
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    }
    if replay.flows() != result.flows {
        return Err(GoldbergRaoError::Invariant);
    }
    validate_feasible_flow(graph, source, sink, replay.flows())?;
    let start_value = flow_value(graph, source, &result.initial_flows)?;
    let final_value = flow_value(graph, source, &result.flows)?;
    let increase = final_value
        .checked_sub(start_value)
        .ok_or(GoldbergRaoError::Invariant)?;
    if increase != result.value || increase > result.delta {
        return Err(GoldbergRaoError::Invariant);
    }
    if result.blocking == (result.value == result.delta) {
        return Err(GoldbergRaoError::Invariant);
    }
    if result.blocking
        && admissible_path_exists_after_usage(
            graph.nodes().len(),
            source,
            sink,
            &analysis.admissible_arcs,
            &used,
        )?
    {
        return Err(GoldbergRaoError::Invariant);
    }
    let mut expected_metrics = GoldbergRaoMetrics::default();
    let mut expected = execute_binary_step(
        &start,
        source,
        sink,
        result.upper_bound,
        result.delta,
        &mut expected_metrics,
        None,
    )?
    .result;
    // Metrics are execution evidence rather than part of the mathematical
    // primitive witness. Goldberg--Rao invokes this checker with cumulative
    // solver counters, while the standalone primitive publishes local ones.
    expected.metrics = result.metrics;
    if expected != *result {
        return Err(GoldbergRaoError::Invariant);
    }
    Ok(())
}

struct InternalRun {
    result: GoldbergRaoResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<InternalRun, GoldbergRaoError> {
    validate_graph(graph, source, sink)?;
    GoldbergRaoKernel::new(graph, source, sink, with_trace)?.run()
}

enum PhaseProgress {
    Continue,
    NextPhase,
    Optimal,
}

struct GoldbergRaoKernel<'graph> {
    graph: &'graph FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    state: ResidualState<'graph>,
    metrics: GoldbergRaoMetrics,
    recorder: Option<FlowTraceRecorder<'graph>>,
    upper_bound: u128,
}

impl<'graph> GoldbergRaoKernel<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        with_trace: bool,
    ) -> Result<Self, GoldbergRaoError> {
        let state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
        let mut metrics = GoldbergRaoMetrics::default();
        let recorder = with_trace
            .then(|| {
                FlowTraceRecorder::new(graph, trace_snapshot(graph, &state, &[], &[], metrics))
            })
            .transpose()?;
        let mut checkpoints = recorder
            .as_ref()
            .map(|_| BinaryCheckpointCollector::new(metrics.residual_arc_scans));
        let upper_bound =
            residual_cut_capacity(&state, &[source], &mut metrics, checkpoints.as_mut())?;
        let mut kernel = Self {
            graph,
            source,
            sink,
            state,
            metrics,
            recorder,
            upper_bound,
        };
        if let Some(checkpoints) = checkpoints {
            for checkpoint in checkpoints.finish(kernel.state.flows()) {
                kernel.record_analysis_checkpoint(&checkpoint)?;
            }
        }
        kernel.record_event(
            &[],
            &[],
            EventKind::Initialize,
            Some(("gap-upper-bound", to_i128(upper_bound)?)),
        )?;
        Ok(kernel)
    }

    fn run(mut self) -> Result<InternalRun, GoldbergRaoError> {
        'solver: while self.upper_bound > 0 {
            let delta = self.begin_phase()?;
            loop {
                match self.run_update(delta)? {
                    PhaseProgress::Continue => {}
                    PhaseProgress::NextPhase => break,
                    PhaseProgress::Optimal => break 'solver,
                }
            }
        }
        self.finish()
    }

    fn begin_phase(&mut self) -> Result<u128, GoldbergRaoError> {
        bump_transition(&mut self.metrics)?;
        increment(&mut self.metrics.phases)?;
        let delta = phase_delta(
            self.upper_bound,
            self.graph.nodes().len(),
            self.graph.edges().len(),
        )?;
        self.record_event(&[], &[], EventKind::Phase, Some(("delta", to_i128(delta)?)))?;
        Ok(delta)
    }

    fn run_update(&mut self, delta: u128) -> Result<PhaseProgress, GoldbergRaoError> {
        let mut checkpoints = self
            .recorder
            .as_ref()
            .map(|_| BinaryCheckpointCollector::new(self.metrics.residual_arc_scans));
        let analysis = analyze_binary_network(
            &self.state,
            self.sink,
            delta,
            &mut self.metrics,
            checkpoints.as_mut(),
        )?;
        if let Some(checkpoints) = checkpoints {
            for checkpoint in checkpoints.finish(self.state.flows()) {
                self.record_analysis_checkpoint(&checkpoint)?;
            }
        }
        let Some(source_distance) = analysis.distances[self.source.as_usize()] else {
            return Ok(PhaseProgress::Optimal);
        };
        let zero_arcs = u128::try_from(analysis.base_zero_arcs.len())
            .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
        self.record_event(
            &analysis.distances,
            &[],
            EventKind::Lengths,
            Some(("zero-length-arcs", to_i128(zero_arcs)?)),
        )?;
        if let Some(progress) = self.update_gap_cut(&analysis, source_distance)? {
            return Ok(progress);
        }
        let node_count = u64::try_from(self.graph.nodes().len())
            .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
        if source_distance > node_count {
            return Err(GoldbergRaoError::Invariant);
        }
        self.apply_binary_update(analysis, delta)?;
        Ok(PhaseProgress::Continue)
    }

    fn update_gap_cut(
        &mut self,
        analysis: &BinaryAnalysis,
        source_distance: u64,
    ) -> Result<Option<PhaseProgress>, GoldbergRaoError> {
        if source_distance == 0 {
            return Ok(None);
        }
        let mut checkpoints = self
            .recorder
            .as_ref()
            .map(|_| BinaryCheckpointCollector::new(self.metrics.residual_arc_scans));
        let cut = minimum_canonical_cut(
            &self.state,
            self.source,
            &analysis.distances,
            &mut self.metrics,
            checkpoints.as_mut(),
        )?;
        if let Some(checkpoints) = checkpoints {
            for checkpoint in checkpoints.finish(self.state.flows()) {
                self.record_analysis_checkpoint(&checkpoint)?;
            }
        }
        self.record_event_with_focus(
            &analysis.distances,
            &[],
            &cut.crossing_arcs,
            EventKind::CanonicalCut,
            Some(("cut-capacity", to_i128(cut.capacity)?)),
        )?;
        if cut.capacity.saturating_mul(2) > self.upper_bound {
            return Ok(None);
        }
        self.upper_bound = cut.capacity;
        increment(&mut self.metrics.cut_updates)?;
        self.record_event(
            &analysis.distances,
            &[],
            EventKind::UpdateCut,
            Some(("gap-upper-bound", to_i128(self.upper_bound)?)),
        )?;
        Ok(Some(if self.upper_bound == 0 {
            PhaseProgress::Optimal
        } else {
            PhaseProgress::NextPhase
        }))
    }

    fn apply_binary_update(
        &mut self,
        analysis: BinaryAnalysis,
        delta: u128,
    ) -> Result<(), GoldbergRaoError> {
        let mut checkpoints = self
            .recorder
            .as_ref()
            .map(|_| BinaryCheckpointCollector::new(self.metrics.residual_arc_scans));
        let execution = execute_binary_step_from_analysis(
            &self.state,
            self.source,
            self.sink,
            self.upper_bound,
            delta,
            analysis,
            &mut self.metrics,
            checkpoints.as_mut(),
        )?;
        if let Some(checkpoints) = checkpoints {
            for checkpoint in checkpoints.finish(execution.state.flows()) {
                self.record_analysis_checkpoint(&checkpoint)?;
            }
        }
        check_binary_blocking_step(self.graph, self.source, self.sink, &execution.result)?;
        let special_arcs = u128::try_from(execution.result.special_arcs.len())
            .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
        self.record_event(
            &execution.result.distances,
            &[],
            EventKind::Special,
            Some(("special-arcs", to_i128(special_arcs)?)),
        )?;
        let component_count = execution
            .result
            .component_of
            .iter()
            .copied()
            .max()
            .map_or(0, |value| value + 1);
        self.record_event(
            &execution.result.distances,
            &[],
            EventKind::Contract,
            Some((
                "components",
                to_i128(
                    u128::try_from(component_count)
                        .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
                )?,
            )),
        )?;
        self.commit_execution(execution)
    }

    fn commit_execution(
        &mut self,
        execution: StepExecution<'graph>,
    ) -> Result<(), GoldbergRaoError> {
        self.state = execution.state;
        increment(&mut self.metrics.update_steps)?;
        self.metrics.augmented_units = self
            .metrics
            .augmented_units
            .checked_add(execution.result.value)
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        let counter = if execution.result.blocking {
            &mut self.metrics.blocking_updates
        } else {
            &mut self.metrics.delta_limited_updates
        };
        increment(counter)?;
        self.record_event(
            &execution.result.distances,
            &execution.last_path,
            EventKind::Augment,
            Some(("delta-flow", to_i128(execution.result.value)?)),
        )?;
        self.record_event(
            &execution.result.distances,
            &[],
            EventKind::Lift,
            Some((
                "routing-paths",
                i128::from(execution.component_routing_paths),
            )),
        )
    }

    fn record_event(
        &mut self,
        distances: &[Option<u64>],
        active_path: &[ResidualArcId],
        kind: EventKind,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), GoldbergRaoError> {
        record(
            self.recorder.as_mut(),
            self.graph,
            &self.state,
            distances,
            active_path,
            None,
            self.metrics,
            kind,
            detail,
        )
    }

    fn record_event_with_focus(
        &mut self,
        distances: &[Option<u64>],
        active_path: &[ResidualArcId],
        focus_arcs: &[ResidualArcId],
        kind: EventKind,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), GoldbergRaoError> {
        record(
            self.recorder.as_mut(),
            self.graph,
            &self.state,
            distances,
            active_path,
            Some(focus_arcs),
            self.metrics,
            kind,
            detail,
        )
    }

    fn record_analysis_checkpoint(
        &mut self,
        checkpoint: &BinaryAnalysisCheckpoint,
    ) -> Result<(), GoldbergRaoError> {
        let Some(recorder) = self.recorder.as_mut() else {
            return Ok(());
        };
        if recorder.event_count() >= GOLDBERG_RAO_MAX_TRACE_EVENTS {
            return Err(GoldbergRaoError::WorkLimit);
        }
        let state = ResidualState::from_flows(self.graph, &checkpoint.flows)?;
        let snapshot = trace_snapshot(
            self.graph,
            &state,
            &checkpoint.distances,
            &checkpoint.focus_arcs,
            checkpoint.metrics,
        );
        recorder.record_transition_with_detail_and_focus(
            checkpoint.kind.goldberg_rao_metadata(),
            &snapshot,
            Some((checkpoint.kind.detail_label(), checkpoint.detail)),
            checkpoint
                .focus_arcs
                .iter()
                .cloned()
                .map(FlowTraceEntityRef::ResidualArc)
                .collect(),
        )?;
        Ok(())
    }

    fn finish(mut self) -> Result<InternalRun, GoldbergRaoError> {
        let flows = self.state.flows().to_vec();
        let certificate = check_max_flow(self.graph, self.source, self.sink, &flows)?;
        self.record_event(
            &[],
            &[],
            EventKind::Optimal,
            Some(("flow-value", certificate.value)),
        )?;
        Ok(InternalRun {
            result: GoldbergRaoResult {
                flows,
                certificate,
                metrics: self.metrics,
            },
            trace: self.recorder.map(FlowTraceRecorder::finish),
        })
    }
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), GoldbergRaoError> {
    if graph.nodes().len() > GOLDBERG_RAO_MAX_NODES || graph.edges().len() > GOLDBERG_RAO_MAX_EDGES
    {
        return Err(GoldbergRaoError::AdmissionLimit);
    }
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
        || graph.edges().iter().any(|edge| edge.lower() != 0)
    {
        return Err(GoldbergRaoError::GraphRequirement);
    }
    Ok(())
}

#[derive(Clone)]
struct BinaryAnalysis {
    distances: Vec<Option<u64>>,
    base_zero_arcs: Vec<ResidualArcId>,
    special_arcs: Vec<ResidualArcId>,
    admissible_arcs: Vec<ResidualArc>,
    zero_admissible_arcs: Vec<ResidualArc>,
    component_of: Vec<usize>,
    components: Vec<Vec<NodeIndex>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinaryInspectionKind {
    InspectInitialCutArc,
    EnumerateResidualArc,
    BuildIncomingArc,
    RelaxDistanceArc,
    ClassifyBinaryLength,
    BuildZeroSccAdjacency,
    TraverseZeroSccReverseArc,
    InspectCanonicalCutArc,
    InspectContractedArc,
    BuildInternalAdjacency,
    TraverseInternalArc,
    ApplyContractedFlow,
    ApplyLiftPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingBinaryCheckpoint {
    kind: BinaryInspectionKind,
    focus_arcs: Vec<ResidualArcId>,
    distances: Vec<Option<u64>>,
    metrics: GoldbergRaoMetrics,
    detail: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BinaryAnalysisCheckpoint {
    kind: BinaryInspectionKind,
    focus_arcs: Vec<ResidualArcId>,
    distances: Vec<Option<u64>>,
    flows: Vec<u64>,
    metrics: GoldbergRaoMetrics,
    detail: i128,
}

#[derive(Debug)]
struct BinaryCheckpointCollector {
    checkpoints: Vec<BinaryAnalysisCheckpoint>,
    pending: Option<PendingBinaryCheckpoint>,
    last_emitted_scan: u128,
}

impl BinaryCheckpointCollector {
    fn new(initial_scan_count: u128) -> Self {
        Self {
            checkpoints: Vec::new(),
            pending: None,
            last_emitted_scan: initial_scan_count,
        }
    }

    fn observe_scan(
        &mut self,
        kind: BinaryInspectionKind,
        arc: &ResidualArcId,
        flows: &[u64],
        distances: &[Option<u64>],
        metrics: GoldbergRaoMetrics,
        detail: i128,
    ) {
        let pending = PendingBinaryCheckpoint {
            kind,
            focus_arcs: vec![arc.clone()],
            distances: distances.to_vec(),
            metrics,
            detail,
        };
        let scan_count = metrics.residual_arc_scans;
        let should_emit = scan_count <= BINARY_TRACE_DENSE_PREFIX
            || scan_count.saturating_sub(self.last_emitted_scan)
                >= BINARY_BLOCKING_TRACE_SCAN_BLOCK_MAX;
        if should_emit {
            self.push_pending(pending, flows);
        } else {
            self.pending = Some(pending);
        }
    }

    fn record_operation(
        &mut self,
        kind: BinaryInspectionKind,
        focus_arcs: Vec<ResidualArcId>,
        flows: &[u64],
        distances: &[Option<u64>],
        metrics: GoldbergRaoMetrics,
        detail: i128,
    ) {
        self.flush(flows);
        self.checkpoints.push(BinaryAnalysisCheckpoint {
            kind,
            focus_arcs,
            distances: distances.to_vec(),
            flows: flows.to_vec(),
            metrics,
            detail,
        });
    }

    fn finish(mut self, flows: &[u64]) -> Vec<BinaryAnalysisCheckpoint> {
        self.flush(flows);
        self.checkpoints
    }

    fn flush(&mut self, flows: &[u64]) {
        if let Some(pending) = self.pending.take() {
            self.push_pending(pending, flows);
        }
    }

    fn push_pending(&mut self, pending: PendingBinaryCheckpoint, flows: &[u64]) {
        self.last_emitted_scan = pending.metrics.residual_arc_scans;
        self.checkpoints.push(BinaryAnalysisCheckpoint {
            kind: pending.kind,
            focus_arcs: pending.focus_arcs,
            distances: pending.distances,
            flows: flows.to_vec(),
            metrics: pending.metrics,
            detail: pending.detail,
        });
        self.pending = None;
    }
}

fn analyze_binary_network(
    state: &ResidualState<'_>,
    sink: NodeIndex,
    delta: u128,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<BinaryAnalysis, GoldbergRaoError> {
    let arcs = residual_arcs(state, metrics, checkpoints.as_deref_mut())?;
    increment(&mut metrics.distance_searches)?;
    let distances = binary_distances(
        state.flows(),
        state.graph().nodes().len(),
        sink,
        &arcs,
        delta,
        metrics,
        checkpoints.as_deref_mut(),
    )?;
    let mut base_zero_arcs = Vec::new();
    let mut special_arcs = Vec::new();
    let mut admissible_arcs = Vec::new();
    let mut zero_admissible_arcs = Vec::new();
    for arc in arcs {
        scan(metrics)?;
        let base_zero = u128::from(arc.capacity) >= triple(delta)?;
        if base_zero {
            base_zero_arcs.push(arc.id.clone());
            increment(&mut metrics.zero_length_arc_observations)?;
        }
        let special = is_special(state, &arc, &distances, delta)?;
        if special {
            special_arcs.push(arc.id.clone());
            increment(&mut metrics.special_arc_observations)?;
        }
        let length = u64::from(!(base_zero || special));
        let admissible = match (distances[arc.from.as_usize()], distances[arc.to.as_usize()]) {
            (Some(from), Some(to)) => from == to.saturating_add(length),
            _ => false,
        };
        record_binary_checkpoint(
            checkpoints.as_deref_mut(),
            BinaryInspectionKind::ClassifyBinaryLength,
            &arc.id,
            state.flows(),
            &distances,
            *metrics,
            i128::from(length),
        );
        if admissible {
            if length == 0 {
                zero_admissible_arcs.push(arc.clone());
            }
            admissible_arcs.push(arc);
        }
    }
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        checkpoints.flush(state.flows());
    }
    base_zero_arcs.sort_unstable();
    special_arcs.sort_unstable();
    admissible_arcs.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    zero_admissible_arcs.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    let (component_of, components) = strongly_connected_components(
        state.graph().nodes().len(),
        &zero_admissible_arcs,
        state.flows(),
        &distances,
        metrics,
        checkpoints,
    )?;
    for component in &components {
        if component.len() > 1 {
            increment(&mut metrics.nontrivial_contractions)?;
        }
    }
    Ok(BinaryAnalysis {
        distances,
        base_zero_arcs,
        special_arcs,
        admissible_arcs,
        zero_admissible_arcs,
        component_of,
        components,
    })
}

fn residual_arcs(
    state: &ResidualState<'_>,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<Vec<ResidualArc>, GoldbergRaoError> {
    let mut arcs = Vec::new();
    for node in 0..state.graph().nodes().len() {
        let node = NodeIndex::try_from_usize(node).ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        let outgoing = state.outgoing_arcs(node);
        for arc in outgoing {
            scan(metrics)?;
            record_binary_checkpoint(
                checkpoints.as_deref_mut(),
                BinaryInspectionKind::EnumerateResidualArc,
                &arc.id,
                state.flows(),
                &[],
                *metrics,
                i128::from(arc.capacity),
            );
            arcs.push(arc);
        }
    }
    if let Some(checkpoints) = checkpoints {
        checkpoints.flush(state.flows());
    }
    arcs.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    Ok(arcs)
}

fn binary_distances(
    flows: &[u64],
    node_count: usize,
    sink: NodeIndex,
    arcs: &[ResidualArc],
    delta: u128,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<Vec<Option<u64>>, GoldbergRaoError> {
    let mut incoming = vec![Vec::<(NodeIndex, u64, ResidualArcId)>::new(); node_count];
    let mut distance = vec![None; node_count];
    distance[sink.as_usize()] = Some(0_u64);
    for arc in arcs {
        scan(metrics)?;
        let length = u64::from(u128::from(arc.capacity) < triple(delta)?);
        incoming[arc.to.as_usize()].push((arc.from, length, arc.id.clone()));
        record_binary_checkpoint(
            checkpoints.as_deref_mut(),
            BinaryInspectionKind::BuildIncomingArc,
            &arc.id,
            flows,
            &distance,
            *metrics,
            i128::from(length),
        );
    }
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        checkpoints.flush(flows);
    }
    for neighbors in &mut incoming {
        neighbors.sort_unstable();
    }
    let mut settled = vec![false; node_count];
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0_u64, sink.as_usize())));
    while let Some(Reverse((current_distance, node))) = heap.pop() {
        if settled[node] || distance[node] != Some(current_distance) {
            continue;
        }
        settled[node] = true;
        for (predecessor, length, arc) in &incoming[node] {
            scan(metrics)?;
            let candidate = current_distance
                .checked_add(*length)
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            let slot = &mut distance[predecessor.as_usize()];
            if slot.is_none_or(|old| candidate < old) {
                *slot = Some(candidate);
                heap.push(Reverse((candidate, predecessor.as_usize())));
            }
            record_binary_checkpoint(
                checkpoints.as_deref_mut(),
                BinaryInspectionKind::RelaxDistanceArc,
                arc,
                flows,
                &distance,
                *metrics,
                i128::from(candidate),
            );
        }
    }
    if let Some(checkpoints) = checkpoints {
        checkpoints.flush(flows);
    }
    Ok(distance)
}

fn record_binary_checkpoint(
    checkpoints: Option<&mut BinaryCheckpointCollector>,
    kind: BinaryInspectionKind,
    arc: &ResidualArcId,
    flows: &[u64],
    distances: &[Option<u64>],
    metrics: GoldbergRaoMetrics,
    detail: i128,
) {
    if let Some(checkpoints) = checkpoints {
        checkpoints.observe_scan(kind, arc, flows, distances, metrics, detail);
    }
}

fn is_special(
    state: &ResidualState<'_>,
    arc: &ResidualArc,
    distances: &[Option<u64>],
    delta: u128,
) -> Result<bool, GoldbergRaoError> {
    let capacity = u128::from(arc.capacity);
    if capacity < double(delta)?
        || capacity >= triple(delta)?
        || distances[arc.from.as_usize()] != distances[arc.to.as_usize()]
    {
        return Ok(false);
    }
    let reverse = ResidualArcId::new(
        arc.id.original_edge().clone(),
        match arc.id.direction() {
            ResidualDirection::Forward => ResidualDirection::Reverse,
            ResidualDirection::Reverse => ResidualDirection::Forward,
        },
    );
    let threshold = triple(delta)?;
    Ok(state
        .arc(&reverse)
        .is_some_and(|reverse_arc| u128::from(reverse_arc.capacity) >= threshold))
}

#[derive(Clone)]
struct CanonicalCut {
    capacity: u128,
    crossing_arcs: Vec<ResidualArcId>,
}

fn minimum_canonical_cut(
    state: &ResidualState<'_>,
    source: NodeIndex,
    distances: &[Option<u64>],
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<CanonicalCut, GoldbergRaoError> {
    let source_distance = distances[source.as_usize()].ok_or(GoldbergRaoError::Invariant)?;
    if source_distance == 0 {
        return Err(GoldbergRaoError::Invariant);
    }
    let arcs = residual_arcs(state, metrics, checkpoints.as_deref_mut())?;
    let mut best: Option<CanonicalCut> = None;
    for level in 1..=source_distance {
        increment(&mut metrics.canonical_cut_evaluations)?;
        let mut capacity = 0_u128;
        let mut crossing_arcs = Vec::new();
        for arc in &arcs {
            scan(metrics)?;
            record_binary_checkpoint(
                checkpoints.as_deref_mut(),
                BinaryInspectionKind::InspectCanonicalCutArc,
                &arc.id,
                state.flows(),
                distances,
                *metrics,
                i128::from(level),
            );
            let from_source_side = distances[arc.from.as_usize()].is_none_or(|d| d >= level);
            let to_source_side = distances[arc.to.as_usize()].is_none_or(|d| d >= level);
            if from_source_side && !to_source_side {
                capacity = capacity
                    .checked_add(u128::from(arc.capacity))
                    .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
                crossing_arcs.push(arc.id.clone());
            }
        }
        if let Some(checkpoints) = checkpoints.as_deref_mut() {
            checkpoints.flush(state.flows());
        }
        if best.as_ref().is_none_or(|old| capacity < old.capacity) {
            best = Some(CanonicalCut {
                capacity,
                crossing_arcs,
            });
        }
    }
    best.ok_or(GoldbergRaoError::Invariant)
}

struct StepExecution<'graph> {
    state: ResidualState<'graph>,
    result: BinaryBlockingStepResult,
    last_path: Vec<ResidualArcId>,
    component_routing_paths: u64,
}

fn execute_binary_step<'graph>(
    state: &ResidualState<'graph>,
    source: NodeIndex,
    sink: NodeIndex,
    upper_bound: u128,
    delta: u128,
    metrics: &mut GoldbergRaoMetrics,
    checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<StepExecution<'graph>, GoldbergRaoError> {
    let mut checkpoints = checkpoints;
    let analysis = analyze_binary_network(state, sink, delta, metrics, checkpoints.as_deref_mut())?;
    execute_binary_step_from_analysis(
        state,
        source,
        sink,
        upper_bound,
        delta,
        analysis,
        metrics,
        checkpoints,
    )
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "the binary primitive keeps phase invariants and the optional source recorder explicit"
)]
fn execute_binary_step_from_analysis<'graph>(
    state: &ResidualState<'graph>,
    source: NodeIndex,
    sink: NodeIndex,
    upper_bound: u128,
    delta: u128,
    analysis: BinaryAnalysis,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<StepExecution<'graph>, GoldbergRaoError> {
    let source_component = analysis.component_of[source.as_usize()];
    let sink_component = analysis.component_of[sink.as_usize()];
    let mut operations = Vec::new();
    let (value, last_path) = if source_component == sink_component {
        let path = find_internal_path(
            state.flows(),
            source,
            sink,
            source_component,
            &analysis.component_of,
            &analysis.zero_admissible_arcs,
            metrics,
            checkpoints.as_deref_mut(),
            &analysis.distances,
        )?
        .ok_or(GoldbergRaoError::Invariant)?;
        let bottleneck = path
            .iter()
            .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
            .min()
            .ok_or(GoldbergRaoError::Invariant)?;
        let amount = u64::try_from(delta.min(u128::from(bottleneck)))
            .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
        for id in &path {
            operations.push(BinaryBlockingAugmentation {
                arc: id.clone(),
                amount,
            });
        }
        (u128::from(amount), path)
    } else {
        let contracted = contracted_blocking_flow(
            state.flows(),
            analysis.components.len(),
            source_component,
            sink_component,
            &analysis.admissible_arcs,
            &analysis.component_of,
            delta,
            metrics,
            checkpoints.as_deref_mut(),
            &analysis.distances,
        )?;
        operations.extend(contracted.operations);
        (contracted.value, contracted.last_path)
    };
    if value > delta {
        return Err(GoldbergRaoError::Invariant);
    }
    let mut next = state.clone();
    let cross_operation_count = operations.len();
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        checkpoints.flush(state.flows());
    }
    for operation in &operations {
        next.augment(std::slice::from_ref(&operation.arc), operation.amount)?;
    }
    if let Some(checkpoints) = checkpoints.as_deref_mut()
        && !operations.is_empty()
    {
        checkpoints.record_operation(
            BinaryInspectionKind::ApplyContractedFlow,
            operations
                .iter()
                .map(|operation| operation.arc.clone())
                .collect(),
            next.flows(),
            &analysis.distances,
            *metrics,
            i128::try_from(value).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
        );
    }
    let routing_paths = lift_component_balances(
        state.graph(),
        &mut next,
        source,
        sink,
        value,
        &analysis,
        &mut operations,
        metrics,
        checkpoints,
    )?;
    if operations.len() < cross_operation_count {
        return Err(GoldbergRaoError::Invariant);
    }
    let result = BinaryBlockingStepResult {
        initial_flows: state.flows().to_vec(),
        flows: next.flows().to_vec(),
        upper_bound,
        delta,
        value,
        blocking: value < delta,
        distances: analysis.distances,
        base_zero_arcs: analysis.base_zero_arcs,
        special_arcs: analysis.special_arcs,
        admissible_arcs: analysis
            .admissible_arcs
            .iter()
            .map(|arc| arc.id.clone())
            .collect(),
        zero_admissible_arcs: analysis
            .zero_admissible_arcs
            .iter()
            .map(|arc| arc.id.clone())
            .collect(),
        component_of: analysis.component_of,
        augmentation: operations,
        metrics: GoldbergRaoMetrics::default(),
    };
    Ok(StepExecution {
        state: next,
        result,
        last_path,
        component_routing_paths: routing_paths,
    })
}

struct ContractedFlow {
    value: u128,
    operations: Vec<BinaryBlockingAugmentation>,
    last_path: Vec<ResidualArcId>,
}

#[derive(Clone)]
struct ContractedArc {
    id: ResidualArcId,
    from: usize,
    to: usize,
    capacity: u64,
}

#[allow(
    clippy::too_many_arguments,
    reason = "contracted routing consumes one fully checked analysis boundary without hidden ambient state"
)]
fn contracted_blocking_flow(
    flows: &[u64],
    component_count: usize,
    source: usize,
    sink: usize,
    admissible_arcs: &[ResidualArc],
    component_of: &[usize],
    delta: u128,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
    distances: &[Option<u64>],
) -> Result<ContractedFlow, GoldbergRaoError> {
    let mut arcs = admissible_arcs
        .iter()
        .filter_map(|arc| {
            let from = component_of[arc.from.as_usize()];
            let to = component_of[arc.to.as_usize()];
            (from != to).then(|| ContractedArc {
                id: arc.id.clone(),
                from,
                to,
                capacity: arc.capacity,
            })
        })
        .collect::<Vec<_>>();
    arcs.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    let mut outgoing = vec![Vec::<usize>::new(); component_count];
    for (index, arc) in arcs.iter().enumerate() {
        outgoing[arc.from].push(index);
    }
    let mut total = 0_u128;
    let mut operations = Vec::new();
    let mut last_path = Vec::new();
    while total < delta {
        let mut parent = vec![None::<usize>; component_count];
        let mut seen = vec![false; component_count];
        let mut queue = VecDeque::new();
        seen[source] = true;
        queue.push_back(source);
        while let Some(node) = queue.pop_front() {
            for &arc_index in &outgoing[node] {
                scan(metrics)?;
                let arc = &arcs[arc_index];
                record_binary_checkpoint(
                    checkpoints.as_deref_mut(),
                    BinaryInspectionKind::InspectContractedArc,
                    &arc.id,
                    flows,
                    distances,
                    *metrics,
                    i128::from(arc.capacity),
                );
                if arc.capacity > 0 && !seen[arc.to] {
                    seen[arc.to] = true;
                    parent[arc.to] = Some(arc_index);
                    queue.push_back(arc.to);
                }
            }
        }
        if let Some(checkpoints) = checkpoints.as_deref_mut() {
            checkpoints.flush(flows);
        }
        if !seen[sink] {
            break;
        }
        let mut path_indices = Vec::new();
        let mut node = sink;
        while node != source {
            let arc_index = parent[node].ok_or(GoldbergRaoError::Invariant)?;
            path_indices.push(arc_index);
            node = arcs[arc_index].from;
        }
        path_indices.reverse();
        let remaining = delta - total;
        let bottleneck = path_indices
            .iter()
            .map(|&index| u128::from(arcs[index].capacity))
            .min()
            .ok_or(GoldbergRaoError::Invariant)?;
        let amount = u64::try_from(remaining.min(bottleneck))
            .map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
        last_path.clear();
        for &index in &path_indices {
            arcs[index].capacity -= amount;
            last_path.push(arcs[index].id.clone());
            operations.push(BinaryBlockingAugmentation {
                arc: arcs[index].id.clone(),
                amount,
            });
        }
        total = total
            .checked_add(u128::from(amount))
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        increment(&mut metrics.contracted_augmentations)?;
        bump_transition(metrics)?;
    }
    Ok(ContractedFlow {
        value: total,
        operations,
        last_path,
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::too_many_lines,
    reason = "component lifting is one atomic conservation repair with interleaved source checkpoints"
)]
fn lift_component_balances(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    value: u128,
    analysis: &BinaryAnalysis,
    operations: &mut Vec<BinaryBlockingAugmentation>,
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<u64, GoldbergRaoError> {
    let before =
        ResidualState::from_flows(graph, &operations_initial_flows(graph, state, operations)?)?;
    let mut balance = flow_difference_divergence(graph, before.flows(), state.flows())?;
    for value in &mut balance {
        *value = value
            .checked_neg()
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    }
    let value_i128 = to_i128(value)?;
    balance[source.as_usize()] = balance[source.as_usize()]
        .checked_add(value_i128)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    balance[sink.as_usize()] = balance[sink.as_usize()]
        .checked_sub(value_i128)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    let mut routing_paths = 0_u64;
    for (component, nodes) in analysis.components.iter().enumerate() {
        let root = if nodes.contains(&source) {
            source
        } else if nodes.contains(&sink) {
            sink
        } else {
            *nodes.first().ok_or(GoldbergRaoError::Invariant)?
        };
        for &node in nodes {
            let amount = balance[node.as_usize()];
            if node == root || amount <= 0 {
                continue;
            }
            let path = find_internal_path(
                state.flows(),
                node,
                root,
                component,
                &analysis.component_of,
                &analysis.zero_admissible_arcs,
                metrics,
                checkpoints.as_deref_mut(),
                &analysis.distances,
            )?
            .ok_or(GoldbergRaoError::Invariant)?;
            if let Some(checkpoints) = checkpoints.as_deref_mut() {
                checkpoints.flush(state.flows());
            }
            apply_routing_path(state, &path, amount, operations)?;
            if let Some(checkpoints) = checkpoints.as_deref_mut() {
                checkpoints.record_operation(
                    BinaryInspectionKind::ApplyLiftPath,
                    path.clone(),
                    state.flows(),
                    &analysis.distances,
                    *metrics,
                    amount,
                );
            }
            balance[node.as_usize()] -= amount;
            balance[root.as_usize()] = balance[root.as_usize()]
                .checked_add(amount)
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            increment(&mut routing_paths)?;
            increment(&mut metrics.component_routing_paths)?;
        }
        for &node in nodes {
            let deficit = balance[node.as_usize()];
            if node == root || deficit >= 0 {
                continue;
            }
            let amount = deficit
                .checked_neg()
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            let path = find_internal_path(
                state.flows(),
                root,
                node,
                component,
                &analysis.component_of,
                &analysis.zero_admissible_arcs,
                metrics,
                checkpoints.as_deref_mut(),
                &analysis.distances,
            )?
            .ok_or(GoldbergRaoError::Invariant)?;
            if let Some(checkpoints) = checkpoints.as_deref_mut() {
                checkpoints.flush(state.flows());
            }
            apply_routing_path(state, &path, amount, operations)?;
            if let Some(checkpoints) = checkpoints.as_deref_mut() {
                checkpoints.record_operation(
                    BinaryInspectionKind::ApplyLiftPath,
                    path.clone(),
                    state.flows(),
                    &analysis.distances,
                    *metrics,
                    amount,
                );
            }
            balance[root.as_usize()] = balance[root.as_usize()]
                .checked_sub(amount)
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            balance[node.as_usize()] += amount;
            increment(&mut routing_paths)?;
            increment(&mut metrics.component_routing_paths)?;
        }
        if nodes.iter().any(|node| balance[node.as_usize()] != 0) {
            return Err(GoldbergRaoError::Invariant);
        }
    }
    validate_feasible_flow(graph, source, sink, state.flows())?;
    Ok(routing_paths)
}

fn operations_initial_flows(
    graph: &FlowNetwork,
    final_state: &ResidualState<'_>,
    operations: &[BinaryBlockingAugmentation],
) -> Result<Vec<u64>, GoldbergRaoError> {
    let mut flows = final_state.flows().to_vec();
    for operation in operations.iter().rev() {
        let edge = graph
            .edge_index(operation.arc.original_edge())
            .ok_or(GoldbergRaoError::Invariant)?;
        let slot = flows
            .get_mut(edge.as_usize())
            .ok_or(GoldbergRaoError::Invariant)?;
        match operation.arc.direction() {
            ResidualDirection::Forward => {
                *slot = slot
                    .checked_sub(operation.amount)
                    .ok_or(GoldbergRaoError::Invariant)?;
            }
            ResidualDirection::Reverse => {
                *slot = slot
                    .checked_add(operation.amount)
                    .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            }
        }
    }
    Ok(flows)
}

fn flow_difference_divergence(
    graph: &FlowNetwork,
    before: &[u64],
    after: &[u64],
) -> Result<Vec<i128>, GoldbergRaoError> {
    if before.len() != graph.edges().len() || after.len() != graph.edges().len() {
        return Err(GoldbergRaoError::Invariant);
    }
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for ((edge, &old), &new) in graph.edges().iter().zip(before).zip(after) {
        let change = i128::from(new)
            .checked_sub(i128::from(old))
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        divergence[edge.from().as_usize()] = divergence[edge.from().as_usize()]
            .checked_add(change)
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        divergence[edge.to().as_usize()] = divergence[edge.to().as_usize()]
            .checked_sub(change)
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    }
    Ok(divergence)
}

fn apply_routing_path(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    amount: i128,
    operations: &mut Vec<BinaryBlockingAugmentation>,
) -> Result<(), GoldbergRaoError> {
    let amount = u64::try_from(amount).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?;
    if amount == 0 || path.is_empty() {
        return Err(GoldbergRaoError::Invariant);
    }
    state.augment(path, amount)?;
    operations.extend(
        path.iter()
            .cloned()
            .map(|arc| BinaryBlockingAugmentation { arc, amount }),
    );
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "the bounded component search keeps topology, source state, and trace state explicit"
)]
fn find_internal_path(
    flows: &[u64],
    source: NodeIndex,
    sink: NodeIndex,
    component: usize,
    component_of: &[usize],
    arcs: &[ResidualArc],
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
    distances: &[Option<u64>],
) -> Result<Option<Vec<ResidualArcId>>, GoldbergRaoError> {
    if source == sink {
        return Ok(Some(Vec::new()));
    }
    let node_count = component_of.len();
    let mut outgoing = vec![Vec::<&ResidualArc>::new(); node_count];
    for arc in arcs {
        scan(metrics)?;
        record_binary_checkpoint(
            checkpoints.as_deref_mut(),
            BinaryInspectionKind::BuildInternalAdjacency,
            &arc.id,
            flows,
            distances,
            *metrics,
            i128::try_from(component).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
        );
        if component_of[arc.from.as_usize()] == component
            && component_of[arc.to.as_usize()] == component
        {
            outgoing[arc.from.as_usize()].push(arc);
        }
    }
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        checkpoints.flush(flows);
    }
    for list in &mut outgoing {
        list.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    }
    let mut parent = vec![None::<ResidualArcId>; node_count];
    let mut seen = vec![false; node_count];
    let mut queue = VecDeque::new();
    seen[source.as_usize()] = true;
    queue.push_back(source);
    while let Some(node) = queue.pop_front() {
        for arc in &outgoing[node.as_usize()] {
            scan(metrics)?;
            record_binary_checkpoint(
                checkpoints.as_deref_mut(),
                BinaryInspectionKind::TraverseInternalArc,
                &arc.id,
                flows,
                distances,
                *metrics,
                i128::try_from(component).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
            );
            if !seen[arc.to.as_usize()] {
                seen[arc.to.as_usize()] = true;
                parent[arc.to.as_usize()] = Some(arc.id.clone());
                queue.push_back(arc.to);
            }
        }
    }
    if let Some(checkpoints) = checkpoints {
        checkpoints.flush(flows);
    }
    if !seen[sink.as_usize()] {
        return Ok(None);
    }
    let mut path = Vec::new();
    let mut node = sink;
    while node != source {
        let id = parent[node.as_usize()]
            .clone()
            .ok_or(GoldbergRaoError::Invariant)?;
        let arc = arcs
            .iter()
            .find(|arc| arc.id == id)
            .ok_or(GoldbergRaoError::Invariant)?;
        path.push(id);
        node = arc.from;
    }
    path.reverse();
    Ok(Some(path))
}

fn strongly_connected_components(
    node_count: usize,
    arcs: &[ResidualArc],
    flows: &[u64],
    distances: &[Option<u64>],
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<(Vec<usize>, Vec<Vec<NodeIndex>>), GoldbergRaoError> {
    let mut outgoing = vec![Vec::<(usize, ResidualArcId)>::new(); node_count];
    let mut incoming = vec![Vec::<(usize, ResidualArcId)>::new(); node_count];
    for arc in arcs {
        scan(metrics)?;
        record_binary_checkpoint(
            checkpoints.as_deref_mut(),
            BinaryInspectionKind::BuildZeroSccAdjacency,
            &arc.id,
            flows,
            distances,
            *metrics,
            i128::try_from(arc.to.as_usize()).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
        );
        outgoing[arc.from.as_usize()].push((arc.to.as_usize(), arc.id.clone()));
        incoming[arc.to.as_usize()].push((arc.from.as_usize(), arc.id.clone()));
    }
    if let Some(checkpoints) = checkpoints.as_deref_mut() {
        checkpoints.flush(flows);
    }
    for list in outgoing.iter_mut().chain(&mut incoming) {
        list.sort_unstable();
        list.dedup();
    }
    let mut visited = vec![false; node_count];
    let mut order = Vec::with_capacity(node_count);
    for start in 0..node_count {
        if visited[start] {
            continue;
        }
        let mut stack = vec![(start, 0_usize)];
        visited[start] = true;
        while let Some((node, next)) = stack.last_mut() {
            if *next < outgoing[*node].len() {
                let neighbor = outgoing[*node][*next].0;
                *next += 1;
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push((neighbor, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut raw_components = Vec::<Vec<usize>>::new();
    let mut assigned = vec![false; node_count];
    while let Some(start) = order.pop() {
        if assigned[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        assigned[start] = true;
        while let Some(node) = stack.pop() {
            component.push(node);
            for (neighbor, arc) in incoming[node].iter().rev() {
                scan(metrics)?;
                record_binary_checkpoint(
                    checkpoints.as_deref_mut(),
                    BinaryInspectionKind::TraverseZeroSccReverseArc,
                    arc,
                    flows,
                    distances,
                    *metrics,
                    i128::try_from(*neighbor).map_err(|_| GoldbergRaoError::ArithmeticOverflow)?,
                );
                if !assigned[*neighbor] {
                    assigned[*neighbor] = true;
                    stack.push(*neighbor);
                }
            }
        }
        component.sort_unstable();
        raw_components.push(component);
    }
    if let Some(checkpoints) = checkpoints {
        checkpoints.flush(flows);
    }
    raw_components.sort_unstable_by_key(|component| component[0]);
    let mut component_of = vec![usize::MAX; node_count];
    let mut components = Vec::with_capacity(raw_components.len());
    for (ordinal, component) in raw_components.into_iter().enumerate() {
        let mut nodes = Vec::with_capacity(component.len());
        for node in component {
            component_of[node] = ordinal;
            nodes
                .push(NodeIndex::try_from_usize(node).ok_or(GoldbergRaoError::ArithmeticOverflow)?);
        }
        components.push(nodes);
    }
    if component_of.contains(&usize::MAX) {
        return Err(GoldbergRaoError::Invariant);
    }
    Ok((component_of, components))
}

fn admissible_path_exists_after_usage(
    node_count: usize,
    source: NodeIndex,
    sink: NodeIndex,
    admissible: &[ResidualArc],
    used: &BTreeMap<ResidualArcId, u128>,
) -> Result<bool, GoldbergRaoError> {
    let mut outgoing = vec![Vec::<NodeIndex>::new(); node_count];
    for arc in admissible {
        let remaining = u128::from(arc.capacity)
            .checked_sub(used.get(&arc.id).copied().unwrap_or(0))
            .ok_or(GoldbergRaoError::Invariant)?;
        if remaining > 0 {
            outgoing[arc.from.as_usize()].push(arc.to);
        }
    }
    let mut seen = vec![false; node_count];
    let mut queue = VecDeque::new();
    seen[source.as_usize()] = true;
    queue.push_back(source);
    while let Some(node) = queue.pop_front() {
        for &next in &outgoing[node.as_usize()] {
            if !seen[next.as_usize()] {
                seen[next.as_usize()] = true;
                queue.push_back(next);
            }
        }
    }
    Ok(seen[sink.as_usize()])
}

fn validate_feasible_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    flows: &[u64],
) -> Result<(), GoldbergRaoError> {
    let state = ResidualState::from_flows(graph, flows)?;
    let mut divergence = vec![0_i128; graph.nodes().len()];
    for (edge, &flow) in graph.edges().iter().zip(state.flows()) {
        divergence[edge.from().as_usize()] = divergence[edge.from().as_usize()]
            .checked_add(i128::from(flow))
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        divergence[edge.to().as_usize()] = divergence[edge.to().as_usize()]
            .checked_sub(i128::from(flow))
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    }
    if divergence
        .iter()
        .enumerate()
        .any(|(node, &value)| node != source.as_usize() && node != sink.as_usize() && value != 0)
        || divergence[source.as_usize()] != -divergence[sink.as_usize()]
        || divergence[source.as_usize()] < 0
    {
        return Err(GoldbergRaoError::Invariant);
    }
    Ok(())
}

fn flow_value(
    graph: &FlowNetwork,
    source: NodeIndex,
    flows: &[u64],
) -> Result<u128, GoldbergRaoError> {
    let mut value = 0_i128;
    for (edge, &flow) in graph.edges().iter().zip(flows) {
        if edge.from() == source {
            value = value
                .checked_add(i128::from(flow))
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        }
        if edge.to() == source {
            value = value
                .checked_sub(i128::from(flow))
                .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
        }
    }
    u128::try_from(value).map_err(|_| GoldbergRaoError::Invariant)
}

fn residual_cut_capacity(
    state: &ResidualState<'_>,
    source_side: &[NodeIndex],
    metrics: &mut GoldbergRaoMetrics,
    mut checkpoints: Option<&mut BinaryCheckpointCollector>,
) -> Result<u128, GoldbergRaoError> {
    let side = source_side
        .iter()
        .map(|node| node.as_usize())
        .collect::<BTreeSet<_>>();
    let mut capacity = 0_u128;
    for &node in source_side {
        for arc in state.outgoing_arcs(node) {
            scan(metrics)?;
            record_binary_checkpoint(
                checkpoints.as_deref_mut(),
                BinaryInspectionKind::InspectInitialCutArc,
                &arc.id,
                state.flows(),
                &[],
                *metrics,
                i128::from(arc.capacity),
            );
            if !side.contains(&arc.to.as_usize()) {
                capacity = capacity
                    .checked_add(u128::from(arc.capacity))
                    .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
            }
        }
    }
    if let Some(checkpoints) = checkpoints {
        checkpoints.flush(state.flows());
    }
    Ok(capacity)
}

fn phase_delta(
    upper_bound: u128,
    node_count: usize,
    edge_count: usize,
) -> Result<u128, GoldbergRaoError> {
    let sqrt_m = ceil_sqrt(edge_count.max(1) as u128);
    let n = node_count.max(1) as u128;
    let n_two_thirds = ceil_cuberoot(
        n.checked_mul(n)
            .ok_or(GoldbergRaoError::ArithmeticOverflow)?,
    );
    // This is the integral ceiling form of min(F/sqrt(m), F/n^(2/3))
    // printed immediately before Lemma 6.3 in the source paper.
    let denominator = sqrt_m.max(n_two_thirds).max(1);
    ceil_div(upper_bound, denominator)
}

fn ceil_div(numerator: u128, denominator: u128) -> Result<u128, GoldbergRaoError> {
    if denominator == 0 {
        return Err(GoldbergRaoError::ArithmeticOverflow);
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    Ok(quotient + u128::from(remainder != 0))
}

fn ceil_sqrt(value: u128) -> u128 {
    let mut low = 0_u128;
    let mut high = value;
    while low < high {
        let middle = low + (high - low) / 2;
        if middle == 0 {
            low = 1;
            continue;
        }
        let quotient = value / middle;
        let required = quotient + u128::from(!value.is_multiple_of(middle));
        if middle >= required {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

fn ceil_cuberoot(value: u128) -> u128 {
    let mut low = 0_u128;
    let mut high = value;
    while low < high {
        let middle = low + (high - low) / 2;
        let square = middle.checked_mul(middle);
        let cube = square.and_then(|square| square.checked_mul(middle));
        if cube.is_some_and(|cube| cube >= value) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

fn double(value: u128) -> Result<u128, GoldbergRaoError> {
    value
        .checked_mul(2)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)
}

fn triple(value: u128) -> Result<u128, GoldbergRaoError> {
    value
        .checked_mul(3)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)
}

fn to_i128(value: u128) -> Result<i128, GoldbergRaoError> {
    i128::try_from(value).map_err(|_| GoldbergRaoError::ArithmeticOverflow)
}

fn increment(value: &mut u64) -> Result<(), GoldbergRaoError> {
    *value = value
        .checked_add(1)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    Ok(())
}

fn add_scans(metrics: &mut GoldbergRaoMetrics, amount: usize) -> Result<(), GoldbergRaoError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(amount as u128)
        .ok_or(GoldbergRaoError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > GOLDBERG_RAO_MAX_RESIDUAL_ARC_SCANS {
        return Err(GoldbergRaoError::WorkLimit);
    }
    Ok(())
}

fn scan(metrics: &mut GoldbergRaoMetrics) -> Result<(), GoldbergRaoError> {
    add_scans(metrics, 1)
}

fn bump_transition(metrics: &mut GoldbergRaoMetrics) -> Result<(), GoldbergRaoError> {
    increment(&mut metrics.state_transitions)?;
    if metrics.state_transitions > GOLDBERG_RAO_MAX_STATE_TRANSITIONS {
        return Err(GoldbergRaoError::WorkLimit);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum EventKind {
    Initialize,
    Phase,
    Lengths,
    CanonicalCut,
    Special,
    Contract,
    Augment,
    Lift,
    UpdateCut,
    Optimal,
}

#[derive(Clone, Copy)]
enum BinaryPrimitiveEventKind {
    EnumerateResidualArc,
    BuildIncomingArc,
    RelaxDistanceArc,
    Analyze,
    InspectArc,
    Contract,
    Complete,
}

impl BinaryPrimitiveEventKind {
    const fn metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::EnumerateResidualArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:enumerate-positive-residual-arc",
            },
            Self::BuildIncomingArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.build-reverse-zero-one-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:insert-reverse-zero-one-arc",
            },
            Self::RelaxDistanceArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.relax-binary-distance",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:relax-reverse-zero-one-distance",
            },
            Self::Analyze => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.analyze-binary-network",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "binary-blocking-flow:classify-lengths-and-compute-distances",
            },
            Self::InspectArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-binary-length",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:inspect-one-positive-residual-arc",
            },
            Self::Contract => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.contract-zero-scc",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "binary-blocking-flow:contract-zero-length-admissible-components",
            },
            Self::Complete => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.complete-primitive",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "binary-blocking-flow:augment-contract-and-lift-atomically",
            },
        }
    }
}

impl BinaryInspectionKind {
    const fn metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::InspectInitialCutArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-initial-cut-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:inspect-source-cut-residual-arc",
            },
            Self::EnumerateResidualArc => BinaryPrimitiveEventKind::EnumerateResidualArc.metadata(),
            Self::BuildIncomingArc => BinaryPrimitiveEventKind::BuildIncomingArc.metadata(),
            Self::RelaxDistanceArc => BinaryPrimitiveEventKind::RelaxDistanceArc.metadata(),
            Self::ClassifyBinaryLength => BinaryPrimitiveEventKind::InspectArc.metadata(),
            Self::BuildZeroSccAdjacency => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.build-zero-scc-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:insert-zero-length-scc-arc",
            },
            Self::TraverseZeroSccReverseArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-zero-scc-reverse-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:traverse-zero-length-reverse-arc",
            },
            Self::InspectCanonicalCutArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-canonical-cut-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:inspect-distance-cut-arc",
            },
            Self::InspectContractedArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-contracted-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:inspect-contracted-admissible-arc",
            },
            Self::BuildInternalAdjacency => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.build-lift-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:insert-component-lift-arc",
            },
            Self::TraverseInternalArc => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.inspect-lift-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "binary-blocking-flow:inspect-component-lift-arc",
            },
            Self::ApplyContractedFlow => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.apply-contracted-flow",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "binary-blocking-flow:apply-contracted-flow-to-original-arcs",
            },
            Self::ApplyLiftPath => FlowTraceEventMetadata {
                catalog_id: "binary-blocking-flow.apply-lift-path",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "binary-blocking-flow:route-one-component-balance",
            },
        }
    }

    const fn detail_label(self) -> &'static str {
        match self {
            Self::InspectInitialCutArc => "initial-cut-capacity",
            Self::EnumerateResidualArc => "residual-capacity",
            Self::BuildIncomingArc => "base-binary-length",
            Self::RelaxDistanceArc => "candidate-distance",
            Self::ClassifyBinaryLength => "binary-length",
            Self::BuildZeroSccAdjacency => "scc-adjacency-target",
            Self::TraverseZeroSccReverseArc => "scc-reverse-target",
            Self::InspectCanonicalCutArc => "canonical-cut-level",
            Self::InspectContractedArc => "contracted-residual-capacity",
            Self::BuildInternalAdjacency | Self::TraverseInternalArc => "lift-component",
            Self::ApplyContractedFlow => "contracted-flow",
            Self::ApplyLiftPath => "lifted-flow",
        }
    }

    const fn goldberg_rao_metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::InspectInitialCutArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-initial-cut-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:inspect-source-cut-residual-arc",
            },
            Self::EnumerateResidualArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:enumerate-positive-residual-arc",
            },
            Self::BuildIncomingArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.build-reverse-zero-one-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:insert-reverse-zero-one-arc",
            },
            Self::RelaxDistanceArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.relax-binary-distance",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:relax-reverse-zero-one-distance",
            },
            Self::ClassifyBinaryLength => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-binary-length",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:inspect-one-positive-residual-arc",
            },
            Self::BuildZeroSccAdjacency => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.build-zero-scc-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:insert-zero-length-scc-arc",
            },
            Self::TraverseZeroSccReverseArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-zero-scc-reverse-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:traverse-zero-length-reverse-arc",
            },
            Self::InspectCanonicalCutArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-canonical-cut-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:inspect-distance-cut-arc",
            },
            Self::InspectContractedArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-contracted-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:inspect-contracted-admissible-arc",
            },
            Self::BuildInternalAdjacency => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.build-lift-adjacency",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:insert-component-lift-arc",
            },
            Self::TraverseInternalArc => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.inspect-lift-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "goldberg-rao:inspect-component-lift-arc",
            },
            Self::ApplyContractedFlow => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.apply-contracted-flow",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:apply-contracted-flow-to-original-arcs",
            },
            Self::ApplyLiftPath => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.apply-lift-path",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:route-one-component-balance",
            },
        }
    }
}

impl EventKind {
    const fn metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::Initialize => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.initialize-cut-gap",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "goldberg-rao:initialize-source-cut-upper-bound",
            },
            Self::Phase => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.start-gap-phase",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "goldberg-rao:set-integral-delta-from-gap",
            },
            Self::Lengths => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.binary-length-distance",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:length-zero-if-residual-at-least-three-delta",
            },
            Self::CanonicalCut => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.minimum-canonical-cut",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:evaluate-distance-cuts",
            },
            Self::Special => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.mark-special-arcs",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:lower-special-arc-length-to-zero",
            },
            Self::Contract => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.contract-zero-scc",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:contract-zero-length-admissible-components",
            },
            Self::Augment => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.blocking-or-delta-flow",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:augment-contracted-admissible-dag",
            },
            Self::Lift => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.lift-component-flow",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "goldberg-rao:route-component-balances-through-in-out-paths",
            },
            Self::UpdateCut => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.halve-cut-gap",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "goldberg-rao:replace-current-cut-after-half-gap",
            },
            Self::Optimal => FlowTraceEventMetadata {
                catalog_id: "goldberg-rao.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "goldberg-rao:return-certified-integral-flow",
            },
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    distances: &[Option<u64>],
    active_path: &[ResidualArcId],
    focus_arcs: Option<&[ResidualArcId]>,
    metrics: GoldbergRaoMetrics,
    kind: EventKind,
    detail: Option<(&'static str, i128)>,
) -> Result<(), GoldbergRaoError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    if recorder.event_count() >= GOLDBERG_RAO_MAX_TRACE_EVENTS {
        return Err(GoldbergRaoError::WorkLimit);
    }
    let snapshot = trace_snapshot(graph, state, distances, active_path, metrics);
    if let Some(focus_arcs) = focus_arcs {
        let focus = if matches!(kind, EventKind::CanonicalCut) {
            focus_arcs
                .iter()
                .cloned()
                .map(FlowTraceEntityRef::ResidualArc)
                .collect()
        } else {
            residual_arc_entity_refs(graph, state, focus_arcs)?
        };
        recorder.record_transition_with_detail_and_focus(
            kind.metadata(),
            &snapshot,
            detail,
            focus,
        )?;
    } else {
        recorder.record_transition_with_detail(kind.metadata(), &snapshot, detail)?;
    }
    Ok(())
}

fn trace_snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    distances: &[Option<u64>],
    active_path: &[ResidualArcId],
    metrics: GoldbergRaoMetrics,
) -> FlowTraceSnapshot {
    let labels = if distances.is_empty() {
        vec![None; graph.nodes().len()]
    } else {
        distances
            .iter()
            .map(|distance| distance.map(i128::from))
            .collect()
    };
    let search_order = distances
        .iter()
        .enumerate()
        .filter_map(|(node, distance)| distance.and_then(|_| NodeIndex::try_from_usize(node)))
        .collect();
    FlowTraceSnapshot::capture(
        graph,
        state,
        labels,
        search_order,
        active_path.to_vec(),
        Vec::new(),
        FlowTraceMetrics {
            bfs_runs: u128::from(metrics.distance_searches),
            relaxation_passes: u128::from(metrics.phases),
            residual_arc_scans: metrics.residual_arc_scans,
            augmentations: u128::from(metrics.update_steps),
            path_searches: u128::from(metrics.canonical_cut_evaluations),
            scaling_phases: 0,
            blocking_flow_phases: u128::from(metrics.blocking_updates),
            relabels: u128::from(metrics.zero_length_arc_observations),
            retreats: u128::from(metrics.special_arc_observations),
            reverse_bfs_runs: u128::from(metrics.nontrivial_contractions),
            gap_terminations: u128::from(metrics.cut_updates),
            pushes: u128::from(metrics.contracted_augmentations),
            saturating_pushes: u128::from(metrics.delta_limited_updates),
            nonsaturating_pushes: u128::from(metrics.component_routing_paths),
            discharges: metrics.augmented_units,
            active_vertex_selections: u128::from(metrics.state_transitions),
        },
    )
}

fn binary_primitive_snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    distances: &[Option<u64>],
    active_arcs: &[ResidualArcId],
    metrics: GoldbergRaoMetrics,
) -> FlowTraceSnapshot {
    trace_snapshot(graph, state, distances, active_arcs, metrics)
}

fn binary_component_count(component_of: &[usize]) -> usize {
    component_of
        .iter()
        .copied()
        .max()
        .map_or(0, |component| component.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::certificate::check_max_flow;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn graph(edges: &[(&str, &str, &str, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "b", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edges = edges
            .iter()
            .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge"),
                from: NodeId::parse(from).expect("from"),
                to: NodeId::parse(to).expect("to"),
                lower: 0,
                capacity,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, edges).expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    fn indexed_graph(
        node_count: usize,
        edges: &[(usize, usize, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = (0..node_count)
            .map(|index| FlowNode::new(NodeId::parse(&format!("v{index}")).expect("node id"), 0))
            .collect();
        let edges = edges
            .iter()
            .enumerate()
            .map(|(index, &(from, to, capacity))| UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("e{index:04}")).expect("edge id"),
                from: NodeId::parse(&format!("v{from}")).expect("from"),
                to: NodeId::parse(&format!("v{to}")).expect("to"),
                lower: 0,
                capacity,
                cost: 0,
            })
            .collect();
        let graph = FlowNetwork::new(nodes, edges).expect("indexed graph");
        let source = graph
            .node_index(&NodeId::parse("v0").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse(&format!("v{}", node_count - 1)).expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    fn set_flow(graph: &FlowNetwork, flows: &mut [u64], edge: &str, value: u64) {
        let index = graph
            .edge_index(&EdgeId::parse(edge).expect("edge id"))
            .expect("edge index");
        flows[index.as_usize()] = value;
    }

    fn edge_flow(graph: &FlowNetwork, flows: &[u64], edge: &str) -> u64 {
        let index = graph
            .edge_index(&EdgeId::parse(edge).expect("edge id"))
            .expect("edge index");
        flows[index.as_usize()]
    }

    fn next_random(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn primitive_returns_delta_or_a_verified_blocking_flow() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 9),
            ("ab", "a", "b", 9),
            ("bt", "b", "t", 9),
        ]);
        let step = solve_binary_blocking_step(&graph, source, sink, &[0, 0, 0], 9, 2)
            .expect("primitive step");
        assert_eq!(step.value, 2);
        assert!(!step.blocking);
        assert_eq!(step.flows, [2, 2, 2]);
        check_binary_blocking_step(&graph, source, sink, &step).expect("primitive certificate");
    }

    #[test]
    fn primitive_returns_a_zero_value_blocking_flow_when_no_admissible_path_exists() {
        let (graph, source, sink) = graph(&[]);
        let step = solve_binary_blocking_step(&graph, source, sink, &[], 1, 1)
            .expect("zero blocking step");
        assert_eq!(step.value, 0);
        assert!(step.blocking);
        assert!(step.augmentation.is_empty());
        check_binary_blocking_step(&graph, source, sink, &step).expect("primitive certificate");
    }

    #[test]
    fn component_lift_routes_cross_component_imbalance_inside_a_zero_scc() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 9),
            ("ab", "a", "b", 9),
            ("ba", "b", "a", 9),
            ("bt", "b", "t", 9),
        ]);
        let step = solve_binary_blocking_step(&graph, source, sink, &[0; 4], 9, 2)
            .expect("lifted primitive");
        let a = graph
            .node_index(&NodeId::parse("a").expect("a"))
            .expect("a index");
        let b = graph
            .node_index(&NodeId::parse("b").expect("b"))
            .expect("b index");
        assert_eq!(
            step.component_of[a.as_usize()],
            step.component_of[b.as_usize()]
        );
        assert_eq!(edge_flow(&graph, &step.flows, "sa"), 2);
        assert_eq!(edge_flow(&graph, &step.flows, "ab"), 2);
        assert_eq!(edge_flow(&graph, &step.flows, "ba"), 0);
        assert_eq!(edge_flow(&graph, &step.flows, "bt"), 2);
        assert!(step.augmentation.iter().any(|operation| {
            operation.arc.original_edge().as_str() == "ab"
                && operation.arc.direction() == ResidualDirection::Forward
        }));
        check_binary_blocking_step(&graph, source, sink, &step).expect("lift certificate");
    }

    #[test]
    fn special_arc_classification_uses_both_residual_directions_and_equal_labels() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 12),
            ("ab", "a", "b", 10),
            ("bt", "b", "t", 12),
            ("at", "a", "t", 12),
        ]);
        let mut initial = vec![0; graph.edges().len()];
        set_flow(&graph, &mut initial, "sa", 6);
        set_flow(&graph, &mut initial, "ab", 6);
        set_flow(&graph, &mut initial, "bt", 6);
        let step = solve_binary_blocking_step(&graph, source, sink, &initial, 20, 2)
            .expect("special-arc step");
        let expected =
            ResidualArcId::new(EdgeId::parse("ab").expect("ab"), ResidualDirection::Forward);
        assert!(step.special_arcs.contains(&expected));
        check_binary_blocking_step(&graph, source, sink, &step).expect("special certificate");
    }

    #[test]
    fn primitive_checker_rejects_corrupted_labels_components_operations_and_blocking_flag() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 9),
            ("ab", "a", "b", 9),
            ("bt", "b", "t", 9),
        ]);
        let step = solve_binary_blocking_step(&graph, source, sink, &[0; 3], 9, 2)
            .expect("primitive step");

        let mut wrong = step.clone();
        wrong.distances[source.as_usize()] = Some(99);
        assert!(check_binary_blocking_step(&graph, source, sink, &wrong).is_err());

        let mut wrong = step.clone();
        wrong.component_of[source.as_usize()] = usize::MAX;
        assert!(check_binary_blocking_step(&graph, source, sink, &wrong).is_err());

        let mut wrong = step.clone();
        wrong.augmentation[0].amount += 1;
        assert!(check_binary_blocking_step(&graph, source, sink, &wrong).is_err());

        let mut wrong = step;
        wrong.blocking = !wrong.blocking;
        assert!(check_binary_blocking_step(&graph, source, sink, &wrong).is_err());
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one transcript test verifies scan identity, work accounting, contraction, lift, and replay together"
    )]
    fn standalone_first_step_derives_parameters_and_replays_source_boundaries() {
        let (network, source, sink) = graph(&[
            ("sa", "s", "a", 12),
            ("ab", "a", "b", 12),
            ("ba", "b", "a", 12),
            ("bt", "b", "t", 12),
        ]);
        let run = trace_binary_blocking_first_step(&network, source, sink, &[0; 4])
            .expect("standalone trace");
        assert_eq!(run.result.upper_bound, 12);
        assert_eq!(run.result.delta, 4);
        assert_eq!(run.result.value, 4);
        assert!(!run.result.blocking);
        assert_eq!(
            run.events.first().map(|event| event.catalog_id.as_str()),
            Some("binary-blocking-flow.inspect-initial-cut-arc")
        );
        assert!(run.events.iter().all(|event| {
            if !matches!(
                event.catalog_id.as_str(),
                "binary-blocking-flow.inspect-residual-arc"
                    | "binary-blocking-flow.build-reverse-zero-one-adjacency"
                    | "binary-blocking-flow.relax-binary-distance"
                    | "binary-blocking-flow.inspect-binary-length"
            ) {
                return true;
            }
            matches!(
                event.entity_refs.as_slice(),
                [FlowTraceEntityRef::ResidualArc(_)]
            )
        }));
        assert!(run.events.iter().any(|event| {
            event.catalog_id == "binary-blocking-flow.analyze-binary-network"
                && event.minimum_granularity == TraceGranularityV1::Phase
        }));
        assert_eq!(
            run.events.last().map(|event| event.catalog_id.as_str()),
            Some("binary-blocking-flow.complete-primitive")
        );
        assert_eq!(
            run.events
                .iter()
                .filter(|event| {
                    event.catalog_id == "binary-blocking-flow.inspect-binary-length"
                })
                .count(),
            run.base_snapshot
                .residual_capacities
                .iter()
                .filter(|(_, capacity)| *capacity > 0)
                .count()
        );
        assert!(run.events.iter().any(|event| {
            event.catalog_id == "binary-blocking-flow.contract-zero-scc"
                && event.minimum_granularity == TraceGranularityV1::Operation
        }));
        for catalog_id in [
            "binary-blocking-flow.build-zero-scc-adjacency",
            "binary-blocking-flow.inspect-zero-scc-reverse-arc",
            "binary-blocking-flow.inspect-contracted-arc",
            "binary-blocking-flow.build-lift-adjacency",
            "binary-blocking-flow.inspect-lift-arc",
            "binary-blocking-flow.apply-contracted-flow",
            "binary-blocking-flow.apply-lift-path",
        ] {
            assert!(
                run.events
                    .iter()
                    .any(|event| event.catalog_id == catalog_id),
                "fixture must exercise {catalog_id}"
            );
        }
        let mut previous_scans = run.base_snapshot.metrics.residual_arc_scans;
        for event in &run.events {
            let after = event
                .patches
                .iter()
                .find_map(|patch| match patch {
                    crate::trace::FlowTracePatch::Metric {
                        metric: crate::trace::FlowTraceMetricId::ResidualArcScans,
                        after,
                        ..
                    } => Some(*after),
                    _ => None,
                })
                .unwrap_or(previous_scans);
            let delta = after - previous_scans;
            if delta > 0 {
                assert_eq!(event.minimum_granularity, TraceGranularityV1::Micro);
                assert!(delta <= BINARY_BLOCKING_TRACE_SCAN_BLOCK_MAX);
                assert!(matches!(
                    event.entity_refs.as_slice(),
                    [FlowTraceEntityRef::ResidualArc(_)]
                ));
            }
            previous_scans = after;
        }
        assert_eq!(previous_scans, run.result.metrics.residual_arc_scans);
        let mut replay = run.base_snapshot.clone();
        for event in &run.events {
            crate::trace::apply_trace_event(
                &network,
                &mut replay,
                event,
                crate::trace::FlowTraceDirection::Forward,
            )
            .expect("forward replay");
        }
        assert_eq!(replay, run.final_snapshot);
        for event in run.events.iter().rev() {
            crate::trace::apply_trace_event(
                &network,
                &mut replay,
                event,
                crate::trace::FlowTraceDirection::Reverse,
            )
            .expect("reverse replay");
        }
        assert_eq!(replay, run.base_snapshot);

        let (empty, empty_source, empty_sink) = graph(&[]);
        let no_gap = solve_binary_blocking_first_step(&empty, empty_source, empty_sink, &[])
            .expect("zero residual source cut is normalized");
        assert_eq!(no_gap.upper_bound, 1);
        assert_eq!(no_gap.delta, 1);
        assert_eq!(no_gap.value, 0);
        assert!(no_gap.blocking);
    }

    #[test]
    fn solver_matches_independent_certificate_with_parallel_opposite_and_self_loop() {
        let (graph, source, sink) = graph(&[
            ("sa0", "s", "a", 7),
            ("sa1", "s", "a", 3),
            ("as", "a", "s", 2),
            ("ab", "a", "b", 6),
            ("at", "a", "t", 3),
            ("bt", "b", "t", 8),
            ("loop", "b", "b", 9),
        ]);
        let result = solve_goldberg_rao(&graph, source, sink).expect("Goldberg-Rao");
        let certificate = check_max_flow(&graph, source, sink, &result.flows).expect("certificate");
        assert_eq!(result.certificate, certificate);
        assert_eq!(certificate.value, 9);
        assert!(result.metrics.update_steps > 0);
        assert!(result.metrics.distance_searches > 0);
    }

    #[test]
    fn solver_matches_edmonds_karp_on_many_deterministic_tiny_multigraphs() {
        for case in 0..128_u64 {
            let node_count = 2 + usize::try_from(case % 6).expect("small node count");
            let mut random = case.wrapping_add(1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if next_random(&mut random).is_multiple_of(4) {
                        edges.push((from, to, next_random(&mut random) % 17));
                        if next_random(&mut random).is_multiple_of(9) {
                            edges.push((from, to, next_random(&mut random) % 17));
                        }
                    }
                }
            }
            let (graph, source, sink) = indexed_graph(node_count, &edges);
            let expected = solve_edmonds_karp(&graph, source, sink)
                .unwrap_or_else(|error| panic!("case {case} oracle failed: {error}"));
            let actual = solve_goldberg_rao(&graph, source, sink)
                .unwrap_or_else(|error| panic!("case {case} Goldberg-Rao failed: {error}"));
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "case {case} value mismatch with edges {edges:?}"
            );
            assert_eq!(
                actual.certificate.cut_bound, expected.certificate.cut_bound,
                "case {case} cut mismatch with edges {edges:?}"
            );
        }
    }

    #[test]
    fn parallel_maximum_capacity_edges_use_exact_wide_cut_arithmetic() {
        let (graph, source, sink) = indexed_graph(2, &[(0, 1, u64::MAX), (0, 1, u64::MAX)]);
        let result = solve_goldberg_rao(&graph, source, sink).expect("wide exact flow");
        assert_eq!(result.certificate.value, i128::from(u64::MAX) * 2);
        assert_eq!(result.flows, [u64::MAX, u64::MAX]);
    }

    #[test]
    fn integral_phase_scale_rounds_roots_and_division_up() {
        assert_eq!(ceil_sqrt(0), 0);
        assert_eq!(ceil_sqrt(1), 1);
        assert_eq!(ceil_sqrt(2), 2);
        assert_eq!(ceil_sqrt(4), 2);
        assert_eq!(ceil_sqrt(5), 3);
        assert_eq!(ceil_cuberoot(0), 0);
        assert_eq!(ceil_cuberoot(1), 1);
        assert_eq!(ceil_cuberoot(2), 2);
        assert_eq!(ceil_cuberoot(8), 2);
        assert_eq!(ceil_cuberoot(9), 3);
        assert_eq!(ceil_div(10, 3), Ok(4));
        assert_eq!(phase_delta(10, 8, 9), Ok(3));
    }

    #[test]
    fn trace_replays_and_exposes_binary_phase_boundaries() {
        let (graph, source, sink) = graph(&[
            ("sa", "s", "a", 12),
            ("ab", "a", "b", 12),
            ("ba", "b", "a", 12),
            ("bt", "b", "t", 12),
            ("at", "a", "t", 2),
        ]);
        let run = trace_goldberg_rao(&graph, source, sink).expect("trace");
        let ids = run
            .events
            .iter()
            .map(|event| event.catalog_id.as_str())
            .collect::<BTreeSet<_>>();
        for id in [
            "goldberg-rao.binary-length-distance",
            "goldberg-rao.minimum-canonical-cut",
            "goldberg-rao.contract-zero-scc",
            "goldberg-rao.blocking-or-delta-flow",
            "goldberg-rao.lift-component-flow",
            "goldberg-rao.optimal",
        ] {
            assert!(ids.contains(id), "missing {id}");
        }
        let canonical_cut = run
            .events
            .iter()
            .find(|event| event.catalog_id == "goldberg-rao.minimum-canonical-cut")
            .expect("canonical cut event");
        assert!(
            canonical_cut
                .entity_refs
                .iter()
                .any(|entity| matches!(entity, crate::trace::FlowTraceEntityRef::ResidualArc(_))),
            "canonical cut must identify the residual arcs that form the cut"
        );
        let mut snapshot = run.base_snapshot.clone();
        for event in &run.events {
            crate::trace::apply_trace_event(
                &graph,
                &mut snapshot,
                event,
                crate::trace::FlowTraceDirection::Forward,
            )
            .expect("forward");
        }
        assert_eq!(snapshot, run.final_snapshot);
        for event in run.events.iter().rev() {
            crate::trace::apply_trace_event(
                &graph,
                &mut snapshot,
                event,
                crate::trace::FlowTraceDirection::Reverse,
            )
            .expect("backward");
        }
        assert_eq!(snapshot, run.base_snapshot);
    }

    #[test]
    fn lower_bounds_are_rejected_instead_of_silently_changing_the_source_domain() {
        let (mut graph, source, sink) = graph(&[("sa", "s", "a", 3), ("at", "a", "t", 3)]);
        let nodes = graph.nodes().to_vec();
        let edges = graph
            .edges()
            .iter()
            .map(|edge| UnresolvedFlowEdge {
                id: edge.id().clone(),
                from: graph.node(edge.from()).expect("from").id().clone(),
                to: graph.node(edge.to()).expect("to").id().clone(),
                lower: 1,
                capacity: edge.capacity(),
                cost: edge.cost(),
            })
            .collect();
        graph = FlowNetwork::new(nodes, edges).expect("lower graph");
        assert_eq!(
            solve_goldberg_rao(&graph, source, sink),
            Err(GoldbergRaoError::GraphRequirement)
        );
    }
}
