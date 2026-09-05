//! Exact bounded double-scaling minimum-cost flow.
//!
//! The implementation follows Ahuja--Goldberg--Orlin--Tarjan's explicit
//! admissible-path build. A lower-adjusted capacitated problem is transformed
//! into the uncapacitated bipartite transportation network from Ahuja,
//! Magnanti, and Orlin §2.4. Cost error `epsilon` is scaled in the outer loop;
//! integral imbalance `delta` is scaled inside each improve-approximation.

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{
    FeasibilityError, FeasibilityExecution, FeasibilityUse, find_feasible_flow,
};
use crate::model::{FlowEdge, FlowNetwork};
use crate::residual::ResidualDirection;

/// Conservative interactive node limit for the explicit transportation build.
pub const DOUBLE_SCALING_MAX_NODES: usize = 128;
/// Conservative interactive edge limit for the explicit transportation build.
pub const DOUBLE_SCALING_MAX_EDGES: usize = 1_024;
/// Deterministic ceiling for advance, retreat, relabel, and augmentation events.
pub const DOUBLE_SCALING_MAX_TRANSITIONS: u128 = 200_000;
/// Deterministic ceiling for transformed residual-arc inspections.
pub const DOUBLE_SCALING_MAX_ARC_SCANS: u128 = 20_000_000;

/// Which of the two transportation arcs derived from an original edge is used.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DoubleScalingBranch {
    /// The costed tail-to-edge-node arc; its flow is the original residual flow.
    Flow,
    /// The zero-cost head-to-edge-node slack arc.
    Slack,
}

/// Stable identity of one residual arc in the transformed transportation graph.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DoubleScalingArcId {
    /// Canonical original-edge ordinal.
    pub edge_index: usize,
    /// Costed flow branch or zero-cost slack branch.
    pub branch: DoubleScalingBranch,
    /// Forward transportation arc or its residual reversal.
    pub direction: ResidualDirection,
}

/// Stable identity of a transformed transportation node.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DoubleScalingNodeRef {
    /// Left-side node corresponding to one original node.
    Original(usize),
    /// Right-side node corresponding to one positive-width original edge.
    Edge(usize),
}

/// A semantic boundary in the explicit double-scaling procedure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoubleScalingStage {
    /// Transformation exists but no algorithm event has been published.
    Ready,
    /// Lower-bound removal and transportation mapping were published.
    Initialize,
    /// A cost-error phase reset the transformed flow and shifted right prices.
    StartCostPhase,
    /// A new integral imbalance scale began.
    StartCapacityPhase,
    /// A large-excess root was selected.
    SelectRoot,
    /// One transformed residual arc was inspected by the current-arc scan.
    InspectArc,
    /// One admissible residual arc extended the partial path.
    Advance,
    /// A dead-end path tip changed price by the current cost error.
    Relabel,
    /// The now-inadmissible predecessor arc was removed from the path.
    Retreat,
    /// Exact `delta` flow was sent over a completed admissible path.
    Augment,
    /// One improve-approximation produced a feasible transformed flow.
    CompleteCostPhase,
    /// Independent residual certification proved the mapped original flow optimal.
    Optimal,
}

/// Exact deterministic counters from the explicit transportation kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DoubleScalingMetrics {
    /// Outer cost-error phases entered.
    pub cost_phases: u64,
    /// Inner integral imbalance phases entered.
    pub capacity_phases: u64,
    /// Admissible path searches started.
    pub path_searches: u64,
    /// Admissible arcs appended to partial paths.
    pub advances: u64,
    /// Dead-end price changes.
    pub relabels: u64,
    /// Path arcs removed after a nonroot relabel.
    pub retreats: u64,
    /// Exact-delta path augmentations.
    pub augmentations: u64,
    /// Transformed arc-flow slots reset at cost-phase boundaries.
    pub transformed_arc_resets: u128,
    /// Candidate transformed residual arcs inspected by current-arc scans.
    pub transformed_arc_scans: u128,
}

/// Complete transformed state at one reversible semantic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoubleScalingSnapshot {
    /// Last feasible mapped original flow, retained during pseudoflow phases.
    pub display_flows: Vec<u64>,
    /// Flow on both transformed branches for every positive-width edge.
    pub transformed_flows: Vec<[u128; 2]>,
    /// Project-sign exact integer prices for all left nodes then right edge nodes.
    pub prices: Vec<i128>,
    /// Required divergence minus current divergence in transformed-node order.
    pub imbalances: Vec<i128>,
    /// Persistent current-arc cursor for every transformed node.
    pub cursors: Vec<usize>,
    /// Current admissible partial path.
    pub active_path: Vec<DoubleScalingArcId>,
    /// Exact transformed residual arc inspected at this boundary.
    pub inspected_arc: Option<DoubleScalingArcId>,
    /// Selected path root, when a path search is active.
    pub selected_root: Option<DoubleScalingNodeRef>,
    /// Negative-imbalance path endpoint at an augmentation boundary.
    pub selected_deficit: Option<DoubleScalingNodeRef>,
    /// Scaled current epsilon error after the phase reduction.
    pub epsilon: i128,
    /// Positive cost multiplier used to avoid fractional epsilon.
    pub cost_multiplier: i128,
    /// Current integral imbalance scale, or zero outside an inner phase.
    pub delta: u128,
    /// One-based cost phase ordinal.
    pub cost_phase: u64,
    /// One-based capacity phase ordinal within the current cost phase.
    pub capacity_phase: u64,
    /// Semantic boundary kind.
    pub stage: DoubleScalingStage,
    /// Deterministic counters at this boundary.
    pub metrics: DoubleScalingMetrics,
}

/// One reversible source-algorithm event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoubleScalingTraceEvent {
    /// Stable event catalog identity.
    pub catalog_id: &'static str,
    /// Snapshot before the atomic event.
    pub before: DoubleScalingSnapshot,
    /// Snapshot after the atomic event.
    pub after: DoubleScalingSnapshot,
}

/// Certified double-scaling result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoubleScalingResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed minimum-cost certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic source-kernel counters.
    pub metrics: DoubleScalingMetrics,
    /// Positive cost multiplier used by the exact integer implementation.
    pub cost_multiplier: i128,
    /// Initial scaled epsilon before the first improve-approximation.
    pub initial_epsilon: i128,
    /// Final exact algorithm state retained by the fast profile.
    pub final_snapshot: DoubleScalingSnapshot,
}

/// Certified result plus the complete semantic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoubleScalingTraceResult {
    /// Same canonical result returned by the fast profile.
    pub result: DoubleScalingResult,
    /// Boundary before initialization is published.
    pub base_snapshot: DoubleScalingSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<DoubleScalingTraceEvent>,
    /// Independently certified optimal boundary.
    pub final_snapshot: DoubleScalingSnapshot,
}

/// Double-scaling construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DoubleScalingError {
    /// Original graph exceeds the explicit interactive admission band.
    #[error("graph exceeds double-scaling admission limits")]
    AdmissionLimit,
    /// A deterministic transition or scan ceiling was reached.
    #[error("double-scaling work limit reached")]
    WorkLimit,
    /// Requested lower-aware balances are infeasible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Final independent minimum-cost certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded the declared integer domain.
    #[error("double-scaling arithmetic overflow")]
    ArithmeticOverflow,
    /// A transformed admissible path, imbalance, cursor, or phase was invalid.
    #[error("double-scaling invariant failed")]
    Invariant,
    /// Independent replay rejected the constructed trace.
    #[error("double-scaling trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Debug)]
struct VariableEdge {
    original_index: usize,
    from: usize,
    to: usize,
    width: u64,
    scaled_cost: i128,
}

#[derive(Clone, Debug)]
struct TransportModel {
    original_nodes: usize,
    variable_edges: Vec<VariableEdge>,
    required: Vec<i128>,
    outgoing: Vec<Vec<DoubleScalingArcId>>,
    cost_multiplier: i128,
}

impl TransportModel {
    fn node_count(&self) -> usize {
        self.original_nodes + self.variable_edges.len()
    }

    fn node_ref(&self, index: usize) -> Option<DoubleScalingNodeRef> {
        if index < self.original_nodes {
            Some(DoubleScalingNodeRef::Original(index))
        } else {
            self.variable_edges
                .get(index.checked_sub(self.original_nodes)?)
                .map(|edge| DoubleScalingNodeRef::Edge(edge.original_index))
        }
    }

    fn node_index(&self, node: DoubleScalingNodeRef) -> Option<usize> {
        match node {
            DoubleScalingNodeRef::Original(index) => (index < self.original_nodes).then_some(index),
            DoubleScalingNodeRef::Edge(original_index) => self
                .variable_edges
                .binary_search_by_key(&original_index, |edge| edge.original_index)
                .ok()
                .map(|index| self.original_nodes + index),
        }
    }

    fn variable_index(&self, original_index: usize) -> Option<usize> {
        self.variable_edges
            .binary_search_by_key(&original_index, |edge| edge.original_index)
            .ok()
    }

    fn arc_endpoints(&self, id: DoubleScalingArcId) -> Option<(usize, usize)> {
        let variable = self.variable_index(id.edge_index)?;
        let edge = &self.variable_edges[variable];
        let left = match id.branch {
            DoubleScalingBranch::Flow => edge.from,
            DoubleScalingBranch::Slack => edge.to,
        };
        let right = self.original_nodes + variable;
        Some(match id.direction {
            ResidualDirection::Forward => (left, right),
            ResidualDirection::Reverse => (right, left),
        })
    }

    fn arc_cost(&self, id: DoubleScalingArcId) -> Option<i128> {
        let edge = &self.variable_edges[self.variable_index(id.edge_index)?];
        let forward = match id.branch {
            DoubleScalingBranch::Flow => edge.scaled_cost,
            DoubleScalingBranch::Slack => 0,
        };
        Some(match id.direction {
            ResidualDirection::Forward => forward,
            ResidualDirection::Reverse => -forward,
        })
    }

    fn branch_index(branch: DoubleScalingBranch) -> usize {
        match branch {
            DoubleScalingBranch::Flow => 0,
            DoubleScalingBranch::Slack => 1,
        }
    }

    fn residual_exists(&self, state: &WorkingState, id: DoubleScalingArcId) -> bool {
        let Some(variable) = self.variable_index(id.edge_index) else {
            return false;
        };
        match id.direction {
            ResidualDirection::Forward => true,
            ResidualDirection::Reverse => {
                state.transformed_flows[variable][Self::branch_index(id.branch)] > 0
            }
        }
    }

    fn reduced_cost(
        &self,
        prices: &[i128],
        id: DoubleScalingArcId,
    ) -> Result<i128, DoubleScalingError> {
        let (from, to) = self
            .arc_endpoints(id)
            .ok_or(DoubleScalingError::Invariant)?;
        self.arc_cost(id)
            .and_then(|cost| cost.checked_add(prices[from]))
            .and_then(|value| value.checked_sub(prices[to]))
            .ok_or(DoubleScalingError::ArithmeticOverflow)
    }
}

#[derive(Clone, Debug)]
struct WorkingState {
    display_flows: Vec<u64>,
    transformed_flows: Vec<[u128; 2]>,
    prices: Vec<i128>,
    imbalances: Vec<i128>,
    cursors: Vec<usize>,
    active_path: Vec<DoubleScalingArcId>,
    inspected_arc: Option<DoubleScalingArcId>,
    selected_root: Option<usize>,
    selected_deficit: Option<usize>,
    epsilon: i128,
    delta: u128,
    cost_phase: u64,
    capacity_phase: u64,
    stage: DoubleScalingStage,
    metrics: DoubleScalingMetrics,
    transitions: u128,
}

struct InternalRun {
    result: DoubleScalingResult,
    base_snapshot: DoubleScalingSnapshot,
    events: Vec<DoubleScalingTraceEvent>,
    final_snapshot: DoubleScalingSnapshot,
}

/// Solves a lower-aware balanced minimum-cost flow by explicit double scaling.
///
/// # Errors
///
/// Rejects admission, infeasibility, checked arithmetic, deterministic work,
/// source-invariant, replay, or independent certificate failures.
pub fn solve_double_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DoubleScalingResult, DoubleScalingError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its source-side feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_double_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DoubleScalingResult, DoubleScalingError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every cost phase, capacity phase, path, relabel, retreat, and augmentation.
///
/// # Errors
///
/// Returns the same failures as [`solve_double_scaling`].
pub fn trace_double_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DoubleScalingTraceResult, DoubleScalingError> {
    let run = solve_internal(graph, required_divergence, true)?;
    Ok(DoubleScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    })
}

/// Traces double scaling while explicitly publishing its source-side initial
/// feasible-flow construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_double_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DoubleScalingTraceResult, DoubleScalingError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    Ok(DoubleScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    })
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
) -> Result<InternalRun, DoubleScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, trace_enabled, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, DoubleScalingError> {
    validate_admission(graph, required_divergence)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let model = transform(graph, required_divergence)?;
    let initial_epsilon = initial_epsilon(&model)?;
    let mut state = WorkingState {
        display_flows: feasible.flows,
        transformed_flows: vec![[0, 0]; model.variable_edges.len()],
        prices: vec![0; model.node_count()],
        imbalances: model.required.clone(),
        cursors: vec![0; model.node_count()],
        active_path: Vec::new(),
        inspected_arc: None,
        selected_root: None,
        selected_deficit: None,
        epsilon: initial_epsilon,
        delta: 0,
        cost_phase: 0,
        capacity_phase: 0,
        stage: DoubleScalingStage::Ready,
        metrics: DoubleScalingMetrics::default(),
        transitions: 0,
    };
    let base_snapshot = snapshot(&model, &state)?;
    let mut events = Vec::new();
    publish(
        &model,
        &mut state,
        DoubleScalingStage::Initialize,
        "double-scaling.initialize-transportation",
        trace_enabled,
        &mut events,
        |_| Ok(()),
    )?;

    while state.epsilon > 1 {
        let previous_epsilon = state.epsilon;
        let target_epsilon = previous_epsilon / 2;
        start_cost_phase(
            &model,
            &mut state,
            previous_epsilon,
            target_epsilon,
            trace_enabled,
            &mut events,
        )?;
        run_capacity_phases(&model, &mut state, trace_enabled, &mut events)?;
        complete_cost_phase(graph, &model, &mut state, trace_enabled, &mut events)?;
    }

    let flows = state.display_flows.clone();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    publish(
        &model,
        &mut state,
        DoubleScalingStage::Optimal,
        "double-scaling.optimal",
        trace_enabled,
        &mut events,
        |state| {
            state.delta = 0;
            state.active_path.clear();
            state.selected_root = None;
            state.selected_deficit = None;
            Ok(())
        },
    )?;
    let final_snapshot = snapshot(&model, &state)?;
    let result = DoubleScalingResult {
        flows,
        certificate,
        metrics: state.metrics,
        cost_multiplier: model.cost_multiplier,
        initial_epsilon,
        final_snapshot: final_snapshot.clone(),
    };
    if trace_enabled {
        check_double_scaling_trace(
            graph,
            required_divergence,
            &DoubleScalingTraceResult {
                result: result.clone(),
                base_snapshot: base_snapshot.clone(),
                events: events.clone(),
                final_snapshot: final_snapshot.clone(),
            },
        )?;
    }
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn validate_admission(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<(), DoubleScalingError> {
    if graph.nodes().len() > DOUBLE_SCALING_MAX_NODES
        || graph.edges().len() > DOUBLE_SCALING_MAX_EDGES
    {
        return Err(DoubleScalingError::AdmissionLimit);
    }
    if required_divergence.len() != graph.nodes().len()
        || required_divergence
            .iter()
            .try_fold(0_i128, |sum, value| sum.checked_add(*value))
            .ok_or(DoubleScalingError::ArithmeticOverflow)?
            != 0
    {
        return Err(DoubleScalingError::Invariant);
    }
    Ok(())
}

fn transform(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<TransportModel, DoubleScalingError> {
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower_flows)?;
    let adjusted = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(required, lower)| {
            required
                .checked_sub(lower)
                .ok_or(DoubleScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let variable_count = graph
        .edges()
        .iter()
        .filter(|edge| edge.capacity() > edge.lower())
        .count();
    let transformed_nodes = graph
        .nodes()
        .len()
        .checked_add(variable_count)
        .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    let cost_multiplier = i128::try_from(transformed_nodes)
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    let mut variable_edges = Vec::with_capacity(variable_count);
    let mut required = adjusted;
    for (original_index, edge) in graph.edges().iter().enumerate() {
        let width = edge.capacity() - edge.lower();
        if width == 0 {
            continue;
        }
        let scaled_cost = i128::from(edge.cost())
            .checked_mul(cost_multiplier)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
        required[edge.to().as_usize()] = required[edge.to().as_usize()]
            .checked_add(i128::from(width))
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
        required.push(-i128::from(width));
        variable_edges.push(VariableEdge {
            original_index,
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            width,
            scaled_cost,
        });
    }
    let mut outgoing = vec![Vec::new(); transformed_nodes];
    for (variable, edge) in variable_edges.iter().enumerate() {
        let right = graph.nodes().len() + variable;
        for (branch, left) in [
            (DoubleScalingBranch::Flow, edge.from),
            (DoubleScalingBranch::Slack, edge.to),
        ] {
            outgoing[left].push(DoubleScalingArcId {
                edge_index: edge.original_index,
                branch,
                direction: ResidualDirection::Forward,
            });
            outgoing[right].push(DoubleScalingArcId {
                edge_index: edge.original_index,
                branch,
                direction: ResidualDirection::Reverse,
            });
        }
    }
    for arcs in &mut outgoing {
        arcs.sort_unstable();
    }
    let sum = required.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(DoubleScalingError::ArithmeticOverflow)
    })?;
    if sum != 0
        || required[..graph.nodes().len()]
            .iter()
            .any(|value| *value < 0)
    {
        return Err(DoubleScalingError::Invariant);
    }
    Ok(TransportModel {
        original_nodes: graph.nodes().len(),
        variable_edges,
        required,
        outgoing,
        cost_multiplier,
    })
}

fn initial_epsilon(model: &TransportModel) -> Result<i128, DoubleScalingError> {
    let maximum = model
        .variable_edges
        .iter()
        .try_fold(0_i128, |maximum, edge| {
            edge.scaled_cost
                .checked_abs()
                .map(|cost| maximum.max(cost))
                .ok_or(DoubleScalingError::ArithmeticOverflow)
        })?;
    // Even a zero-cost instance needs one improve-approximation: the
    // transportation pseudoflow still has to become feasible. Starting at two
    // also leaves the final scaled epsilon exactly one.
    let mut epsilon = 2_i128;
    while epsilon < maximum {
        epsilon = epsilon
            .checked_mul(2)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    }
    Ok(epsilon)
}

fn snapshot(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<DoubleScalingSnapshot, DoubleScalingError> {
    Ok(DoubleScalingSnapshot {
        display_flows: state.display_flows.clone(),
        transformed_flows: state.transformed_flows.clone(),
        prices: state.prices.clone(),
        imbalances: state.imbalances.clone(),
        cursors: state.cursors.clone(),
        active_path: state.active_path.clone(),
        inspected_arc: state.inspected_arc,
        selected_root: state
            .selected_root
            .map(|node| model.node_ref(node).ok_or(DoubleScalingError::Invariant))
            .transpose()?,
        selected_deficit: state
            .selected_deficit
            .map(|node| model.node_ref(node).ok_or(DoubleScalingError::Invariant))
            .transpose()?,
        epsilon: state.epsilon,
        cost_multiplier: model.cost_multiplier,
        delta: state.delta,
        cost_phase: state.cost_phase,
        capacity_phase: state.capacity_phase,
        stage: state.stage,
        metrics: state.metrics,
    })
}

fn publish(
    model: &TransportModel,
    state: &mut WorkingState,
    next_stage: DoubleScalingStage,
    catalog_id: &'static str,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
    mutation: impl FnOnce(&mut WorkingState) -> Result<(), DoubleScalingError>,
) -> Result<(), DoubleScalingError> {
    let before = trace_enabled.then(|| snapshot(model, state)).transpose()?;
    state.inspected_arc = None;
    mutation(state)?;
    state.stage = next_stage;
    validate_snapshot(model, state)?;
    if let Some(before) = before {
        events.push(DoubleScalingTraceEvent {
            catalog_id,
            before,
            after: snapshot(model, state)?,
        });
    }
    Ok(())
}

fn start_cost_phase(
    model: &TransportModel,
    state: &mut WorkingState,
    previous_epsilon: i128,
    target_epsilon: i128,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    publish(
        model,
        state,
        DoubleScalingStage::StartCostPhase,
        "double-scaling.start-cost-phase",
        trace_enabled,
        events,
        |state| {
            state.cost_phase = state
                .cost_phase
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.metrics.cost_phases = state
                .metrics
                .cost_phases
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.capacity_phase = 0;
            state.epsilon = target_epsilon;
            state.delta = 0;
            state.active_path.clear();
            state.selected_root = None;
            state.selected_deficit = None;
            for flow in &mut state.transformed_flows {
                *flow = [0, 0];
                state.metrics.transformed_arc_resets = state
                    .metrics
                    .transformed_arc_resets
                    .checked_add(2)
                    .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            }
            state.imbalances.clone_from(&model.required);
            state.cursors.fill(0);
            for right in model.original_nodes..model.node_count() {
                state.prices[right] = state.prices[right]
                    .checked_sub(previous_epsilon)
                    .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            }
            Ok(())
        },
    )
}

fn run_capacity_phases(
    model: &TransportModel,
    state: &mut WorkingState,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    let maximum = maximum_positive_imbalance(&state.imbalances)?;
    let mut delta = highest_power_of_two(maximum)?;
    while delta > 0 {
        start_capacity_phase(model, state, delta, trace_enabled, events)?;
        while let Some(root) = select_large_excess(&state.imbalances, delta)? {
            select_root(model, state, root, trace_enabled, events)?;
            find_and_augment_path(model, state, root, delta, trace_enabled, events)?;
        }
        delta /= 2;
    }
    if state.imbalances.iter().any(|imbalance| *imbalance != 0) {
        return Err(DoubleScalingError::Invariant);
    }
    Ok(())
}

fn start_capacity_phase(
    model: &TransportModel,
    state: &mut WorkingState,
    delta: u128,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    publish(
        model,
        state,
        DoubleScalingStage::StartCapacityPhase,
        "double-scaling.start-capacity-phase",
        trace_enabled,
        events,
        |state| {
            state.capacity_phase = state
                .capacity_phase
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.metrics.capacity_phases = state
                .metrics
                .capacity_phases
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.delta = delta;
            state.active_path.clear();
            state.selected_root = None;
            state.selected_deficit = None;
            Ok(())
        },
    )
}

fn select_root(
    model: &TransportModel,
    state: &mut WorkingState,
    root: usize,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    publish(
        model,
        state,
        DoubleScalingStage::SelectRoot,
        "double-scaling.select-large-excess-root",
        trace_enabled,
        events,
        |state| {
            increment_transition(state)?;
            state.metrics.path_searches = state
                .metrics
                .path_searches
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.active_path.clear();
            state.selected_root = Some(root);
            state.selected_deficit = None;
            Ok(())
        },
    )
}

fn find_and_augment_path(
    model: &TransportModel,
    state: &mut WorkingState,
    root: usize,
    delta: u128,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    let mut path_nodes = vec![root];
    loop {
        let tip = *path_nodes.last().ok_or(DoubleScalingError::Invariant)?;
        if tip != root && state.imbalances[tip] < 0 {
            augment_path(model, state, root, tip, delta, trace_enabled, events)?;
            return Ok(());
        }
        let next_arc = loop {
            let Some((arc_index, arc, admissible)) =
                inspect_next_arc(model, state, tip, trace_enabled, events)?
            else {
                break None;
            };
            if admissible {
                break Some((arc_index, arc));
            }
        };
        if let Some((arc_index, arc)) = next_arc {
            let (_, head) = model
                .arc_endpoints(arc)
                .ok_or(DoubleScalingError::Invariant)?;
            if path_nodes.contains(&head) {
                return Err(DoubleScalingError::Invariant);
            }
            publish(
                model,
                state,
                DoubleScalingStage::Advance,
                "double-scaling.advance-admissible-path",
                trace_enabled,
                events,
                |state| {
                    increment_transition(state)?;
                    state.metrics.advances = state
                        .metrics
                        .advances
                        .checked_add(1)
                        .ok_or(DoubleScalingError::ArithmeticOverflow)?;
                    state.cursors[tip] = arc_index;
                    state.active_path.push(arc);
                    Ok(())
                },
            )?;
            path_nodes.push(head);
            continue;
        }

        publish(
            model,
            state,
            DoubleScalingStage::Relabel,
            "double-scaling.relabel-dead-end-tip",
            trace_enabled,
            events,
            |state| {
                increment_transition(state)?;
                state.metrics.relabels = state
                    .metrics
                    .relabels
                    .checked_add(1)
                    .ok_or(DoubleScalingError::ArithmeticOverflow)?;
                state.prices[tip] = state.prices[tip]
                    .checked_sub(state.epsilon)
                    .ok_or(DoubleScalingError::ArithmeticOverflow)?;
                state.cursors[tip] = 0;
                Ok(())
            },
        )?;
        if tip == root {
            continue;
        }
        publish(
            model,
            state,
            DoubleScalingStage::Retreat,
            "double-scaling.retreat-inadmissible-predecessor",
            trace_enabled,
            events,
            |state| {
                increment_transition(state)?;
                state.metrics.retreats = state
                    .metrics
                    .retreats
                    .checked_add(1)
                    .ok_or(DoubleScalingError::ArithmeticOverflow)?;
                state
                    .active_path
                    .pop()
                    .ok_or(DoubleScalingError::Invariant)?;
                Ok(())
            },
        )?;
        path_nodes.pop();
    }
}

fn inspect_next_arc(
    model: &TransportModel,
    state: &mut WorkingState,
    node: usize,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<Option<(usize, DoubleScalingArcId, bool)>, DoubleScalingError> {
    let outgoing = model
        .outgoing
        .get(node)
        .ok_or(DoubleScalingError::Invariant)?;
    let arc_index = *state
        .cursors
        .get(node)
        .ok_or(DoubleScalingError::Invariant)?;
    let Some(&arc) = outgoing.get(arc_index) else {
        return Ok(None);
    };
    let admissible =
        model.residual_exists(state, arc) && model.reduced_cost(&state.prices, arc)? < 0;
    let before = trace_enabled.then(|| snapshot(model, state)).transpose()?;
    add_scans(state, 1)?;
    state.inspected_arc = Some(arc);
    if !admissible {
        state.cursors[node] = arc_index
            .checked_add(1)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    }
    state.stage = DoubleScalingStage::InspectArc;
    if let Some(before) = before {
        validate_snapshot(model, state)?;
        events.push(DoubleScalingTraceEvent {
            catalog_id: "double-scaling.inspect-transformed-residual-arc",
            before,
            after: snapshot(model, state)?,
        });
    }
    Ok(Some((arc_index, arc, admissible)))
}

fn augment_path(
    model: &TransportModel,
    state: &mut WorkingState,
    root: usize,
    deficit: usize,
    delta: u128,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    let amount = i128::try_from(delta).map_err(|_| DoubleScalingError::ArithmeticOverflow)?;
    let path = state.active_path.clone();
    publish(
        model,
        state,
        DoubleScalingStage::Augment,
        "double-scaling.augment-exact-delta",
        trace_enabled,
        events,
        |state| {
            increment_transition(state)?;
            state.metrics.augmentations = state
                .metrics
                .augmentations
                .checked_add(1)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            for id in &path {
                update_transformed_flow(model, state, *id, delta)?;
            }
            state.imbalances[root] = state.imbalances[root]
                .checked_sub(amount)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.imbalances[deficit] = state.imbalances[deficit]
                .checked_add(amount)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            state.selected_deficit = Some(deficit);
            Ok(())
        },
    )?;
    Ok(())
}

fn update_transformed_flow(
    model: &TransportModel,
    state: &mut WorkingState,
    id: DoubleScalingArcId,
    delta: u128,
) -> Result<(), DoubleScalingError> {
    let variable = model
        .variable_index(id.edge_index)
        .ok_or(DoubleScalingError::Invariant)?;
    let branch = TransportModel::branch_index(id.branch);
    match id.direction {
        ResidualDirection::Forward => {
            state.transformed_flows[variable][branch] = state.transformed_flows[variable][branch]
                .checked_add(delta)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
        }
        ResidualDirection::Reverse => {
            state.transformed_flows[variable][branch] = state.transformed_flows[variable][branch]
                .checked_sub(delta)
                .ok_or(DoubleScalingError::Invariant)?;
        }
    }
    Ok(())
}

fn complete_cost_phase(
    graph: &FlowNetwork,
    model: &TransportModel,
    state: &mut WorkingState,
    trace_enabled: bool,
    events: &mut Vec<DoubleScalingTraceEvent>,
) -> Result<(), DoubleScalingError> {
    let mapped = map_original_flows(graph, model, state)?;
    publish(
        model,
        state,
        DoubleScalingStage::CompleteCostPhase,
        "double-scaling.complete-cost-phase",
        trace_enabled,
        events,
        |state| {
            state.display_flows = mapped;
            state.delta = 0;
            state.active_path.clear();
            state.selected_root = None;
            state.selected_deficit = None;
            Ok(())
        },
    )
}

fn map_original_flows(
    graph: &FlowNetwork,
    model: &TransportModel,
    state: &WorkingState,
) -> Result<Vec<u64>, DoubleScalingError> {
    let mut flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        let flow = state.transformed_flows[variable][0];
        let slack = state.transformed_flows[variable][1];
        if flow
            .checked_add(slack)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?
            != u128::from(edge.width)
            || flow > u128::from(edge.width)
        {
            return Err(DoubleScalingError::Invariant);
        }
        let incremental =
            u64::try_from(flow).map_err(|_| DoubleScalingError::ArithmeticOverflow)?;
        flows[edge.original_index] = flows[edge.original_index]
            .checked_add(incremental)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    }
    Ok(flows)
}

fn maximum_positive_imbalance(values: &[i128]) -> Result<u128, DoubleScalingError> {
    values.iter().try_fold(0_u128, |maximum, value| {
        if *value <= 0 {
            Ok(maximum)
        } else {
            u128::try_from(*value)
                .map(|value| maximum.max(value))
                .map_err(|_| DoubleScalingError::ArithmeticOverflow)
        }
    })
}

fn highest_power_of_two(value: u128) -> Result<u128, DoubleScalingError> {
    if value == 0 {
        return Ok(0);
    }
    1_u128
        .checked_shl(127_u32.saturating_sub(value.leading_zeros()))
        .ok_or(DoubleScalingError::ArithmeticOverflow)
}

fn select_large_excess(values: &[i128], delta: u128) -> Result<Option<usize>, DoubleScalingError> {
    let threshold = i128::try_from(delta).map_err(|_| DoubleScalingError::ArithmeticOverflow)?;
    Ok(values.iter().position(|value| *value >= threshold))
}

fn increment_transition(state: &mut WorkingState) -> Result<(), DoubleScalingError> {
    state.transitions = state
        .transitions
        .checked_add(1)
        .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    if state.transitions > DOUBLE_SCALING_MAX_TRANSITIONS {
        return Err(DoubleScalingError::WorkLimit);
    }
    Ok(())
}

fn add_scans(state: &mut WorkingState, scans: u128) -> Result<(), DoubleScalingError> {
    state.metrics.transformed_arc_scans = state
        .metrics
        .transformed_arc_scans
        .checked_add(scans)
        .ok_or(DoubleScalingError::ArithmeticOverflow)?;
    if state.metrics.transformed_arc_scans > DOUBLE_SCALING_MAX_ARC_SCANS {
        return Err(DoubleScalingError::WorkLimit);
    }
    Ok(())
}

fn validate_snapshot(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    if state.transformed_flows.len() != model.variable_edges.len()
        || state.prices.len() != model.node_count()
        || state.imbalances.len() != model.node_count()
        || state.cursors.len() != model.node_count()
        || state.epsilon < 1
        || (state.stage == DoubleScalingStage::InspectArc) != state.inspected_arc.is_some()
        || state
            .cursors
            .iter()
            .zip(&model.outgoing)
            .any(|(cursor, outgoing)| *cursor > outgoing.len())
    {
        return Err(DoubleScalingError::Invariant);
    }
    match state.stage {
        DoubleScalingStage::Ready | DoubleScalingStage::Initialize => {
            if state.cost_phase != 0
                || state.capacity_phase != 0
                || state.delta != 0
                || state.selected_root.is_some()
                || state.selected_deficit.is_some()
                || !state.active_path.is_empty()
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
        DoubleScalingStage::StartCostPhase => {
            if state.cost_phase == 0
                || state.capacity_phase != 0
                || state.delta != 0
                || state.selected_root.is_some()
                || state.selected_deficit.is_some()
                || !state.active_path.is_empty()
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
        DoubleScalingStage::StartCapacityPhase => {
            if state.cost_phase == 0
                || state.capacity_phase == 0
                || state.delta == 0
                || state.selected_root.is_some()
                || state.selected_deficit.is_some()
                || !state.active_path.is_empty()
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
        DoubleScalingStage::SelectRoot
        | DoubleScalingStage::InspectArc
        | DoubleScalingStage::Advance
        | DoubleScalingStage::Relabel
        | DoubleScalingStage::Retreat => {
            if state.cost_phase == 0
                || state.capacity_phase == 0
                || state.delta == 0
                || state.selected_root.is_none()
                || state.selected_deficit.is_some()
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
        DoubleScalingStage::Augment => {
            if state.cost_phase == 0
                || state.capacity_phase == 0
                || state.delta == 0
                || state.selected_root.is_none()
                || state.selected_deficit.is_none()
                || state.active_path.is_empty()
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
        DoubleScalingStage::CompleteCostPhase | DoubleScalingStage::Optimal => {
            if state.cost_phase == 0
                || state.delta != 0
                || state.selected_root.is_some()
                || state.selected_deficit.is_some()
                || !state.active_path.is_empty()
                || state.imbalances.iter().any(|value| *value != 0)
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
    }
    validate_transport_imbalances(model, state)?;
    validate_epsilon_optimality(model, state)?;
    validate_admissible_acyclic(model, state)?;
    validate_active_path(model, state)?;
    validate_inspected_arc(model, state)
}

fn validate_transport_imbalances(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    let mut actual = vec![0_i128; model.node_count()];
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        let right = model.original_nodes + variable;
        for (branch, left) in [
            (DoubleScalingBranch::Flow, edge.from),
            (DoubleScalingBranch::Slack, edge.to),
        ] {
            let flow = i128::try_from(
                state.transformed_flows[variable][TransportModel::branch_index(branch)],
            )
            .map_err(|_| DoubleScalingError::ArithmeticOverflow)?;
            actual[left] = actual[left]
                .checked_add(flow)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
            actual[right] = actual[right]
                .checked_sub(flow)
                .ok_or(DoubleScalingError::ArithmeticOverflow)?;
        }
    }
    for ((required, actual), recorded) in model.required.iter().zip(actual).zip(&state.imbalances) {
        if required
            .checked_sub(actual)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?
            != *recorded
        {
            return Err(DoubleScalingError::Invariant);
        }
    }
    if state
        .imbalances
        .iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(*value))
        .ok_or(DoubleScalingError::ArithmeticOverflow)?
        != 0
    {
        return Err(DoubleScalingError::Invariant);
    }
    Ok(())
}

fn validate_epsilon_optimality(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    for outgoing in &model.outgoing {
        for id in outgoing {
            if model.residual_exists(state, *id)
                && model.reduced_cost(&state.prices, *id)? < -state.epsilon
            {
                return Err(DoubleScalingError::Invariant);
            }
        }
    }
    Ok(())
}

fn validate_admissible_acyclic(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    let mut color = vec![0_u8; model.node_count()];
    for start in 0..model.node_count() {
        if color[start] == 0 {
            visit_admissible(model, state, start, &mut color)?;
        }
    }
    Ok(())
}

fn visit_admissible(
    model: &TransportModel,
    state: &WorkingState,
    node: usize,
    color: &mut [u8],
) -> Result<(), DoubleScalingError> {
    color[node] = 1;
    for id in &model.outgoing[node] {
        if !model.residual_exists(state, *id) || model.reduced_cost(&state.prices, *id)? >= 0 {
            continue;
        }
        let (_, to) = model
            .arc_endpoints(*id)
            .ok_or(DoubleScalingError::Invariant)?;
        match color[to] {
            0 => visit_admissible(model, state, to, color)?,
            1 => return Err(DoubleScalingError::Invariant),
            _ => {}
        }
    }
    color[node] = 2;
    Ok(())
}

fn validate_active_path(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    let Some(root) = state.selected_root else {
        return state
            .active_path
            .is_empty()
            .then_some(())
            .ok_or(DoubleScalingError::Invariant);
    };
    let threshold =
        i128::try_from(state.delta).map_err(|_| DoubleScalingError::ArithmeticOverflow)?;
    let root_imbalance = if state.stage == DoubleScalingStage::Augment {
        state.imbalances[root]
            .checked_add(threshold)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?
    } else {
        state.imbalances[root]
    };
    if root_imbalance < threshold {
        return Err(DoubleScalingError::Invariant);
    }
    let mut node = root;
    let mut seen = vec![false; model.node_count()];
    seen[node] = true;
    for (path_index, id) in state.active_path.iter().enumerate() {
        let (from, to) = model
            .arc_endpoints(*id)
            .ok_or(DoubleScalingError::Invariant)?;
        if from != node || seen[to] {
            return Err(DoubleScalingError::Invariant);
        }
        if state.stage != DoubleScalingStage::Augment {
            if !model.residual_exists(state, *id) {
                return Err(DoubleScalingError::Invariant);
            }
            let reduced = model.reduced_cost(&state.prices, *id)?;
            let relabeled_predecessor = state.stage == DoubleScalingStage::Relabel
                && path_index + 1 == state.active_path.len();
            if (relabeled_predecessor && reduced < 0) || (!relabeled_predecessor && reduced >= 0) {
                return Err(DoubleScalingError::Invariant);
            }
        }
        seen[to] = true;
        node = to;
    }
    if let Some(deficit) = state.selected_deficit {
        if deficit != node {
            return Err(DoubleScalingError::Invariant);
        }
        let previous_deficit = state.imbalances[deficit]
            .checked_sub(threshold)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?;
        if previous_deficit >= 0 {
            return Err(DoubleScalingError::Invariant);
        }
    }
    Ok(())
}

fn validate_inspected_arc(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<(), DoubleScalingError> {
    let Some(inspected) = state.inspected_arc else {
        return Ok(());
    };
    let mut tip = state.selected_root.ok_or(DoubleScalingError::Invariant)?;
    for arc in &state.active_path {
        let (from, to) = model
            .arc_endpoints(*arc)
            .ok_or(DoubleScalingError::Invariant)?;
        if from != tip {
            return Err(DoubleScalingError::Invariant);
        }
        tip = to;
    }
    let outgoing = model
        .outgoing
        .get(tip)
        .ok_or(DoubleScalingError::Invariant)?;
    let arc_index = outgoing
        .iter()
        .position(|arc| *arc == inspected)
        .ok_or(DoubleScalingError::Invariant)?;
    let admissible = model.residual_exists(state, inspected)
        && model.reduced_cost(&state.prices, inspected)? < 0;
    let expected_cursor = if admissible {
        arc_index
    } else {
        arc_index
            .checked_add(1)
            .ok_or(DoubleScalingError::ArithmeticOverflow)?
    };
    if state.cursors[tip] != expected_cursor {
        return Err(DoubleScalingError::Invariant);
    }
    Ok(())
}

/// Independently replays every public double-scaling transition.
///
/// The checker reconstructs the transportation model and canonical initial
/// state from the original graph. It does not trust event deltas, cursors,
/// prices, imbalance vectors, mapped display flow, metrics, or the final
/// optimality certificate.
///
/// # Errors
///
/// Rejects discontinuity, a noncanonical source choice, an invalid residual
/// scan, an incorrect price or flow update, or a final-result mismatch.
pub fn check_double_scaling_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &DoubleScalingTraceResult,
) -> Result<(), DoubleScalingError> {
    validate_admission(graph, required_divergence).map_err(trace_error)?;
    let model = transform(graph, required_divergence).map_err(trace_error)?;
    let feasible = find_feasible_flow(graph, required_divergence).map_err(trace_error)?;
    let expected_initial_epsilon = initial_epsilon(&model).map_err(trace_error)?;
    let expected_base = snapshot(
        &model,
        &WorkingState {
            display_flows: feasible.flows,
            transformed_flows: vec![[0, 0]; model.variable_edges.len()],
            prices: vec![0; model.node_count()],
            imbalances: model.required.clone(),
            cursors: vec![0; model.node_count()],
            active_path: Vec::new(),
            inspected_arc: None,
            selected_root: None,
            selected_deficit: None,
            epsilon: expected_initial_epsilon,
            delta: 0,
            cost_phase: 0,
            capacity_phase: 0,
            stage: DoubleScalingStage::Ready,
            metrics: DoubleScalingMetrics::default(),
            transitions: 0,
        },
    )
    .map_err(trace_error)?;
    if trace.events.is_empty()
        || trace.base_snapshot != expected_base
        || trace.final_snapshot.stage != DoubleScalingStage::Optimal
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.display_flows != trace.result.flows
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.result.cost_multiplier != model.cost_multiplier
        || trace.result.initial_epsilon != expected_initial_epsilon
    {
        return Err(DoubleScalingError::TraceVerification);
    }

    let mut current = trace.base_snapshot.clone();
    validate_public_double_scaling_snapshot(graph, required_divergence, &model, &current)?;
    for event in &trace.events {
        if event.before != current {
            return Err(DoubleScalingError::TraceVerification);
        }
        validate_public_double_scaling_snapshot(graph, required_divergence, &model, &event.after)?;
        replay_double_scaling_event(graph, &model, event)?;
        current = event.after.clone();
    }
    if current != trace.final_snapshot {
        return Err(DoubleScalingError::TraceVerification);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)
        .map_err(trace_error)?;
    if certificate != trace.result.certificate {
        return Err(DoubleScalingError::TraceVerification);
    }
    Ok(())
}

fn trace_error<T>(_: T) -> DoubleScalingError {
    DoubleScalingError::TraceVerification
}

fn validate_public_double_scaling_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    model: &TransportModel,
    public: &DoubleScalingSnapshot,
) -> Result<(), DoubleScalingError> {
    let state = working_state_from_snapshot(model, public)?;
    validate_snapshot(model, &state).map_err(trace_error)?;
    if public.display_flows.len() != graph.edges().len()
        || graph
            .edges()
            .iter()
            .zip(&public.display_flows)
            .any(|(edge, flow)| *flow < edge.lower() || *flow > edge.capacity())
        || divergences(graph, &public.display_flows).map_err(trace_error)? != required_divergence
    {
        return Err(DoubleScalingError::TraceVerification);
    }
    Ok(())
}

fn working_state_from_snapshot(
    model: &TransportModel,
    public: &DoubleScalingSnapshot,
) -> Result<WorkingState, DoubleScalingError> {
    if public.cost_multiplier != model.cost_multiplier {
        return Err(DoubleScalingError::TraceVerification);
    }
    Ok(WorkingState {
        display_flows: public.display_flows.clone(),
        transformed_flows: public.transformed_flows.clone(),
        prices: public.prices.clone(),
        imbalances: public.imbalances.clone(),
        cursors: public.cursors.clone(),
        active_path: public.active_path.clone(),
        inspected_arc: public.inspected_arc,
        selected_root: public
            .selected_root
            .map(|node| {
                model
                    .node_index(node)
                    .ok_or(DoubleScalingError::TraceVerification)
            })
            .transpose()?,
        selected_deficit: public
            .selected_deficit
            .map(|node| {
                model
                    .node_index(node)
                    .ok_or(DoubleScalingError::TraceVerification)
            })
            .transpose()?,
        epsilon: public.epsilon,
        delta: public.delta,
        cost_phase: public.cost_phase,
        capacity_phase: public.capacity_phase,
        stage: public.stage,
        metrics: public.metrics,
        transitions: 0,
    })
}

fn replay_double_scaling_event(
    graph: &FlowNetwork,
    model: &TransportModel,
    event: &DoubleScalingTraceEvent,
) -> Result<(), DoubleScalingError> {
    if event.catalog_id != double_scaling_catalog_id(event.after.stage)
        || !valid_double_scaling_predecessor(event.before.stage, event.after.stage)
    {
        return Err(DoubleScalingError::TraceVerification);
    }
    let mut expected = working_state_from_snapshot(model, &event.before)?;
    match event.after.stage {
        DoubleScalingStage::Ready => return Err(DoubleScalingError::TraceVerification),
        DoubleScalingStage::Initialize => {}
        DoubleScalingStage::StartCostPhase => replay_start_cost_phase(model, &mut expected)?,
        DoubleScalingStage::StartCapacityPhase => replay_start_capacity_phase(&mut expected)?,
        DoubleScalingStage::SelectRoot => replay_select_root(&mut expected)?,
        DoubleScalingStage::InspectArc => replay_inspect_arc(model, &mut expected)?,
        DoubleScalingStage::Advance => replay_advance(model, &mut expected)?,
        DoubleScalingStage::Relabel => replay_relabel(model, &mut expected)?,
        DoubleScalingStage::Retreat => {
            expected
                .active_path
                .pop()
                .ok_or(DoubleScalingError::TraceVerification)?;
            expected.metrics.retreats = checked_increment(expected.metrics.retreats)?;
        }
        DoubleScalingStage::Augment => replay_augment(model, &mut expected)?,
        DoubleScalingStage::CompleteCostPhase => {
            if expected.imbalances.iter().any(|imbalance| *imbalance != 0) {
                return Err(DoubleScalingError::TraceVerification);
            }
            expected.display_flows =
                map_original_flows(graph, model, &expected).map_err(trace_error)?;
            expected.delta = 0;
            expected.active_path.clear();
            expected.selected_root = None;
            expected.selected_deficit = None;
        }
        DoubleScalingStage::Optimal => {
            expected.delta = 0;
            expected.active_path.clear();
            expected.selected_root = None;
            expected.selected_deficit = None;
        }
    }
    expected.stage = event.after.stage;
    let expected_public = snapshot(model, &expected).map_err(trace_error)?;
    if expected_public != event.after {
        return Err(DoubleScalingError::TraceVerification);
    }
    Ok(())
}

fn double_scaling_catalog_id(stage: DoubleScalingStage) -> &'static str {
    match stage {
        DoubleScalingStage::Ready => "double-scaling.invalid-ready",
        DoubleScalingStage::Initialize => "double-scaling.initialize-transportation",
        DoubleScalingStage::StartCostPhase => "double-scaling.start-cost-phase",
        DoubleScalingStage::StartCapacityPhase => "double-scaling.start-capacity-phase",
        DoubleScalingStage::SelectRoot => "double-scaling.select-large-excess-root",
        DoubleScalingStage::InspectArc => "double-scaling.inspect-transformed-residual-arc",
        DoubleScalingStage::Advance => "double-scaling.advance-admissible-path",
        DoubleScalingStage::Relabel => "double-scaling.relabel-dead-end-tip",
        DoubleScalingStage::Retreat => "double-scaling.retreat-inadmissible-predecessor",
        DoubleScalingStage::Augment => "double-scaling.augment-exact-delta",
        DoubleScalingStage::CompleteCostPhase => "double-scaling.complete-cost-phase",
        DoubleScalingStage::Optimal => "double-scaling.optimal",
    }
}

fn valid_double_scaling_predecessor(before: DoubleScalingStage, after: DoubleScalingStage) -> bool {
    match after {
        DoubleScalingStage::Ready => false,
        DoubleScalingStage::Initialize => before == DoubleScalingStage::Ready,
        DoubleScalingStage::StartCostPhase => matches!(
            before,
            DoubleScalingStage::Initialize | DoubleScalingStage::CompleteCostPhase
        ),
        DoubleScalingStage::StartCapacityPhase => matches!(
            before,
            DoubleScalingStage::StartCostPhase
                | DoubleScalingStage::StartCapacityPhase
                | DoubleScalingStage::Augment
        ),
        DoubleScalingStage::SelectRoot => matches!(
            before,
            DoubleScalingStage::StartCapacityPhase | DoubleScalingStage::Augment
        ),
        DoubleScalingStage::InspectArc => matches!(
            before,
            DoubleScalingStage::SelectRoot
                | DoubleScalingStage::InspectArc
                | DoubleScalingStage::Advance
                | DoubleScalingStage::Relabel
                | DoubleScalingStage::Retreat
        ),
        DoubleScalingStage::Advance => before == DoubleScalingStage::InspectArc,
        DoubleScalingStage::Relabel => matches!(
            before,
            DoubleScalingStage::SelectRoot
                | DoubleScalingStage::InspectArc
                | DoubleScalingStage::Advance
                | DoubleScalingStage::Relabel
                | DoubleScalingStage::Retreat
        ),
        DoubleScalingStage::Retreat => before == DoubleScalingStage::Relabel,
        DoubleScalingStage::Augment => before == DoubleScalingStage::Advance,
        DoubleScalingStage::CompleteCostPhase => matches!(
            before,
            DoubleScalingStage::StartCapacityPhase | DoubleScalingStage::Augment
        ),
        DoubleScalingStage::Optimal => before == DoubleScalingStage::CompleteCostPhase,
    }
}

fn checked_increment(value: u64) -> Result<u64, DoubleScalingError> {
    value
        .checked_add(1)
        .ok_or(DoubleScalingError::TraceVerification)
}

fn replay_start_cost_phase(
    model: &TransportModel,
    state: &mut WorkingState,
) -> Result<(), DoubleScalingError> {
    if state.epsilon <= 1 || state.epsilon % 2 != 0 {
        return Err(DoubleScalingError::TraceVerification);
    }
    let previous_epsilon = state.epsilon;
    state.epsilon /= 2;
    state.cost_phase = checked_increment(state.cost_phase)?;
    state.metrics.cost_phases = checked_increment(state.metrics.cost_phases)?;
    state.capacity_phase = 0;
    state.delta = 0;
    state.active_path.clear();
    state.selected_root = None;
    state.selected_deficit = None;
    for flow in &mut state.transformed_flows {
        *flow = [0, 0];
        state.metrics.transformed_arc_resets = state
            .metrics
            .transformed_arc_resets
            .checked_add(2)
            .ok_or(DoubleScalingError::TraceVerification)?;
    }
    state.imbalances.clone_from(&model.required);
    state.cursors.fill(0);
    for right in model.original_nodes..model.node_count() {
        state.prices[right] = state.prices[right]
            .checked_sub(previous_epsilon)
            .ok_or(DoubleScalingError::TraceVerification)?;
    }
    Ok(())
}

fn replay_start_capacity_phase(state: &mut WorkingState) -> Result<(), DoubleScalingError> {
    let expected_delta = if state.capacity_phase == 0 {
        highest_power_of_two(maximum_positive_imbalance(&state.imbalances).map_err(trace_error)?)
            .map_err(trace_error)?
    } else {
        state.delta / 2
    };
    if expected_delta == 0 {
        return Err(DoubleScalingError::TraceVerification);
    }
    state.capacity_phase = checked_increment(state.capacity_phase)?;
    state.metrics.capacity_phases = checked_increment(state.metrics.capacity_phases)?;
    state.delta = expected_delta;
    state.active_path.clear();
    state.selected_root = None;
    state.selected_deficit = None;
    Ok(())
}

fn replay_select_root(state: &mut WorkingState) -> Result<(), DoubleScalingError> {
    let root = select_large_excess(&state.imbalances, state.delta)
        .map_err(trace_error)?
        .ok_or(DoubleScalingError::TraceVerification)?;
    state.metrics.path_searches = checked_increment(state.metrics.path_searches)?;
    state.active_path.clear();
    state.selected_root = Some(root);
    state.selected_deficit = None;
    Ok(())
}

fn active_path_tip(
    model: &TransportModel,
    state: &WorkingState,
) -> Result<usize, DoubleScalingError> {
    let mut tip = state
        .selected_root
        .ok_or(DoubleScalingError::TraceVerification)?;
    for id in &state.active_path {
        let (from, to) = model
            .arc_endpoints(*id)
            .ok_or(DoubleScalingError::TraceVerification)?;
        if from != tip {
            return Err(DoubleScalingError::TraceVerification);
        }
        tip = to;
    }
    Ok(tip)
}

fn replay_inspect_arc(
    model: &TransportModel,
    state: &mut WorkingState,
) -> Result<(), DoubleScalingError> {
    let tip = active_path_tip(model, state)?;
    let arc_index = *state
        .cursors
        .get(tip)
        .ok_or(DoubleScalingError::TraceVerification)?;
    let arc = *model
        .outgoing
        .get(tip)
        .and_then(|outgoing| outgoing.get(arc_index))
        .ok_or(DoubleScalingError::TraceVerification)?;
    let admissible = model.residual_exists(state, arc)
        && model
            .reduced_cost(&state.prices, arc)
            .map_err(trace_error)?
            < 0;
    state.metrics.transformed_arc_scans = state
        .metrics
        .transformed_arc_scans
        .checked_add(1)
        .ok_or(DoubleScalingError::TraceVerification)?;
    if state.metrics.transformed_arc_scans > DOUBLE_SCALING_MAX_ARC_SCANS {
        return Err(DoubleScalingError::TraceVerification);
    }
    state.inspected_arc = Some(arc);
    if !admissible {
        state.cursors[tip] = arc_index
            .checked_add(1)
            .ok_or(DoubleScalingError::TraceVerification)?;
    }
    Ok(())
}

fn replay_advance(
    model: &TransportModel,
    state: &mut WorkingState,
) -> Result<(), DoubleScalingError> {
    let tip = active_path_tip(model, state)?;
    let arc_index = *state
        .cursors
        .get(tip)
        .ok_or(DoubleScalingError::TraceVerification)?;
    let arc = state
        .inspected_arc
        .take()
        .ok_or(DoubleScalingError::TraceVerification)?;
    if model
        .outgoing
        .get(tip)
        .and_then(|outgoing| outgoing.get(arc_index))
        != Some(&arc)
        || !model.residual_exists(state, arc)
        || model
            .reduced_cost(&state.prices, arc)
            .map_err(trace_error)?
            >= 0
    {
        return Err(DoubleScalingError::TraceVerification);
    }
    let (_, head) = model
        .arc_endpoints(arc)
        .ok_or(DoubleScalingError::TraceVerification)?;
    let root = state
        .selected_root
        .ok_or(DoubleScalingError::TraceVerification)?;
    let mut path_nodes = vec![root];
    let mut node = root;
    for id in &state.active_path {
        let (from, to) = model
            .arc_endpoints(*id)
            .ok_or(DoubleScalingError::TraceVerification)?;
        if from != node {
            return Err(DoubleScalingError::TraceVerification);
        }
        path_nodes.push(to);
        node = to;
    }
    if path_nodes.contains(&head) {
        return Err(DoubleScalingError::TraceVerification);
    }
    state.metrics.advances = checked_increment(state.metrics.advances)?;
    state.cursors[tip] = arc_index;
    state.active_path.push(arc);
    Ok(())
}

fn replay_relabel(
    model: &TransportModel,
    state: &mut WorkingState,
) -> Result<(), DoubleScalingError> {
    let tip = active_path_tip(model, state)?;
    if state.cursors[tip]
        != model
            .outgoing
            .get(tip)
            .ok_or(DoubleScalingError::TraceVerification)?
            .len()
    {
        return Err(DoubleScalingError::TraceVerification);
    }
    state.inspected_arc = None;
    state.metrics.relabels = checked_increment(state.metrics.relabels)?;
    state.prices[tip] = state.prices[tip]
        .checked_sub(state.epsilon)
        .ok_or(DoubleScalingError::TraceVerification)?;
    state.cursors[tip] = 0;
    Ok(())
}

fn replay_augment(
    model: &TransportModel,
    state: &mut WorkingState,
) -> Result<(), DoubleScalingError> {
    let root = state
        .selected_root
        .ok_or(DoubleScalingError::TraceVerification)?;
    let deficit = active_path_tip(model, state)?;
    let amount = i128::try_from(state.delta).map_err(trace_error)?;
    if state.imbalances[root] < amount || state.imbalances[deficit] >= 0 {
        return Err(DoubleScalingError::TraceVerification);
    }
    let path = state.active_path.clone();
    if path.is_empty() {
        return Err(DoubleScalingError::TraceVerification);
    }
    for id in &path {
        if !model.residual_exists(state, *id)
            || model
                .reduced_cost(&state.prices, *id)
                .map_err(trace_error)?
                >= 0
        {
            return Err(DoubleScalingError::TraceVerification);
        }
        update_transformed_flow(model, state, *id, state.delta).map_err(trace_error)?;
    }
    state.imbalances[root] = state.imbalances[root]
        .checked_sub(amount)
        .ok_or(DoubleScalingError::TraceVerification)?;
    state.imbalances[deficit] = state.imbalances[deficit]
        .checked_add(amount)
        .ok_or(DoubleScalingError::TraceVerification)?;
    state.selected_deficit = Some(deficit);
    state.metrics.augmentations = checked_increment(state.metrics.augmentations)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_minimum_mean_cycle_canceling;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), supply))
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("network")
    }

    fn target(graph: &FlowNetwork) -> Vec<i128> {
        graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect()
    }

    #[test]
    fn traces_nested_cost_and_capacity_scaling_with_exact_replay() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 3, -4),
                ("bc", "b", "c", 0, 3, 1),
                ("ca", "c", "a", 0, 3, 1),
                ("ac", "a", "c", 0, 2, 2),
            ],
        );
        let traced = trace_double_scaling(&graph, &target(&graph)).expect("trace");
        let fast = solve_double_scaling(&graph, &target(&graph)).expect("fast");

        assert_eq!(traced.result, fast);
        assert_eq!(traced.result.certificate.total_cost, -6);
        assert!(traced.result.metrics.cost_phases > 0);
        assert!(traced.result.metrics.capacity_phases > 0);
        assert!(traced.result.metrics.augmentations > 0);
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.after.stage == DoubleScalingStage::Relabel })
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.after.stage == DoubleScalingStage::Augment })
        );
        check_double_scaling_trace(&graph, &target(&graph), &traced).expect("checked trace");
    }

    #[test]
    fn publishes_each_transformed_arc_scan_with_exact_identity() {
        let graph = network(
            &[("a", 0), ("b", 0), ("c", 0)],
            &[
                ("ab", "a", "b", 0, 3, -4),
                ("bc", "b", "c", 0, 3, 1),
                ("ca", "c", "a", 0, 3, 1),
                ("ac", "a", "c", 0, 2, 2),
            ],
        );
        let traced = trace_double_scaling(&graph, &target(&graph)).expect("trace");
        let inspections = traced
            .events
            .iter()
            .filter(|event| event.after.stage == DoubleScalingStage::InspectArc)
            .collect::<Vec<_>>();

        assert_eq!(
            u128::try_from(inspections.len()).expect("small trace"),
            traced.result.metrics.transformed_arc_scans
        );
        assert!(inspections.iter().all(|event| {
            event.catalog_id == "double-scaling.inspect-transformed-residual-arc"
                && event.after.inspected_arc.is_some()
                && event.after.metrics.transformed_arc_scans
                    == event.before.metrics.transformed_arc_scans + 1
        }));
        assert!(inspections.iter().any(|event| {
            event
                .before
                .cursors
                .iter()
                .zip(&event.after.cursors)
                .any(|(before, after)| before != after)
        }));
        assert!(inspections.iter().any(|event| {
            event.before.cursors == event.after.cursors
                && event
                    .after
                    .inspected_arc
                    .is_some_and(|arc| event.before.inspected_arc != Some(arc))
        }));
    }

    #[test]
    fn supports_zero_cost_lower_bounds_supplies_parallel_arcs_and_self_loops() {
        let graph = network(
            &[("s", 2), ("t", -2)],
            &[
                ("cheap", "s", "t", 1, 3, -2),
                ("parallel", "s", "t", 0, 3, 4),
                ("return", "t", "s", 0, 2, 1),
                ("loop", "s", "s", 0, 1, -1),
                ("zero", "t", "t", 0, 2, 0),
            ],
        );
        let traced = trace_double_scaling(&graph, &target(&graph)).expect("trace");

        check_min_cost_flow(&graph, &target(&graph), &traced.result.flows).expect("certificate");
        assert_eq!(traced.result.certificate.total_cost, -6);
        assert_eq!(traced.final_snapshot.epsilon, 1);
        check_double_scaling_trace(&graph, &target(&graph), &traced).expect("checked trace");
    }

    #[test]
    fn deterministic_small_graphs_match_minimum_mean_cycle_canceling() {
        let mut seed = 0x0051_c41e_u64;
        for case in 0..32 {
            let node_count = 2 + usize::try_from(next(&mut seed) % 4).expect("small");
            let nodes = (0..node_count)
                .map(|index| (format!("v{index}"), 0_i64))
                .collect::<Vec<_>>();
            let mut edges = Vec::new();
            for from in 0..node_count {
                for to in 0..node_count {
                    if next(&mut seed).is_multiple_of(3) {
                        continue;
                    }
                    edges.push((
                        format!("e{from}_{to}"),
                        format!("v{from}"),
                        format!("v{to}"),
                        0,
                        1 + next(&mut seed) % 3,
                        i64::try_from(next(&mut seed) % 9).expect("cost") - 4,
                    ));
                }
            }
            if edges.is_empty() {
                edges.push((
                    "fallback".to_owned(),
                    "v0".to_owned(),
                    "v1".to_owned(),
                    0,
                    1,
                    0,
                ));
            }
            let graph = FlowNetwork::new(
                nodes
                    .iter()
                    .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), *supply))
                    .collect(),
                edges
                    .iter()
                    .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower: *lower,
                        capacity: *capacity,
                        cost: *cost,
                    })
                    .collect(),
            )
            .expect("graph");
            let required = vec![0_i128; node_count];
            let expected = solve_minimum_mean_cycle_canceling(&graph, &required)
                .unwrap_or_else(|error| panic!("MMCC case {case}: {error}"));
            let actual = solve_double_scaling(&graph, &required)
                .unwrap_or_else(|error| panic!("double scaling case {case}: {error}"));
            assert_eq!(
                actual.certificate.total_cost, expected.certificate.total_cost,
                "case {case}"
            );
        }
    }

    #[test]
    fn trace_checker_rejects_corrupted_current_arc_cursor() {
        let graph = network(
            &[("s", 1), ("m", 0), ("t", -1)],
            &[
                ("sm", "s", "m", 0, 2, -2),
                ("mt", "m", "t", 0, 2, 1),
                ("st", "s", "t", 0, 2, 3),
            ],
        );
        let required = target(&graph);
        let mut traced = trace_double_scaling(&graph, &required).expect("trace");
        let event = traced
            .events
            .iter_mut()
            .find(|event| event.after.stage == DoubleScalingStage::Advance)
            .expect("advance event");
        event.after.cursors[0] = usize::MAX;

        assert_eq!(
            check_double_scaling_trace(&graph, &required, &traced),
            Err(DoubleScalingError::TraceVerification)
        );
    }

    fn next(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }
}
