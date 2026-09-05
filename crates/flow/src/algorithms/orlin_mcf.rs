//! Orlin's strongly-polynomial capacitated minimum-cost-flow algorithm.
//!
//! The implementation follows Orlin (1993), Sections 4 and 5. Each
//! lower-shifted finite-capacity arc is replaced by a demand node and two
//! uncapacitated branches. The enhanced RHS-scaling kernel contracts strongly
//! feasible branches, runs shortest paths after eliminating uncontracted
//! capacity nodes, expands the dual, and recovers a primal optimum on the
//! zero-reduced-cost subnetwork.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{
    CapturedFeasibilityAnchor, FeasibilityError, FeasibilityExecution, FeasibilityUse,
};
use crate::model::{
    EdgeId, FlowEdge, FlowModelError, FlowNetwork, FlowNode, NodeId, NodeIndex, UnresolvedFlowEdge,
};
use crate::residual::ResidualDirection;

/// Conservative original-node limit for the explicit interactive build.
pub const ORLIN_MCF_MAX_NODES: usize = 32;
/// Conservative original-edge limit for the explicit interactive build.
pub const ORLIN_MCF_MAX_EDGES: usize = 96;
/// Maximum transformed original-plus-capacity nodes.
pub const ORLIN_MCF_MAX_TRANSFORMED_NODES: usize = 128;
/// Maximum exact scaling phases.
pub const ORLIN_MCF_MAX_PHASES: u64 = 20_000;
/// Maximum compressed shortest-path augmentations.
pub const ORLIN_MCF_MAX_AUGMENTATIONS: u64 = 100_000;
/// Maximum transformed residual or shortcut scans.
pub const ORLIN_MCF_MAX_SCANS: u128 = 40_000_000;
/// Preserve every small-instance scan and logarithmic witnesses thereafter.
const ORLIN_MCF_TRACE_SCAN_PREFIX: u128 = 512;
/// Maximum number of transformed-arc scans represented by one later Detail boundary.
const ORLIN_MCF_TRACE_SCAN_BLOCK: u128 = 256;

/// One of the two uncapacitated branches replacing a finite-capacity arc.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OrlinMcfBranch {
    /// Costed tail-to-capacity-node branch; its final flow is the variable flow.
    Flow,
    /// Zero-cost head-to-capacity-node branch; its final flow is unused capacity.
    Slack,
}

/// Stable residual identity in the transformed graph.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OrlinMcfArcId {
    /// Original finite-capacity edge.
    pub edge_id: EdgeId,
    /// Flow or slack branch.
    pub branch: OrlinMcfBranch,
    /// Residual direction.
    pub direction: ResidualDirection,
}

/// Identity of a transformed node.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OrlinMcfNodeKind {
    /// Original graph node.
    Original(NodeIndex),
    /// Demand node introduced for one positive-width original edge.
    Capacity(EdgeId),
}

/// Source-defined trace boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrlinMcfStage {
    /// Original bounded instance is validated but not transformed visibly yet.
    Ready,
    /// Section 5 capacity-node transformation is visible.
    TransformCapacities,
    /// Initial dual-feasible potentials are installed.
    InitializeDual,
    /// A no-flow regeneration jumps delta to the maximum component imbalance.
    CompleteRegeneration,
    /// One delta phase begins.
    BeginPhase,
    /// A strongly feasible transformed branch contracts two components.
    Contract,
    /// One transformed branch is inspected as a contraction candidate.
    InspectContractibleArc,
    /// One transformed residual branch is inspected by reverse reachability.
    InspectReachabilityArc,
    /// One transformed residual branch is classified for quotient compression.
    InspectCompressedResidualArc,
    /// One compressed segment is inspected by quotient shortest path search.
    InspectCompressedArc,
    /// A shortest path on the capacity-node-eliminated quotient is selected.
    SelectCompressedPath,
    /// Potentials and exact-delta pseudoflow are updated atomically.
    Augment,
    /// No active source/deficit pair remains at the current delta.
    CompletePhase,
    /// Delta is halved with exact common-denominator arithmetic.
    HalveScale,
    /// Contracted component potentials are interpreted on every transformed node.
    ExpandDual,
    /// A zero-reduced-cost max-flow recovers original bounded flows.
    RecoverPrimal,
    /// The original instance passes an independent min-cost certificate.
    Optimal,
}

/// Deterministic source-algorithm counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OrlinMcfMetrics {
    /// Positive-width edges transformed into demand nodes.
    pub capacity_nodes: u64,
    /// Delta phases entered.
    pub scaling_phases: u64,
    /// Complete regenerations.
    pub complete_regenerations: u64,
    /// Strongly feasible branch contractions.
    pub contractions: u64,
    /// Compressed quotient shortest-path runs.
    pub shortest_path_runs: u64,
    /// Capacity nodes eliminated across shortest-path runs.
    pub eliminated_capacity_nodes: u64,
    /// Two-arc shortcuts materialized across shortest-path runs.
    pub shortcut_arcs: u64,
    /// Exact-delta augmentations.
    pub augmentations: u64,
    /// Transformed residual arcs in augmented paths.
    pub augmented_arcs: u64,
    /// Potential-vector updates.
    pub potential_updates: u64,
    /// Residual arcs and shortcuts inspected.
    pub residual_arc_scans: u128,
    /// Final zero-reduced-cost primal recoveries.
    pub primal_recoveries: u64,
}

/// One transformed node at a public boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfNodeState {
    /// Stable node role.
    pub kind: OrlinMcfNodeKind,
    /// Canonical quotient-component root.
    pub component: usize,
    /// Exact aggregate component imbalance numerator.
    pub component_excess_numerator: i128,
    /// Exact node potential.
    pub potential: i128,
    /// Capped shortest-path dual label repeated for all component members.
    pub distance: Option<i128>,
}

/// One transformed branch at a public boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfArcState {
    /// Stable transformed residual identity in the forward direction.
    pub edge_id: EdgeId,
    /// Flow or slack branch.
    pub branch: OrlinMcfBranch,
    /// Exact nonnegative pseudoflow numerator.
    pub flow_numerator: i128,
    /// Exact forward reduced cost.
    pub reduced_cost: i128,
}

/// Complete state at one reversible source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfSnapshot {
    /// Semantic boundary.
    pub stage: OrlinMcfStage,
    /// Common denominator for delta, transformed flows, and component excesses.
    pub denominator: u128,
    /// Exact delta numerator.
    pub delta_numerator: u128,
    /// Transformed node state in original-node then capacity-node order.
    pub nodes: Vec<OrlinMcfNodeState>,
    /// Two branches per positive-width original edge.
    pub arcs: Vec<OrlinMcfArcState>,
    /// Selected active source component.
    pub source_component: Option<usize>,
    /// Selected active deficit component.
    pub sink_component: Option<usize>,
    /// Expanded transformed residual path represented by compressed Dijkstra.
    pub path: Vec<OrlinMcfArcId>,
    /// Exact transformed segment touched by an internal scan boundary.
    pub inspected_segment: Vec<OrlinMcfArcId>,
    /// Number of uncontracted capacity nodes eliminated in the selected search.
    pub eliminated_capacity_nodes: u64,
    /// Number of two-arc shortcuts in the selected compressed graph.
    pub shortcut_arcs: u64,
    /// Recovered original bounded flows at terminal boundaries.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact counters.
    pub metrics: OrlinMcfMetrics,
}

/// One reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: OrlinMcfSnapshot,
    /// State after the transition.
    pub after: OrlinMcfSnapshot,
    /// Contracted transformed branch, when applicable.
    pub contraction_arc: Option<OrlinMcfArcId>,
    /// Exact augmentation numerator, when applicable.
    pub augmentation_numerator: Option<u128>,
}

/// Certified exact result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfResult {
    /// Original bounded flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent primal/dual certificate on the original instance.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic source counters.
    pub metrics: OrlinMcfMetrics,
    /// Fast-profile terminal state.
    pub final_snapshot: OrlinMcfSnapshot,
}

/// Certified result with every source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrlinMcfTraceResult {
    /// Same result returned by the fast profile.
    pub result: OrlinMcfResult,
    /// Ready boundary.
    pub base_snapshot: OrlinMcfSnapshot,
    /// Reversible transitions.
    pub events: Vec<OrlinMcfTraceEvent>,
    /// Certified terminal boundary.
    pub final_snapshot: OrlinMcfSnapshot,
}

/// Domain, work, arithmetic, construction, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OrlinMcfError {
    /// Input exceeds the conservative explicit transformed-network band.
    #[error("graph exceeds orlin-mcf admission limits")]
    AdmissionLimit,
    /// A deterministic work ceiling was reached.
    #[error("orlin-mcf work limit reached")]
    WorkLimit,
    /// Requested balances are infeasible in the original bounded model.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A temporary zero-reduced-cost network could not be constructed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact integer or dyadic arithmetic overflowed.
    #[error("orlin-mcf arithmetic overflow")]
    ArithmeticOverflow,
    /// A transformed, quotient, dual, path, or recovery invariant failed.
    #[error("orlin-mcf invariant failed")]
    Invariant,
    /// A named internal boundary failed; the label is stable diagnostic context.
    #[error("orlin-mcf invariant failed at {0}")]
    InvariantAt(&'static str),
    /// A public trace did not replay under the source grammar.
    #[error("orlin-mcf trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Debug)]
struct VariableEdge {
    original_index: usize,
    edge_id: EdgeId,
    from: usize,
    to: usize,
    width: u64,
    cost: i128,
}

#[derive(Clone, Debug)]
struct TransformModel {
    original_nodes: usize,
    variable_edges: Vec<VariableEdge>,
    required: Vec<i128>,
    lower_flows: Vec<u64>,
}

impl TransformModel {
    fn node_count(&self) -> usize {
        self.original_nodes + self.variable_edges.len()
    }

    fn variable_index(&self, edge_id: &EdgeId) -> Option<usize> {
        self.variable_edges
            .binary_search_by(|edge| edge.edge_id.cmp(edge_id))
            .ok()
    }

    fn branch_index(branch: OrlinMcfBranch) -> usize {
        match branch {
            OrlinMcfBranch::Flow => 0,
            OrlinMcfBranch::Slack => 1,
        }
    }

    fn node_kind(&self, node: usize) -> Option<OrlinMcfNodeKind> {
        if node < self.original_nodes {
            return NodeIndex::try_from_usize(node).map(OrlinMcfNodeKind::Original);
        }
        self.variable_edges
            .get(node.checked_sub(self.original_nodes)?)
            .map(|edge| OrlinMcfNodeKind::Capacity(edge.edge_id.clone()))
    }

    fn endpoints(&self, variable: usize, branch: OrlinMcfBranch) -> Option<(usize, usize)> {
        let edge = self.variable_edges.get(variable)?;
        let from = match branch {
            OrlinMcfBranch::Flow => edge.from,
            OrlinMcfBranch::Slack => edge.to,
        };
        Some((from, self.original_nodes + variable))
    }

    fn forward_cost(&self, variable: usize, branch: OrlinMcfBranch) -> Option<i128> {
        let edge = self.variable_edges.get(variable)?;
        Some(match branch {
            OrlinMcfBranch::Flow => edge.cost,
            OrlinMcfBranch::Slack => 0,
        })
    }

    fn arc_endpoints(&self, id: &OrlinMcfArcId) -> Option<(usize, usize)> {
        let variable = self.variable_index(&id.edge_id)?;
        let (from, to) = self.endpoints(variable, id.branch)?;
        Some(match id.direction {
            ResidualDirection::Forward => (from, to),
            ResidualDirection::Reverse => (to, from),
        })
    }

    fn arc_cost(&self, id: &OrlinMcfArcId) -> Option<i128> {
        let variable = self.variable_index(&id.edge_id)?;
        let forward = self.forward_cost(variable, id.branch)?;
        Some(match id.direction {
            ResidualDirection::Forward => forward,
            ResidualDirection::Reverse => -forward,
        })
    }
}

#[derive(Clone)]
struct WorkingState {
    denominator: u128,
    delta_numerator: u128,
    flows: Vec<[i128; 2]>,
    potentials: Vec<i128>,
    component_of: Vec<usize>,
    metrics: OrlinMcfMetrics,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReducedArc {
    from: usize,
    to: usize,
    cost: i128,
    segment: Vec<OrlinMcfArcId>,
}

struct SearchResult {
    source: usize,
    sink: usize,
    path: Vec<OrlinMcfArcId>,
    distances_by_node: Vec<Option<i128>>,
    eliminated_capacity_nodes: u64,
    shortcut_arcs: u64,
    scan_checkpoints: Vec<OrlinMcfScanCheckpoint>,
}

#[derive(Clone)]
struct OrlinMcfScanCheckpoint {
    stage: OrlinMcfStage,
    inspected_segment: Vec<OrlinMcfArcId>,
    distances_by_node: Vec<Option<i128>>,
    metrics: OrlinMcfMetrics,
}

struct InternalRun {
    result: OrlinMcfResult,
    base_snapshot: OrlinMcfSnapshot,
    events: Vec<OrlinMcfTraceEvent>,
    final_snapshot: OrlinMcfSnapshot,
}

/// Solves a bounded integral minimum-cost-flow instance using Orlin's Section
/// 5 transformation and Section 4 strongly-polynomial scaling kernel.
///
/// # Errors
///
/// Returns an admission, feasibility, work, arithmetic, invariant, recovery,
/// or certificate failure.
pub fn solve_orlin_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<OrlinMcfResult, OrlinMcfError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting every feasibility kernel executed by this same run
/// to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_orlin_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<OrlinMcfResult, OrlinMcfError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Retains every transform, scaling, contraction, compressed shortest path,
/// augmentation, dual expansion, and primal recovery boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_orlin_mcf`] plus trace verification.
pub fn trace_orlin_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<OrlinMcfTraceResult, OrlinMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_orlin_mcf_with_feasibility(graph, required_divergence, &mut feasibility)
}

/// Retains the source trace while explicitly recording each feasibility
/// kernel executed by this same run.
///
/// # Errors
///
/// Returns the same failures as [`trace_orlin_mcf`].
pub fn trace_orlin_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<OrlinMcfTraceResult, OrlinMcfError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = OrlinMcfTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_orlin_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

fn transform(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<TransformModel, OrlinMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    transform_with_feasibility(graph, required_divergence, &mut feasibility)
}

fn transform_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<TransformModel, OrlinMcfError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > ORLIN_MCF_MAX_NODES
        || graph.edges().len() > ORLIN_MCF_MAX_EDGES
        || required_divergence.len() != graph.nodes().len()
    {
        return Err(OrlinMcfError::AdmissionLimit);
    }
    let balance = required_divergence.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(OrlinMcfError::ArithmeticOverflow)
    })?;
    if balance != 0 {
        return Err(OrlinMcfError::InvariantAt("input-balance"));
    }
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower_flows)?;
    let mut required = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&wanted, actual)| {
            wanted
                .checked_sub(actual)
                .ok_or(OrlinMcfError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut variable_edges = Vec::new();
    for (original_index, edge) in graph.edges().iter().enumerate() {
        let width = edge
            .capacity()
            .checked_sub(edge.lower())
            .ok_or(OrlinMcfError::Invariant)?;
        if width == 0 {
            continue;
        }
        required[edge.to().as_usize()] = required[edge.to().as_usize()]
            .checked_add(i128::from(width))
            .ok_or(OrlinMcfError::ArithmeticOverflow)?;
        required.push(-i128::from(width));
        variable_edges.push(VariableEdge {
            original_index,
            edge_id: edge.id().clone(),
            from: edge.from().as_usize(),
            to: edge.to().as_usize(),
            width,
            cost: i128::from(edge.cost()),
        });
    }
    if graph
        .nodes()
        .len()
        .checked_add(variable_edges.len())
        .is_none_or(|count| count > ORLIN_MCF_MAX_TRANSFORMED_NODES)
    {
        return Err(OrlinMcfError::AdmissionLimit);
    }
    Ok(TransformModel {
        original_nodes: graph.nodes().len(),
        variable_edges,
        required,
        lower_flows,
    })
}

fn initial_potentials(model: &TransformModel) -> Result<Vec<i128>, OrlinMcfError> {
    let mut distances = vec![0_i128; model.node_count()];
    for pass in 0..model.node_count() {
        let mut changed = false;
        for variable in 0..model.variable_edges.len() {
            for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
                let (from, to) = model
                    .endpoints(variable, branch)
                    .ok_or(OrlinMcfError::Invariant)?;
                let candidate = distances[from]
                    .checked_add(
                        model
                            .forward_cost(variable, branch)
                            .ok_or(OrlinMcfError::Invariant)?,
                    )
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
                if candidate < distances[to] {
                    distances[to] = candidate;
                    changed = true;
                    if pass + 1 == model.node_count() {
                        return Err(OrlinMcfError::Invariant);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(distances)
}

fn maximum_magnitude(values: &[i128]) -> u128 {
    values
        .iter()
        .fold(0_u128, |maximum, value| maximum.max(value.unsigned_abs()))
}

fn component_roots(state: &WorkingState) -> impl Iterator<Item = usize> + '_ {
    state
        .component_of
        .iter()
        .enumerate()
        .filter_map(|(node, &root)| (node == root).then_some(root))
}

fn component_excesses(
    model: &TransformModel,
    state: &WorkingState,
) -> Result<Vec<i128>, OrlinMcfError> {
    let denominator =
        i128::try_from(state.denominator).map_err(|_| OrlinMcfError::ArithmeticOverflow)?;
    let mut excess = model
        .required
        .iter()
        .map(|value| {
            value
                .checked_mul(denominator)
                .ok_or(OrlinMcfError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for variable in 0..model.variable_edges.len() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            let (from, to) = model
                .endpoints(variable, branch)
                .ok_or(OrlinMcfError::Invariant)?;
            let flow = state.flows[variable][TransformModel::branch_index(branch)];
            excess[from] = excess[from]
                .checked_sub(flow)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            excess[to] = excess[to]
                .checked_add(flow)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
        }
    }
    let mut aggregate = vec![0_i128; model.node_count()];
    for (node, value) in excess.into_iter().enumerate() {
        let root = state.component_of[node];
        aggregate[root] = aggregate[root]
            .checked_add(value)
            .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    }
    Ok(aggregate)
}

fn reduced_cost(
    model: &TransformModel,
    state: &WorkingState,
    id: &OrlinMcfArcId,
) -> Result<i128, OrlinMcfError> {
    let (from, to) = model.arc_endpoints(id).ok_or(OrlinMcfError::Invariant)?;
    model
        .arc_cost(id)
        .and_then(|cost| cost.checked_add(state.potentials[from]))
        .and_then(|value| value.checked_sub(state.potentials[to]))
        .ok_or(OrlinMcfError::ArithmeticOverflow)
}

fn residual_available(model: &TransformModel, state: &WorkingState, id: &OrlinMcfArcId) -> bool {
    match id.direction {
        ResidualDirection::Forward => true,
        ResidualDirection::Reverse => model.variable_index(&id.edge_id).is_some_and(|variable| {
            state.flows[variable][TransformModel::branch_index(id.branch)] > 0
        }),
    }
}

fn validate_dual(model: &TransformModel, state: &WorkingState) -> Result<(), OrlinMcfError> {
    if state.potentials.len() != model.node_count()
        || state.flows.len() != model.variable_edges.len()
    {
        return Err(OrlinMcfError::InvariantAt("dual-shape"));
    }
    for variable in 0..model.variable_edges.len() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            let id = OrlinMcfArcId {
                edge_id: model.variable_edges[variable].edge_id.clone(),
                branch,
                direction: ResidualDirection::Forward,
            };
            let reduced = reduced_cost(model, state, &id)?;
            let flow = state.flows[variable][TransformModel::branch_index(branch)];
            if reduced < 0 || flow < 0 || (flow > 0 && reduced != 0) {
                return Err(OrlinMcfError::InvariantAt("dual-feasibility"));
            }
        }
    }
    Ok(())
}

fn bump_scan(state: &mut WorkingState) -> Result<(), OrlinMcfError> {
    state.metrics.residual_arc_scans = state
        .metrics
        .residual_arc_scans
        .checked_add(1)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    if state.metrics.residual_arc_scans > ORLIN_MCF_MAX_SCANS {
        return Err(OrlinMcfError::WorkLimit);
    }
    Ok(())
}

const fn should_record_scan(scan: u128) -> bool {
    scan <= ORLIN_MCF_TRACE_SCAN_PREFIX
        || (scan - ORLIN_MCF_TRACE_SCAN_PREFIX).is_multiple_of(ORLIN_MCF_TRACE_SCAN_BLOCK)
}

fn record_scan_checkpoint(
    checkpoints: &mut Option<&mut Vec<OrlinMcfScanCheckpoint>>,
    state: &WorkingState,
    checkpoint_stage: OrlinMcfStage,
    inspected_segment: Vec<OrlinMcfArcId>,
    distances_by_node: Vec<Option<i128>>,
) -> OrlinMcfScanCheckpoint {
    let checkpoint = OrlinMcfScanCheckpoint {
        stage: checkpoint_stage,
        inspected_segment,
        distances_by_node,
        metrics: state.metrics,
    };
    if should_record_scan(state.metrics.residual_arc_scans)
        && let Some(checkpoints) = checkpoints.as_deref_mut()
    {
        checkpoints.push(checkpoint.clone());
    }
    checkpoint
}

fn flush_final_scan_checkpoint(
    checkpoints: &mut Option<&mut Vec<OrlinMcfScanCheckpoint>>,
    final_checkpoint: Option<OrlinMcfScanCheckpoint>,
) {
    if let Some(final_checkpoint) = final_checkpoint
        && let Some(checkpoints) = checkpoints.as_deref_mut()
        && checkpoints
            .last()
            .is_none_or(|checkpoint| checkpoint.metrics != final_checkpoint.metrics)
    {
        checkpoints.push(final_checkpoint);
    }
}

fn component_distances_by_node(
    state: &WorkingState,
    component_distances: &[Option<i128>],
) -> Vec<Option<i128>> {
    state
        .component_of
        .iter()
        .map(|&root| component_distances[root])
        .collect()
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, OrlinMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

#[expect(
    clippy::too_many_lines,
    reason = "the paper's transform, contraction, scaling, expansion, and recovery phases remain auditable in source order"
)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, OrlinMcfError> {
    let model = transform_with_feasibility(graph, required_divergence, feasibility)?;
    let mut state = WorkingState {
        denominator: 1,
        delta_numerator: maximum_magnitude(&model.required),
        flows: vec![[0, 0]; model.variable_edges.len()],
        potentials: vec![0; model.node_count()],
        component_of: (0..model.node_count()).collect(),
        metrics: OrlinMcfMetrics::default(),
    };
    let empty_distances = vec![None; model.node_count()];
    let base_snapshot = snapshot(
        &model,
        &state,
        OrlinMcfStage::Ready,
        None,
        None,
        Vec::new(),
        &empty_distances,
        0,
        0,
        None,
    )?;
    let mut current = base_snapshot.clone();
    let mut events = Vec::new();
    state.metrics.capacity_nodes =
        u64::try_from(model.variable_edges.len()).map_err(|_| OrlinMcfError::ArithmeticOverflow)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "orlin-mcf.transform-capacities",
        snapshot(
            &model,
            &state,
            OrlinMcfStage::TransformCapacities,
            None,
            None,
            Vec::new(),
            &empty_distances,
            0,
            0,
            None,
        )?,
        None,
        None,
    );
    state.potentials = initial_potentials(&model)?;
    validate_dual(&model, &state)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "orlin-mcf.initialize-dual",
        snapshot(
            &model,
            &state,
            OrlinMcfStage::InitializeDual,
            None,
            None,
            Vec::new(),
            &empty_distances,
            0,
            0,
            None,
        )?,
        None,
        None,
    );

    while has_imbalanced_component(&model, &state)? {
        if state.metrics.scaling_phases >= ORLIN_MCF_MAX_PHASES {
            return Err(OrlinMcfError::WorkLimit);
        }
        if should_complete_regenerate(&model, &state)? {
            state.delta_numerator = maximum_component_excess(&model, &state)?;
            state.metrics.complete_regenerations = state
                .metrics
                .complete_regenerations
                .checked_add(1)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "orlin-mcf.complete-regeneration",
                snapshot(
                    &model,
                    &state,
                    OrlinMcfStage::CompleteRegeneration,
                    None,
                    None,
                    Vec::new(),
                    &empty_distances,
                    0,
                    0,
                    None,
                )?,
                None,
                None,
            );
        }
        if state.delta_numerator == 0 {
            return Err(OrlinMcfError::Invariant);
        }
        state.metrics.scaling_phases = state
            .metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(OrlinMcfError::ArithmeticOverflow)?;
        publish(
            &mut current,
            &mut events,
            record_trace,
            "orlin-mcf.begin-phase",
            snapshot(
                &model,
                &state,
                OrlinMcfStage::BeginPhase,
                None,
                None,
                Vec::new(),
                &empty_distances,
                0,
                0,
                None,
            )?,
            None,
            None,
        );

        loop {
            loop {
                let mut scan_checkpoints = Vec::new();
                let contracted = next_contractible_arc(
                    &model,
                    &mut state,
                    record_trace.then_some(&mut scan_checkpoints),
                )?;
                publish_scan_checkpoints(
                    &model,
                    &state,
                    &mut current,
                    &mut events,
                    record_trace,
                    scan_checkpoints,
                )?;
                let Some(contracted) = contracted else {
                    break;
                };
                let (from, to) = model
                    .arc_endpoints(&contracted)
                    .ok_or(OrlinMcfError::Invariant)?;
                merge_components(&mut state, from, to)?;
                state.metrics.contractions = state
                    .metrics
                    .contractions
                    .checked_add(1)
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
                if state.metrics.contractions
                    >= u64::try_from(model.node_count())
                        .map_err(|_| OrlinMcfError::ArithmeticOverflow)?
                {
                    return Err(OrlinMcfError::WorkLimit);
                }
                publish(
                    &mut current,
                    &mut events,
                    record_trace,
                    "orlin-mcf.contract",
                    snapshot(
                        &model,
                        &state,
                        OrlinMcfStage::Contract,
                        None,
                        None,
                        Vec::new(),
                        &empty_distances,
                        0,
                        0,
                        None,
                    )?,
                    Some(contracted),
                    None,
                );
            }

            let mut scan_checkpoints = Vec::new();
            let selected = select_reachable_active_source(
                &model,
                &mut state,
                record_trace.then_some(&mut scan_checkpoints),
            )?;
            publish_scan_checkpoints(
                &model,
                &state,
                &mut current,
                &mut events,
                record_trace,
                scan_checkpoints,
            )?;
            let Some((source, sinks)) = selected else {
                break;
            };
            if state.metrics.augmentations >= ORLIN_MCF_MAX_AUGMENTATIONS {
                return Err(OrlinMcfError::WorkLimit);
            }
            let mut search =
                compressed_shortest_path(&model, &mut state, source, &sinks, record_trace)?;
            let scan_checkpoints = std::mem::take(&mut search.scan_checkpoints);
            publish_scan_checkpoints(
                &model,
                &state,
                &mut current,
                &mut events,
                record_trace,
                scan_checkpoints,
            )?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "orlin-mcf.select-compressed-path",
                snapshot(
                    &model,
                    &state,
                    OrlinMcfStage::SelectCompressedPath,
                    Some(search.source),
                    Some(search.sink),
                    search.path.clone(),
                    &search.distances_by_node,
                    search.eliminated_capacity_nodes,
                    search.shortcut_arcs,
                    None,
                )?,
                None,
                None,
            );
            apply_search(&model, &mut state, &search)?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "orlin-mcf.augment",
                snapshot(
                    &model,
                    &state,
                    OrlinMcfStage::Augment,
                    Some(search.source),
                    Some(search.sink),
                    search.path,
                    &search.distances_by_node,
                    search.eliminated_capacity_nodes,
                    search.shortcut_arcs,
                    None,
                )?,
                None,
                Some(state.delta_numerator),
            );
        }

        publish(
            &mut current,
            &mut events,
            record_trace,
            "orlin-mcf.complete-phase",
            snapshot(
                &model,
                &state,
                OrlinMcfStage::CompletePhase,
                None,
                None,
                Vec::new(),
                &empty_distances,
                0,
                0,
                None,
            )?,
            None,
            None,
        );
        halve_scale(&mut state)?;
        publish(
            &mut current,
            &mut events,
            record_trace,
            "orlin-mcf.halve-scale",
            snapshot(
                &model,
                &state,
                OrlinMcfStage::HalveScale,
                None,
                None,
                Vec::new(),
                &empty_distances,
                0,
                0,
                None,
            )?,
            None,
            None,
        );
    }

    publish(
        &mut current,
        &mut events,
        record_trace,
        "orlin-mcf.expand-dual",
        snapshot(
            &model,
            &state,
            OrlinMcfStage::ExpandDual,
            None,
            None,
            Vec::new(),
            &empty_distances,
            0,
            0,
            None,
        )?,
        None,
        None,
    );
    let flows = recover_original_primal(graph, required_divergence, &model, &state, feasibility)?;
    state.metrics.primal_recoveries = 1;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "orlin-mcf.recover-primal",
        snapshot(
            &model,
            &state,
            OrlinMcfStage::RecoverPrimal,
            None,
            None,
            Vec::new(),
            &empty_distances,
            0,
            0,
            Some(flows.clone()),
        )?,
        None,
        None,
    );
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "orlin-mcf.optimal",
        snapshot(
            &model,
            &state,
            OrlinMcfStage::Optimal,
            None,
            None,
            Vec::new(),
            &empty_distances,
            0,
            0,
            Some(flows.clone()),
        )?,
        None,
        None,
    );
    let final_snapshot = current;
    let result = OrlinMcfResult {
        flows,
        certificate,
        metrics: state.metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn has_imbalanced_component(
    model: &TransformModel,
    state: &WorkingState,
) -> Result<bool, OrlinMcfError> {
    let excess = component_excesses(model, state)?;
    Ok(component_roots(state).any(|root| excess[root] != 0))
}

fn maximum_component_excess(
    model: &TransformModel,
    state: &WorkingState,
) -> Result<u128, OrlinMcfError> {
    let excess = component_excesses(model, state)?;
    Ok(component_roots(state).fold(0_u128, |maximum, root| {
        maximum.max(excess[root].unsigned_abs())
    }))
}

fn should_complete_regenerate(
    model: &TransformModel,
    state: &WorkingState,
) -> Result<bool, OrlinMcfError> {
    for variable in 0..model.variable_edges.len() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            let (from, to) = model
                .endpoints(variable, branch)
                .ok_or(OrlinMcfError::Invariant)?;
            if state.flows[variable][TransformModel::branch_index(branch)] != 0
                && state.component_of[from] != state.component_of[to]
            {
                return Ok(false);
            }
        }
    }
    let maximum = maximum_component_excess(model, state)?;
    Ok(maximum > 0 && maximum < state.delta_numerator)
}

fn next_contractible_arc(
    model: &TransformModel,
    state: &mut WorkingState,
    mut scan_checkpoints: Option<&mut Vec<OrlinMcfScanCheckpoint>>,
) -> Result<Option<OrlinMcfArcId>, OrlinMcfError> {
    let threshold = state
        .delta_numerator
        .checked_mul(
            u128::try_from(model.node_count()).map_err(|_| OrlinMcfError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    let mut final_checkpoint = None;
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            bump_scan(state)?;
            let inspected = OrlinMcfArcId {
                edge_id: edge.edge_id.clone(),
                branch,
                direction: ResidualDirection::Forward,
            };
            final_checkpoint = Some(record_scan_checkpoint(
                &mut scan_checkpoints,
                state,
                OrlinMcfStage::InspectContractibleArc,
                vec![inspected.clone()],
                vec![None; model.node_count()],
            ));
            let (from, to) = model
                .endpoints(variable, branch)
                .ok_or(OrlinMcfError::Invariant)?;
            if state.component_of[from] == state.component_of[to]
                || u128::try_from(state.flows[variable][TransformModel::branch_index(branch)])
                    .map_err(|_| OrlinMcfError::Invariant)?
                    < threshold
            {
                continue;
            }
            let id = inspected;
            if reduced_cost(model, state, &id)? != 0 {
                return Err(OrlinMcfError::Invariant);
            }
            flush_final_scan_checkpoint(&mut scan_checkpoints, final_checkpoint);
            return Ok(Some(id));
        }
    }
    flush_final_scan_checkpoint(&mut scan_checkpoints, final_checkpoint);
    Ok(None)
}

fn merge_components(
    state: &mut WorkingState,
    left_node: usize,
    right_node: usize,
) -> Result<(), OrlinMcfError> {
    let left = state.component_of[left_node];
    let right = state.component_of[right_node];
    if left == right {
        return Err(OrlinMcfError::Invariant);
    }
    let keep = left.min(right);
    let remove = left.max(right);
    for root in &mut state.component_of {
        if *root == remove {
            *root = keep;
        }
    }
    Ok(())
}

fn active_components(
    model: &TransformModel,
    state: &WorkingState,
) -> Result<(Vec<usize>, Vec<usize>), OrlinMcfError> {
    let excess = component_excesses(model, state)?;
    let three_delta = state
        .delta_numerator
        .checked_mul(3)
        .and_then(|value| i128::try_from(value).ok())
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    let sources = component_roots(state)
        .filter(|&root| {
            excess[root]
                .checked_mul(4)
                .is_some_and(|value| value >= three_delta)
        })
        .collect();
    let sinks = component_roots(state)
        .filter(|&root| {
            excess[root]
                .checked_mul(4)
                .is_some_and(|value| value <= -three_delta)
        })
        .collect();
    Ok((sources, sinks))
}

fn all_residual_arcs(model: &TransformModel, state: &WorkingState) -> Vec<OrlinMcfArcId> {
    let mut arcs = Vec::with_capacity(model.variable_edges.len().saturating_mul(4));
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            arcs.push(OrlinMcfArcId {
                edge_id: edge.edge_id.clone(),
                branch,
                direction: ResidualDirection::Forward,
            });
            if state.flows[variable][TransformModel::branch_index(branch)] > 0 {
                arcs.push(OrlinMcfArcId {
                    edge_id: edge.edge_id.clone(),
                    branch,
                    direction: ResidualDirection::Reverse,
                });
            }
        }
    }
    arcs
}

fn select_reachable_active_source(
    model: &TransformModel,
    state: &mut WorkingState,
    mut scan_checkpoints: Option<&mut Vec<OrlinMcfScanCheckpoint>>,
) -> Result<Option<(usize, Vec<usize>)>, OrlinMcfError> {
    let (sources, sinks) = active_components(model, state)?;
    if sources.is_empty() || sinks.is_empty() {
        return Ok(None);
    }
    let residual = all_residual_arcs(model, state);
    let mut can_reach_sink = vec![false; model.node_count()];
    let mut queue = VecDeque::new();
    for &sink in &sinks {
        can_reach_sink[sink] = true;
        queue.push_back(sink);
    }
    let mut final_checkpoint = None;
    while let Some(to_root) = queue.pop_front() {
        for id in &residual {
            bump_scan(state)?;
            final_checkpoint = Some(record_scan_checkpoint(
                &mut scan_checkpoints,
                state,
                OrlinMcfStage::InspectReachabilityArc,
                vec![id.clone()],
                vec![None; model.node_count()],
            ));
            let (from, to) = model.arc_endpoints(id).ok_or(OrlinMcfError::Invariant)?;
            let from_root = state.component_of[from];
            let to = state.component_of[to];
            if to == to_root && from_root != to && !can_reach_sink[from_root] {
                can_reach_sink[from_root] = true;
                queue.push_back(from_root);
            }
        }
    }
    flush_final_scan_checkpoint(&mut scan_checkpoints, final_checkpoint);
    Ok(sources
        .into_iter()
        .find(|&candidate| can_reach_sink[candidate])
        .map(|source| (source, sinks)))
}

struct CompressedGraph {
    arcs: Vec<ReducedArc>,
    incoming_to_eliminated: Vec<Vec<OrlinMcfArcId>>,
    eliminated: Vec<bool>,
    eliminated_count: u64,
    shortcut_count: u64,
}

fn build_compressed_graph(
    model: &TransformModel,
    state: &mut WorkingState,
    source: usize,
    sinks: &[usize],
    scan_checkpoints: &mut Option<&mut Vec<OrlinMcfScanCheckpoint>>,
) -> Result<CompressedGraph, OrlinMcfError> {
    let mut kept = vec![false; model.node_count()];
    for node in 0..model.original_nodes {
        kept[state.component_of[node]] = true;
    }
    kept[source] = true;
    for &sink in sinks {
        kept[sink] = true;
    }
    let mut eliminated = vec![false; model.node_count()];
    for root in component_roots(state) {
        eliminated[root] = !kept[root];
    }
    let residual = all_residual_arcs(model, state);
    let mut direct = Vec::new();
    let mut incoming = vec![Vec::new(); model.node_count()];
    let mut outgoing = vec![Vec::new(); model.node_count()];
    let mut final_checkpoint = None;
    for id in residual {
        bump_scan(state)?;
        final_checkpoint = Some(record_scan_checkpoint(
            scan_checkpoints,
            state,
            OrlinMcfStage::InspectCompressedResidualArc,
            vec![id.clone()],
            vec![None; model.node_count()],
        ));
        let (from, to) = model.arc_endpoints(&id).ok_or(OrlinMcfError::Invariant)?;
        let from = state.component_of[from];
        let to = state.component_of[to];
        if from == to {
            continue;
        }
        match (kept[from], kept[to]) {
            (true, true) => direct.push(ReducedArc {
                from,
                to,
                cost: reduced_cost(model, state, &id)?,
                segment: vec![id],
            }),
            (true, false) => incoming[to].push(id),
            (false, true) => outgoing[from].push(id),
            (false, false) => {
                return Err(OrlinMcfError::InvariantAt("adjacent-capacity-components"));
            }
        }
    }
    flush_final_scan_checkpoint(scan_checkpoints, final_checkpoint);
    let mut shortcut_count = 0_u64;
    for root in component_roots(state).filter(|&root| eliminated[root]) {
        for first in &incoming[root] {
            let (from, _) = model.arc_endpoints(first).ok_or(OrlinMcfError::Invariant)?;
            let from = state.component_of[from];
            for second in &outgoing[root] {
                let (_, to) = model
                    .arc_endpoints(second)
                    .ok_or(OrlinMcfError::Invariant)?;
                let to = state.component_of[to];
                if from == to {
                    continue;
                }
                let cost = reduced_cost(model, state, first)?
                    .checked_add(reduced_cost(model, state, second)?)
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
                direct.push(ReducedArc {
                    from,
                    to,
                    cost,
                    segment: vec![first.clone(), second.clone()],
                });
                shortcut_count = shortcut_count
                    .checked_add(1)
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            }
        }
    }
    direct.sort();
    let eliminated_count = u64::try_from(
        component_roots(state)
            .filter(|&root| eliminated[root])
            .count(),
    )
    .map_err(|_| OrlinMcfError::ArithmeticOverflow)?;
    state.metrics.eliminated_capacity_nodes = state
        .metrics
        .eliminated_capacity_nodes
        .checked_add(eliminated_count)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    state.metrics.shortcut_arcs = state
        .metrics
        .shortcut_arcs
        .checked_add(shortcut_count)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    Ok(CompressedGraph {
        arcs: direct,
        incoming_to_eliminated: incoming,
        eliminated,
        eliminated_count,
        shortcut_count,
    })
}

#[allow(clippy::too_many_lines)]
fn compressed_shortest_path(
    model: &TransformModel,
    state: &mut WorkingState,
    source: usize,
    sinks: &[usize],
    record_trace: bool,
) -> Result<SearchResult, OrlinMcfError> {
    state.metrics.shortest_path_runs = state
        .metrics
        .shortest_path_runs
        .checked_add(1)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    let mut scan_checkpoints = Vec::new();
    let mut checkpoint_sink = record_trace.then_some(&mut scan_checkpoints);
    let compressed = build_compressed_graph(model, state, source, sinks, &mut checkpoint_sink)?;
    let mut distances = vec![None; model.node_count()];
    let mut hops = vec![usize::MAX; model.node_count()];
    let mut predecessor = vec![None::<Vec<OrlinMcfArcId>>; model.node_count()];
    let mut settled = vec![false; model.node_count()];
    let mut heap = BinaryHeap::new();
    distances[source] = Some(0_i128);
    hops[source] = 0;
    heap.push(Reverse((0_i128, 0_usize, source)));
    let mut final_checkpoint = None;
    while let Some(Reverse((distance, hop_count, root))) = heap.pop() {
        if settled[root] || distances[root] != Some(distance) || hops[root] != hop_count {
            continue;
        }
        settled[root] = true;
        for arc in &compressed.arcs {
            bump_scan(state)?;
            final_checkpoint = Some(record_scan_checkpoint(
                &mut checkpoint_sink,
                state,
                OrlinMcfStage::InspectCompressedArc,
                arc.segment.clone(),
                component_distances_by_node(state, &distances),
            ));
            if arc.from != root || arc.cost < 0 {
                continue;
            }
            let candidate_distance = distance
                .checked_add(arc.cost)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            let candidate_hops = hop_count
                .checked_add(arc.segment.len())
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            let replace = distances[arc.to].is_none_or(|old_distance| {
                let old_segment = predecessor[arc.to]
                    .as_ref()
                    .map_or(arc.segment.as_slice(), Vec::as_slice);
                (candidate_distance, candidate_hops, arc.segment.as_slice())
                    < (old_distance, hops[arc.to], old_segment)
            });
            if replace {
                distances[arc.to] = Some(candidate_distance);
                hops[arc.to] = candidate_hops;
                predecessor[arc.to] = Some(arc.segment.clone());
                heap.push(Reverse((candidate_distance, candidate_hops, arc.to)));
            }
        }
    }
    let sink = sinks
        .iter()
        .copied()
        .filter_map(|candidate| {
            distances[candidate].map(|distance| (distance, hops[candidate], candidate))
        })
        .min()
        .map(|(_, _, candidate)| candidate)
        .ok_or(OrlinMcfError::InvariantAt("compressed-sink"))?;
    let sink_distance =
        distances[sink].ok_or(OrlinMcfError::InvariantAt("compressed-sink-distance"))?;
    let path = reconstruct_compressed_path(model, state, source, sink, &predecessor)?;
    let eliminated_roots = component_roots(state)
        .filter(|&root| compressed.eliminated[root])
        .collect::<Vec<_>>();
    for root in eliminated_roots {
        let mut best = None::<(i128, OrlinMcfArcId)>;
        for id in &compressed.incoming_to_eliminated[root] {
            bump_scan(state)?;
            final_checkpoint = Some(record_scan_checkpoint(
                &mut checkpoint_sink,
                state,
                OrlinMcfStage::InspectCompressedArc,
                vec![id.clone()],
                component_distances_by_node(state, &distances),
            ));
            let (from, _) = model.arc_endpoints(id).ok_or(OrlinMcfError::Invariant)?;
            let from = state.component_of[from];
            let Some(base) = distances[from] else {
                continue;
            };
            let candidate = base
                .checked_add(reduced_cost(model, state, id)?)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            if best
                .as_ref()
                .is_none_or(|current| (&candidate, id) < (&current.0, &current.1))
            {
                best = Some((candidate, id.clone()));
            }
        }
        distances[root] = best.map(|(distance, _)| distance);
    }
    let distances_by_node = state
        .component_of
        .iter()
        .map(|&root| Some(distances[root].unwrap_or(sink_distance).min(sink_distance)))
        .collect();
    flush_final_scan_checkpoint(&mut checkpoint_sink, final_checkpoint);
    let _ = checkpoint_sink.take();
    Ok(SearchResult {
        source,
        sink,
        path,
        distances_by_node,
        eliminated_capacity_nodes: compressed.eliminated_count,
        shortcut_arcs: compressed.shortcut_count,
        scan_checkpoints,
    })
}

fn reconstruct_compressed_path(
    model: &TransformModel,
    state: &WorkingState,
    source: usize,
    sink: usize,
    predecessor: &[Option<Vec<OrlinMcfArcId>>],
) -> Result<Vec<OrlinMcfArcId>, OrlinMcfError> {
    let mut segments = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let segment = predecessor
            .get(cursor)
            .and_then(Clone::clone)
            .ok_or(OrlinMcfError::InvariantAt("compressed-predecessor"))?;
        let first = segment
            .first()
            .ok_or(OrlinMcfError::InvariantAt("compressed-empty-segment"))?;
        let (from, _) = model.arc_endpoints(first).ok_or(OrlinMcfError::Invariant)?;
        cursor = state.component_of[from];
        segments.push(segment);
        if segments.len() > model.node_count() {
            return Err(OrlinMcfError::InvariantAt("compressed-predecessor-cycle"));
        }
    }
    segments.reverse();
    Ok(segments.into_iter().flatten().collect())
}

fn apply_search(
    model: &TransformModel,
    state: &mut WorkingState,
    search: &SearchResult,
) -> Result<(), OrlinMcfError> {
    if search.distances_by_node.len() != model.node_count() {
        return Err(OrlinMcfError::InvariantAt("search-distance-shape"));
    }
    for (potential, distance) in state.potentials.iter_mut().zip(&search.distances_by_node) {
        if let Some(distance) = distance {
            *potential = potential
                .checked_add(*distance)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
        }
    }
    for id in &search.path {
        let variable = model
            .variable_index(&id.edge_id)
            .ok_or(OrlinMcfError::Invariant)?;
        let flow = &mut state.flows[variable][TransformModel::branch_index(id.branch)];
        let delta =
            i128::try_from(state.delta_numerator).map_err(|_| OrlinMcfError::ArithmeticOverflow)?;
        match id.direction {
            ResidualDirection::Forward => {
                *flow = flow
                    .checked_add(delta)
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
            }
            ResidualDirection::Reverse => {
                *flow = flow
                    .checked_sub(delta)
                    .ok_or(OrlinMcfError::ArithmeticOverflow)?;
                if *flow < 0 {
                    return Err(OrlinMcfError::InvariantAt("reverse-augmentation"));
                }
            }
        }
        if reduced_cost(model, state, id)? != 0 {
            return Err(OrlinMcfError::InvariantAt("augmented-arc-tightness"));
        }
    }
    state.metrics.augmentations = state
        .metrics
        .augmentations
        .checked_add(1)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    state.metrics.augmented_arcs = state
        .metrics
        .augmented_arcs
        .checked_add(
            u64::try_from(search.path.len()).map_err(|_| OrlinMcfError::ArithmeticOverflow)?,
        )
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    state.metrics.potential_updates = state
        .metrics
        .potential_updates
        .checked_add(1)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    validate_dual(model, state)
}

fn halve_scale(state: &mut WorkingState) -> Result<(), OrlinMcfError> {
    state.denominator = state
        .denominator
        .checked_mul(2)
        .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    for branches in &mut state.flows {
        for flow in branches {
            *flow = flow
                .checked_mul(2)
                .ok_or(OrlinMcfError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn recover_original_primal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    model: &TransformModel,
    state: &WorkingState,
    feasibility: &mut FeasibilityExecution,
) -> Result<Vec<u64>, OrlinMcfError> {
    validate_dual(model, state)?;
    let node_ids = (0..model.node_count())
        .map(|node| NodeId::parse(&format!("orlin-node-{node:03}")))
        .collect::<Result<Vec<_>, _>>()?;
    let nodes = node_ids
        .iter()
        .cloned()
        .map(|id| FlowNode::new(id, 0))
        .collect::<Vec<_>>();
    let mut unresolved = Vec::with_capacity(model.variable_edges.len().saturating_mul(2));
    let mut ids = vec![[None::<EdgeId>, None::<EdgeId>]; model.variable_edges.len()];
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            let branch_name = match branch {
                OrlinMcfBranch::Flow => "flow",
                OrlinMcfBranch::Slack => "slack",
            };
            let id = EdgeId::parse(&format!("orlin-arc-{variable:03}-{branch_name}"))?;
            ids[variable][TransformModel::branch_index(branch)] = Some(id.clone());
            let transformed_id = OrlinMcfArcId {
                edge_id: edge.edge_id.clone(),
                branch,
                direction: ResidualDirection::Forward,
            };
            let (from, to) = model
                .endpoints(variable, branch)
                .ok_or(OrlinMcfError::Invariant)?;
            unresolved.push(UnresolvedFlowEdge {
                id,
                from: node_ids[from].clone(),
                to: node_ids[to].clone(),
                lower: 0,
                capacity: if reduced_cost(model, state, &transformed_id)? == 0 {
                    edge.width
                } else {
                    0
                },
                cost: 0,
            });
        }
    }
    let zero_graph = FlowNetwork::new(nodes, unresolved)?;
    let transformed = feasibility
        .find_feasible_flow(
            &zero_graph,
            &model.required,
            FeasibilityUse::BeforeEvent {
                anchor: CapturedFeasibilityAnchor {
                    catalog_id: "orlin-mcf.recover-primal",
                    occurrence: 1,
                },
            },
        )?
        .flows;
    let mut flows = model.lower_flows.clone();
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        let flow_id = ids[variable][0].as_ref().ok_or(OrlinMcfError::Invariant)?;
        let slack_id = ids[variable][1].as_ref().ok_or(OrlinMcfError::Invariant)?;
        let flow_index = zero_graph
            .edge_index(flow_id)
            .ok_or(OrlinMcfError::Invariant)?
            .as_usize();
        let slack_index = zero_graph
            .edge_index(slack_id)
            .ok_or(OrlinMcfError::Invariant)?
            .as_usize();
        let variable_flow = transformed[flow_index];
        if variable_flow
            .checked_add(transformed[slack_index])
            .filter(|&total| total == edge.width)
            .is_none()
        {
            return Err(OrlinMcfError::Invariant);
        }
        flows[edge.original_index] = flows[edge.original_index]
            .checked_add(variable_flow)
            .ok_or(OrlinMcfError::ArithmeticOverflow)?;
    }
    check_min_cost_flow(graph, required_divergence, &flows)?;
    Ok(flows)
}

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot construction names every source-visible boundary field"
)]
fn snapshot(
    model: &TransformModel,
    state: &WorkingState,
    boundary_stage: OrlinMcfStage,
    source_component: Option<usize>,
    sink_component: Option<usize>,
    path: Vec<OrlinMcfArcId>,
    distances: &[Option<i128>],
    eliminated_capacity_nodes: u64,
    shortcut_arcs: u64,
    certified_flows: Option<Vec<u64>>,
) -> Result<OrlinMcfSnapshot, OrlinMcfError> {
    if distances.len() != model.node_count() {
        return Err(OrlinMcfError::Invariant);
    }
    let excess = component_excesses(model, state)?;
    let nodes = (0..model.node_count())
        .map(|node| {
            let component = state.component_of[node];
            Ok(OrlinMcfNodeState {
                kind: model.node_kind(node).ok_or(OrlinMcfError::Invariant)?,
                component,
                component_excess_numerator: excess[component],
                potential: state.potentials[node],
                distance: distances[node],
            })
        })
        .collect::<Result<Vec<_>, OrlinMcfError>>()?;
    let mut arcs = Vec::with_capacity(model.variable_edges.len().saturating_mul(2));
    for (variable, edge) in model.variable_edges.iter().enumerate() {
        for branch in [OrlinMcfBranch::Flow, OrlinMcfBranch::Slack] {
            let id = OrlinMcfArcId {
                edge_id: edge.edge_id.clone(),
                branch,
                direction: ResidualDirection::Forward,
            };
            arcs.push(OrlinMcfArcState {
                edge_id: edge.edge_id.clone(),
                branch,
                flow_numerator: state.flows[variable][TransformModel::branch_index(branch)],
                reduced_cost: reduced_cost(model, state, &id)?,
            });
        }
    }
    Ok(OrlinMcfSnapshot {
        stage: boundary_stage,
        denominator: state.denominator,
        delta_numerator: state.delta_numerator,
        nodes,
        arcs,
        source_component,
        sink_component,
        path,
        inspected_segment: Vec::new(),
        eliminated_capacity_nodes,
        shortcut_arcs,
        certified_flows,
        metrics: state.metrics,
    })
}

fn publish_scan_checkpoints(
    model: &TransformModel,
    state: &WorkingState,
    current: &mut OrlinMcfSnapshot,
    events: &mut Vec<OrlinMcfTraceEvent>,
    record_trace: bool,
    checkpoints: Vec<OrlinMcfScanCheckpoint>,
) -> Result<(), OrlinMcfError> {
    for checkpoint in checkpoints {
        let mut checkpoint_state = state.clone();
        checkpoint_state.metrics = checkpoint.metrics;
        let mut next = snapshot(
            model,
            &checkpoint_state,
            checkpoint.stage,
            None,
            None,
            Vec::new(),
            &checkpoint.distances_by_node,
            0,
            0,
            None,
        )?;
        next.inspected_segment = checkpoint.inspected_segment;
        let catalog_id = match checkpoint.stage {
            OrlinMcfStage::InspectContractibleArc => "orlin-mcf.inspect-contractible-arc",
            OrlinMcfStage::InspectReachabilityArc => "orlin-mcf.inspect-reachability-arc",
            OrlinMcfStage::InspectCompressedResidualArc => {
                "orlin-mcf.inspect-compressed-residual-arc"
            }
            OrlinMcfStage::InspectCompressedArc => "orlin-mcf.inspect-compressed-arc",
            _ => return Err(OrlinMcfError::InvariantAt("scan-checkpoint-stage")),
        };
        publish(current, events, record_trace, catalog_id, next, None, None);
    }
    Ok(())
}

fn publish(
    current: &mut OrlinMcfSnapshot,
    events: &mut Vec<OrlinMcfTraceEvent>,
    record_trace: bool,
    catalog_id: &'static str,
    next: OrlinMcfSnapshot,
    contraction_arc: Option<OrlinMcfArcId>,
    augmentation_numerator: Option<u128>,
) {
    if record_trace {
        events.push(OrlinMcfTraceEvent {
            catalog_id,
            before: current.clone(),
            after: next.clone(),
            contraction_arc,
            augmentation_numerator,
        });
    }
    *current = next;
}

fn state_from_snapshot(
    model: &TransformModel,
    public: &OrlinMcfSnapshot,
) -> Result<WorkingState, OrlinMcfError> {
    if public.nodes.len() != model.node_count()
        || public.arcs.len() != model.variable_edges.len().saturating_mul(2)
        || public.denominator == 0
    {
        return Err(OrlinMcfError::TraceVerification);
    }
    let mut flows = vec![[0_i128; 2]; model.variable_edges.len()];
    for (index, arc) in public.arcs.iter().enumerate() {
        let variable = index / 2;
        let expected_branch = if index % 2 == 0 {
            OrlinMcfBranch::Flow
        } else {
            OrlinMcfBranch::Slack
        };
        if arc.edge_id != model.variable_edges[variable].edge_id || arc.branch != expected_branch {
            return Err(OrlinMcfError::TraceVerification);
        }
        flows[variable][TransformModel::branch_index(arc.branch)] = arc.flow_numerator;
    }
    Ok(WorkingState {
        denominator: public.denominator,
        delta_numerator: public.delta_numerator,
        flows,
        potentials: public.nodes.iter().map(|node| node.potential).collect(),
        component_of: public.nodes.iter().map(|node| node.component).collect(),
        metrics: public.metrics,
    })
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    model: &TransformModel,
    public: &OrlinMcfSnapshot,
) -> Result<(), OrlinMcfError> {
    let state = state_from_snapshot(model, public)?;
    for node in 0..model.node_count() {
        if public.nodes[node].kind
            != model
                .node_kind(node)
                .ok_or(OrlinMcfError::TraceVerification)?
            || state.component_of[node] >= model.node_count()
        {
            return Err(OrlinMcfError::TraceVerification);
        }
        let root = state.component_of[node];
        if state.component_of[root] != root || root > node {
            return Err(OrlinMcfError::TraceVerification);
        }
    }
    let excess = component_excesses(model, &state).map_err(|_| OrlinMcfError::TraceVerification)?;
    for (node, public_node) in public.nodes.iter().enumerate() {
        if public_node.component_excess_numerator != excess[state.component_of[node]] {
            return Err(OrlinMcfError::TraceVerification);
        }
    }
    let dual_required = !matches!(
        public.stage,
        OrlinMcfStage::Ready | OrlinMcfStage::TransformCapacities
    );
    if dual_required && validate_dual(model, &state).is_err() {
        return Err(OrlinMcfError::TraceVerification);
    }
    for (index, arc) in public.arcs.iter().enumerate() {
        let id = OrlinMcfArcId {
            edge_id: arc.edge_id.clone(),
            branch: arc.branch,
            direction: ResidualDirection::Forward,
        };
        if arc.reduced_cost
            != reduced_cost(model, &state, &id).map_err(|_| OrlinMcfError::TraceVerification)?
            || arc.flow_numerator != state.flows[index / 2][index % 2]
        {
            return Err(OrlinMcfError::TraceVerification);
        }
    }
    let inspection_stage = matches!(
        public.stage,
        OrlinMcfStage::InspectContractibleArc
            | OrlinMcfStage::InspectReachabilityArc
            | OrlinMcfStage::InspectCompressedResidualArc
            | OrlinMcfStage::InspectCompressedArc
    );
    if inspection_stage == public.inspected_segment.is_empty()
        || (inspection_stage && !public.path.is_empty())
    {
        return Err(OrlinMcfError::TraceVerification);
    }
    for id in &public.inspected_segment {
        if model.arc_endpoints(id).is_none() || !residual_available(model, &state, id) {
            return Err(OrlinMcfError::TraceVerification);
        }
    }
    if public.path.is_empty() {
        if public.source_component.is_some() || public.sink_component.is_some() {
            return Err(OrlinMcfError::TraceVerification);
        }
    } else {
        let mut cursor = public
            .source_component
            .ok_or(OrlinMcfError::TraceVerification)?;
        for id in &public.path {
            if public.stage == OrlinMcfStage::SelectCompressedPath
                && !residual_available(model, &state, id)
            {
                return Err(OrlinMcfError::TraceVerification);
            }
            let (from, to) = model
                .arc_endpoints(id)
                .ok_or(OrlinMcfError::TraceVerification)?;
            if state.component_of[from] != cursor {
                return Err(OrlinMcfError::TraceVerification);
            }
            cursor = state.component_of[to];
            if public.stage == OrlinMcfStage::Augment
                && reduced_cost(model, &state, id).map_err(|_| OrlinMcfError::TraceVerification)?
                    != 0
            {
                return Err(OrlinMcfError::TraceVerification);
            }
        }
        if Some(cursor) != public.sink_component {
            return Err(OrlinMcfError::TraceVerification);
        }
    }
    if let Some(flows) = &public.certified_flows {
        check_min_cost_flow(graph, required_divergence, flows)
            .map_err(|_| OrlinMcfError::TraceVerification)?;
    } else if matches!(
        public.stage,
        OrlinMcfStage::RecoverPrimal | OrlinMcfStage::Optimal
    ) {
        return Err(OrlinMcfError::TraceVerification);
    }
    Ok(())
}

/// Independently validates graph-bound snapshot arithmetic, dual feasibility,
/// path continuity, recovered optimum, event continuity, and the deterministic
/// source transition sequence.
///
/// # Errors
///
/// Returns [`OrlinMcfError::TraceVerification`] for a forged trace.
pub fn check_orlin_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &OrlinMcfTraceResult,
) -> Result<(), OrlinMcfError> {
    let model = transform(graph, required_divergence)?;
    validate_public_snapshot(graph, required_divergence, &model, &trace.base_snapshot)?;
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor {
            return Err(OrlinMcfError::TraceVerification);
        }
        validate_public_snapshot(graph, required_divergence, &model, &event.after)?;
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.flows
            != trace
                .final_snapshot
                .certified_flows
                .clone()
                .unwrap_or_default()
    {
        return Err(OrlinMcfError::TraceVerification);
    }
    check_min_cost_flow(graph, required_divergence, &trace.result.flows)
        .map_err(|_| OrlinMcfError::TraceVerification)?;
    let expected = solve_internal(graph, required_divergence, true)?;
    if trace.base_snapshot != expected.base_snapshot
        || trace.events != expected.events
        || trace.final_snapshot != expected.final_snapshot
        || trace.result != expected.result
    {
        return Err(OrlinMcfError::TraceVerification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, NodeId};

    fn graph(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
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
                        from: NodeId::parse(from).expect("from"),
                        to: NodeId::parse(to).expect("to"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("graph")
    }

    fn required_divergence(graph: &FlowNetwork) -> Vec<i128> {
        graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect()
    }

    #[test]
    fn section_five_transform_routes_flow_and_slack_to_capacity_nodes() {
        let graph = graph(
            &[("s", 3), ("m", 0), ("t", -3)],
            &[
                ("a", "s", "m", 0, 3, 1),
                ("b", "m", "t", 0, 3, 2),
                ("expensive", "s", "t", 0, 3, 9),
            ],
        );
        let required = required_divergence(&graph);
        let trace = trace_orlin_mcf(&graph, &required).expect("solve");
        assert_eq!(trace.result.flows, vec![3, 3, 0]);
        assert_eq!(trace.result.certificate.total_cost, 9);
        assert_eq!(trace.base_snapshot.stage, OrlinMcfStage::Ready);
        assert_eq!(trace.base_snapshot.metrics.capacity_nodes, 0);
        assert_eq!(
            trace.events.first().map(|event| event.catalog_id),
            Some("orlin-mcf.transform-capacities")
        );
        assert_eq!(trace.events[0].after.metrics.capacity_nodes, 3);
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.after.stage == OrlinMcfStage::SelectCompressedPath)
        );
        let scans = trace
            .events
            .iter()
            .filter(|event| !event.after.inspected_segment.is_empty())
            .collect::<Vec<_>>();
        assert!(
            scans.len() > 8,
            "small search must expose its real scan work"
        );
        assert!(scans.iter().all(|event| {
            event.after.inspected_segment.len() <= 2
                && event.after.metrics.residual_arc_scans > event.before.metrics.residual_arc_scans
        }));
        assert!(trace.result.metrics.eliminated_capacity_nodes > 0);
    }

    #[test]
    fn every_scan_block_ends_on_an_actual_inspected_segment() {
        let node_count = 16;
        let nodes = (0..node_count)
            .map(|index| {
                let supply = if index == 0 {
                    100
                } else if index + 1 == node_count {
                    -100
                } else {
                    0
                };
                FlowNode::new(
                    NodeId::parse(&format!("n{index:02}")).expect("node id"),
                    supply,
                )
            })
            .collect();
        let mut edges = (0..node_count - 1)
            .map(|index| UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("path-{index:02}")).expect("edge id"),
                from: NodeId::parse(&format!("n{index:02}")).expect("tail"),
                to: NodeId::parse(&format!("n{:02}", index + 1)).expect("head"),
                lower: 0,
                capacity: 100,
                cost: 1,
            })
            .collect::<Vec<_>>();
        edges.push(UnresolvedFlowEdge {
            id: EdgeId::parse("expensive").expect("edge id"),
            from: NodeId::parse("n00").expect("tail"),
            to: NodeId::parse("n15").expect("head"),
            lower: 0,
            capacity: 100,
            cost: 50,
        });
        let graph = FlowNetwork::new(nodes, edges).expect("scan-heavy graph");
        let required = required_divergence(&graph);
        let trace = trace_orlin_mcf(&graph, &required).expect("scan-heavy trace");
        let mut saw_bounded_block = false;
        for event in &trace.events {
            let before = event.before.metrics.residual_arc_scans;
            let after = event.after.metrics.residual_arc_scans;
            if after == before {
                continue;
            }
            assert!(matches!(
                event.after.stage,
                OrlinMcfStage::InspectContractibleArc
                    | OrlinMcfStage::InspectReachabilityArc
                    | OrlinMcfStage::InspectCompressedResidualArc
                    | OrlinMcfStage::InspectCompressedArc
            ));
            assert!(!event.after.inspected_segment.is_empty());
            assert!(after - before <= ORLIN_MCF_TRACE_SCAN_BLOCK);
            saw_bounded_block |= after - before > 1;
        }
        assert!(trace.final_snapshot.metrics.residual_arc_scans > ORLIN_MCF_TRACE_SCAN_PREFIX);
        assert!(saw_bounded_block);
    }

    #[test]
    fn lower_bounds_parallel_opposites_and_negative_self_loop_are_exact() {
        let graph = graph(
            &[("a", 1), ("b", -1)],
            &[
                ("ab-low", "a", "b", 1, 3, 5),
                ("ab-cheap", "a", "b", 0, 2, -1),
                ("ba", "b", "a", 0, 1, 4),
                ("loop", "a", "a", 0, 2, -3),
            ],
        );
        let required = required_divergence(&graph);
        let result = solve_orlin_mcf(&graph, &required).expect("solve");
        // FlowNetwork canonicalizes by edge id: ab-cheap, ab-low, ba, loop.
        assert_eq!(result.flows, vec![0, 1, 0, 2]);
        assert_eq!(result.certificate.total_cost, -1);
    }

    #[test]
    fn fast_trace_and_independent_certificate_agree() {
        let graph = graph(
            &[("s", 2), ("x", 0), ("t", -2)],
            &[
                ("sx", "s", "x", 0, 2, -2),
                ("xt", "x", "t", 0, 2, 3),
                ("st", "s", "t", 0, 2, 4),
            ],
        );
        let required = required_divergence(&graph);
        let fast = solve_orlin_mcf(&graph, &required).expect("fast");
        let trace = trace_orlin_mcf(&graph, &required).expect("trace");
        assert_eq!(fast, trace.result);
        check_orlin_mcf_trace(&graph, &required, &trace).expect("checker");
        let mut corrupt = trace.clone();
        corrupt.events[1].after.nodes[0].potential += 1;
        assert_eq!(
            check_orlin_mcf_trace(&graph, &required, &corrupt),
            Err(OrlinMcfError::TraceVerification)
        );
    }

    #[test]
    fn deterministic_small_feasible_networks_are_independently_certified() {
        fn next(state: &mut u64) -> u64 {
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *state
        }

        for seed in 0_u64..64 {
            let mut random = seed.wrapping_add(1);
            let node_count = 2 + usize::try_from(next(&mut random) % 5).expect("small node count");
            let edge_count =
                node_count + usize::try_from(next(&mut random) % 9).expect("small edge count");
            let mut divergence = vec![0_i128; node_count];
            let mut edges = Vec::with_capacity(edge_count);
            for edge_index in 0..edge_count {
                let from = if edge_index < node_count {
                    edge_index
                } else {
                    usize::try_from(next(&mut random)).expect("usize random") % node_count
                };
                let to = if edge_index < node_count {
                    (edge_index + 1) % node_count
                } else {
                    usize::try_from(next(&mut random)).expect("usize random") % node_count
                };
                let lower = next(&mut random) % 3;
                let width = next(&mut random) % 5;
                let capacity = lower + width;
                let witness = lower
                    + if width == 0 {
                        0
                    } else {
                        next(&mut random) % (width + 1)
                    };
                divergence[from] += i128::from(witness);
                divergence[to] -= i128::from(witness);
                let cost = i64::try_from(next(&mut random) % 13).expect("small cost") - 6;
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("edge-{edge_index:02}")).expect("edge id"),
                    from: NodeId::parse(&format!("node-{from:02}")).expect("from"),
                    to: NodeId::parse(&format!("node-{to:02}")).expect("to"),
                    lower,
                    capacity,
                    cost,
                });
            }
            let nodes = divergence
                .iter()
                .enumerate()
                .map(|(node, &supply)| {
                    FlowNode::new(
                        NodeId::parse(&format!("node-{node:02}")).expect("node id"),
                        i64::try_from(supply).expect("small supply"),
                    )
                })
                .collect();
            let graph = FlowNetwork::new(nodes, edges).expect("random feasible graph");
            let result = solve_orlin_mcf(&graph, &divergence)
                .unwrap_or_else(|error| panic!("seed {seed}: {error:?}; graph={graph:?}"));
            assert_eq!(
                check_min_cost_flow(&graph, &divergence, &result.flows)
                    .expect("independent certificate"),
                result.certificate,
                "seed {seed}",
            );
            if seed % 8 == 0 {
                let trace = trace_orlin_mcf(&graph, &divergence).expect("checked trace");
                assert_eq!(trace.result, result, "seed {seed}");
            }
        }
    }

    #[test]
    fn infeasible_and_admission_limits_fail_closed() {
        let infeasible = graph(&[("s", 2), ("t", -2)], &[("st", "s", "t", 0, 1, 0)]);
        let required = required_divergence(&infeasible);
        assert!(matches!(
            solve_orlin_mcf(&infeasible, &required),
            Err(OrlinMcfError::Feasibility(_))
        ));

        let nodes = (0..=ORLIN_MCF_MAX_NODES)
            .map(|index| (format!("n{index}"), 0_i64))
            .collect::<Vec<_>>();
        let oversized = FlowNetwork::new(
            nodes
                .iter()
                .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), *supply))
                .collect(),
            Vec::new(),
        )
        .expect("oversized graph");
        let required = required_divergence(&oversized);
        assert_eq!(
            solve_orlin_mcf(&oversized, &required),
            Err(OrlinMcfError::AdmissionLimit)
        );
    }
}
