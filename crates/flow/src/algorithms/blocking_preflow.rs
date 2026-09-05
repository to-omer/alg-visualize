//! Cubic blocking-flow kernels based on layered-network preflows.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot,
};

/// Conservative node limit for the cubic blocking-preflow solvers.
pub const BLOCKING_PREFLOW_MAX_NODES: usize = 256;
/// Conservative edge limit for the cubic blocking-preflow solvers.
pub const BLOCKING_PREFLOW_MAX_EDGES: usize = 2_048;
/// Positive residual-arc inspections allowed in one run.
pub const BLOCKING_PREFLOW_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Traceable state transitions allowed in one run.
pub const BLOCKING_PREFLOW_MAX_STATE_TRANSITIONS: u64 = 100_000;

/// Exact counters shared by Karzanov and MPM.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlockingPreflowMetrics {
    /// Level-BFS invocations, including the final unsuccessful search.
    pub bfs_runs: u64,
    /// Positive residual or layered arcs inspected.
    pub residual_arc_scans: u128,
    /// Completed blocking-flow phases.
    pub blocking_flow_phases: u64,
    /// Single-arc residual mutations inside layered-network work.
    pub pushes: u64,
    /// Pushes that exhaust their selected residual arc.
    pub saturating_pushes: u64,
    /// Pushes that stop before exhausting their selected residual arc.
    pub nonsaturating_pushes: u64,
    /// Vertices selected and removed by minimum-potential MPM work.
    pub vertex_eliminations: u64,
    /// Karzanov balancing iterations that return stranded excess.
    pub balancing_iterations: u64,
}

/// Certified output of a layered-network preflow solver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockingPreflowResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed max-flow/min-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact deterministic work counters.
    pub metrics: BlockingPreflowMetrics,
}

/// Certified output with a complete reversible trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockingPreflowTraceResult {
    /// Same result produced by the fast profile.
    pub result: BlockingPreflowResult,
    /// Replay boundary before the first level BFS.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after the verified optimal event.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Blocking-preflow construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BlockingPreflowError {
    /// Input exceeds the practical admission band.
    #[error("graph exceeds blocking-preflow admission limits")]
    AdmissionLimit,
    /// Lower-bound circulation construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The independent max-flow certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact arithmetic exceeded its declared domain.
    #[error("blocking-preflow arithmetic overflow")]
    ArithmeticOverflow,
    /// The layered-network or preflow invariant was contradicted.
    #[error("blocking-preflow invariant failed")]
    Invariant,
    /// The bounded interactive work ceiling was exceeded.
    #[error("blocking-preflow work ceiling exceeded")]
    WorkLimit,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves maximum flow using Karzanov's layered-network preflow method.
///
/// # Errors
///
/// Rejects out-of-band inputs, infeasible lower bounds, work-limit breaches,
/// invariant failures, or a result rejected by the independent certificate.
pub fn solve_karzanov_preflow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BlockingPreflowResult, BlockingPreflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_blocking_preflow_preset_with_feasibility(
        graph,
        source,
        sink,
        BlockingPreflowExecutionPreset::Karzanov,
        &mut feasibility,
    )
}

/// Solves maximum flow using the Malhotra–Pramodh Kumar–Maheshwari method.
///
/// # Errors
///
/// Returns the same bounded construction and certificate failures as
/// [`solve_karzanov_preflow`].
pub fn solve_mpm(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BlockingPreflowResult, BlockingPreflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_blocking_preflow_preset_with_feasibility(
        graph,
        source,
        sink,
        BlockingPreflowExecutionPreset::Mpm,
        &mut feasibility,
    )
}

/// Solves one blocking-preflow preset while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_blocking_preflow_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: BlockingPreflowExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPreflowResult, BlockingPreflowError> {
    solve_internal(graph, source, sink, preset, false, feasibility).map(|run| run.result)
}

/// Traces Karzanov level construction, pushing, balancing, and phase events.
///
/// # Errors
///
/// Returns the same failures as [`solve_karzanov_preflow`], plus trace failures.
pub fn trace_karzanov_preflow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BlockingPreflowTraceResult, BlockingPreflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_blocking_preflow_preset_with_feasibility(
        graph,
        source,
        sink,
        BlockingPreflowExecutionPreset::Karzanov,
        &mut feasibility,
    )
}

/// Traces MPM minimum-potential selections, directional pushes, and removals.
///
/// # Errors
///
/// Returns the same failures as [`solve_mpm`], plus trace failures.
pub fn trace_mpm(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<BlockingPreflowTraceResult, BlockingPreflowError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_blocking_preflow_preset_with_feasibility(
        graph,
        source,
        sink,
        BlockingPreflowExecutionPreset::Mpm,
        &mut feasibility,
    )
}

/// Traces one blocking-preflow preset while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_blocking_preflow_preset_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: BlockingPreflowExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPreflowTraceResult, BlockingPreflowError> {
    trace_internal(graph, source, sink, preset, feasibility)
}

/// Closed set of layered blocking-preflow execution presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockingPreflowExecutionPreset {
    /// Karzanov's layered-network preflow method.
    Karzanov,
    /// Malhotra--Pramodh Kumar--Maheshwari vertex-potential method.
    Mpm,
}

struct BlockingPreflowInternalRun {
    result: BlockingPreflowResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn trace_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: BlockingPreflowExecutionPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPreflowTraceResult, BlockingPreflowError> {
    let run = solve_internal(graph, source, sink, preset, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(BlockingPreflowError::Invariant)?;
    Ok(BlockingPreflowTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    preset: BlockingPreflowExecutionPreset,
    with_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<BlockingPreflowInternalRun, BlockingPreflowError> {
    admit(graph)?;
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut metrics = BlockingPreflowMetrics::default();
    let mut transitions = 0_u64;
    let mut recorder = start_trace_recorder(graph, &state, metrics, with_trace)?;

    loop {
        metrics.bfs_runs = checked_increment(metrics.bfs_runs)?;
        let level = build_layered_network(&state, source, &mut metrics)?;
        record_event(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            event_metadata(preset, BlockingPreflowEvent::LevelBfs)?,
            TraceView::from_levels(&level)?,
            None,
            &mut transitions,
        )?;
        if level.distances[sink.as_usize()].is_none() {
            break;
        }
        match preset {
            BlockingPreflowExecutionPreset::Karzanov => run_karzanov_phase(
                graph,
                &mut state,
                source,
                sink,
                &level,
                &mut metrics,
                recorder.as_mut(),
                &mut transitions,
            )?,
            BlockingPreflowExecutionPreset::Mpm => run_mpm_phase(
                graph,
                &mut state,
                source,
                sink,
                &level,
                &mut metrics,
                recorder.as_mut(),
                &mut transitions,
            )?,
        }
        if layered_path_exists(&state, &level, source, sink)? {
            return Err(BlockingPreflowError::Invariant);
        }
        metrics.blocking_flow_phases = checked_increment(metrics.blocking_flow_phases)?;
        record_event(
            recorder.as_mut(),
            graph,
            &state,
            metrics,
            event_metadata(preset, BlockingPreflowEvent::BlockingFlow)?,
            TraceView::from_levels(&level)?,
            None,
            &mut transitions,
        )?;
    }

    finish_run(
        graph,
        &state,
        source,
        sink,
        preset,
        metrics,
        recorder,
        &mut transitions,
    )
}

fn admit(graph: &FlowNetwork) -> Result<(), BlockingPreflowError> {
    if graph.nodes().len() > BLOCKING_PREFLOW_MAX_NODES
        || graph.edges().len() > BLOCKING_PREFLOW_MAX_EDGES
    {
        Err(BlockingPreflowError::AdmissionLimit)
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_run(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    preset: BlockingPreflowExecutionPreset,
    metrics: BlockingPreflowMetrics,
    mut recorder: Option<FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<BlockingPreflowInternalRun, BlockingPreflowError> {
    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record_event(
        recorder.as_mut(),
        graph,
        state,
        metrics,
        event_metadata(preset, BlockingPreflowEvent::Optimal)?,
        TraceView::empty(graph),
        None,
        transitions,
    )?;
    Ok(BlockingPreflowInternalRun {
        result: BlockingPreflowResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

struct LayeredNetwork {
    distances: Vec<Option<usize>>,
    search_order: Vec<NodeIndex>,
    topological_order: Vec<NodeIndex>,
    outgoing: Vec<Vec<ResidualArcId>>,
    incoming: Vec<Vec<ResidualArcId>>,
}

fn build_layered_network(
    state: &ResidualState<'_>,
    source: NodeIndex,
    metrics: &mut BlockingPreflowMetrics,
) -> Result<LayeredNetwork, BlockingPreflowError> {
    let node_count = state.graph().nodes().len();
    let mut distances = vec![None; node_count];
    let mut search_order = vec![source];
    let mut queue = VecDeque::from([source]);
    distances[source.as_usize()] = Some(0);
    while let Some(node) = queue.pop_front() {
        let distance: usize = distances[node.as_usize()].ok_or(BlockingPreflowError::Invariant)?;
        for arc in state.outgoing_arcs(node) {
            increment_scans(metrics)?;
            if distances[arc.to.as_usize()].is_some() {
                continue;
            }
            distances[arc.to.as_usize()] = Some(
                distance
                    .checked_add(1)
                    .ok_or(BlockingPreflowError::ArithmeticOverflow)?,
            );
            search_order.push(arc.to);
            queue.push_back(arc.to);
        }
    }

    let mut outgoing = vec![Vec::new(); node_count];
    let mut incoming = vec![Vec::new(); node_count];
    for &node in &search_order {
        let from_level: usize =
            distances[node.as_usize()].ok_or(BlockingPreflowError::Invariant)?;
        for arc in state.outgoing_arcs(node) {
            increment_scans(metrics)?;
            if distances[arc.to.as_usize()] == from_level.checked_add(1) {
                outgoing[node.as_usize()].push(arc.id.clone());
                incoming[arc.to.as_usize()].push(arc.id);
            }
        }
    }
    let mut topological_order = search_order.clone();
    topological_order.sort_unstable_by_key(|node| {
        (
            distances[node.as_usize()].unwrap_or(usize::MAX),
            node.as_usize(),
        )
    });
    Ok(LayeredNetwork {
        distances,
        search_order,
        topological_order,
        outgoing,
        incoming,
    })
}

fn layered_path_exists(
    state: &ResidualState<'_>,
    level: &LayeredNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<bool, BlockingPreflowError> {
    let mut seen = vec![false; state.graph().nodes().len()];
    let mut queue = VecDeque::from([source]);
    seen[source.as_usize()] = true;
    while let Some(node) = queue.pop_front() {
        for id in &level.outgoing[node.as_usize()] {
            let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
            if arc.capacity == 0 || seen[arc.to.as_usize()] {
                continue;
            }
            if arc.to == sink {
                return Ok(true);
            }
            seen[arc.to.as_usize()] = true;
            queue.push_back(arc.to);
        }
    }
    Ok(false)
}

struct MpmState {
    alive: Vec<bool>,
    incoming_capacity: Vec<u128>,
    outgoing_capacity: Vec<u128>,
}

impl MpmState {
    fn new(
        state: &ResidualState<'_>,
        level: &LayeredNetwork,
    ) -> Result<Self, BlockingPreflowError> {
        let node_count = state.graph().nodes().len();
        let mut incoming_capacity = vec![0_u128; node_count];
        let mut outgoing_capacity = vec![0_u128; node_count];
        for &node in &level.topological_order {
            for id in &level.outgoing[node.as_usize()] {
                let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
                outgoing_capacity[node.as_usize()] = outgoing_capacity[node.as_usize()]
                    .checked_add(u128::from(arc.capacity))
                    .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
                incoming_capacity[arc.to.as_usize()] = incoming_capacity[arc.to.as_usize()]
                    .checked_add(u128::from(arc.capacity))
                    .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
            }
        }
        Ok(Self {
            alive: level.distances.iter().map(Option::is_some).collect(),
            incoming_capacity,
            outgoing_capacity,
        })
    }

    fn potential(&self, node: NodeIndex, source: NodeIndex, sink: NodeIndex) -> u128 {
        if node == source {
            self.outgoing_capacity[node.as_usize()]
        } else if node == sink {
            self.incoming_capacity[node.as_usize()]
        } else {
            self.incoming_capacity[node.as_usize()].min(self.outgoing_capacity[node.as_usize()])
        }
    }

    fn labels(
        &self,
        graph: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
    ) -> Result<Vec<Option<i128>>, BlockingPreflowError> {
        graph
            .node_indices()
            .map(|node| {
                if !self.alive[node.as_usize()] {
                    return Ok(None);
                }
                checked_detail(self.potential(node, source, sink)).map(Some)
            })
            .collect()
    }

    fn subtract_arc(
        &mut self,
        from: NodeIndex,
        to: NodeIndex,
        amount: u64,
    ) -> Result<(), BlockingPreflowError> {
        let amount = u128::from(amount);
        self.outgoing_capacity[from.as_usize()] = self.outgoing_capacity[from.as_usize()]
            .checked_sub(amount)
            .ok_or(BlockingPreflowError::Invariant)?;
        self.incoming_capacity[to.as_usize()] = self.incoming_capacity[to.as_usize()]
            .checked_sub(amount)
            .ok_or(BlockingPreflowError::Invariant)?;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn run_mpm_phase(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    level: &LayeredNetwork,
    metrics: &mut BlockingPreflowMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    let mut mpm = MpmState::new(state, level)?;
    while let Some(node) = select_minimum_potential(graph, &mpm, source, sink) {
        let potential = mpm.potential(node, source, sink);
        metrics.vertex_eliminations = checked_increment(metrics.vertex_eliminations)?;
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            event_metadata(
                BlockingPreflowExecutionPreset::Mpm,
                BlockingPreflowEvent::MpmSelect,
            )?,
            TraceView {
                labels: mpm.labels(graph, source, sink)?,
                search_order: vec![node],
                path: Vec::new(),
            },
            Some(("potential", checked_detail(potential)?)),
            transitions,
        )?;
        if node != source && potential > 0 {
            push_mpm_direction(
                graph,
                state,
                source,
                sink,
                node,
                potential,
                level,
                &mut mpm,
                MpmDirection::Backward,
                metrics,
                recorder.as_deref_mut(),
                transitions,
            )?;
        }
        if node != sink && potential > 0 {
            push_mpm_direction(
                graph,
                state,
                source,
                sink,
                node,
                potential,
                level,
                &mut mpm,
                MpmDirection::Forward,
                metrics,
                recorder.as_deref_mut(),
                transitions,
            )?;
        }
        remove_mpm_vertex(state, level, node, &mut mpm)?;
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            event_metadata(
                BlockingPreflowExecutionPreset::Mpm,
                BlockingPreflowEvent::MpmRemove,
            )?,
            TraceView {
                labels: mpm.labels(graph, source, sink)?,
                search_order: vec![node],
                path: Vec::new(),
            },
            Some(("potential", checked_detail(potential)?)),
            transitions,
        )?;
        if node == source || node == sink {
            break;
        }
    }
    Ok(())
}

fn select_minimum_potential(
    graph: &FlowNetwork,
    mpm: &MpmState,
    source: NodeIndex,
    sink: NodeIndex,
) -> Option<NodeIndex> {
    graph
        .node_indices()
        .filter(|node| mpm.alive[node.as_usize()])
        .min_by_key(|&node| (mpm.potential(node, source, sink), node.as_usize()))
}

#[derive(Clone, Copy)]
enum MpmDirection {
    Forward,
    Backward,
}

#[allow(clippy::too_many_arguments)]
fn push_mpm_direction(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    root: NodeIndex,
    amount: u128,
    level: &LayeredNetwork,
    mpm: &mut MpmState,
    direction: MpmDirection,
    metrics: &mut BlockingPreflowMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    let node_count = graph.nodes().len();
    let mut pending = vec![0_u128; node_count];
    let mut queued = vec![false; node_count];
    let mut queue = VecDeque::from([root]);
    pending[root.as_usize()] = amount;
    queued[root.as_usize()] = true;
    while let Some(node) = queue.pop_front() {
        queued[node.as_usize()] = false;
        if matches!(direction, MpmDirection::Forward) && node == sink
            || matches!(direction, MpmDirection::Backward) && node == source
        {
            pending[node.as_usize()] = 0;
            continue;
        }
        let candidates = match direction {
            MpmDirection::Forward => &level.outgoing[node.as_usize()],
            MpmDirection::Backward => &level.incoming[node.as_usize()],
        };
        for id in candidates {
            if pending[node.as_usize()] == 0 {
                break;
            }
            increment_scans(metrics)?;
            let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
            let adjacent = match direction {
                MpmDirection::Forward => arc.to,
                MpmDirection::Backward => arc.from,
            };
            if !mpm.alive[adjacent.as_usize()] || arc.capacity == 0 {
                continue;
            }
            let delta = pending[node.as_usize()]
                .min(u128::from(arc.capacity))
                .try_into()
                .map_err(|_| BlockingPreflowError::ArithmeticOverflow)?;
            state.augment(std::slice::from_ref(id), delta)?;
            pending[node.as_usize()] = pending[node.as_usize()]
                .checked_sub(u128::from(delta))
                .ok_or(BlockingPreflowError::Invariant)?;
            pending[adjacent.as_usize()] = pending[adjacent.as_usize()]
                .checked_add(u128::from(delta))
                .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
            mpm.subtract_arc(arc.from, arc.to, delta)?;
            count_push(metrics, delta == arc.capacity)?;
            if !queued[adjacent.as_usize()] {
                queue.push_back(adjacent);
                queued[adjacent.as_usize()] = true;
            }
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                event_metadata(
                    BlockingPreflowExecutionPreset::Mpm,
                    match direction {
                        MpmDirection::Forward => BlockingPreflowEvent::MpmPushForward,
                        MpmDirection::Backward => BlockingPreflowEvent::MpmPushBackward,
                    },
                )?,
                TraceView {
                    labels: mpm.labels(graph, source, sink)?,
                    search_order: vec![node, adjacent],
                    path: vec![id.clone()],
                },
                Some(("delta", i128::from(delta))),
                transitions,
            )?;
        }
        if pending[node.as_usize()] != 0 {
            return Err(BlockingPreflowError::Invariant);
        }
    }
    Ok(())
}

fn remove_mpm_vertex(
    state: &ResidualState<'_>,
    level: &LayeredNetwork,
    node: NodeIndex,
    mpm: &mut MpmState,
) -> Result<(), BlockingPreflowError> {
    if !mpm.alive[node.as_usize()] {
        return Err(BlockingPreflowError::Invariant);
    }
    for id in &level.outgoing[node.as_usize()] {
        let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
        if mpm.alive[arc.to.as_usize()] {
            mpm.subtract_arc(arc.from, arc.to, arc.capacity)?;
        }
    }
    for id in &level.incoming[node.as_usize()] {
        let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
        if mpm.alive[arc.from.as_usize()] {
            mpm.subtract_arc(arc.from, arc.to, arc.capacity)?;
        }
    }
    mpm.alive[node.as_usize()] = false;
    Ok(())
}

#[derive(Clone)]
struct Contribution {
    arc: ResidualArcId,
    amount: u64,
}

#[allow(clippy::too_many_arguments)]
fn run_karzanov_phase(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    level: &LayeredNetwork,
    metrics: &mut BlockingPreflowMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    let node_count = graph.nodes().len();
    let mut excess = vec![0_u128; node_count];
    let mut contributions = vec![Vec::<Contribution>::new(); node_count];
    let mut outgoing_cursor = vec![0_usize; node_count];
    let mut frozen = BTreeSet::new();
    let mut initialized = Vec::new();
    let mut initialized_total = 0_u128;
    for id in &level.outgoing[source.as_usize()] {
        let arc = state.arc(id).ok_or(BlockingPreflowError::Invariant)?;
        increment_scans(metrics)?;
        if arc.capacity == 0 {
            continue;
        }
        state.augment(std::slice::from_ref(id), arc.capacity)?;
        excess[arc.to.as_usize()] = excess[arc.to.as_usize()]
            .checked_add(u128::from(arc.capacity))
            .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
        contributions[arc.to.as_usize()].push(Contribution {
            arc: id.clone(),
            amount: arc.capacity,
        });
        initialized_total = initialized_total
            .checked_add(u128::from(arc.capacity))
            .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
        initialized.push(id.clone());
        count_push(metrics, true)?;
    }
    record_event(
        recorder.as_deref_mut(),
        graph,
        state,
        *metrics,
        event_metadata(
            BlockingPreflowExecutionPreset::Karzanov,
            BlockingPreflowEvent::KarzanovInitialize,
        )?,
        TraceView {
            labels: level.labels()?,
            search_order: vec![source],
            path: initialized,
        },
        Some(("delta", checked_detail(initialized_total)?)),
        transitions,
    )?;

    loop {
        push_karzanov_excess(
            graph,
            state,
            source,
            sink,
            level,
            &mut excess,
            &mut contributions,
            &mut outgoing_cursor,
            &frozen,
            metrics,
            recorder.as_deref_mut(),
            transitions,
        )?;
        let Some(node) = select_karzanov_balance(level, &excess, source, sink) else {
            break;
        };
        balance_karzanov_vertex(
            graph,
            state,
            source,
            level,
            node,
            &mut excess,
            &mut contributions,
            &mut frozen,
            metrics,
            recorder.as_deref_mut(),
            transitions,
        )?;
    }
    if graph
        .node_indices()
        .any(|node| node != source && node != sink && excess[node.as_usize()] != 0)
    {
        return Err(BlockingPreflowError::Invariant);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_karzanov_excess(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    sink: NodeIndex,
    level: &LayeredNetwork,
    excess: &mut [u128],
    contributions: &mut [Vec<Contribution>],
    outgoing_cursor: &mut [usize],
    frozen: &BTreeSet<ResidualArcId>,
    metrics: &mut BlockingPreflowMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    for &node in &level.topological_order {
        if node == source || node == sink {
            continue;
        }
        while excess[node.as_usize()] > 0 {
            let Some(id) = level.outgoing[node.as_usize()]
                .get(outgoing_cursor[node.as_usize()])
                .cloned()
            else {
                break;
            };
            increment_scans(metrics)?;
            let arc = state.arc(&id).ok_or(BlockingPreflowError::Invariant)?;
            if frozen.contains(&id) || arc.capacity == 0 {
                outgoing_cursor[node.as_usize()] = outgoing_cursor[node.as_usize()]
                    .checked_add(1)
                    .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
                continue;
            }
            let delta: u64 = excess[node.as_usize()]
                .min(u128::from(arc.capacity))
                .try_into()
                .map_err(|_| BlockingPreflowError::ArithmeticOverflow)?;
            state.augment(std::slice::from_ref(&id), delta)?;
            excess[node.as_usize()] = excess[node.as_usize()]
                .checked_sub(u128::from(delta))
                .ok_or(BlockingPreflowError::Invariant)?;
            excess[arc.to.as_usize()] = excess[arc.to.as_usize()]
                .checked_add(u128::from(delta))
                .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
            contributions[arc.to.as_usize()].push(Contribution {
                arc: id.clone(),
                amount: delta,
            });
            let saturated = delta == arc.capacity;
            count_push(metrics, saturated)?;
            if saturated {
                outgoing_cursor[node.as_usize()] = outgoing_cursor[node.as_usize()]
                    .checked_add(1)
                    .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
            }
            record_event(
                recorder.as_deref_mut(),
                graph,
                state,
                *metrics,
                event_metadata(
                    BlockingPreflowExecutionPreset::Karzanov,
                    BlockingPreflowEvent::KarzanovPush,
                )?,
                TraceView {
                    labels: level.labels()?,
                    search_order: vec![node, arc.to],
                    path: vec![id],
                },
                Some(("delta", i128::from(delta))),
                transitions,
            )?;
        }
    }
    Ok(())
}

fn select_karzanov_balance(
    level: &LayeredNetwork,
    excess: &[u128],
    source: NodeIndex,
    sink: NodeIndex,
) -> Option<NodeIndex> {
    level
        .topological_order
        .iter()
        .copied()
        .filter(|&node| node != source && node != sink && excess[node.as_usize()] > 0)
        .max_by_key(|&node| {
            (
                level.distances[node.as_usize()].unwrap_or(0),
                node.as_usize(),
            )
        })
}

#[allow(clippy::too_many_arguments)]
fn balance_karzanov_vertex(
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    source: NodeIndex,
    level: &LayeredNetwork,
    node: NodeIndex,
    excess: &mut [u128],
    contributions: &mut [Vec<Contribution>],
    frozen: &mut BTreeSet<ResidualArcId>,
    metrics: &mut BlockingPreflowMetrics,
    mut recorder: Option<&mut FlowTraceRecorder<'_>>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    while excess[node.as_usize()] > 0 {
        let mut contribution = contributions[node.as_usize()]
            .pop()
            .ok_or(BlockingPreflowError::Invariant)?;
        let layer_arc = state
            .arc(&contribution.arc)
            .ok_or(BlockingPreflowError::Invariant)?;
        let delta: u64 = excess[node.as_usize()]
            .min(u128::from(contribution.amount))
            .try_into()
            .map_err(|_| BlockingPreflowError::ArithmeticOverflow)?;
        let reverse = reverse_id(&contribution.arc);
        let reverse_capacity = state
            .arc(&reverse)
            .ok_or(BlockingPreflowError::Invariant)?
            .capacity;
        state.augment(std::slice::from_ref(&reverse), delta)?;
        excess[node.as_usize()] = excess[node.as_usize()]
            .checked_sub(u128::from(delta))
            .ok_or(BlockingPreflowError::Invariant)?;
        if layer_arc.from != source {
            excess[layer_arc.from.as_usize()] = excess[layer_arc.from.as_usize()]
                .checked_add(u128::from(delta))
                .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
        }
        contribution.amount = contribution
            .amount
            .checked_sub(delta)
            .ok_or(BlockingPreflowError::Invariant)?;
        if contribution.amount > 0 {
            contributions[node.as_usize()].push(contribution);
        }
        count_push(metrics, delta == reverse_capacity)?;
        record_event(
            recorder.as_deref_mut(),
            graph,
            state,
            *metrics,
            event_metadata(
                BlockingPreflowExecutionPreset::Karzanov,
                BlockingPreflowEvent::KarzanovBalance,
            )?,
            TraceView {
                labels: level.labels()?,
                search_order: vec![node, layer_arc.from],
                path: vec![reverse],
            },
            Some(("delta", i128::from(delta))),
            transitions,
        )?;
    }
    metrics.balancing_iterations = checked_increment(metrics.balancing_iterations)?;
    let incoming = level.incoming[node.as_usize()].clone();
    frozen.extend(incoming.iter().cloned());
    record_event(
        recorder,
        graph,
        state,
        *metrics,
        event_metadata(
            BlockingPreflowExecutionPreset::Karzanov,
            BlockingPreflowEvent::KarzanovFreeze,
        )?,
        TraceView {
            labels: level.labels()?,
            search_order: vec![node],
            path: incoming.clone(),
        },
        Some((
            "incoming-arcs",
            i128::try_from(incoming.len()).map_err(|_| BlockingPreflowError::ArithmeticOverflow)?,
        )),
        transitions,
    )?;
    Ok(())
}

fn reverse_id(id: &ResidualArcId) -> ResidualArcId {
    ResidualArcId::new(
        id.original_edge().clone(),
        match id.direction() {
            ResidualDirection::Forward => ResidualDirection::Reverse,
            ResidualDirection::Reverse => ResidualDirection::Forward,
        },
    )
}

fn count_push(
    metrics: &mut BlockingPreflowMetrics,
    saturating: bool,
) -> Result<(), BlockingPreflowError> {
    metrics.pushes = checked_increment(metrics.pushes)?;
    if metrics.pushes > BLOCKING_PREFLOW_MAX_STATE_TRANSITIONS {
        return Err(BlockingPreflowError::WorkLimit);
    }
    if saturating {
        metrics.saturating_pushes = checked_increment(metrics.saturating_pushes)?;
    } else {
        metrics.nonsaturating_pushes = checked_increment(metrics.nonsaturating_pushes)?;
    }
    Ok(())
}

fn increment_scans(metrics: &mut BlockingPreflowMetrics) -> Result<(), BlockingPreflowError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(BlockingPreflowError::ArithmeticOverflow)?;
    if metrics.residual_arc_scans > BLOCKING_PREFLOW_MAX_RESIDUAL_ARC_SCANS {
        return Err(BlockingPreflowError::WorkLimit);
    }
    Ok(())
}

const fn checked_increment(value: u64) -> Result<u64, BlockingPreflowError> {
    match value.checked_add(1) {
        Some(value) => Ok(value),
        None => Err(BlockingPreflowError::ArithmeticOverflow),
    }
}

fn checked_detail(value: u128) -> Result<i128, BlockingPreflowError> {
    i128::try_from(value).map_err(|_| BlockingPreflowError::ArithmeticOverflow)
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    metrics: BlockingPreflowMetrics,
    with_trace: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, BlockingPreflowError> {
    if !with_trace {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        current_excess(graph, state)?,
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot)
        .map(Some)
        .map_err(Into::into)
}

struct TraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
}

impl TraceView {
    fn from_levels(level: &LayeredNetwork) -> Result<Self, BlockingPreflowError> {
        Ok(Self {
            labels: level.labels()?,
            search_order: level.search_order.clone(),
            path: Vec::new(),
        })
    }

    fn empty(graph: &FlowNetwork) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
        }
    }
}

impl LayeredNetwork {
    fn labels(&self) -> Result<Vec<Option<i128>>, BlockingPreflowError> {
        self.distances
            .iter()
            .map(|distance| {
                distance
                    .map(|value| {
                        i128::try_from(value).map_err(|_| BlockingPreflowError::ArithmeticOverflow)
                    })
                    .transpose()
            })
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn record_event(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: BlockingPreflowMetrics,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
    transitions: &mut u64,
) -> Result<(), BlockingPreflowError> {
    *transitions = checked_increment(*transitions)?;
    if *transitions > BLOCKING_PREFLOW_MAX_STATE_TRANSITIONS {
        return Err(BlockingPreflowError::WorkLimit);
    }
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.labels,
        view.search_order,
        view.path,
        current_excess(graph, state)?,
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
    Ok(())
}

fn current_excess(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
) -> Result<Vec<i128>, BlockingPreflowError> {
    divergences(graph, state.flows())?
        .into_iter()
        .map(|value| {
            value
                .checked_neg()
                .ok_or(BlockingPreflowError::ArithmeticOverflow)
        })
        .collect()
}

const fn trace_metrics(metrics: BlockingPreflowMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.bfs_runs as u128,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: 0,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: metrics.blocking_flow_phases as u128,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: metrics.pushes as u128,
        saturating_pushes: metrics.saturating_pushes as u128,
        nonsaturating_pushes: metrics.nonsaturating_pushes as u128,
        discharges: metrics.balancing_iterations as u128,
        active_vertex_selections: metrics.vertex_eliminations as u128,
    }
}

#[derive(Clone, Copy)]
enum BlockingPreflowEvent {
    LevelBfs,
    KarzanovInitialize,
    KarzanovPush,
    KarzanovBalance,
    KarzanovFreeze,
    MpmSelect,
    MpmPushForward,
    MpmPushBackward,
    MpmRemove,
    BlockingFlow,
    Optimal,
}

fn event_metadata(
    preset: BlockingPreflowExecutionPreset,
    event: BlockingPreflowEvent,
) -> Result<FlowTraceEventMetadata, BlockingPreflowError> {
    match preset {
        BlockingPreflowExecutionPreset::Karzanov => karzanov_event_metadata(event),
        BlockingPreflowExecutionPreset::Mpm => mpm_event_metadata(event),
    }
}

fn karzanov_event_metadata(
    event: BlockingPreflowEvent,
) -> Result<FlowTraceEventMetadata, BlockingPreflowError> {
    Ok(match event {
        BlockingPreflowEvent::LevelBfs => trace_metadata(
            "karzanov-preflow.level-bfs",
            TraceGranularityV1::Phase,
            "karzanov-preflow:build-shortest-path-layered-network",
        ),
        BlockingPreflowEvent::KarzanovInitialize => trace_metadata(
            "karzanov-preflow.initialize-preflow",
            TraceGranularityV1::Operation,
            "karzanov-preflow:saturate-layer-arcs-leaving-source",
        ),
        BlockingPreflowEvent::KarzanovPush => trace_metadata(
            "karzanov-preflow.push",
            TraceGranularityV1::Micro,
            "karzanov-preflow:push-excess-forward-in-topological-order",
        ),
        BlockingPreflowEvent::KarzanovBalance => trace_metadata(
            "karzanov-preflow.balance",
            TraceGranularityV1::Micro,
            "karzanov-preflow:return-stranded-excess-by-lifo-incoming-stack",
        ),
        BlockingPreflowEvent::KarzanovFreeze => trace_metadata(
            "karzanov-preflow.freeze-incoming",
            TraceGranularityV1::Operation,
            "karzanov-preflow:freeze-arcs-entering-balanced-vertex",
        ),
        BlockingPreflowEvent::BlockingFlow => trace_metadata(
            "karzanov-preflow.blocking-flow",
            TraceGranularityV1::Phase,
            "karzanov-preflow:complete-layered-network-blocking-flow",
        ),
        BlockingPreflowEvent::Optimal => trace_metadata(
            "karzanov-preflow.optimal",
            TraceGranularityV1::Phase,
            "karzanov-preflow:return-certified-max-flow-min-cut",
        ),
        BlockingPreflowEvent::MpmSelect
        | BlockingPreflowEvent::MpmPushForward
        | BlockingPreflowEvent::MpmPushBackward
        | BlockingPreflowEvent::MpmRemove => return Err(BlockingPreflowError::Invariant),
    })
}

fn mpm_event_metadata(
    event: BlockingPreflowEvent,
) -> Result<FlowTraceEventMetadata, BlockingPreflowError> {
    Ok(match event {
        BlockingPreflowEvent::LevelBfs => trace_metadata(
            "mpm.level-bfs",
            TraceGranularityV1::Phase,
            "mpm:build-shortest-path-layered-network",
        ),
        BlockingPreflowEvent::MpmSelect => trace_metadata(
            "mpm.select-potential",
            TraceGranularityV1::Operation,
            "mpm:select-stable-minimum-vertex-potential",
        ),
        BlockingPreflowEvent::MpmPushForward => trace_metadata(
            "mpm.push-forward",
            TraceGranularityV1::Micro,
            "mpm:distribute-potential-toward-sink",
        ),
        BlockingPreflowEvent::MpmPushBackward => trace_metadata(
            "mpm.push-backward",
            TraceGranularityV1::Micro,
            "mpm:distribute-potential-backward-toward-source",
        ),
        BlockingPreflowEvent::MpmRemove => trace_metadata(
            "mpm.remove-vertex",
            TraceGranularityV1::Operation,
            "mpm:delete-selected-vertex-and-update-potentials",
        ),
        BlockingPreflowEvent::BlockingFlow => trace_metadata(
            "mpm.blocking-flow",
            TraceGranularityV1::Phase,
            "mpm:complete-layered-network-blocking-flow",
        ),
        BlockingPreflowEvent::Optimal => trace_metadata(
            "mpm.optimal",
            TraceGranularityV1::Phase,
            "mpm:return-certified-max-flow-min-cut",
        ),
        BlockingPreflowEvent::KarzanovInitialize
        | BlockingPreflowEvent::KarzanovPush
        | BlockingPreflowEvent::KarzanovBalance
        | BlockingPreflowEvent::KarzanovFreeze => return Err(BlockingPreflowError::Invariant),
    })
}

const fn trace_metadata(
    catalog_id: &'static str,
    minimum_granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
) -> FlowTraceEventMetadata {
    FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(
        node_ids: &[&str],
        edges: &[(&str, &str, &str, u64, u64)],
    ) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
                .collect(),
            edges
                .iter()
                .map(|&(id, from, to, lower, capacity)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge id"),
                    from: NodeId::parse(from).expect("tail"),
                    to: NodeId::parse(to).expect("head"),
                    lower,
                    capacity,
                    cost: 0,
                })
                .collect(),
        )
        .expect("valid graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (graph, source, sink)
    }

    #[test]
    fn both_cubic_blocking_solvers_match_the_independent_certificate() {
        let (graph, source, sink) = network(
            &["s", "a", "b", "c", "d", "t"],
            &[
                ("sa", "s", "a", 0, 7),
                ("sb", "s", "b", 0, 5),
                ("ac", "a", "c", 0, 4),
                ("ad", "a", "d", 0, 3),
                ("bc", "b", "c", 0, 2),
                ("bd", "b", "d", 0, 4),
                ("ct", "c", "t", 0, 6),
                ("dt", "d", "t", 0, 7),
            ],
        );
        let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
        for actual in [
            solve_karzanov_preflow(&graph, source, sink).expect("Karzanov"),
            solve_mpm(&graph, source, sink).expect("MPM"),
        ] {
            assert_eq!(actual.certificate, expected.certificate);
            assert!(actual.metrics.blocking_flow_phases > 0);
            assert!(actual.metrics.pushes > 0);
            assert_eq!(
                actual.metrics.pushes,
                actual.metrics.saturating_pushes + actual.metrics.nonsaturating_pushes
            );
        }
    }

    #[test]
    fn karzanov_balances_a_dead_layer_branch_and_mpm_removes_a_zero_potential_vertex() {
        let (graph, source, sink) = network(
            &["s", "a", "x", "y", "t"],
            &[
                ("sa", "s", "a", 0, 3),
                ("at", "a", "t", 0, 3),
                ("sx", "s", "x", 0, 2),
                ("xy", "x", "y", 0, 2),
            ],
        );
        let karzanov = trace_karzanov_preflow(&graph, source, sink).expect("Karzanov trace");
        assert!(karzanov.result.metrics.balancing_iterations > 0);
        assert!(karzanov.events.iter().any(|event| {
            event.catalog_id == "karzanov-preflow.freeze-incoming"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.label == "incoming-arcs")
        }));
        let mpm = trace_mpm(&graph, source, sink).expect("MPM trace");
        assert!(mpm.result.metrics.vertex_eliminations > 0);
        assert!(mpm.events.iter().any(|event| {
            event.catalog_id == "mpm.select-potential"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.value == 0)
        }));
    }

    #[test]
    fn traces_replay_forward_and_reverse_and_fast_results_match() {
        let (graph, source, sink) = network(
            &["s", "a", "b", "t"],
            &[
                ("sa", "s", "a", 1, 5),
                ("sb", "s", "b", 0, 4),
                ("ab", "a", "b", 0, 2),
                ("at", "a", "t", 1, 3),
                ("bt", "b", "t", 0, 6),
            ],
        );
        let fixtures = [
            (
                solve_karzanov_preflow(&graph, source, sink).expect("Karzanov fast"),
                trace_karzanov_preflow(&graph, source, sink).expect("Karzanov trace"),
            ),
            (
                solve_mpm(&graph, source, sink).expect("MPM fast"),
                trace_mpm(&graph, source, sink).expect("MPM trace"),
            ),
        ];
        for (fast, traced) in fixtures {
            assert_eq!(traced.result, fast);
            let mut replay = traced.base_snapshot.clone();
            for event in &traced.events {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                    .expect("forward replay");
            }
            assert_eq!(replay, traced.final_snapshot);
            assert_eq!(replay.flows, fast.flows);
            for event in traced.events.iter().rev() {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                    .expect("reverse replay");
            }
            assert_eq!(replay, traced.base_snapshot);
        }
    }

    #[test]
    fn bounded_cyclic_capacity_family_matches_edmonds_karp() {
        const ARCS: [(&str, &str, &str); 10] = [
            ("sa", "s", "a"),
            ("sb", "s", "b"),
            ("st", "s", "t"),
            ("as", "a", "s"),
            ("ab", "a", "b"),
            ("at", "a", "t"),
            ("bs", "b", "s"),
            ("ba", "b", "a"),
            ("bt", "b", "t"),
            ("ta", "t", "a"),
        ];
        for seed in 0_u64..32 {
            let edges = ARCS
                .iter()
                .enumerate()
                .map(|(index, &(id, from, to))| {
                    let capacity = (seed
                        .wrapping_mul(17)
                        .wrapping_add((index as u64).wrapping_mul(29)))
                        % 11;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&["s", "a", "b", "t"], &edges);
            let expected = solve_edmonds_karp(&graph, source, sink).expect("oracle");
            for actual in [
                solve_karzanov_preflow(&graph, source, sink).expect("Karzanov"),
                solve_mpm(&graph, source, sink).expect("MPM"),
            ] {
                assert_eq!(
                    actual.certificate.value, expected.certificate.value,
                    "capacity fixture {seed}"
                );
                assert_eq!(actual.certificate.value, actual.certificate.cut_bound);
            }
        }
    }
}
