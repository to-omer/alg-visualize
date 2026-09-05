//! Orlin's enhanced right-hand-side capacity scaling on uncapacitated
//! transshipment networks.
//!
//! This module implements the strongly-polynomial core from Orlin (1993),
//! Section 4.  Finite project capacities are accepted only as a nonbinding
//! encoding of the source algorithm's uncapacitated arcs.  The full
//! capacitated-node transformation from Section 5 belongs to the separate
//! `orlin-mcf` descriptor.

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
    EdgeId, FlowEdge, FlowModelError, FlowNetwork, FlowNode, NodeIndex, UnresolvedFlowEdge,
};
use crate::residual::{ResidualArcId, ResidualDirection};

/// Conservative node limit for the explicit contracted-network kernel.
pub const ENHANCED_CAPACITY_SCALING_MAX_NODES: usize = 64;
/// Conservative edge limit for repeated explicit quotient scans.
pub const ENHANCED_CAPACITY_SCALING_MAX_EDGES: usize = 512;
/// Maximum exact scaling phases.
pub const ENHANCED_CAPACITY_SCALING_MAX_PHASES: u64 = 20_000;
/// Maximum shortest-path augmentations.
pub const ENHANCED_CAPACITY_SCALING_MAX_AUGMENTATIONS: u64 = 100_000;
/// Maximum contractions.
pub const ENHANCED_CAPACITY_SCALING_MAX_CONTRACTIONS: u64 = 63;
/// Maximum original residual arcs inspected by quotient Dijkstra and audits.
pub const ENHANCED_CAPACITY_SCALING_MAX_SCANS: u128 = 20_000_000;
/// Exact residual-scan prefix retained by the interactive trace before
/// switching to bounded scan blocks.
pub const ENHANCED_CAPACITY_SCALING_TRACE_SCAN_PREFIX: u128 = 512;
/// Maximum number of later residual inspections represented by one Detail
/// checkpoint.
pub const ENHANCED_CAPACITY_SCALING_TRACE_SCAN_BLOCK: u128 = 256;

/// Source-defined event boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnhancedCapacityScalingStage {
    /// Lower-bound-shifted zero pseudoflow before cost reweighting.
    Ready,
    /// A globally dual-feasible initial potential was installed.
    Initialize,
    /// A no-flow phase jump set delta to the maximum current imbalance.
    CompleteRegeneration,
    /// A new delta phase began.
    BeginPhase,
    /// One strongly feasible arc merged two quotient nodes.
    Contract,
    /// One original residual direction inspected by a quotient scan.
    InspectResidualArc,
    /// A source-defined active source/sink pair and shortest path were chosen.
    SelectPath,
    /// Potentials and the exact-delta pseudoflow were updated atomically.
    Augment,
    /// The current active source or deficit set became empty.
    CompletePhase,
    /// Delta was halved using an exact common dyadic denominator.
    HalveScale,
    /// Contracted dual prices were expanded and a zero-reduced-cost flow found.
    RecoverPrimal,
    /// The original bounded model passed an independent min-cost certificate.
    Optimal,
}

/// One quotient node, identified by its smallest canonical original member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCapacityScalingComponent {
    /// Stable component identity.
    pub id: NodeIndex,
    /// Canonically sorted original members.
    pub members: Vec<NodeIndex>,
    /// Exact imbalance numerator under the snapshot denominator.
    pub excess_numerator: i128,
}

/// Deterministic source-algorithm counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EnhancedCapacityScalingMetrics {
    /// Delta phases entered.
    pub scaling_phases: u64,
    /// Phase jumps larger than the ordinary halving schedule.
    pub complete_regenerations: u64,
    /// Strongly feasible arc contractions.
    pub contractions: u64,
    /// Quotient shortest-path solves.
    pub shortest_path_runs: u64,
    /// Exact-delta path augmentations.
    pub augmentations: u64,
    /// Original residual arcs in selected quotient paths.
    pub augmented_arcs: u64,
    /// Potential updates after shortest paths.
    pub potential_updates: u64,
    /// Original residual arcs inspected.
    pub residual_arc_scans: u128,
    /// Final zero-reduced-cost feasibility recoveries.
    pub primal_recoveries: u64,
}

/// Complete state at one reversible source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCapacityScalingSnapshot {
    /// Semantic boundary.
    pub stage: EnhancedCapacityScalingStage,
    /// Exact common denominator for all virtual pseudoflows and imbalances.
    pub denominator: u128,
    /// Exact delta numerator under `denominator`.
    pub delta_numerator: u128,
    /// Lower-bound-shifted virtual arc-flow numerators in canonical edge order.
    pub virtual_flow_numerators: Vec<i128>,
    /// Original-node dual potentials in canonical order.
    pub potentials: Vec<i128>,
    /// Current quotient partition and aggregate excess.
    pub components: Vec<EnhancedCapacityScalingComponent>,
    /// Selected active source component.
    pub source_component: Option<NodeIndex>,
    /// Selected active deficit component.
    pub sink_component: Option<NodeIndex>,
    /// Selected quotient residual path.
    pub path: Vec<ResidualArcId>,
    /// Quotient shortest-path distance repeated for every original member.
    pub distances: Vec<Option<i128>>,
    /// Independently recovered original bounded flow at terminal boundaries.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact work counters.
    pub metrics: EnhancedCapacityScalingMetrics,
}

/// One reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCapacityScalingTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: EnhancedCapacityScalingSnapshot,
    /// State after the transition.
    pub after: EnhancedCapacityScalingSnapshot,
    /// Strongly feasible original arc for a contraction.
    pub contraction_arc: Option<EdgeId>,
    /// Exact augmentation numerator for an augment transition.
    pub augmentation_numerator: Option<u128>,
}

/// Certified exact result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCapacityScalingResult {
    /// Original bounded flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent exact primal/dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic algorithm counters.
    pub metrics: EnhancedCapacityScalingMetrics,
    /// Terminal source state for the fast profile.
    pub final_snapshot: EnhancedCapacityScalingSnapshot,
}

/// Certified result with all quotient and scaling boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCapacityScalingTraceResult {
    /// Same result returned by the fast profile.
    pub result: EnhancedCapacityScalingResult,
    /// Ready boundary.
    pub base_snapshot: EnhancedCapacityScalingSnapshot,
    /// Reversible transitions.
    pub events: Vec<EnhancedCapacityScalingTraceEvent>,
    /// Certified terminal boundary.
    pub final_snapshot: EnhancedCapacityScalingSnapshot,
}

/// Domain, arithmetic, work, construction, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum EnhancedCapacityScalingError {
    /// Input exceeds the explicit quotient-network admission band.
    #[error("graph exceeds enhanced-capacity-scaling admission limits")]
    AdmissionLimit,
    /// A deterministic work ceiling was reached.
    #[error("enhanced-capacity-scaling work limit reached")]
    WorkLimit,
    /// The source algorithm requires a strongly connected uncapacitated graph.
    #[error("enhanced capacity scaling requires a strongly connected positive-width graph")]
    StrongConnectivity,
    /// Finite project capacities do not encode nonbinding uncapacitated arcs.
    #[error("enhanced capacity scaling requires nonbinding transshipment capacities")]
    CapacityBound,
    /// A negative forward cycle makes the uncapacitated source problem unbounded.
    #[error("enhanced-capacity-scaling transshipment is unbounded")]
    Unbounded,
    /// Requested balances are infeasible in the original bounded model.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A temporary recovery network could not be constructed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact integer or dyadic arithmetic overflowed.
    #[error("enhanced-capacity-scaling arithmetic overflow")]
    ArithmeticOverflow,
    /// A quotient, path, dual, or recovery invariant failed.
    #[error("enhanced-capacity-scaling invariant failed")]
    Invariant,
    /// A public trace did not replay under the source transition grammar.
    #[error("enhanced-capacity-scaling trace verification failed")]
    TraceVerification,
}

/// Solves the uncapacitated transshipment specialization of Orlin's enhanced
/// RHS-scaling algorithm.
///
/// # Errors
///
/// Returns a domain, feasibility, work-limit, arithmetic, invariant, recovery,
/// or certificate error.
pub fn solve_enhanced_capacity_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<EnhancedCapacityScalingResult, EnhancedCapacityScalingError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting both its feasibility precheck and transformed
/// primal-recovery construction to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_enhanced_capacity_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<EnhancedCapacityScalingResult, EnhancedCapacityScalingError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Solves while retaining every regeneration, phase, contraction, shortest
/// path, exact-delta augmentation, and primal recovery boundary.
///
/// # Errors
///
/// Returns the same errors as [`solve_enhanced_capacity_scaling`] plus trace
/// verification failures.
pub fn trace_enhanced_capacity_scaling(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<EnhancedCapacityScalingTraceResult, EnhancedCapacityScalingError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let trace = EnhancedCapacityScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_enhanced_capacity_scaling_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Traces enhanced capacity scaling while explicitly publishing both its
/// feasibility precheck and transformed primal-recovery subroutine.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_enhanced_capacity_scaling_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<EnhancedCapacityScalingTraceResult, EnhancedCapacityScalingError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = EnhancedCapacityScalingTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_enhanced_capacity_scaling_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

struct InternalRun {
    result: EnhancedCapacityScalingResult,
    base_snapshot: EnhancedCapacityScalingSnapshot,
    events: Vec<EnhancedCapacityScalingTraceEvent>,
    final_snapshot: EnhancedCapacityScalingSnapshot,
}

struct WorkingState<'graph> {
    graph: &'graph FlowNetwork,
    required_variable: Vec<i128>,
    denominator: u128,
    delta_numerator: u128,
    virtual_flows: Vec<i128>,
    potentials: Vec<i128>,
    component_of: Vec<usize>,
    metrics: EnhancedCapacityScalingMetrics,
}

#[derive(Clone)]
struct SearchResult {
    source: usize,
    sink: usize,
    path: Vec<ResidualArcId>,
    distances_by_node: Vec<Option<i128>>,
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, EnhancedCapacityScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

#[expect(
    clippy::too_many_lines,
    reason = "the source paper's phase state machine is kept in one auditable sequence"
)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, EnhancedCapacityScalingError> {
    let (required_variable, initial_potentials) =
        validate_and_initialize_with_feasibility(graph, required_divergence, feasibility)?;
    let delta_numerator = maximum_magnitude(&required_variable);
    let mut work = WorkingState {
        graph,
        required_variable,
        denominator: 1,
        delta_numerator,
        virtual_flows: vec![0; graph.edges().len()],
        potentials: vec![0; graph.nodes().len()],
        component_of: (0..graph.nodes().len()).collect(),
        metrics: EnhancedCapacityScalingMetrics::default(),
    };
    let base_snapshot = snapshot(
        &work,
        EnhancedCapacityScalingStage::Ready,
        None,
        None,
        Vec::new(),
        vec![None; graph.nodes().len()],
        None,
    )?;
    let mut current = base_snapshot.clone();
    let mut events = Vec::new();
    work.potentials = initial_potentials;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "enhanced-capacity-scaling.initialize",
        snapshot(
            &work,
            EnhancedCapacityScalingStage::Initialize,
            None,
            None,
            Vec::new(),
            vec![None; graph.nodes().len()],
            None,
        )?,
        None,
        None,
    );

    while has_imbalanced_component(&work)? {
        if work.metrics.scaling_phases >= ENHANCED_CAPACITY_SCALING_MAX_PHASES {
            return Err(EnhancedCapacityScalingError::WorkLimit);
        }
        if should_complete_regenerate(&work)? {
            work.delta_numerator = maximum_component_excess(&work)?;
            work.metrics.complete_regenerations = work
                .metrics
                .complete_regenerations
                .checked_add(1)
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "enhanced-capacity-scaling.complete-regeneration",
                snapshot(
                    &work,
                    EnhancedCapacityScalingStage::CompleteRegeneration,
                    None,
                    None,
                    Vec::new(),
                    vec![None; graph.nodes().len()],
                    None,
                )?,
                None,
                None,
            );
        }
        if work.delta_numerator == 0 {
            return Err(EnhancedCapacityScalingError::Invariant);
        }
        work.metrics.scaling_phases = work
            .metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
        publish(
            &mut current,
            &mut events,
            record_trace,
            "enhanced-capacity-scaling.begin-phase",
            snapshot(
                &work,
                EnhancedCapacityScalingStage::BeginPhase,
                None,
                None,
                Vec::new(),
                vec![None; graph.nodes().len()],
                None,
            )?,
            None,
            None,
        );

        loop {
            loop {
                let mut scan_checkpoints = Vec::new();
                let edge_index =
                    next_contractible_arc(&mut work, record_trace, &mut scan_checkpoints)?;
                publish_scan_checkpoints(&mut current, &mut events, record_trace, scan_checkpoints);
                let Some(edge_index) = edge_index else {
                    break;
                };
                let edge = graph
                    .edges()
                    .get(edge_index)
                    .ok_or(EnhancedCapacityScalingError::Invariant)?;
                let contraction_arc = edge.id().clone();
                merge_components(&mut work, edge.from().as_usize(), edge.to().as_usize())?;
                work.metrics.contractions = work
                    .metrics
                    .contractions
                    .checked_add(1)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
                if work.metrics.contractions > ENHANCED_CAPACITY_SCALING_MAX_CONTRACTIONS {
                    return Err(EnhancedCapacityScalingError::WorkLimit);
                }
                publish(
                    &mut current,
                    &mut events,
                    record_trace,
                    "enhanced-capacity-scaling.contract",
                    snapshot(
                        &work,
                        EnhancedCapacityScalingStage::Contract,
                        None,
                        None,
                        Vec::new(),
                        vec![None; graph.nodes().len()],
                        None,
                    )?,
                    Some(contraction_arc),
                    None,
                );
            }

            let Some((source, sink)) = select_active_pair(&work)? else {
                break;
            };
            if work.metrics.augmentations >= ENHANCED_CAPACITY_SCALING_MAX_AUGMENTATIONS {
                return Err(EnhancedCapacityScalingError::WorkLimit);
            }
            let mut scan_checkpoints = Vec::new();
            let search = shortest_quotient_path(
                &mut work,
                source,
                sink,
                record_trace,
                &mut scan_checkpoints,
            )?;
            publish_scan_checkpoints(&mut current, &mut events, record_trace, scan_checkpoints);
            publish(
                &mut current,
                &mut events,
                record_trace,
                "enhanced-capacity-scaling.select-path",
                snapshot(
                    &work,
                    EnhancedCapacityScalingStage::SelectPath,
                    NodeIndex::try_from_usize(search.source),
                    NodeIndex::try_from_usize(search.sink),
                    search.path.clone(),
                    search.distances_by_node.clone(),
                    None,
                )?,
                None,
                None,
            );
            apply_search(&mut work, &search)?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "enhanced-capacity-scaling.augment",
                snapshot(
                    &work,
                    EnhancedCapacityScalingStage::Augment,
                    NodeIndex::try_from_usize(search.source),
                    NodeIndex::try_from_usize(search.sink),
                    search.path,
                    search.distances_by_node,
                    None,
                )?,
                None,
                Some(work.delta_numerator),
            );
        }

        publish(
            &mut current,
            &mut events,
            record_trace,
            "enhanced-capacity-scaling.complete-phase",
            snapshot(
                &work,
                EnhancedCapacityScalingStage::CompletePhase,
                None,
                None,
                Vec::new(),
                vec![None; graph.nodes().len()],
                None,
            )?,
            None,
            None,
        );
        halve_scale(&mut work)?;
        publish(
            &mut current,
            &mut events,
            record_trace,
            "enhanced-capacity-scaling.halve-scale",
            snapshot(
                &work,
                EnhancedCapacityScalingStage::HalveScale,
                None,
                None,
                Vec::new(),
                vec![None; graph.nodes().len()],
                None,
            )?,
            None,
            None,
        );
    }

    let flows = recover_primal(graph, required_divergence, &work.potentials, feasibility)?;
    work.metrics.primal_recoveries = 1;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "enhanced-capacity-scaling.recover-primal",
        snapshot(
            &work,
            EnhancedCapacityScalingStage::RecoverPrimal,
            None,
            None,
            Vec::new(),
            vec![None; graph.nodes().len()],
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
        "enhanced-capacity-scaling.optimal",
        snapshot(
            &work,
            EnhancedCapacityScalingStage::Optimal,
            None,
            None,
            Vec::new(),
            vec![None; graph.nodes().len()],
            Some(flows.clone()),
        )?,
        None,
        None,
    );
    let final_snapshot = current;
    let result = EnhancedCapacityScalingResult {
        flows,
        certificate,
        metrics: work.metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn validate_and_initialize(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<(Vec<i128>, Vec<i128>), EnhancedCapacityScalingError> {
    let mut feasibility = FeasibilityExecution::untracked();
    validate_and_initialize_with_feasibility(graph, required_divergence, &mut feasibility)
}

fn validate_and_initialize_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<(Vec<i128>, Vec<i128>), EnhancedCapacityScalingError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > ENHANCED_CAPACITY_SCALING_MAX_NODES
        || graph.edges().len() > ENHANCED_CAPACITY_SCALING_MAX_EDGES
    {
        return Err(EnhancedCapacityScalingError::AdmissionLimit);
    }
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    if lower_divergence.len() != required_divergence.len() {
        return Err(EnhancedCapacityScalingError::Invariant);
    }
    let required_variable = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, actual)| {
            required
                .checked_sub(actual)
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_nonbinding_widths(graph, &required_variable)?;
    validate_strong_connectivity(graph)?;
    let potentials = initial_feasible_potentials(graph)?;
    Ok((required_variable, potentials))
}

fn validate_nonbinding_widths(
    graph: &FlowNetwork,
    required_variable: &[i128],
) -> Result<(), EnhancedCapacityScalingError> {
    let total_positive = required_variable.iter().try_fold(0_u128, |sum, &value| {
        if value <= 0 {
            return Ok(sum);
        }
        sum.checked_add(
            u128::try_from(value).map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
        )
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)
    })?;
    let required_width = u64::try_from(total_positive.max(1))
        .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?;
    if graph.edges().iter().any(|edge| {
        edge.capacity()
            .checked_sub(edge.lower())
            .is_none_or(|width| width < required_width)
    }) {
        return Err(EnhancedCapacityScalingError::CapacityBound);
    }
    Ok(())
}

fn validate_strong_connectivity(graph: &FlowNetwork) -> Result<(), EnhancedCapacityScalingError> {
    let node_count = graph.nodes().len();
    let reaches_all = |reverse: bool| {
        let mut seen = vec![false; node_count];
        let mut queue = VecDeque::from([0_usize]);
        seen[0] = true;
        while let Some(node) = queue.pop_front() {
            for edge in graph.edges() {
                let (from, to) = if reverse {
                    (edge.to().as_usize(), edge.from().as_usize())
                } else {
                    (edge.from().as_usize(), edge.to().as_usize())
                };
                if from == node && !seen[to] {
                    seen[to] = true;
                    queue.push_back(to);
                }
            }
        }
        seen.into_iter().all(|value| value)
    };
    if !reaches_all(false) || !reaches_all(true) {
        return Err(EnhancedCapacityScalingError::StrongConnectivity);
    }
    Ok(())
}

fn initial_feasible_potentials(
    graph: &FlowNetwork,
) -> Result<Vec<i128>, EnhancedCapacityScalingError> {
    let node_count = graph.nodes().len();
    let mut distance = vec![0_i128; node_count];
    for pass in 0..node_count {
        let mut changed = false;
        for edge in graph.edges() {
            let candidate = distance[edge.from().as_usize()]
                .checked_add(i128::from(edge.cost()))
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
            if candidate < distance[edge.to().as_usize()] {
                distance[edge.to().as_usize()] = candidate;
                changed = true;
                if pass + 1 == node_count {
                    return Err(EnhancedCapacityScalingError::Unbounded);
                }
            }
        }
        if !changed {
            break;
        }
    }
    validate_dual(graph, &distance, &[0; 0])?;
    Ok(distance)
}

fn maximum_magnitude(values: &[i128]) -> u128 {
    values
        .iter()
        .fold(0_u128, |maximum, value| maximum.max(value.unsigned_abs()))
}

fn component_excesses(work: &WorkingState<'_>) -> Result<Vec<i128>, EnhancedCapacityScalingError> {
    let mut excess = work
        .required_variable
        .iter()
        .map(|value| {
            value
                .checked_mul(
                    i128::try_from(work.denominator)
                        .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
                )
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (edge, &flow) in work.graph.edges().iter().zip(&work.virtual_flows) {
        excess[edge.from().as_usize()] = excess[edge.from().as_usize()]
            .checked_sub(flow)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
        excess[edge.to().as_usize()] = excess[edge.to().as_usize()]
            .checked_add(flow)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    }
    let mut aggregate = vec![0_i128; excess.len()];
    for (node, value) in excess.into_iter().enumerate() {
        let root = work.component_of[node];
        aggregate[root] = aggregate[root]
            .checked_add(value)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    }
    Ok(aggregate)
}

fn has_imbalanced_component(work: &WorkingState<'_>) -> Result<bool, EnhancedCapacityScalingError> {
    let excess = component_excesses(work)?;
    Ok(component_roots(work).any(|root| excess[root] != 0))
}

fn maximum_component_excess(work: &WorkingState<'_>) -> Result<u128, EnhancedCapacityScalingError> {
    let excess = component_excesses(work)?;
    component_roots(work).try_fold(0_u128, |maximum, root| {
        Ok(maximum.max(excess[root].unsigned_abs()))
    })
}

fn should_complete_regenerate(
    work: &WorkingState<'_>,
) -> Result<bool, EnhancedCapacityScalingError> {
    if work
        .graph
        .edges()
        .iter()
        .zip(&work.virtual_flows)
        .any(|(edge, &flow)| {
            flow != 0
                && work.component_of[edge.from().as_usize()]
                    != work.component_of[edge.to().as_usize()]
        })
    {
        return Ok(false);
    }
    let maximum = maximum_component_excess(work)?;
    Ok(maximum > 0 && maximum < work.delta_numerator)
}

fn component_roots<'a>(work: &'a WorkingState<'_>) -> impl Iterator<Item = usize> + 'a {
    work.component_of
        .iter()
        .enumerate()
        .filter_map(|(node, &root)| (node == root).then_some(root))
}

fn next_contractible_arc(
    work: &mut WorkingState<'_>,
    record_trace: bool,
    scan_checkpoints: &mut Vec<EnhancedCapacityScalingSnapshot>,
) -> Result<Option<usize>, EnhancedCapacityScalingError> {
    let threshold = work
        .delta_numerator
        .checked_mul(
            u128::try_from(work.graph.nodes().len())
                .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
        )
        .and_then(|value| value.checked_mul(3))
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    let mut last_inspected_arc = None;
    for index in 0..work.graph.edges().len() {
        work.metrics.residual_arc_scans = work
            .metrics
            .residual_arc_scans
            .checked_add(1)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
        check_scan_limit(work)?;
        let edge = &work.graph.edges()[index];
        let inspected_arc = ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward);
        last_inspected_arc = Some(inspected_arc.clone());
        record_scan_checkpoint(work, inspected_arc, record_trace, scan_checkpoints)?;
        if work.component_of[edge.from().as_usize()] == work.component_of[edge.to().as_usize()]
            || u128::try_from(work.virtual_flows[index])
                .map_err(|_| EnhancedCapacityScalingError::Invariant)?
                < threshold
        {
            continue;
        }
        if forward_reduced_cost(work, index)? != 0 {
            return Err(EnhancedCapacityScalingError::Invariant);
        }
        flush_scan_checkpoint(work, last_inspected_arc, record_trace, scan_checkpoints)?;
        return Ok(Some(index));
    }
    flush_scan_checkpoint(work, last_inspected_arc, record_trace, scan_checkpoints)?;
    Ok(None)
}

fn merge_components(
    work: &mut WorkingState<'_>,
    left_node: usize,
    right_node: usize,
) -> Result<(), EnhancedCapacityScalingError> {
    let left = work.component_of[left_node];
    let right = work.component_of[right_node];
    if left == right {
        return Err(EnhancedCapacityScalingError::Invariant);
    }
    let keep = left.min(right);
    let remove = left.max(right);
    for root in &mut work.component_of {
        if *root == remove {
            *root = keep;
        }
    }
    Ok(())
}

fn select_active_pair(
    work: &WorkingState<'_>,
) -> Result<Option<(usize, usize)>, EnhancedCapacityScalingError> {
    let excess = component_excesses(work)?;
    let threshold = work
        .delta_numerator
        .checked_mul(3)
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    let threshold =
        i128::try_from(threshold).map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?;
    let source = component_roots(work).find(|&root| {
        excess[root]
            .checked_mul(4)
            .is_some_and(|value| value >= threshold)
    });
    let sink = component_roots(work).find(|&root| {
        excess[root]
            .checked_mul(4)
            .is_some_and(|value| value <= -threshold)
    });
    Ok(source.zip(sink))
}

fn shortest_quotient_path(
    work: &mut WorkingState<'_>,
    source: usize,
    sink: usize,
    record_trace: bool,
    scan_checkpoints: &mut Vec<EnhancedCapacityScalingSnapshot>,
) -> Result<SearchResult, EnhancedCapacityScalingError> {
    work.metrics.shortest_path_runs = work
        .metrics
        .shortest_path_runs
        .checked_add(1)
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    let node_count = work.graph.nodes().len();
    let mut distances = vec![None; node_count];
    let mut hops = vec![usize::MAX; node_count];
    let mut predecessor = vec![None::<ResidualArcId>; node_count];
    let mut settled = vec![false; node_count];
    let mut heap = BinaryHeap::new();
    let mut last_inspected_arc = None;
    distances[source] = Some(0_i128);
    hops[source] = 0;
    heap.push(Reverse((0_i128, 0_usize, source)));
    while let Some(Reverse((distance, hop_count, root))) = heap.pop() {
        if settled[root] || distances[root] != Some(distance) || hops[root] != hop_count {
            continue;
        }
        settled[root] = true;
        for index in 0..work.graph.edges().len() {
            let edge = &work.graph.edges()[index];
            for direction in [ResidualDirection::Forward, ResidualDirection::Reverse] {
                work.metrics.residual_arc_scans = work
                    .metrics
                    .residual_arc_scans
                    .checked_add(1)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
                check_scan_limit(work)?;
                let inspected_arc = ResidualArcId::new(edge.id().clone(), direction);
                last_inspected_arc = Some(inspected_arc.clone());
                record_scan_checkpoint(work, inspected_arc, record_trace, scan_checkpoints)?;
                let (from, to, available) = match direction {
                    ResidualDirection::Forward => (
                        work.component_of[edge.from().as_usize()],
                        work.component_of[edge.to().as_usize()],
                        true,
                    ),
                    ResidualDirection::Reverse => (
                        work.component_of[edge.to().as_usize()],
                        work.component_of[edge.from().as_usize()],
                        work.virtual_flows[index] > 0,
                    ),
                };
                if !available || from != root || from == to {
                    continue;
                }
                let reduced = residual_reduced_cost(work, index, direction)?;
                if reduced < 0 {
                    return Err(EnhancedCapacityScalingError::Invariant);
                }
                let candidate_distance = distance
                    .checked_add(reduced)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
                let candidate_hops = hop_count
                    .checked_add(1)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
                let id = ResidualArcId::new(edge.id().clone(), direction);
                let current = distances[to].map(|value| {
                    (
                        value,
                        hops[to],
                        predecessor[to].as_ref().map_or(&id, |existing| existing),
                    )
                });
                if current.is_none_or(|(old_distance, old_hops, old_id)| {
                    (candidate_distance, candidate_hops, &id) < (old_distance, old_hops, old_id)
                }) {
                    distances[to] = Some(candidate_distance);
                    hops[to] = candidate_hops;
                    predecessor[to] = Some(id);
                    heap.push(Reverse((candidate_distance, candidate_hops, to)));
                }
            }
        }
    }
    flush_scan_checkpoint(work, last_inspected_arc, record_trace, scan_checkpoints)?;
    if distances[sink].is_none() {
        return Err(EnhancedCapacityScalingError::StrongConnectivity);
    }
    let path = reconstruct_path(work, source, sink, &predecessor)?;
    let distances_by_node = work
        .component_of
        .iter()
        .map(|&root| distances[root])
        .collect();
    Ok(SearchResult {
        source,
        sink,
        path,
        distances_by_node,
    })
}

fn reconstruct_path(
    work: &WorkingState<'_>,
    source: usize,
    sink: usize,
    predecessor: &[Option<ResidualArcId>],
) -> Result<Vec<ResidualArcId>, EnhancedCapacityScalingError> {
    let mut path = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor
            .get(cursor)
            .and_then(Clone::clone)
            .ok_or(EnhancedCapacityScalingError::Invariant)?;
        let index = work
            .graph
            .edge_index(id.original_edge())
            .ok_or(EnhancedCapacityScalingError::Invariant)?
            .as_usize();
        let edge = &work.graph.edges()[index];
        cursor = match id.direction() {
            ResidualDirection::Forward => work.component_of[edge.from().as_usize()],
            ResidualDirection::Reverse => work.component_of[edge.to().as_usize()],
        };
        path.push(id);
        if path.len() > work.component_of.len() {
            return Err(EnhancedCapacityScalingError::Invariant);
        }
    }
    path.reverse();
    Ok(path)
}

fn apply_search(
    work: &mut WorkingState<'_>,
    search: &SearchResult,
) -> Result<(), EnhancedCapacityScalingError> {
    if search.distances_by_node.len() != work.graph.nodes().len() {
        return Err(EnhancedCapacityScalingError::Invariant);
    }
    for (potential, distance) in work.potentials.iter_mut().zip(&search.distances_by_node) {
        *potential = potential
            .checked_add(distance.ok_or(EnhancedCapacityScalingError::StrongConnectivity)?)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    }
    for id in &search.path {
        let index = work
            .graph
            .edge_index(id.original_edge())
            .ok_or(EnhancedCapacityScalingError::Invariant)?
            .as_usize();
        match id.direction() {
            ResidualDirection::Forward => {
                work.virtual_flows[index] = work.virtual_flows[index]
                    .checked_add(
                        i128::try_from(work.delta_numerator)
                            .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
                    )
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
            }
            ResidualDirection::Reverse => {
                work.virtual_flows[index] = work.virtual_flows[index]
                    .checked_sub(
                        i128::try_from(work.delta_numerator)
                            .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
                    )
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
                if work.virtual_flows[index] < 0 {
                    return Err(EnhancedCapacityScalingError::Invariant);
                }
            }
        }
        if residual_reduced_cost(work, index, id.direction())? != 0 {
            return Err(EnhancedCapacityScalingError::Invariant);
        }
    }
    work.metrics.augmentations = work
        .metrics
        .augmentations
        .checked_add(1)
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    work.metrics.augmented_arcs = work
        .metrics
        .augmented_arcs
        .checked_add(
            u64::try_from(search.path.len())
                .map_err(|_| EnhancedCapacityScalingError::ArithmeticOverflow)?,
        )
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    work.metrics.potential_updates = work
        .metrics
        .potential_updates
        .checked_add(1)
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    validate_dual(work.graph, &work.potentials, &work.virtual_flows)?;
    Ok(())
}

fn halve_scale(work: &mut WorkingState<'_>) -> Result<(), EnhancedCapacityScalingError> {
    work.denominator = work
        .denominator
        .checked_mul(2)
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    for flow in &mut work.virtual_flows {
        *flow = flow
            .checked_mul(2)
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    }
    Ok(())
}

fn forward_reduced_cost(
    work: &WorkingState<'_>,
    edge_index: usize,
) -> Result<i128, EnhancedCapacityScalingError> {
    residual_reduced_cost(work, edge_index, ResidualDirection::Forward)
}

fn residual_reduced_cost(
    work: &WorkingState<'_>,
    edge_index: usize,
    direction: ResidualDirection,
) -> Result<i128, EnhancedCapacityScalingError> {
    let edge = work
        .graph
        .edges()
        .get(edge_index)
        .ok_or(EnhancedCapacityScalingError::Invariant)?;
    let forward = i128::from(edge.cost())
        .checked_add(work.potentials[edge.from().as_usize()])
        .and_then(|value| value.checked_sub(work.potentials[edge.to().as_usize()]))
        .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
    match direction {
        ResidualDirection::Forward => Ok(forward),
        ResidualDirection::Reverse => forward
            .checked_neg()
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow),
    }
}

fn validate_dual(
    graph: &FlowNetwork,
    potentials: &[i128],
    virtual_flows: &[i128],
) -> Result<(), EnhancedCapacityScalingError> {
    if potentials.len() != graph.nodes().len()
        || (!virtual_flows.is_empty() && virtual_flows.len() != graph.edges().len())
    {
        return Err(EnhancedCapacityScalingError::Invariant);
    }
    for (index, edge) in graph.edges().iter().enumerate() {
        let reduced = i128::from(edge.cost())
            .checked_add(potentials[edge.from().as_usize()])
            .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
            .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
        if reduced < 0 || (!virtual_flows.is_empty() && virtual_flows[index] > 0 && reduced != 0) {
            return Err(EnhancedCapacityScalingError::Invariant);
        }
    }
    Ok(())
}

fn recover_primal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    potentials: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<Vec<u64>, EnhancedCapacityScalingError> {
    validate_dual(graph, potentials, &[0; 0])?;
    let lower = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    let target = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, actual)| {
            required
                .checked_sub(actual)
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let nodes = graph
        .nodes()
        .iter()
        .map(|node| FlowNode::new(node.id().clone(), 0))
        .collect();
    let edges = graph
        .edges()
        .iter()
        .map(|edge| {
            let reduced = i128::from(edge.cost())
                .checked_add(potentials[edge.from().as_usize()])
                .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?;
            let width = edge
                .capacity()
                .checked_sub(edge.lower())
                .ok_or(EnhancedCapacityScalingError::Invariant)?;
            Ok(UnresolvedFlowEdge {
                id: edge.id().clone(),
                from: graph
                    .node(edge.from())
                    .ok_or(EnhancedCapacityScalingError::Invariant)?
                    .id()
                    .clone(),
                to: graph
                    .node(edge.to())
                    .ok_or(EnhancedCapacityScalingError::Invariant)?
                    .id()
                    .clone(),
                lower: 0,
                capacity: if reduced == 0 { width } else { 0 },
                cost: 0,
            })
        })
        .collect::<Result<Vec<_>, EnhancedCapacityScalingError>>()?;
    let zero_graph = FlowNetwork::new(nodes, edges)?;
    let variable = feasibility
        .find_feasible_flow(
            &zero_graph,
            &target,
            FeasibilityUse::BeforeEvent {
                anchor: CapturedFeasibilityAnchor {
                    catalog_id: "enhanced-capacity-scaling.recover-primal",
                    occurrence: 1,
                },
            },
        )?
        .flows;
    lower
        .into_iter()
        .zip(variable)
        .map(|(base, value)| {
            base.checked_add(value)
                .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)
        })
        .collect()
}

fn snapshot(
    work: &WorkingState<'_>,
    stage: EnhancedCapacityScalingStage,
    source_component: Option<NodeIndex>,
    sink_component: Option<NodeIndex>,
    path: Vec<ResidualArcId>,
    distances: Vec<Option<i128>>,
    certified_flows: Option<Vec<u64>>,
) -> Result<EnhancedCapacityScalingSnapshot, EnhancedCapacityScalingError> {
    let excess = component_excesses(work)?;
    let components = component_roots(work)
        .map(|root| {
            Ok(EnhancedCapacityScalingComponent {
                id: NodeIndex::try_from_usize(root)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?,
                members: work
                    .component_of
                    .iter()
                    .enumerate()
                    .filter(|&(_, &component)| component == root)
                    .map(|(node, _)| NodeIndex::try_from_usize(node))
                    .collect::<Option<Vec<_>>>()
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?,
                excess_numerator: excess[root],
            })
        })
        .collect::<Result<Vec<_>, EnhancedCapacityScalingError>>()?;
    Ok(EnhancedCapacityScalingSnapshot {
        stage,
        denominator: work.denominator,
        delta_numerator: work.delta_numerator,
        virtual_flow_numerators: work.virtual_flows.clone(),
        potentials: work.potentials.clone(),
        components,
        source_component,
        sink_component,
        path,
        distances,
        certified_flows,
        metrics: work.metrics,
    })
}

fn publish(
    current: &mut EnhancedCapacityScalingSnapshot,
    events: &mut Vec<EnhancedCapacityScalingTraceEvent>,
    record_trace: bool,
    catalog_id: &'static str,
    next: EnhancedCapacityScalingSnapshot,
    contraction_arc: Option<EdgeId>,
    augmentation_numerator: Option<u128>,
) {
    if record_trace {
        events.push(EnhancedCapacityScalingTraceEvent {
            catalog_id,
            before: current.clone(),
            after: next.clone(),
            contraction_arc,
            augmentation_numerator,
        });
    }
    *current = next;
}

fn record_scan_checkpoint(
    work: &WorkingState<'_>,
    arc: ResidualArcId,
    record_trace: bool,
    checkpoints: &mut Vec<EnhancedCapacityScalingSnapshot>,
) -> Result<(), EnhancedCapacityScalingError> {
    if record_trace
        && (work.metrics.residual_arc_scans <= ENHANCED_CAPACITY_SCALING_TRACE_SCAN_PREFIX
            || work
                .metrics
                .residual_arc_scans
                .is_multiple_of(ENHANCED_CAPACITY_SCALING_TRACE_SCAN_BLOCK))
    {
        checkpoints.push(snapshot(
            work,
            EnhancedCapacityScalingStage::InspectResidualArc,
            None,
            None,
            vec![arc],
            vec![None; work.graph.nodes().len()],
            None,
        )?);
    }
    Ok(())
}

fn flush_scan_checkpoint(
    work: &WorkingState<'_>,
    arc: Option<ResidualArcId>,
    record_trace: bool,
    checkpoints: &mut Vec<EnhancedCapacityScalingSnapshot>,
) -> Result<(), EnhancedCapacityScalingError> {
    if record_trace
        && let Some(arc) = arc
        && checkpoints.last().is_none_or(|checkpoint| {
            checkpoint.metrics.residual_arc_scans != work.metrics.residual_arc_scans
        })
    {
        checkpoints.push(snapshot(
            work,
            EnhancedCapacityScalingStage::InspectResidualArc,
            None,
            None,
            vec![arc],
            vec![None; work.graph.nodes().len()],
            None,
        )?);
    }
    Ok(())
}

fn publish_scan_checkpoints(
    current: &mut EnhancedCapacityScalingSnapshot,
    events: &mut Vec<EnhancedCapacityScalingTraceEvent>,
    record_trace: bool,
    checkpoints: Vec<EnhancedCapacityScalingSnapshot>,
) {
    for checkpoint in checkpoints {
        publish(
            current,
            events,
            record_trace,
            "enhanced-capacity-scaling.inspect-residual-arc",
            checkpoint,
            None,
            None,
        );
    }
}

fn check_scan_limit(work: &WorkingState<'_>) -> Result<(), EnhancedCapacityScalingError> {
    if work.metrics.residual_arc_scans > ENHANCED_CAPACITY_SCALING_MAX_SCANS {
        return Err(EnhancedCapacityScalingError::WorkLimit);
    }
    Ok(())
}

/// Independently validates public trace continuity, source grammar, quotient
/// partitions, exact dyadic state, dual feasibility, and the final certificate.
///
/// # Errors
///
/// Returns [`EnhancedCapacityScalingError::TraceVerification`] for a forged or
/// inconsistent boundary.
pub fn check_enhanced_capacity_scaling_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &EnhancedCapacityScalingTraceResult,
) -> Result<(), EnhancedCapacityScalingError> {
    if trace.base_snapshot.stage != EnhancedCapacityScalingStage::Ready
        || trace.final_snapshot.stage != EnhancedCapacityScalingStage::Optimal
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.flows
            != trace
                .final_snapshot
                .certified_flows
                .clone()
                .unwrap_or_default()
        || trace.events.is_empty()
    {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    let expected = solve_internal(graph, required_divergence, true)?;
    if expected.result != trace.result
        || expected.base_snapshot != trace.base_snapshot
        || expected.events != trace.events
        || expected.final_snapshot != trace.final_snapshot
    {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    let (required_variable, _) = validate_and_initialize(graph, required_divergence)?;
    validate_public_snapshot(graph, &required_variable, &trace.base_snapshot)?;
    let mut current = trace.base_snapshot.clone();
    for event in &trace.events {
        if event.before != current || event.catalog_id != catalog_id(event.after.stage) {
            return Err(EnhancedCapacityScalingError::TraceVerification);
        }
        validate_public_snapshot(graph, &required_variable, &event.after)?;
        validate_transition(graph, &required_variable, event)?;
        current = event.after.clone();
    }
    if current != trace.final_snapshot {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    for event in trace.events.iter().rev() {
        if current != event.after {
            return Err(EnhancedCapacityScalingError::TraceVerification);
        }
        current = event.before.clone();
    }
    if current != trace.base_snapshot {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if certificate != trace.result.certificate {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    Ok(())
}

const fn catalog_id(stage: EnhancedCapacityScalingStage) -> &'static str {
    match stage {
        EnhancedCapacityScalingStage::Ready => "enhanced-capacity-scaling.ready",
        EnhancedCapacityScalingStage::Initialize => "enhanced-capacity-scaling.initialize",
        EnhancedCapacityScalingStage::CompleteRegeneration => {
            "enhanced-capacity-scaling.complete-regeneration"
        }
        EnhancedCapacityScalingStage::BeginPhase => "enhanced-capacity-scaling.begin-phase",
        EnhancedCapacityScalingStage::Contract => "enhanced-capacity-scaling.contract",
        EnhancedCapacityScalingStage::InspectResidualArc => {
            "enhanced-capacity-scaling.inspect-residual-arc"
        }
        EnhancedCapacityScalingStage::SelectPath => "enhanced-capacity-scaling.select-path",
        EnhancedCapacityScalingStage::Augment => "enhanced-capacity-scaling.augment",
        EnhancedCapacityScalingStage::CompletePhase => "enhanced-capacity-scaling.complete-phase",
        EnhancedCapacityScalingStage::HalveScale => "enhanced-capacity-scaling.halve-scale",
        EnhancedCapacityScalingStage::RecoverPrimal => "enhanced-capacity-scaling.recover-primal",
        EnhancedCapacityScalingStage::Optimal => "enhanced-capacity-scaling.optimal",
    }
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    required_variable: &[i128],
    snapshot: &EnhancedCapacityScalingSnapshot,
) -> Result<(), EnhancedCapacityScalingError> {
    if snapshot.denominator == 0
        || snapshot.virtual_flow_numerators.len() != graph.edges().len()
        || snapshot.potentials.len() != graph.nodes().len()
        || snapshot.distances.len() != graph.nodes().len()
        || snapshot
            .virtual_flow_numerators
            .iter()
            .any(|&flow| flow < 0)
    {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    let mut component_of = vec![usize::MAX; graph.nodes().len()];
    for component in &snapshot.components {
        if component.members.is_empty()
            || component.id != component.members[0]
            || component.members.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(EnhancedCapacityScalingError::TraceVerification);
        }
        for &member in &component.members {
            let slot = component_of
                .get_mut(member.as_usize())
                .ok_or(EnhancedCapacityScalingError::TraceVerification)?;
            if *slot != usize::MAX {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
            *slot = component.id.as_usize();
        }
    }
    if component_of.contains(&usize::MAX) {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    let work = WorkingState {
        graph,
        required_variable: required_variable.to_vec(),
        denominator: snapshot.denominator,
        delta_numerator: snapshot.delta_numerator,
        virtual_flows: snapshot.virtual_flow_numerators.clone(),
        potentials: snapshot.potentials.clone(),
        component_of,
        metrics: snapshot.metrics,
    };
    let expected = component_excesses(&work)?;
    if snapshot
        .components
        .iter()
        .any(|component| expected[component.id.as_usize()] != component.excess_numerator)
    {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    if snapshot.stage != EnhancedCapacityScalingStage::Ready {
        validate_dual(
            graph,
            &snapshot.potentials,
            &snapshot.virtual_flow_numerators,
        )?;
    }
    if let Some(flows) = &snapshot.certified_flows
        && flows.len() != graph.edges().len()
    {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "all source transition cases stay together as one exhaustive trace grammar"
)]
fn validate_transition(
    graph: &FlowNetwork,
    required_variable: &[i128],
    event: &EnhancedCapacityScalingTraceEvent,
) -> Result<(), EnhancedCapacityScalingError> {
    match event.after.stage {
        EnhancedCapacityScalingStage::Initialize => {
            if event.before.stage != EnhancedCapacityScalingStage::Ready
                || event.after.virtual_flow_numerators != event.before.virtual_flow_numerators
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::CompleteRegeneration => {
            if event
                .after
                .virtual_flow_numerators
                .iter()
                .any(|&flow| flow != 0)
                || event.after.delta_numerator
                    != event
                        .after
                        .components
                        .iter()
                        .map(|component| component.excess_numerator.unsigned_abs())
                        .max()
                        .unwrap_or(0)
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::Contract => {
            if event.contraction_arc.is_none()
                || event.after.components.len() + 1 != event.before.components.len()
                || event.after.virtual_flow_numerators != event.before.virtual_flow_numerators
                || event.after.potentials != event.before.potentials
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::InspectResidualArc => {
            if event.after.path.len() != 1
                || event.after.source_component.is_some()
                || event.after.sink_component.is_some()
                || event.after.distances.iter().any(Option::is_some)
                || event.after.virtual_flow_numerators != event.before.virtual_flow_numerators
                || event.after.potentials != event.before.potentials
                || event.after.components != event.before.components
                || event.after.denominator != event.before.denominator
                || event.after.delta_numerator != event.before.delta_numerator
                || event.after.metrics.residual_arc_scans <= event.before.metrics.residual_arc_scans
                || event.after.metrics.residual_arc_scans - event.before.metrics.residual_arc_scans
                    > ENHANCED_CAPACITY_SCALING_TRACE_SCAN_BLOCK
                || graph
                    .edge_index(event.after.path[0].original_edge())
                    .is_none()
                || event.contraction_arc.is_some()
                || event.augmentation_numerator.is_some()
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::SelectPath => {
            if event.after.path.is_empty()
                || event.after.source_component.is_none()
                || event.after.sink_component.is_none()
                || event.after.virtual_flow_numerators != event.before.virtual_flow_numerators
                || event.after.potentials != event.before.potentials
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::Augment => {
            if event.before.stage != EnhancedCapacityScalingStage::SelectPath
                || event.augmentation_numerator != Some(event.before.delta_numerator)
                || event.after.path != event.before.path
                || event.after.distances != event.before.distances
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
            let component_of = component_map(graph.nodes().len(), &event.before.components)?;
            let mut work = WorkingState {
                graph,
                required_variable: required_variable.to_vec(),
                denominator: event.before.denominator,
                delta_numerator: event.before.delta_numerator,
                virtual_flows: event.before.virtual_flow_numerators.clone(),
                potentials: event.before.potentials.clone(),
                component_of,
                metrics: event.before.metrics,
            };
            let search = SearchResult {
                source: event
                    .before
                    .source_component
                    .ok_or(EnhancedCapacityScalingError::TraceVerification)?
                    .as_usize(),
                sink: event
                    .before
                    .sink_component
                    .ok_or(EnhancedCapacityScalingError::TraceVerification)?
                    .as_usize(),
                path: event.before.path.clone(),
                distances_by_node: event.before.distances.clone(),
            };
            apply_search(&mut work, &search)?;
            if work.virtual_flows != event.after.virtual_flow_numerators
                || work.potentials != event.after.potentials
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::HalveScale => {
            if event.after.denominator
                != event
                    .before
                    .denominator
                    .checked_mul(2)
                    .ok_or(EnhancedCapacityScalingError::ArithmeticOverflow)?
                || event.after.delta_numerator != event.before.delta_numerator
                || event
                    .before
                    .virtual_flow_numerators
                    .iter()
                    .zip(&event.after.virtual_flow_numerators)
                    .any(|(&before, &after)| before.checked_mul(2) != Some(after))
            {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::RecoverPrimal | EnhancedCapacityScalingStage::Optimal => {
            if event.after.certified_flows.is_none() {
                return Err(EnhancedCapacityScalingError::TraceVerification);
            }
        }
        EnhancedCapacityScalingStage::BeginPhase | EnhancedCapacityScalingStage::CompletePhase => {}
        EnhancedCapacityScalingStage::Ready => {
            return Err(EnhancedCapacityScalingError::TraceVerification);
        }
    }
    Ok(())
}

fn component_map(
    node_count: usize,
    components: &[EnhancedCapacityScalingComponent],
) -> Result<Vec<usize>, EnhancedCapacityScalingError> {
    let mut map = vec![usize::MAX; node_count];
    for component in components {
        for member in &component.members {
            let slot = map
                .get_mut(member.as_usize())
                .ok_or(EnhancedCapacityScalingError::TraceVerification)?;
            *slot = component.id.as_usize();
        }
    }
    if map.contains(&usize::MAX) {
        return Err(EnhancedCapacityScalingError::TraceVerification);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::model::{EdgeId, NodeId};

    fn graph(nodes: &[&str], edges: &[(&str, &str, &str, u64, i64)]) -> FlowNetwork {
        let nodes = nodes
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edges = edges
            .iter()
            .map(|&(id, from, to, capacity, cost)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge id"),
                from: NodeId::parse(from).expect("tail"),
                to: NodeId::parse(to).expect("head"),
                lower: 0,
                capacity,
                cost,
            })
            .collect();
        FlowNetwork::new(nodes, edges).expect("graph")
    }

    fn graph_with_lower(
        nodes: &[&str],
        edges: &[(&str, &str, &str, u64, u64, i64)],
    ) -> FlowNetwork {
        let nodes = nodes
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edges = edges
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
            .collect();
        FlowNetwork::new(nodes, edges).expect("graph")
    }

    #[test]
    fn routes_transshipment_and_matches_cycle_canceling() {
        let graph = graph(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 10, 1),
                ("ac", "a", "c", 10, 5),
                ("ba", "b", "a", 10, 4),
                ("bc", "b", "c", 10, 1),
                ("ca", "c", "a", 10, 4),
                ("cb", "c", "b", 10, 4),
            ],
        );
        let target = [5, 0, -5];
        let fast = solve_enhanced_capacity_scaling(&graph, &target).expect("fast");
        let traced = trace_enhanced_capacity_scaling(&graph, &target).expect("trace");
        let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.total_cost, 10);
        assert_eq!(fast.certificate.total_cost, oracle.certificate.total_cost);
        assert!(fast.metrics.augmentations > 0);
        let mut published_scans = 0_u128;
        for event in &traced.events {
            let delta = event
                .after
                .metrics
                .residual_arc_scans
                .checked_sub(event.before.metrics.residual_arc_scans)
                .expect("scan counter is monotone");
            if event.after.stage == EnhancedCapacityScalingStage::InspectResidualArc {
                assert!((1..=ENHANCED_CAPACITY_SCALING_TRACE_SCAN_BLOCK).contains(&delta));
                assert_eq!(event.after.path.len(), 1);
                published_scans += delta;
            } else {
                assert_eq!(delta, 0, "residual work leaked into a semantic boundary");
            }
        }
        assert_eq!(published_scans, traced.result.metrics.residual_arc_scans);
    }

    #[test]
    fn rejects_binding_width_and_non_strong_graphs() {
        let narrow = graph(
            &["a", "b"],
            &[
                ("ab-1", "a", "b", 1, 0),
                ("ab-2", "a", "b", 1, 0),
                ("ba-1", "b", "a", 1, 0),
                ("ba-2", "b", "a", 1, 0),
            ],
        );
        assert_eq!(
            solve_enhanced_capacity_scaling(&narrow, &[2, -2]),
            Err(EnhancedCapacityScalingError::CapacityBound)
        );
        let one_way = graph(&["a", "b"], &[("ab", "a", "b", 10, 0)]);
        assert_eq!(
            solve_enhanced_capacity_scaling(&one_way, &[1, -1]),
            Err(EnhancedCapacityScalingError::StrongConnectivity)
        );
    }

    #[test]
    fn rejects_unbounded_negative_cycle() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 10, -2), ("ba", "b", "a", 10, 1)],
        );
        assert_eq!(
            solve_enhanced_capacity_scaling(&graph, &[0, 0]),
            Err(EnhancedCapacityScalingError::Unbounded)
        );
    }

    #[test]
    fn supports_negative_edges_and_lower_bounds() {
        let graph = graph_with_lower(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 1, 10, -3),
                ("ac", "a", "c", 0, 10, 2),
                ("ba", "b", "a", 0, 10, 5),
                ("bc", "b", "c", 0, 10, 1),
                ("ca", "c", "a", 0, 10, 4),
                ("cb", "c", "b", 0, 10, 4),
            ],
        );
        let target = [4, 0, -4];
        let result = solve_enhanced_capacity_scaling(&graph, &target).expect("enhanced");
        let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
        assert_eq!(result.certificate.total_cost, oracle.certificate.total_cost);
        assert_eq!(
            divergences(&graph, &result.flows).expect("divergences"),
            target
        );
        assert!(result.flows[0] >= 1);
    }

    #[test]
    fn contracts_strongly_feasible_arcs_during_a_phase() {
        let graph = graph(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 20, 0),
                ("ac", "a", "c", 20, 4),
                ("ba", "b", "a", 20, 4),
                ("bc", "b", "c", 20, 0),
                ("ca", "c", "a", 20, 4),
                ("cb", "c", "b", 20, 4),
            ],
        );
        let trace = trace_enhanced_capacity_scaling(&graph, &[5, -4, -1]).expect("trace");
        assert!(trace.result.metrics.contractions > 0);
        assert!(trace.events.iter().any(|event| {
            event.after.stage == EnhancedCapacityScalingStage::Contract
                && event.contraction_arc.is_some()
        }));
    }

    #[test]
    fn trace_checker_rejects_forged_exact_state() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 10, 1), ("ba", "b", "a", 10, 2)],
        );
        let mut trace = trace_enhanced_capacity_scaling(&graph, &[3, -3]).expect("trace");
        trace.events[0].after.delta_numerator = trace.events[0]
            .after
            .delta_numerator
            .checked_add(1)
            .expect("small delta");
        assert_eq!(
            check_enhanced_capacity_scaling_trace(&graph, &[3, -3], &trace),
            Err(EnhancedCapacityScalingError::TraceVerification)
        );
    }

    #[test]
    fn admission_limits_are_inclusive_and_reject_the_next_value() {
        let node_ids = (0..ENHANCED_CAPACITY_SCALING_MAX_NODES)
            .map(|index| format!("n-{index}"))
            .collect::<Vec<_>>();
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect::<Vec<_>>();
        let edges = (0..node_ids.len())
            .map(|index| UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("e-{index}")).expect("edge id"),
                from: NodeId::parse(&node_ids[index]).expect("tail"),
                to: NodeId::parse(&node_ids[(index + 1) % node_ids.len()]).expect("head"),
                lower: 0,
                capacity: 1,
                cost: 0,
            })
            .collect();
        let at_node_limit = FlowNetwork::new(nodes, edges).expect("graph");
        assert!(
            solve_enhanced_capacity_scaling(
                &at_node_limit,
                &vec![0; ENHANCED_CAPACITY_SCALING_MAX_NODES]
            )
            .is_ok()
        );

        let too_many_nodes = (0..=ENHANCED_CAPACITY_SCALING_MAX_NODES)
            .map(|index| FlowNode::new(NodeId::parse(&format!("x-{index}")).expect("node id"), 0))
            .collect();
        let oversized = FlowNetwork::new(too_many_nodes, Vec::new()).expect("graph");
        assert_eq!(
            solve_enhanced_capacity_scaling(
                &oversized,
                &vec![0; ENHANCED_CAPACITY_SCALING_MAX_NODES + 1]
            ),
            Err(EnhancedCapacityScalingError::AdmissionLimit)
        );

        let nodes = vec![
            FlowNode::new(NodeId::parse("left").expect("node id"), 0),
            FlowNode::new(NodeId::parse("right").expect("node id"), 0),
        ];
        let parallel = |count: usize| {
            (0..count)
                .map(|index| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("p-{index}")).expect("edge id"),
                    from: NodeId::parse(if index % 2 == 0 { "left" } else { "right" })
                        .expect("tail"),
                    to: NodeId::parse(if index % 2 == 0 { "right" } else { "left" }).expect("head"),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                })
                .collect::<Vec<_>>()
        };
        let at_edge_limit =
            FlowNetwork::new(nodes.clone(), parallel(ENHANCED_CAPACITY_SCALING_MAX_EDGES))
                .expect("graph");
        assert!(solve_enhanced_capacity_scaling(&at_edge_limit, &[0, 0]).is_ok());
        let too_many_edges =
            FlowNetwork::new(nodes, parallel(ENHANCED_CAPACITY_SCALING_MAX_EDGES + 1))
                .expect("graph");
        assert_eq!(
            solve_enhanced_capacity_scaling(&too_many_edges, &[0, 0]),
            Err(EnhancedCapacityScalingError::AdmissionLimit)
        );
    }
}
