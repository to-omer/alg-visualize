//! Sleator--Tarjan dynamic-tree implementation of Dinic blocking flows.

use std::collections::VecDeque;

use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use thiserror::Error;

use super::data_structures::link_cut::{
    DynamicTreeEdge, DynamicTreeVertex, LinkCutError, LinkCutForest,
};
use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{
    ResidualArc, ResidualArcId, ResidualDirection, ResidualError, ResidualState,
};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative node limit for a fully observable dynamic-tree run.
pub const DYNAMIC_TREE_BLOCKING_MAX_NODES: usize = 256;
/// Conservative edge limit for a fully observable dynamic-tree run.
pub const DYNAMIC_TREE_BLOCKING_MAX_EDGES: usize = 2_048;
/// Deterministic positive-residual scan ceiling for fast and trace profiles.
pub const DYNAMIC_TREE_BLOCKING_MAX_RESIDUAL_ARC_SCANS: u128 = 10_000_000;
/// Deterministic link/update/cut/prune ceiling for fast and trace profiles.
pub const DYNAMIC_TREE_BLOCKING_MAX_STATE_TRANSITIONS: u64 = 100_000;
/// Eager semantic event ceiling for the observable trace profile.
pub const DYNAMIC_TREE_BLOCKING_MAX_TRACE_EVENTS: usize = 25_000;
/// Preserve every small residual scan and geometric progress on larger inputs.
const DYNAMIC_TREE_BLOCKING_TRACE_SCAN_PREFIX: u128 = 512;

/// Exact counters from the dynamic-tree level-graph kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeBlockingMetrics {
    /// Level BFS invocations, including the final failed search.
    pub bfs_runs: u64,
    /// Positive residual arcs inspected by BFS or level-graph construction.
    pub residual_arc_scans: u128,
    /// Completed reachable blocking-flow phases.
    pub blocking_flow_phases: u64,
    /// Complete source-to-sink path augmentations.
    pub augmentations: u64,
    /// Candidate arcs linked into the represented forest.
    pub tree_links: u64,
    /// Saturated or pruned represented arcs cut from the forest.
    pub tree_cuts: u64,
    /// Root-path minimum queries, including zero-edge cleanup queries.
    pub path_minimum_queries: u64,
    /// Lazy root-path residual-capacity updates.
    pub path_updates: u64,
    /// Dead roots pruned by deleting every incoming level arc.
    pub dead_end_prunes: u64,
    /// Link, root-path update, cut, and dead-root-prune transitions.
    pub state_transitions: u64,
}

/// Certified maximum flow produced with dynamic-tree blocking phases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeBlockingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact source-conformance counters.
    pub metrics: DynamicTreeBlockingMetrics,
}

/// Certified result and reversible dynamic-tree trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeBlockingTraceResult {
    /// Same canonical result as the fast profile.
    pub result: DynamicTreeBlockingResult,
    /// Boundary before the first BFS.
    pub base_snapshot: FlowTraceSnapshot,
    /// Complete semantic event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Independently verified optimum boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Dynamic-tree blocking-flow construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicTreeBlockingError {
    /// Graph exceeds the bounded interactive implementation band.
    #[error("graph exceeds dynamic-tree blocking-flow admission limits")]
    AdmissionLimit,
    /// Lower-bound feasible-flow construction failed.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation or reconstruction failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent certificate rejected the candidate.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// Exact operation counter overflowed.
    #[error("dynamic-tree blocking-flow metric overflow")]
    MetricOverflow,
    /// Deterministic scan or state-transition budget was exhausted.
    #[error("dynamic-tree blocking-flow deterministic work limit exceeded")]
    WorkLimit,
    /// Level-graph or represented-forest state contradicted the source algorithm.
    #[error("dynamic-tree blocking-flow invariant failed: {0}")]
    Invariant(String),
}

/// Solves maximum flow with Sleator--Tarjan dynamic-tree blocking phases.
///
/// # Errors
///
/// Rejects out-of-band input, infeasible lower bounds, a dynamic-tree or level
/// invariant failure, metric overflow, or a failed independent certificate.
pub fn solve_dynamic_tree_blocking_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DynamicTreeBlockingResult, DynamicTreeBlockingError> {
    solve_internal(graph, source, sink, false).map(|run| run.result)
}

/// Solves maximum flow while publishing link, path-update, cut, and prune events.
///
/// # Errors
///
/// Returns the same failures as [`solve_dynamic_tree_blocking_flow`], plus
/// reversible trace validation failures.
pub fn trace_dynamic_tree_blocking_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<DynamicTreeBlockingTraceResult, DynamicTreeBlockingError> {
    let run = solve_internal(graph, source, sink, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or_else(|| {
        DynamicTreeBlockingError::Invariant("trace recorder result is absent".to_owned())
    })?;
    Ok(DynamicTreeBlockingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Solves dynamic-tree blocking flow while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_dynamic_tree_blocking_flow_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreeBlockingResult, DynamicTreeBlockingError> {
    solve_internal_with_feasibility(graph, source, sink, false, feasibility).map(|run| run.result)
}

/// Traces dynamic-tree blocking flow while explicitly publishing auxiliary
/// feasibility work to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_dynamic_tree_blocking_flow_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreeBlockingTraceResult, DynamicTreeBlockingError> {
    let run = solve_internal_with_feasibility(graph, source, sink, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or_else(|| {
        DynamicTreeBlockingError::Invariant("trace recorder result is absent".to_owned())
    })?;
    Ok(DynamicTreeBlockingTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: DynamicTreeBlockingResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
) -> Result<InternalRun, DynamicTreeBlockingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, source, sink, with_trace, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    with_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, DynamicTreeBlockingError> {
    if graph.nodes().len() > DYNAMIC_TREE_BLOCKING_MAX_NODES
        || graph.edges().len() > DYNAMIC_TREE_BLOCKING_MAX_EDGES
    {
        return Err(DynamicTreeBlockingError::AdmissionLimit);
    }
    let initial =
        feasibility.find_max_flow_initial(graph, source, sink, FeasibilityUse::InitialFlow)?;
    let mut state = ResidualState::from_flows(graph, &initial.flows)?;
    let mut metrics = DynamicTreeBlockingMetrics::default();
    let mut recorder = if with_trace {
        let snapshot = trace_snapshot(
            graph,
            &state,
            &vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            metrics,
        );
        Some(FlowTraceRecorder::new(graph, snapshot)?)
    } else {
        None
    };

    loop {
        increment(&mut metrics.bfs_runs)?;
        let (level, level_scan_checkpoints) =
            build_levels(&state, source, &mut metrics, with_trace)?;
        for checkpoint in level_scan_checkpoints {
            record(
                recorder.as_mut(),
                graph,
                &state,
                &checkpoint.level,
                Vec::new(),
                vec![checkpoint.arc],
                checkpoint.metrics,
                "dynamic-tree-blocking.inspect-residual-arc",
                TraceGranularityV1::Micro,
                "dynamic-tree-blocking:inspect-residual-arc",
            )?;
        }
        record(
            recorder.as_mut(),
            graph,
            &state,
            &level,
            Vec::new(),
            Vec::new(),
            metrics,
            "dynamic-tree-blocking.level-bfs",
            TraceGranularityV1::Phase,
            "dynamic-tree-blocking:build-level-graph",
        )?;
        if level.distances[sink.as_usize()].is_none() {
            break;
        }
        let (level_arcs, level_arc_checkpoints) =
            build_level_arcs(&state, &level, &mut metrics, with_trace)?;
        for checkpoint in level_arc_checkpoints {
            record(
                recorder.as_mut(),
                graph,
                &state,
                &checkpoint.level,
                Vec::new(),
                vec![checkpoint.arc],
                checkpoint.metrics,
                "dynamic-tree-blocking.inspect-residual-arc",
                TraceGranularityV1::Micro,
                "dynamic-tree-blocking:classify-level-arc",
            )?;
        }
        let mut phase = BlockingPhase::new(graph.nodes().len(), level_arcs)?;
        state = phase.run(
            graph,
            &state,
            source,
            sink,
            &level,
            &mut metrics,
            recorder.as_mut(),
        )?;
        increment(&mut metrics.blocking_flow_phases)?;
        record(
            recorder.as_mut(),
            graph,
            &state,
            &level,
            phase.forest_arcs(),
            Vec::new(),
            metrics,
            "dynamic-tree-blocking.blocking-flow",
            TraceGranularityV1::Phase,
            "dynamic-tree-blocking:complete-blocking-flow",
        )?;
    }

    let flows = state.flows().to_vec();
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    record(
        recorder.as_mut(),
        graph,
        &state,
        &LevelGraph::empty(graph.nodes().len()),
        Vec::new(),
        Vec::new(),
        metrics,
        "dynamic-tree-blocking.optimal",
        TraceGranularityV1::Phase,
        "dynamic-tree-blocking:return-certified-flow",
    )?;
    Ok(InternalRun {
        result: DynamicTreeBlockingResult {
            flows,
            certificate,
            metrics,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

#[derive(Clone, Debug)]
struct LevelGraph {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

impl LevelGraph {
    fn empty(node_count: usize) -> Self {
        Self {
            distances: vec![None; node_count],
            search_order: Vec::new(),
        }
    }

    fn focused_arc(&self, from: NodeIndex, to: NodeIndex) -> Self {
        Self {
            distances: self.distances.clone(),
            search_order: vec![from, to],
        }
    }
}

#[derive(Clone, Debug)]
struct LevelScanCheckpoint {
    level: LevelGraph,
    arc: ResidualArcId,
    metrics: DynamicTreeBlockingMetrics,
}

fn build_levels(
    state: &ResidualState<'_>,
    source: NodeIndex,
    metrics: &mut DynamicTreeBlockingMetrics,
    record_trace: bool,
) -> Result<(LevelGraph, Vec<LevelScanCheckpoint>), DynamicTreeBlockingError> {
    let mut distances = vec![None; state.graph().nodes().len()];
    let mut search_order = vec![source];
    let mut queue = VecDeque::from([source]);
    let mut checkpoints = Vec::new();
    distances[source.as_usize()] = Some(0_i128);
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            increment_scan(metrics)?;
            if distances[arc.to.as_usize()].is_none() {
                distances[arc.to.as_usize()] =
                    distances[node.as_usize()].and_then(|distance| distance.checked_add(1));
                let Some(_) = distances[arc.to.as_usize()] else {
                    return Err(DynamicTreeBlockingError::MetricOverflow);
                };
                search_order.push(arc.to);
                queue.push_back(arc.to);
            }
            record_level_scan_checkpoint(
                record_trace,
                metrics,
                &distances,
                arc.id,
                arc.from,
                arc.to,
                &mut checkpoints,
            );
        }
    }
    Ok((
        LevelGraph {
            distances,
            search_order,
        },
        checkpoints,
    ))
}

#[derive(Clone, Debug)]
struct LevelArc {
    id: ResidualArcId,
    from: NodeIndex,
    to: NodeIndex,
    capacity: u64,
}

fn build_level_arcs(
    state: &ResidualState<'_>,
    level: &LevelGraph,
    metrics: &mut DynamicTreeBlockingMetrics,
    record_trace: bool,
) -> Result<(Vec<LevelArc>, Vec<LevelScanCheckpoint>), DynamicTreeBlockingError> {
    let mut result = Vec::new();
    let mut checkpoints = Vec::new();
    for &node in &level.search_order {
        let from_level = level.distances[node.as_usize()].ok_or_else(|| {
            DynamicTreeBlockingError::Invariant("reachable level node has no distance".to_owned())
        })?;
        for arc in state.outgoing_arcs(node) {
            let arc_id = arc.id.clone();
            let arc_from = arc.from;
            let arc_to = arc.to;
            increment_scan(metrics)?;
            if level.distances[arc.to.as_usize()] == from_level.checked_add(1) {
                result.push(LevelArc::from(arc));
            }
            record_level_scan_checkpoint(
                record_trace,
                metrics,
                &level.distances,
                arc_id,
                arc_from,
                arc_to,
                &mut checkpoints,
            );
        }
    }
    Ok((result, checkpoints))
}

fn record_level_scan_checkpoint(
    record_trace: bool,
    metrics: &DynamicTreeBlockingMetrics,
    distances: &[Option<i128>],
    arc: ResidualArcId,
    from: NodeIndex,
    to: NodeIndex,
    checkpoints: &mut Vec<LevelScanCheckpoint>,
) {
    if record_trace
        && (metrics.residual_arc_scans <= DYNAMIC_TREE_BLOCKING_TRACE_SCAN_PREFIX
            || metrics.residual_arc_scans.is_power_of_two())
    {
        checkpoints.push(LevelScanCheckpoint {
            level: LevelGraph {
                distances: distances.to_vec(),
                // Distances retain the complete BFS state, while Detail focus
                // names only the primitive residual-arc inspection.
                search_order: vec![from, to],
            },
            arc,
            metrics: *metrics,
        });
    }
}

impl From<ResidualArc> for LevelArc {
    fn from(arc: ResidualArc) -> Self {
        Self {
            id: arc.id,
            from: arc.from,
            to: arc.to,
            capacity: arc.capacity,
        }
    }
}

struct BlockingPhase {
    arcs: Vec<LevelArc>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    current: Vec<usize>,
    deleted: Vec<bool>,
    active: Vec<bool>,
    tree_outgoing: Vec<Option<usize>>,
    used: Vec<u64>,
    vertices: Vec<DynamicTreeVertex>,
    edges: Vec<DynamicTreeEdge>,
    forest: LinkCutForest,
}

impl BlockingPhase {
    fn new(node_count: usize, arcs: Vec<LevelArc>) -> Result<Self, DynamicTreeBlockingError> {
        let mut outgoing = vec![Vec::new(); node_count];
        let mut incoming = vec![Vec::new(); node_count];
        for (slot, arc) in arcs.iter().enumerate() {
            outgoing[arc.from.as_usize()].push(slot);
            incoming[arc.to.as_usize()].push(slot);
        }
        let forest = LinkCutForest::new(node_count, arcs.len());
        let vertices = (0..node_count)
            .map(|index| tree(forest.vertex(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let edges = (0..arcs.len())
            .map(|index| tree(forest.edge(index)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            current: vec![0; node_count],
            deleted: vec![false; arcs.len()],
            active: vec![false; arcs.len()],
            tree_outgoing: vec![None; node_count],
            used: vec![0; arcs.len()],
            arcs,
            outgoing,
            incoming,
            vertices,
            edges,
            forest,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn run<'graph>(
        &mut self,
        graph: &'graph FlowNetwork,
        base_state: &ResidualState<'graph>,
        source: NodeIndex,
        sink: NodeIndex,
        level: &LevelGraph,
        metrics: &mut DynamicTreeBlockingMetrics,
        mut recorder: Option<&mut FlowTraceRecorder<'graph>>,
    ) -> Result<ResidualState<'graph>, DynamicTreeBlockingError> {
        loop {
            let root = tree(
                self.forest
                    .represented_root(self.vertices[source.as_usize()]),
            )?;
            if root == self.vertices[sink.as_usize()] {
                increment(&mut metrics.path_minimum_queries)?;
                let minimum = tree(
                    self.forest
                        .root_path_minimum(self.vertices[source.as_usize()]),
                )?
                .ok_or_else(|| {
                    DynamicTreeBlockingError::Invariant(
                        "source-to-root path has no represented edge".to_owned(),
                    )
                })?;
                let amount = minimum
                    .value
                    .to_u64()
                    .filter(|amount| *amount > 0)
                    .ok_or_else(|| {
                        DynamicTreeBlockingError::Invariant(
                            "root-path minimum is not a positive u64".to_owned(),
                        )
                    })?;
                tree(
                    self.forest
                        .root_path_add(self.vertices[source.as_usize()], &-BigInt::from(amount)),
                )?;
                increment(&mut metrics.path_updates)?;
                increment(&mut metrics.augmentations)?;
                increment_transition(metrics)?;
                if recorder.is_some() {
                    let path = self.active_path(source, sink)?;
                    let current_state = self.progress_state(graph, base_state)?;
                    record(
                        recorder.as_deref_mut(),
                        graph,
                        &current_state,
                        level,
                        self.forest_arcs(),
                        path,
                        *metrics,
                        "dynamic-tree-blocking.augment-root-path",
                        TraceGranularityV1::Operation,
                        "dynamic-tree-blocking:update-root-path",
                    )?;
                }

                loop {
                    increment(&mut metrics.path_minimum_queries)?;
                    let Some(candidate) = tree(
                        self.forest
                            .root_path_minimum_closest_to_root(self.vertices[source.as_usize()]),
                    )?
                    else {
                        break;
                    };
                    if candidate.value.is_negative() {
                        return Err(DynamicTreeBlockingError::Invariant(
                            "root-path residual minimum became negative".to_owned(),
                        ));
                    }
                    if !candidate.value.is_zero() {
                        break;
                    }
                    let slot = candidate.edge.index();
                    self.finalize_and_cut(slot)?;
                    self.deleted[slot] = true;
                    increment(&mut metrics.tree_cuts)?;
                    increment_transition(metrics)?;
                    if recorder.is_some() {
                        let current_state = self.progress_state(graph, base_state)?;
                        record(
                            recorder.as_deref_mut(),
                            graph,
                            &current_state,
                            level,
                            self.forest_arcs(),
                            Vec::new(),
                            *metrics,
                            "dynamic-tree-blocking.cut-saturated",
                            TraceGranularityV1::Operation,
                            "dynamic-tree-blocking:cut-zero-residual-edge",
                        )?;
                    }
                }
                continue;
            }

            let root_index = root.index();
            if let Some(slot) = self.next_outgoing(root_index) {
                let arc = self.arcs[slot].clone();
                tree(self.forest.link_rooted(
                    self.edges[slot],
                    self.vertices[arc.from.as_usize()],
                    self.vertices[arc.to.as_usize()],
                    BigInt::from(arc.capacity),
                ))?;
                self.active[slot] = true;
                self.tree_outgoing[root_index] = Some(slot);
                increment(&mut metrics.tree_links)?;
                increment_transition(metrics)?;
                if recorder.is_some() {
                    let current_state = self.progress_state(graph, base_state)?;
                    let focused_level = level.focused_arc(arc.from, arc.to);
                    record(
                        recorder.as_deref_mut(),
                        graph,
                        &current_state,
                        &focused_level,
                        self.forest_arcs(),
                        vec![arc.id],
                        *metrics,
                        "dynamic-tree-blocking.link-candidate",
                        TraceGranularityV1::Micro,
                        "dynamic-tree-blocking:link-next-outgoing-edge",
                    )?;
                }
                continue;
            }

            if root_index == source.as_usize() {
                break;
            }
            let incoming = self.incoming[root_index].clone();
            for slot in incoming {
                if self.deleted[slot] {
                    continue;
                }
                if self.active[slot] {
                    self.finalize_and_cut(slot)?;
                    increment(&mut metrics.tree_cuts)?;
                }
                self.deleted[slot] = true;
            }
            increment(&mut metrics.dead_end_prunes)?;
            increment_transition(metrics)?;
            if recorder.is_some() {
                let current_state = self.progress_state(graph, base_state)?;
                record(
                    recorder.as_deref_mut(),
                    graph,
                    &current_state,
                    level,
                    self.forest_arcs(),
                    Vec::new(),
                    *metrics,
                    "dynamic-tree-blocking.prune-dead-root",
                    TraceGranularityV1::Operation,
                    "dynamic-tree-blocking:delete-incoming-dead-root-arcs",
                )?;
            }
        }

        self.finalize_remaining()?;
        self.progress_state(graph, base_state)
    }

    fn next_outgoing(&mut self, node: usize) -> Option<usize> {
        let candidates = &self.outgoing[node];
        while self.current[node] < candidates.len() {
            let slot = candidates[self.current[node]];
            self.current[node] += 1;
            if !self.deleted[slot] {
                return Some(slot);
            }
        }
        None
    }

    fn active_path(
        &self,
        source: NodeIndex,
        sink: NodeIndex,
    ) -> Result<Vec<ResidualArcId>, DynamicTreeBlockingError> {
        let mut result = Vec::new();
        let mut node = source.as_usize();
        while node != sink.as_usize() {
            if result.len() >= self.vertices.len() {
                return Err(DynamicTreeBlockingError::Invariant(
                    "represented source path contains a cycle".to_owned(),
                ));
            }
            let slot = self.tree_outgoing[node].ok_or_else(|| {
                DynamicTreeBlockingError::Invariant(
                    "represented source path lacks an outgoing tree edge".to_owned(),
                )
            })?;
            let arc = &self.arcs[slot];
            if arc.from.as_usize() != node || !self.active[slot] {
                return Err(DynamicTreeBlockingError::Invariant(
                    "represented source path edge identity is inconsistent".to_owned(),
                ));
            }
            result.push(arc.id.clone());
            node = arc.to.as_usize();
        }
        Ok(result)
    }

    fn finalize_and_cut(&mut self, slot: usize) -> Result<(), DynamicTreeBlockingError> {
        let value = tree(self.forest.edge_value(self.edges[slot]))?;
        self.used[slot] = used_capacity(self.arcs[slot].capacity, &value)?;
        let cut_value = tree(self.forest.cut_rooted(self.edges[slot]))?;
        if cut_value != value {
            return Err(DynamicTreeBlockingError::Invariant(
                "cut returned a different residual value".to_owned(),
            ));
        }
        self.active[slot] = false;
        let tail = self.arcs[slot].from.as_usize();
        if self.tree_outgoing[tail] != Some(slot) {
            return Err(DynamicTreeBlockingError::Invariant(
                "cut edge is not the tail's represented outgoing edge".to_owned(),
            ));
        }
        self.tree_outgoing[tail] = None;
        Ok(())
    }

    fn finalize_remaining(&mut self) -> Result<(), DynamicTreeBlockingError> {
        for slot in 0..self.arcs.len() {
            if self.active[slot] {
                let value = tree(self.forest.edge_value(self.edges[slot]))?;
                self.used[slot] = used_capacity(self.arcs[slot].capacity, &value)?;
            }
        }
        Ok(())
    }

    fn progress_state<'graph>(
        &mut self,
        graph: &'graph FlowNetwork,
        base_state: &ResidualState<'graph>,
    ) -> Result<ResidualState<'graph>, DynamicTreeBlockingError> {
        let mut amounts = self.used.clone();
        for (slot, amount) in amounts.iter_mut().enumerate() {
            if self.active[slot] {
                let value = tree(self.forest.edge_value(self.edges[slot]))?;
                *amount = used_capacity(self.arcs[slot].capacity, &value)?;
            }
        }
        let mut flows = base_state.flows().to_vec();
        for (arc, amount) in self.arcs.iter().zip(amounts) {
            if amount == 0 {
                continue;
            }
            let edge_index = graph.edge_index(arc.id.original_edge()).ok_or_else(|| {
                DynamicTreeBlockingError::Invariant(
                    "level arc refers to an unknown original edge".to_owned(),
                )
            })?;
            let flow = flows.get_mut(edge_index.as_usize()).ok_or_else(|| {
                DynamicTreeBlockingError::Invariant(
                    "level arc original-edge index is out of range".to_owned(),
                )
            })?;
            *flow = match arc.id.direction() {
                ResidualDirection::Forward => flow.checked_add(amount),
                ResidualDirection::Reverse => flow.checked_sub(amount),
            }
            .ok_or_else(|| {
                DynamicTreeBlockingError::Invariant(
                    "level-arc progress exceeds original residual capacity".to_owned(),
                )
            })?;
        }
        ResidualState::from_flows(graph, &flows).map_err(Into::into)
    }

    fn forest_arcs(&self) -> Vec<ResidualArcId> {
        self.active
            .iter()
            .enumerate()
            .filter(|(_, active)| **active)
            .map(|(slot, _)| self.arcs[slot].id.clone())
            .collect()
    }
}

fn used_capacity(capacity: u64, remaining: &BigInt) -> Result<u64, DynamicTreeBlockingError> {
    let remaining = remaining
        .to_u64()
        .filter(|remaining| *remaining <= capacity)
        .ok_or_else(|| {
            DynamicTreeBlockingError::Invariant(
                "represented edge residual is outside its original capacity".to_owned(),
            )
        })?;
    Ok(capacity - remaining)
}

fn tree<T>(result: Result<T, LinkCutError>) -> Result<T, DynamicTreeBlockingError> {
    result.map_err(|error| DynamicTreeBlockingError::Invariant(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn record(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    level: &LevelGraph,
    forest_arcs: Vec<ResidualArcId>,
    active_path: Vec<ResidualArcId>,
    metrics: DynamicTreeBlockingMetrics,
    catalog_id: &'static str,
    minimum_granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    if recorder.event_count() >= DYNAMIC_TREE_BLOCKING_MAX_TRACE_EVENTS {
        return Err(FlowTraceError::EventLimit);
    }
    let focus_arc_ids = active_path.clone();
    let snapshot = trace_snapshot(
        graph,
        state,
        &level.distances,
        level.search_order.clone(),
        active_path,
        metrics,
    )
    .with_forest_overlay(graph, forest_arcs, Vec::new());
    let metadata = FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    };
    if minimum_granularity == TraceGranularityV1::Micro {
        let mut focus = Vec::new();
        for arc_id in &focus_arc_ids {
            let arc = state.arc(arc_id).ok_or(FlowTraceError::MissingEntity)?;
            focus.push(FlowTraceEntityRef::Node(
                graph.nodes()[arc.from.as_usize()].id().clone(),
            ));
            focus.push(FlowTraceEntityRef::Node(
                graph.nodes()[arc.to.as_usize()].id().clone(),
            ));
            focus.push(FlowTraceEntityRef::Edge(arc_id.original_edge().clone()));
            focus.push(FlowTraceEntityRef::ResidualArc(arc_id.clone()));
        }
        recorder.record_transition_with_detail_and_focus(metadata, &snapshot, None, focus)
    } else {
        recorder.record_transition(metadata, &snapshot)
    }
}

fn trace_snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    labels: &[Option<i128>],
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    metrics: DynamicTreeBlockingMetrics,
) -> FlowTraceSnapshot {
    FlowTraceSnapshot::capture(
        graph,
        state,
        labels.to_vec(),
        search_order,
        active_path,
        Vec::new(),
        FlowTraceMetrics {
            bfs_runs: u128::from(metrics.bfs_runs),
            relaxation_passes: 0,
            residual_arc_scans: metrics.residual_arc_scans,
            augmentations: u128::from(metrics.augmentations),
            path_searches: u128::from(metrics.path_minimum_queries),
            scaling_phases: 0,
            blocking_flow_phases: u128::from(metrics.blocking_flow_phases),
            relabels: 0,
            retreats: u128::from(metrics.dead_end_prunes),
            reverse_bfs_runs: 0,
            gap_terminations: 0,
            pushes: u128::from(metrics.tree_links),
            saturating_pushes: u128::from(metrics.tree_cuts),
            nonsaturating_pushes: u128::from(metrics.path_updates),
            discharges: 0,
            active_vertex_selections: u128::from(metrics.state_transitions),
        },
    )
}

fn increment(counter: &mut u64) -> Result<(), DynamicTreeBlockingError> {
    *counter = counter
        .checked_add(1)
        .ok_or(DynamicTreeBlockingError::MetricOverflow)?;
    Ok(())
}

fn increment_scan(
    metrics: &mut DynamicTreeBlockingMetrics,
) -> Result<(), DynamicTreeBlockingError> {
    metrics.residual_arc_scans = metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(DynamicTreeBlockingError::MetricOverflow)?;
    if metrics.residual_arc_scans > DYNAMIC_TREE_BLOCKING_MAX_RESIDUAL_ARC_SCANS {
        return Err(DynamicTreeBlockingError::WorkLimit);
    }
    Ok(())
}

fn increment_transition(
    metrics: &mut DynamicTreeBlockingMetrics,
) -> Result<(), DynamicTreeBlockingError> {
    increment(&mut metrics.state_transitions)?;
    if metrics.state_transitions > DYNAMIC_TREE_BLOCKING_MAX_STATE_TRANSITIONS {
        return Err(DynamicTreeBlockingError::WorkLimit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::{solve_dinic, solve_edmonds_karp};
    use crate::generator::{FlowGeneratorFamilyV1, generate_flow_graph};
    use crate::generator_fixture::generator_algorithm_fixture;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::scenario::FlowGraphV1;
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(edges: &[(&str, &str, &str, u64, u64)]) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let node_ids = ["a", "b", "s", "t"]
            .into_iter()
            .map(|id| NodeId::parse(id).expect("node id"))
            .collect::<Vec<_>>();
        let graph = FlowNetwork::new(
            node_ids
                .iter()
                .cloned()
                .map(|id| FlowNode::new(id, 0))
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

    fn generated_network(graph: &FlowGraphV1) -> (FlowNetwork, NodeIndex, NodeIndex) {
        let network = FlowNetwork::new(
            graph
                .nodes
                .iter()
                .map(|node| {
                    FlowNode::new(
                        NodeId::parse(&node.id).expect("generated node id"),
                        node.supply.parse().expect("generated supply"),
                    )
                })
                .collect(),
            graph
                .edges
                .iter()
                .map(|edge| UnresolvedFlowEdge {
                    id: EdgeId::parse(&edge.id).expect("generated edge id"),
                    from: NodeId::parse(&edge.from).expect("generated tail"),
                    to: NodeId::parse(&edge.to).expect("generated head"),
                    lower: edge.lower.parse().expect("generated lower"),
                    capacity: edge.capacity.parse().expect("generated capacity"),
                    cost: edge.cost.parse().expect("generated cost"),
                })
                .collect(),
        )
        .expect("generated network validates");
        let source = network
            .node_index(&NodeId::parse("s").expect("source id"))
            .expect("source");
        let sink = network
            .node_index(&NodeId::parse("t").expect("sink id"))
            .expect("sink");
        (network, source, sink)
    }

    #[test]
    fn links_updates_and_cuts_two_disjoint_level_paths() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 7),
            ("sb", "s", "b", 0, 5),
            ("at", "a", "t", 0, 7),
            ("bt", "b", "t", 0, 5),
        ]);
        let expected = solve_dinic(&graph, source, sink).expect("Dinic reference");
        let result = solve_dynamic_tree_blocking_flow(&graph, source, sink)
            .expect("dynamic-tree maximum flow");

        assert_eq!(result.flows, expected.flows);
        assert_eq!(result.certificate, expected.certificate);
        assert_eq!(result.metrics.blocking_flow_phases, 1);
        assert_eq!(result.metrics.augmentations, 2);
        assert_eq!(result.metrics.path_updates, 2);
        assert!(result.metrics.tree_links >= 4);
        assert!(result.metrics.tree_cuts >= 2);
    }

    #[test]
    fn prunes_a_dead_level_root_before_using_the_live_branch() {
        let (graph, source, sink) = network(&[
            ("sa", "s", "a", 0, 9),
            ("sb", "s", "b", 0, 4),
            ("bt", "b", "t", 0, 4),
        ]);
        let traced =
            trace_dynamic_tree_blocking_flow(&graph, source, sink).expect("dead branch is pruned");

        assert_eq!(traced.result.certificate.value, 4);
        assert!(traced.result.metrics.dead_end_prunes > 0);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dynamic-tree-blocking.prune-dead-root")
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "dynamic-tree-blocking.augment-root-path")
        );

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, traced.result.flows);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn lower_bounds_parallel_opposite_and_self_loop_match_dinic() {
        let (graph, source, sink) = network(&[
            ("sa0", "s", "a", 1, 5),
            ("sa1", "s", "a", 0, 2),
            ("at", "a", "t", 1, 7),
            ("as", "a", "s", 0, 3),
            ("aa", "a", "a", 0, 9),
            ("sb", "s", "b", 0, 3),
            ("bt", "b", "t", 0, 3),
        ]);
        let expected = solve_dinic(&graph, source, sink).expect("Dinic lower-bound reference");
        let fast = solve_dynamic_tree_blocking_flow(&graph, source, sink)
            .expect("dynamic-tree lower-bound maximum flow");
        let traced = trace_dynamic_tree_blocking_flow(&graph, source, sink)
            .expect("dynamic-tree lower-bound trace");

        assert_eq!(fast.certificate.value, expected.certificate.value);
        assert_eq!(fast.certificate.cut_bound, expected.certificate.cut_bound);
        assert_eq!(traced.result, fast);
        assert_eq!(traced.final_snapshot.flows, fast.flows);
        assert_eq!(fast.certificate.value, 10);
        assert!(fast.metrics.state_transitions > 0);
    }

    #[test]
    fn bounded_multigraphs_match_independent_edmonds_karp_certificates() {
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
        for seed in 0_u64..64 {
            let edges = ARCS
                .iter()
                .enumerate()
                .map(|(index, &(id, from, to))| {
                    let capacity = seed
                        .wrapping_mul(31)
                        .wrapping_add(u64::try_from(index).expect("small index") * 17)
                        % 9;
                    (id, from, to, 0, capacity)
                })
                .collect::<Vec<_>>();
            let (graph, source, sink) = network(&edges);
            let expected =
                solve_edmonds_karp(&graph, source, sink).expect("reference maximum flow");
            let actual = solve_dynamic_tree_blocking_flow(&graph, source, sink)
                .expect("dynamic-tree maximum flow");
            assert_eq!(
                actual.certificate.value, expected.certificate.value,
                "fixture {seed}"
            );
            assert_eq!(actual.certificate.value, actual.certificate.cut_bound);
        }
    }

    #[test]
    fn twenty_stage_diamond_chain_matches_reference_and_keeps_micro_focus_local() {
        let fixture = generator_algorithm_fixture("diamond-chain").expect("diamond fixture");
        let preset = fixture
            .presets
            .iter()
            .find(|preset| {
                matches!(
                    preset.spec.family,
                    FlowGeneratorFamilyV1::DiamondChain { stages: 20 }
                )
            })
            .expect("twenty-stage preset");
        let generated = generate_flow_graph(&preset.spec).expect("diamond graph");
        let (graph, source, sink) = generated_network(&generated.graph);
        let expected = solve_edmonds_karp(&graph, source, sink).expect("reference maximum flow");
        let traced = trace_dynamic_tree_blocking_flow(&graph, source, sink)
            .expect("dynamic-tree diamond trace");

        assert_eq!(traced.result.certificate.value, expected.certificate.value);
        let mut replay = traced.base_snapshot.clone();
        let mut inspected = 0;
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
            if matches!(
                event.catalog_id.as_str(),
                "dynamic-tree-blocking.inspect-residual-arc"
                    | "dynamic-tree-blocking.link-candidate"
            ) {
                inspected += 1;
                assert!(
                    event
                        .entity_refs
                        .iter()
                        .filter(|entity| matches!(entity, FlowTraceEntityRef::Node(_)))
                        .count()
                        <= 2,
                    "one residual-arc primitive may publish only its endpoint focus"
                );
                assert_eq!(
                    event
                        .entity_refs
                        .iter()
                        .filter(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                        .count(),
                    1,
                    "one residual-arc primitive must publish exactly one residual focus"
                );
                assert!(
                    replay.search_order.len() <= 2,
                    "one residual-arc primitive may focus only its endpoints"
                );
                assert_eq!(
                    replay.active_path.len(),
                    1,
                    "one residual-arc primitive must identify that arc"
                );
            }
        }
        assert!(inspected > 0, "trace must expose residual-arc inspections");
        assert_eq!(replay, traced.final_snapshot);
    }
}
