//! Relaxed most-negative node-disjoint-cycle canceling.
//!
//! The kernel follows Shigeno--Iwata--McCormick's generic primal algorithm.
//! An outer dyadic epsilon scale shifts every positive residual-arc cost by
//! epsilon.  Inside a phase, an exact assignment problem on split node copies
//! chooses a minimum-cost node-disjoint family of residual cycles.  Every
//! selected cycle is pushed to its own bottleneck before the assignment is
//! solved again.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};

/// Conservative node limit for the explicit dense assignment subproblem.
pub const RELAXED_MNDC_MAX_NODES: usize = 64;
/// Conservative original-edge limit for the research visualization kernel.
pub const RELAXED_MNDC_MAX_EDGES: usize = 512;
/// Maximum outer epsilon phases.
pub const RELAXED_MNDC_MAX_PHASES: u64 = 128;
/// Maximum exact assignment solves across all phases.
pub const RELAXED_MNDC_MAX_ASSIGNMENT_SOLVES: u64 = 100_000;
/// Maximum canceled node-disjoint families.
pub const RELAXED_MNDC_MAX_FAMILIES: u64 = 100_000;
/// Maximum dense assignment cells and residual arcs inspected.
pub const RELAXED_MNDC_MAX_SCANS: u128 = 20_000_000;
/// Preserve every early work scan before bounded block aggregation.
const RELAXED_MNDC_TRACE_SCAN_PREFIX: u128 = 512;
/// Maximum number of source scan primitives represented by one later Detail
/// checkpoint. This keeps trace length proportional to real work without
/// materializing millions of SVG frames.
const RELAXED_MNDC_TRACE_SCAN_BLOCK: u128 = 256;

/// Exact dyadic outer relaxation parameter `numerator / denominator`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelaxedMndcEpsilon {
    numerator: u128,
    denominator: u128,
}

impl RelaxedMndcEpsilon {
    /// Nonnegative numerator fixed to the maximum absolute original cost.
    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    /// Positive power-of-two denominator.
    pub const fn denominator(self) -> u128 {
        self.denominator
    }
}

/// Source-defined semantic boundary in the nested relaxation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelaxedMndcStage {
    /// Arbitrary feasible flow before epsilon initialization is published.
    Ready,
    /// Initial `epsilon = C` state.
    Initialize,
    /// Epsilon was halved for a new scaling phase.
    BeginPhase,
    /// One positive residual arc was inspected while building the split graph.
    InspectResidualArc,
    /// One Hungarian row/column cell was inspected.
    InspectAssignmentCell,
    /// The split-node assignment selected an epsilon-MNDC family.
    SelectFamily,
    /// Every cycle in the selected family was pushed to its bottleneck.
    CancelFamily,
    /// Assignment value is nonnegative, proving epsilon-optimality.
    PhaseOptimal,
    /// Epsilon is below `1 / n` and an independent exact certificate passed.
    Optimal,
}

/// Exact deterministic counters from the nested assignment/canceling kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelaxedMndcMetrics {
    /// Outer epsilon phases entered.
    pub scaling_phases: u64,
    /// Minimum-cost perfect assignment problems solved.
    pub assignment_solves: u64,
    /// Hungarian row augmentations.
    pub assignment_augmentations: u64,
    /// Dense assignment cells inspected.
    pub assignment_cell_scans: u128,
    /// Positive residual arcs inspected while building assignment networks.
    pub residual_arc_scans: u128,
    /// Strictly negative node-disjoint families canceled.
    pub canceled_families: u64,
    /// Individual cycles canceled across all families.
    pub canceled_cycles: u64,
    /// Residual arcs participating in canceled cycles.
    pub canceled_cycle_arcs: u64,
    /// Zero-cost assignment cycles removed in favor of artificial identities.
    pub dropped_zero_cycles: u64,
}

/// One selected row of the split-node perfect assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcAssignmentChoice {
    /// Right-copy column matched to this canonical left-copy row.
    pub column: NodeIndex,
    /// Residual arc represented by the match, or `None` for the artificial
    /// zero-cost identity edge.
    pub residual_arc: Option<ResidualArcId>,
}

/// One negative residual cycle in an epsilon-MNDC family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcCycle {
    /// Ordered residual cycle beginning at its smallest canonical node.
    pub arcs: Vec<ResidualArcId>,
    /// Exact assignment-domain cycle cost `denominator * cost + numerator`.
    pub transformed_cost: i128,
}

/// Complete public state at one source-algorithm boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcSnapshot {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Exact outer relaxation parameter.
    pub epsilon: RelaxedMndcEpsilon,
    /// One-based phase ordinal, or zero before the first halving.
    pub phase: u64,
    /// Semantic boundary kind.
    pub stage: RelaxedMndcStage,
    /// Scaled optimum of the current split-node assignment.
    pub assignment_value: Option<i128>,
    /// Left-copy assignment duals in canonical node order.
    pub left_duals: Vec<i128>,
    /// Right-copy assignment duals in canonical node order.
    pub right_duals: Vec<i128>,
    /// Exact perfect assignment, one choice per canonical row.
    pub assignment: Vec<RelaxedMndcAssignmentChoice>,
    /// Strictly negative node-disjoint cycle family selected at this boundary.
    pub family: Vec<RelaxedMndcCycle>,
    /// Exact residual arc inspected at this Detail boundary.
    pub active_residual_arc: Option<ResidualArcId>,
    /// Exact assignment row and column inspected at this Detail boundary.
    pub active_assignment_cell: Option<(NodeIndex, NodeIndex)>,
    /// Exact work counters.
    pub metrics: RelaxedMndcMetrics,
}

/// One reversible source-algorithm transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// Complete state before the atomic transition.
    pub before: RelaxedMndcSnapshot,
    /// Complete state after the atomic transition.
    pub after: RelaxedMndcSnapshot,
    /// Per-cycle bottlenecks for a cancel-family transition.
    pub deltas: Vec<u64>,
}

/// Certified relaxed-MNDC result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcResult {
    /// Exact optimal original-edge flows.
    pub flows: Vec<u64>,
    /// Independently reconstructed primal/dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact deterministic counters.
    pub metrics: RelaxedMndcMetrics,
    /// Exact final algorithm boundary for fast-profile visualization.
    pub final_snapshot: RelaxedMndcSnapshot,
}

/// Certified result with every nested-relaxation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelaxedMndcTraceResult {
    /// Same result returned by the fast profile.
    pub result: RelaxedMndcResult,
    /// Feasible-flow boundary before initialization.
    pub base_snapshot: RelaxedMndcSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<RelaxedMndcTraceEvent>,
    /// Independently certified exact optimum.
    pub final_snapshot: RelaxedMndcSnapshot,
}

/// Construction, bounded-work, arithmetic, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RelaxedMndcError {
    /// Input exceeds the explicit assignment-network band.
    #[error("graph exceeds relaxed-MNDC admission limits")]
    AdmissionLimit,
    /// A deterministic phase, assignment, family, or scan ceiling was reached.
    #[error("relaxed-MNDC work limit reached")]
    WorkLimit,
    /// Requested balances have no feasible bounded flow.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual reconstruction or cycle augmentation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent minimum-cost certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact integer scaling or assignment arithmetic overflowed.
    #[error("relaxed-MNDC arithmetic overflow")]
    ArithmeticOverflow,
    /// Assignment primal/dual evidence or cycle decomposition was invalid.
    #[error("relaxed-MNDC assignment invariant failed")]
    AssignmentInvariant,
    /// A public trace transition failed independent replay.
    #[error("relaxed-MNDC trace verification failed")]
    TraceVerification,
}

/// Solves minimum-cost flow by relaxed most-negative node-disjoint-cycle
/// cancellation.
///
/// # Errors
///
/// Returns admission, feasibility, work-limit, arithmetic, residual,
/// assignment-invariant, or independent-certificate failures.
pub fn solve_relaxed_mndc(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RelaxedMndcResult, RelaxedMndcError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its initial feasible-flow construction to the
/// enclosing execution context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_relaxed_mndc_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RelaxedMndcResult, RelaxedMndcError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Solves while retaining every epsilon phase, exact assignment, selected
/// family, and atomic family cancellation.
///
/// # Errors
///
/// Returns the same failures as [`solve_relaxed_mndc`] plus independent trace
/// verification failures.
pub fn trace_relaxed_mndc(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<RelaxedMndcTraceResult, RelaxedMndcError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let traced = RelaxedMndcTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_relaxed_mndc_trace(graph, required_divergence, &traced)?;
    Ok(traced)
}

/// Traces relaxed MNDC while explicitly publishing its initial feasible-flow
/// construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_relaxed_mndc_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<RelaxedMndcTraceResult, RelaxedMndcError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let traced = RelaxedMndcTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_relaxed_mndc_trace(graph, required_divergence, &traced)?;
    Ok(traced)
}

struct InternalRun {
    result: RelaxedMndcResult,
    base_snapshot: RelaxedMndcSnapshot,
    events: Vec<RelaxedMndcTraceEvent>,
    final_snapshot: RelaxedMndcSnapshot,
}

#[derive(Clone)]
enum RelaxedMndcWorkFocus {
    ResidualArc(ResidualArcId),
    AssignmentCell(NodeIndex, NodeIndex),
}

#[derive(Clone)]
struct RelaxedMndcWorkCheckpoint {
    metrics: RelaxedMndcMetrics,
    focus: RelaxedMndcWorkFocus,
}

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, RelaxedMndcError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, RelaxedMndcError> {
    validate_admission(graph)?;
    let feasible =
        feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::InitialFlow)?;
    let mut residual = ResidualState::from_flows(graph, &feasible.flows)?;
    let maximum_cost = u128::from(
        graph
            .edges()
            .iter()
            .map(|edge| edge.cost().unsigned_abs())
            .max()
            .unwrap_or(0),
    );
    let mut epsilon = RelaxedMndcEpsilon {
        numerator: maximum_cost,
        denominator: 1,
    };
    let mut metrics = RelaxedMndcMetrics::default();
    let base_snapshot = snapshot(
        &residual,
        epsilon,
        0,
        RelaxedMndcStage::Ready,
        None,
        Vec::new(),
        metrics,
    );
    let mut current = base_snapshot.clone();
    let mut events = Vec::new();
    publish(
        &mut current,
        &mut events,
        record_trace,
        "relaxed-most-negative-cycle.initialize",
        snapshot(
            &residual,
            epsilon,
            0,
            RelaxedMndcStage::Initialize,
            None,
            Vec::new(),
            metrics,
        ),
        Vec::new(),
    );

    let node_count = graph.nodes().len();
    let node_count_u128 =
        u128::try_from(node_count).map_err(|_| RelaxedMndcError::ArithmeticOverflow)?;
    let terminal_denominator = maximum_cost
        .checked_mul(node_count_u128)
        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    while maximum_cost > 0 && epsilon.denominator <= terminal_denominator {
        if metrics.scaling_phases >= RELAXED_MNDC_MAX_PHASES {
            return Err(RelaxedMndcError::WorkLimit);
        }
        epsilon.denominator = epsilon
            .denominator
            .checked_mul(2)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
        metrics.scaling_phases = metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
        let phase = metrics.scaling_phases;
        publish(
            &mut current,
            &mut events,
            record_trace,
            "relaxed-most-negative-cycle.begin-phase",
            snapshot(
                &residual,
                epsilon,
                phase,
                RelaxedMndcStage::BeginPhase,
                None,
                Vec::new(),
                metrics,
            ),
            Vec::new(),
        );

        loop {
            if metrics.assignment_solves >= RELAXED_MNDC_MAX_ASSIGNMENT_SOLVES {
                return Err(RelaxedMndcError::WorkLimit);
            }
            let mut work_checkpoints = Vec::new();
            let matrix = build_assignment_matrix(
                &residual,
                epsilon,
                Some(&mut metrics),
                record_trace.then_some(&mut work_checkpoints),
            )?;
            let assignment = solve_assignment(
                &matrix,
                &mut metrics,
                record_trace.then_some(&mut work_checkpoints),
            )?;
            metrics.assignment_solves = metrics
                .assignment_solves
                .checked_add(1)
                .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
            let family = decompose_negative_family(&assignment, &matrix, &mut metrics)?;
            publish_work_checkpoints(
                &mut current,
                &mut events,
                record_trace,
                &residual,
                epsilon,
                phase,
                work_checkpoints,
            );
            if assignment.value >= 0 {
                if !family.is_empty() {
                    return Err(RelaxedMndcError::AssignmentInvariant);
                }
                publish(
                    &mut current,
                    &mut events,
                    record_trace,
                    "relaxed-most-negative-cycle.phase-optimal",
                    snapshot(
                        &residual,
                        epsilon,
                        phase,
                        RelaxedMndcStage::PhaseOptimal,
                        Some(&assignment),
                        Vec::new(),
                        metrics,
                    ),
                    Vec::new(),
                );
                break;
            }
            if family.is_empty() || metrics.canceled_families >= RELAXED_MNDC_MAX_FAMILIES {
                return Err(RelaxedMndcError::WorkLimit);
            }
            publish(
                &mut current,
                &mut events,
                record_trace,
                "relaxed-most-negative-cycle.select-family",
                snapshot(
                    &residual,
                    epsilon,
                    phase,
                    RelaxedMndcStage::SelectFamily,
                    Some(&assignment),
                    family.clone(),
                    metrics,
                ),
                Vec::new(),
            );
            let deltas = cancel_family(&mut residual, &family, &mut metrics)?;
            publish(
                &mut current,
                &mut events,
                record_trace,
                "relaxed-most-negative-cycle.cancel-family",
                snapshot(
                    &residual,
                    epsilon,
                    phase,
                    RelaxedMndcStage::CancelFamily,
                    Some(&assignment),
                    family,
                    metrics,
                ),
                deltas,
            );
        }
    }

    let flows = residual.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        "relaxed-most-negative-cycle.optimal",
        snapshot(
            &residual,
            epsilon,
            metrics.scaling_phases,
            RelaxedMndcStage::Optimal,
            None,
            Vec::new(),
            metrics,
        ),
        Vec::new(),
    );
    let final_snapshot = current;
    let result = RelaxedMndcResult {
        flows,
        certificate,
        metrics,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), RelaxedMndcError> {
    if graph.nodes().len() > RELAXED_MNDC_MAX_NODES || graph.edges().len() > RELAXED_MNDC_MAX_EDGES
    {
        return Err(RelaxedMndcError::AdmissionLimit);
    }
    Ok(())
}

fn snapshot(
    residual: &ResidualState<'_>,
    epsilon: RelaxedMndcEpsilon,
    phase: u64,
    stage: RelaxedMndcStage,
    assignment: Option<&AssignmentSolution>,
    family: Vec<RelaxedMndcCycle>,
    metrics: RelaxedMndcMetrics,
) -> RelaxedMndcSnapshot {
    RelaxedMndcSnapshot {
        flows: residual.flows().to_vec(),
        epsilon,
        phase,
        stage,
        assignment_value: assignment.map(|value| value.value),
        left_duals: assignment.map_or_else(Vec::new, |value| value.left_duals.clone()),
        right_duals: assignment.map_or_else(Vec::new, |value| value.right_duals.clone()),
        assignment: assignment.map_or_else(Vec::new, |value| value.public_choices.clone()),
        family,
        active_residual_arc: None,
        active_assignment_cell: None,
        metrics,
    }
}

fn publish(
    current: &mut RelaxedMndcSnapshot,
    events: &mut Vec<RelaxedMndcTraceEvent>,
    record_trace: bool,
    catalog_id: &'static str,
    next: RelaxedMndcSnapshot,
    deltas: Vec<u64>,
) {
    if record_trace {
        events.push(RelaxedMndcTraceEvent {
            catalog_id,
            before: current.clone(),
            after: next.clone(),
            deltas,
        });
    }
    *current = next;
}

#[allow(clippy::too_many_arguments)]
fn publish_work_checkpoints(
    current: &mut RelaxedMndcSnapshot,
    events: &mut Vec<RelaxedMndcTraceEvent>,
    record_trace: bool,
    residual: &ResidualState<'_>,
    epsilon: RelaxedMndcEpsilon,
    phase: u64,
    checkpoints: Vec<RelaxedMndcWorkCheckpoint>,
) {
    for checkpoint in checkpoints {
        let (stage, catalog_id) = match &checkpoint.focus {
            RelaxedMndcWorkFocus::ResidualArc(_) => (
                RelaxedMndcStage::InspectResidualArc,
                "relaxed-most-negative-cycle.inspect-residual-arc",
            ),
            RelaxedMndcWorkFocus::AssignmentCell(_, _) => (
                RelaxedMndcStage::InspectAssignmentCell,
                "relaxed-most-negative-cycle.inspect-assignment-cell",
            ),
        };
        let mut next = snapshot(
            residual,
            epsilon,
            phase,
            stage,
            None,
            Vec::new(),
            checkpoint.metrics,
        );
        match checkpoint.focus {
            RelaxedMndcWorkFocus::ResidualArc(arc) => next.active_residual_arc = Some(arc),
            RelaxedMndcWorkFocus::AssignmentCell(row, column) => {
                next.active_assignment_cell = Some((row, column));
            }
        }
        publish(current, events, record_trace, catalog_id, next, Vec::new());
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AssignmentCell {
    weight: i128,
    residual_arc: Option<ResidualArcId>,
}

type AssignmentMatrix = Vec<Vec<Option<AssignmentCell>>>;

fn build_assignment_matrix(
    residual: &ResidualState<'_>,
    epsilon: RelaxedMndcEpsilon,
    mut metrics: Option<&mut RelaxedMndcMetrics>,
    mut checkpoints: Option<&mut Vec<RelaxedMndcWorkCheckpoint>>,
) -> Result<AssignmentMatrix, RelaxedMndcError> {
    let node_count = residual.graph().nodes().len();
    let mut matrix = vec![vec![None; node_count]; node_count];
    for (node, row) in matrix.iter_mut().enumerate() {
        row[node] = Some(AssignmentCell {
            weight: 0,
            residual_arc: None,
        });
    }
    let denominator =
        i128::try_from(epsilon.denominator).map_err(|_| RelaxedMndcError::ArithmeticOverflow)?;
    let numerator =
        i128::try_from(epsilon.numerator).map_err(|_| RelaxedMndcError::ArithmeticOverflow)?;
    let mut last_inspected_arc = None;
    for node in residual.graph().node_indices() {
        for arc in residual.outgoing_arcs(node) {
            if arc.capacity == 0 {
                continue;
            }
            if let Some(counters) = metrics.as_deref_mut() {
                last_inspected_arc = Some(arc.id.clone());
                counters.residual_arc_scans = counters
                    .residual_arc_scans
                    .checked_add(1)
                    .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
                check_scan_limit(counters)?;
                if should_publish_work_scan(counters.residual_arc_scans)
                    && let Some(checkpoints) = checkpoints.as_deref_mut()
                {
                    checkpoints.push(RelaxedMndcWorkCheckpoint {
                        metrics: *counters,
                        focus: RelaxedMndcWorkFocus::ResidualArc(arc.id.clone()),
                    });
                }
            }
            let weight = arc
                .cost
                .checked_mul(denominator)
                .and_then(|value| value.checked_add(numerator))
                .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
            let cell = &mut matrix[arc.from.as_usize()][arc.to.as_usize()];
            let replace = cell.as_ref().is_none_or(|existing| {
                weight < existing.weight
                    || (weight == existing.weight
                        && existing
                            .residual_arc
                            .as_ref()
                            .is_some_and(|id| arc.id < *id))
            });
            if replace {
                *cell = Some(AssignmentCell {
                    weight,
                    residual_arc: Some(arc.id),
                });
            }
        }
    }
    if let (Some(counters), Some(checkpoints), Some(arc)) =
        (metrics.as_deref(), checkpoints, last_inspected_arc)
        && checkpoints.last().is_none_or(|checkpoint| {
            checkpoint.metrics.residual_arc_scans != counters.residual_arc_scans
        })
    {
        checkpoints.push(RelaxedMndcWorkCheckpoint {
            metrics: *counters,
            focus: RelaxedMndcWorkFocus::ResidualArc(arc),
        });
    }
    Ok(matrix)
}

const fn should_publish_work_scan(scan: u128) -> bool {
    scan <= RELAXED_MNDC_TRACE_SCAN_PREFIX || scan.is_multiple_of(RELAXED_MNDC_TRACE_SCAN_BLOCK)
}

fn check_scan_limit(metrics: &RelaxedMndcMetrics) -> Result<(), RelaxedMndcError> {
    if metrics
        .assignment_cell_scans
        .checked_add(metrics.residual_arc_scans)
        .is_none_or(|value| value > RELAXED_MNDC_MAX_SCANS)
    {
        return Err(RelaxedMndcError::WorkLimit);
    }
    Ok(())
}

struct AssignmentSolution {
    columns: Vec<usize>,
    public_choices: Vec<RelaxedMndcAssignmentChoice>,
    value: i128,
    left_duals: Vec<i128>,
    right_duals: Vec<i128>,
}

#[allow(clippy::too_many_lines)]
fn solve_assignment(
    matrix: &AssignmentMatrix,
    metrics: &mut RelaxedMndcMetrics,
    mut checkpoints: Option<&mut Vec<RelaxedMndcWorkCheckpoint>>,
) -> Result<AssignmentSolution, RelaxedMndcError> {
    let n = matrix.len();
    if matrix.iter().any(|row| row.len() != n) {
        return Err(RelaxedMndcError::AssignmentInvariant);
    }
    let mut left = vec![0_i128; n + 1];
    let mut right = vec![0_i128; n + 1];
    let mut row_for_column = vec![0_usize; n + 1];
    let mut predecessor_column = vec![0_usize; n + 1];
    let mut last_inspected_cell = None;
    for row in 1..=n {
        row_for_column[0] = row;
        let mut minimum = vec![None::<i128>; n + 1];
        let mut used = vec![false; n + 1];
        let mut column = 0_usize;
        loop {
            used[column] = true;
            let current_row = row_for_column[column];
            let mut delta = None::<i128>;
            let mut next_column = None::<usize>;
            for candidate_column in 1..=n {
                if used[candidate_column] {
                    continue;
                }
                metrics.assignment_cell_scans = metrics
                    .assignment_cell_scans
                    .checked_add(1)
                    .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
                last_inspected_cell = Some((current_row, candidate_column));
                check_scan_limit(metrics)?;
                if should_publish_work_scan(metrics.assignment_cell_scans)
                    && let Some(checkpoints) = checkpoints.as_deref_mut()
                {
                    checkpoints.push(RelaxedMndcWorkCheckpoint {
                        metrics: *metrics,
                        focus: RelaxedMndcWorkFocus::AssignmentCell(
                            NodeIndex::try_from_usize(current_row - 1)
                                .ok_or(RelaxedMndcError::ArithmeticOverflow)?,
                            NodeIndex::try_from_usize(candidate_column - 1)
                                .ok_or(RelaxedMndcError::ArithmeticOverflow)?,
                        ),
                    });
                }
                if let Some(cell) = &matrix[current_row - 1][candidate_column - 1] {
                    let reduced = cell
                        .weight
                        .checked_sub(left[current_row])
                        .and_then(|value| value.checked_sub(right[candidate_column]))
                        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
                    if minimum[candidate_column].is_none_or(|value| reduced < value) {
                        minimum[candidate_column] = Some(reduced);
                        predecessor_column[candidate_column] = column;
                    }
                }
                let Some(candidate) = minimum[candidate_column] else {
                    continue;
                };
                if delta.is_none_or(|value| candidate < value)
                    || (delta == Some(candidate)
                        && next_column.is_none_or(|value| candidate_column < value))
                {
                    delta = Some(candidate);
                    next_column = Some(candidate_column);
                }
            }
            let delta = delta.ok_or(RelaxedMndcError::AssignmentInvariant)?;
            for candidate_column in 0..=n {
                if used[candidate_column] {
                    let matched_row = row_for_column[candidate_column];
                    left[matched_row] = left[matched_row]
                        .checked_add(delta)
                        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
                    right[candidate_column] = right[candidate_column]
                        .checked_sub(delta)
                        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
                } else if let Some(value) = minimum[candidate_column] {
                    minimum[candidate_column] = Some(
                        value
                            .checked_sub(delta)
                            .ok_or(RelaxedMndcError::ArithmeticOverflow)?,
                    );
                }
            }
            column = next_column.ok_or(RelaxedMndcError::AssignmentInvariant)?;
            if row_for_column[column] == 0 {
                break;
            }
        }
        loop {
            let previous = predecessor_column[column];
            row_for_column[column] = row_for_column[previous];
            column = previous;
            if column == 0 {
                break;
            }
        }
        metrics.assignment_augmentations = metrics
            .assignment_augmentations
            .checked_add(1)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    }
    if let (Some(checkpoints), Some((row, column))) = (checkpoints, last_inspected_cell)
        && checkpoints.last().is_none_or(|checkpoint| {
            checkpoint.metrics.assignment_cell_scans != metrics.assignment_cell_scans
        })
    {
        checkpoints.push(RelaxedMndcWorkCheckpoint {
            metrics: *metrics,
            focus: RelaxedMndcWorkFocus::AssignmentCell(
                NodeIndex::try_from_usize(row - 1).ok_or(RelaxedMndcError::ArithmeticOverflow)?,
                NodeIndex::try_from_usize(column - 1)
                    .ok_or(RelaxedMndcError::ArithmeticOverflow)?,
            ),
        });
    }
    let mut columns = vec![usize::MAX; n];
    for (column, &row) in row_for_column.iter().enumerate().skip(1) {
        if row == 0 || columns[row - 1] != usize::MAX {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        columns[row - 1] = column - 1;
    }
    let mut value = 0_i128;
    let mut public_choices = Vec::with_capacity(n);
    for (row, &column) in columns.iter().enumerate() {
        let cell = matrix[row][column]
            .as_ref()
            .ok_or(RelaxedMndcError::AssignmentInvariant)?;
        value = value
            .checked_add(cell.weight)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
        public_choices.push(RelaxedMndcAssignmentChoice {
            column: NodeIndex::try_from_usize(column)
                .ok_or(RelaxedMndcError::ArithmeticOverflow)?,
            residual_arc: cell.residual_arc.clone(),
        });
    }
    let solution = AssignmentSolution {
        columns,
        public_choices,
        value,
        left_duals: left[1..].to_vec(),
        right_duals: right[1..].to_vec(),
    };
    verify_assignment_solution(matrix, &solution)?;
    Ok(solution)
}

fn verify_assignment_solution(
    matrix: &AssignmentMatrix,
    solution: &AssignmentSolution,
) -> Result<(), RelaxedMndcError> {
    let n = matrix.len();
    if solution.columns.len() != n
        || solution.public_choices.len() != n
        || solution.left_duals.len() != n
        || solution.right_duals.len() != n
        || solution
            .columns
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != n
    {
        return Err(RelaxedMndcError::AssignmentInvariant);
    }
    let mut primal = 0_i128;
    for (row, matrix_row) in matrix.iter().enumerate() {
        for (column, cell) in matrix_row.iter().enumerate() {
            let Some(cell) = cell else {
                continue;
            };
            let slack = cell
                .weight
                .checked_sub(solution.left_duals[row])
                .and_then(|value| value.checked_sub(solution.right_duals[column]))
                .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
            if slack < 0 || (solution.columns[row] == column && slack != 0) {
                return Err(RelaxedMndcError::AssignmentInvariant);
            }
        }
        let column = solution.columns[row];
        let cell = matrix_row[column]
            .as_ref()
            .ok_or(RelaxedMndcError::AssignmentInvariant)?;
        let public = &solution.public_choices[row];
        if public.column.as_usize() != column || public.residual_arc != cell.residual_arc {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        primal = primal
            .checked_add(cell.weight)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    }
    let dual = solution
        .left_duals
        .iter()
        .chain(&solution.right_duals)
        .try_fold(0_i128, |total, value| total.checked_add(*value))
        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    if primal != solution.value || primal != dual {
        return Err(RelaxedMndcError::AssignmentInvariant);
    }
    Ok(())
}

fn decompose_negative_family(
    solution: &AssignmentSolution,
    matrix: &AssignmentMatrix,
    metrics: &mut RelaxedMndcMetrics,
) -> Result<Vec<RelaxedMndcCycle>, RelaxedMndcError> {
    let n = solution.columns.len();
    let mut visited = vec![false; n];
    let mut family = Vec::new();
    let mut family_cost = 0_i128;
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut rows = Vec::new();
        let mut row = start;
        while !visited[row] {
            visited[row] = true;
            rows.push(row);
            row = solution.columns[row];
        }
        if row != start {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        let mut arcs = Vec::new();
        let mut transformed_cost = 0_i128;
        for &cycle_row in &rows {
            let column = solution.columns[cycle_row];
            let cell = matrix[cycle_row][column]
                .as_ref()
                .ok_or(RelaxedMndcError::AssignmentInvariant)?;
            transformed_cost = transformed_cost
                .checked_add(cell.weight)
                .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
            if let Some(id) = &cell.residual_arc {
                arcs.push(id.clone());
            } else if column != cycle_row {
                return Err(RelaxedMndcError::AssignmentInvariant);
            }
        }
        if transformed_cost > 0 {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        if transformed_cost == 0 {
            if !arcs.is_empty() {
                metrics.dropped_zero_cycles = metrics
                    .dropped_zero_cycles
                    .checked_add(1)
                    .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
            }
            continue;
        }
        if arcs.len() != rows.len() {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        family_cost = family_cost
            .checked_add(transformed_cost)
            .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
        family.push(RelaxedMndcCycle {
            arcs,
            transformed_cost,
        });
    }
    if family_cost != solution.value {
        return Err(RelaxedMndcError::AssignmentInvariant);
    }
    Ok(family)
}

fn cancel_family(
    residual: &mut ResidualState<'_>,
    family: &[RelaxedMndcCycle],
    metrics: &mut RelaxedMndcMetrics,
) -> Result<Vec<u64>, RelaxedMndcError> {
    let mut used_nodes = BTreeSet::new();
    let mut deltas = Vec::with_capacity(family.len());
    for cycle in family {
        let mut delta = u64::MAX;
        for id in &cycle.arcs {
            let arc = residual
                .arc(id)
                .ok_or(RelaxedMndcError::AssignmentInvariant)?;
            if arc.capacity == 0
                || !used_nodes.insert(arc.from)
                || (cycle.arcs.len() == 1 && arc.from != arc.to)
            {
                return Err(RelaxedMndcError::AssignmentInvariant);
            }
            delta = delta.min(arc.capacity);
        }
        if delta == 0 || delta == u64::MAX {
            return Err(RelaxedMndcError::AssignmentInvariant);
        }
        residual.augment(&cycle.arcs, delta)?;
        deltas.push(delta);
    }
    metrics.canceled_families = metrics
        .canceled_families
        .checked_add(1)
        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    metrics.canceled_cycles = metrics
        .canceled_cycles
        .checked_add(u64::try_from(family.len()).map_err(|_| RelaxedMndcError::ArithmeticOverflow)?)
        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    let arcs = family.iter().try_fold(0_u64, |total, cycle| {
        total
            .checked_add(
                u64::try_from(cycle.arcs.len())
                    .map_err(|_| RelaxedMndcError::ArithmeticOverflow)?,
            )
            .ok_or(RelaxedMndcError::ArithmeticOverflow)
    })?;
    metrics.canceled_cycle_arcs = metrics
        .canceled_cycle_arcs
        .checked_add(arcs)
        .ok_or(RelaxedMndcError::ArithmeticOverflow)?;
    Ok(deltas)
}

/// Independently checks assignment duality, node-disjoint cancellation,
/// epsilon progress, bidirectional snapshot continuity, and the exact result.
///
/// # Errors
///
/// Returns [`RelaxedMndcError::TraceVerification`] when any public boundary is
/// inconsistent with the source algorithm.
#[allow(clippy::too_many_lines)]
pub fn check_relaxed_mndc_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &RelaxedMndcTraceResult,
) -> Result<(), RelaxedMndcError> {
    if trace.base_snapshot.stage != RelaxedMndcStage::Ready
        || trace.final_snapshot.stage != RelaxedMndcStage::Optimal
        || trace.result.final_snapshot != trace.final_snapshot
        || trace.result.flows != trace.final_snapshot.flows
        || trace.events.is_empty()
    {
        return Err(RelaxedMndcError::TraceVerification);
    }
    check_feasible_snapshot(graph, required_divergence, &trace.base_snapshot)?;
    let maximum_cost = u128::from(
        graph
            .edges()
            .iter()
            .map(|edge| edge.cost().unsigned_abs())
            .max()
            .unwrap_or(0),
    );
    if trace.base_snapshot.epsilon
        != (RelaxedMndcEpsilon {
            numerator: maximum_cost,
            denominator: 1,
        })
    {
        return Err(RelaxedMndcError::TraceVerification);
    }
    let mut current = trace.base_snapshot.clone();
    let mut previous_stage = RelaxedMndcStage::Ready;
    for event in &trace.events {
        if event.before != current || event.after.epsilon.numerator != maximum_cost {
            return Err(RelaxedMndcError::TraceVerification);
        }
        if !matches!(
            event.after.stage,
            RelaxedMndcStage::InspectResidualArc | RelaxedMndcStage::InspectAssignmentCell
        ) && (event.after.active_residual_arc.is_some()
            || event.after.active_assignment_cell.is_some())
        {
            return Err(RelaxedMndcError::TraceVerification);
        }
        check_feasible_snapshot(graph, required_divergence, &event.after)?;
        match event.after.stage {
            RelaxedMndcStage::Initialize => {
                if event.catalog_id != "relaxed-most-negative-cycle.initialize"
                    || previous_stage != RelaxedMndcStage::Ready
                    || event.after.flows != event.before.flows
                    || event.after.phase != 0
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
            }
            RelaxedMndcStage::BeginPhase => {
                if event.catalog_id != "relaxed-most-negative-cycle.begin-phase"
                    || !matches!(
                        previous_stage,
                        RelaxedMndcStage::Initialize | RelaxedMndcStage::PhaseOptimal
                    )
                    || event.after.flows != event.before.flows
                    || event.after.phase != event.before.phase + 1
                    || event.after.epsilon.denominator
                        != event
                            .before
                            .epsilon
                            .denominator
                            .checked_mul(2)
                            .ok_or(RelaxedMndcError::ArithmeticOverflow)?
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
            }
            RelaxedMndcStage::InspectResidualArc | RelaxedMndcStage::InspectAssignmentCell => {
                let expected_catalog = if event.after.stage == RelaxedMndcStage::InspectResidualArc
                {
                    "relaxed-most-negative-cycle.inspect-residual-arc"
                } else {
                    "relaxed-most-negative-cycle.inspect-assignment-cell"
                };
                let focus_is_valid = match event.after.stage {
                    RelaxedMndcStage::InspectResidualArc => {
                        event.after.active_residual_arc.as_ref().is_some_and(|id| {
                            ResidualState::from_flows(graph, &event.after.flows)
                                .ok()
                                .and_then(|residual| residual.arc(id))
                                .is_some_and(|arc| arc.capacity > 0)
                        }) && event.after.active_assignment_cell.is_none()
                    }
                    RelaxedMndcStage::InspectAssignmentCell => {
                        event.after.active_residual_arc.is_none()
                            && event
                                .after
                                .active_assignment_cell
                                .is_some_and(|(row, column)| {
                                    row.as_usize() < graph.nodes().len()
                                        && column.as_usize() < graph.nodes().len()
                                })
                    }
                    _ => false,
                };
                if event.catalog_id != expected_catalog
                    || !matches!(
                        previous_stage,
                        RelaxedMndcStage::BeginPhase
                            | RelaxedMndcStage::CancelFamily
                            | RelaxedMndcStage::InspectResidualArc
                            | RelaxedMndcStage::InspectAssignmentCell
                    )
                    || event.after.flows != event.before.flows
                    || event.after.epsilon != event.before.epsilon
                    || event.after.phase != event.before.phase
                    || !focus_is_valid
                    || event.after.assignment_value.is_some()
                    || !event.after.left_duals.is_empty()
                    || !event.after.right_duals.is_empty()
                    || !event.after.assignment.is_empty()
                    || !event.after.family.is_empty()
                    || !event.deltas.is_empty()
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
                let residual_scan_delta = event
                    .after
                    .metrics
                    .residual_arc_scans
                    .checked_sub(event.before.metrics.residual_arc_scans)
                    .ok_or(RelaxedMndcError::TraceVerification)?;
                let cell_scan_delta = event
                    .after
                    .metrics
                    .assignment_cell_scans
                    .checked_sub(event.before.metrics.assignment_cell_scans)
                    .ok_or(RelaxedMndcError::TraceVerification)?;
                let valid_delta = match event.after.stage {
                    RelaxedMndcStage::InspectResidualArc => {
                        residual_scan_delta > 0
                            && residual_scan_delta <= RELAXED_MNDC_TRACE_SCAN_BLOCK
                            && cell_scan_delta == 0
                    }
                    RelaxedMndcStage::InspectAssignmentCell => {
                        cell_scan_delta > 0
                            && cell_scan_delta <= RELAXED_MNDC_TRACE_SCAN_BLOCK
                            && residual_scan_delta == 0
                    }
                    _ => false,
                };
                if !valid_delta {
                    return Err(RelaxedMndcError::TraceVerification);
                }
            }
            RelaxedMndcStage::SelectFamily | RelaxedMndcStage::PhaseOptimal => {
                let expected_catalog = if event.after.stage == RelaxedMndcStage::SelectFamily {
                    "relaxed-most-negative-cycle.select-family"
                } else {
                    "relaxed-most-negative-cycle.phase-optimal"
                };
                if event.catalog_id != expected_catalog
                    || !matches!(
                        previous_stage,
                        RelaxedMndcStage::BeginPhase
                            | RelaxedMndcStage::CancelFamily
                            | RelaxedMndcStage::InspectResidualArc
                            | RelaxedMndcStage::InspectAssignmentCell
                    )
                    || event.after.flows != event.before.flows
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
                verify_snapshot_assignment(graph, &event.after)?;
                let value = event
                    .after
                    .assignment_value
                    .ok_or(RelaxedMndcError::TraceVerification)?;
                if (event.after.stage == RelaxedMndcStage::SelectFamily
                    && (value >= 0 || event.after.family.is_empty()))
                    || (event.after.stage == RelaxedMndcStage::PhaseOptimal
                        && (value < 0 || !event.after.family.is_empty()))
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
            }
            RelaxedMndcStage::CancelFamily => {
                if event.catalog_id != "relaxed-most-negative-cycle.cancel-family"
                    || previous_stage != RelaxedMndcStage::SelectFamily
                    || event.after.epsilon != event.before.epsilon
                    || event.after.phase != event.before.phase
                    || event.after.assignment != event.before.assignment
                    || event.after.family != event.before.family
                    || event.deltas.len() != event.before.family.len()
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
                let mut replay = ResidualState::from_flows(graph, &event.before.flows)?;
                for (cycle, &delta) in event.before.family.iter().zip(&event.deltas) {
                    let bottleneck = cycle
                        .arcs
                        .iter()
                        .map(|id| replay.arc(id).map(|arc| arc.capacity))
                        .collect::<Option<Vec<_>>>()
                        .ok_or(RelaxedMndcError::TraceVerification)?
                        .into_iter()
                        .min()
                        .ok_or(RelaxedMndcError::TraceVerification)?;
                    if delta != bottleneck {
                        return Err(RelaxedMndcError::TraceVerification);
                    }
                    replay.augment(&cycle.arcs, delta)?;
                }
                if replay.flows() != event.after.flows {
                    return Err(RelaxedMndcError::TraceVerification);
                }
            }
            RelaxedMndcStage::Optimal => {
                if event.catalog_id != "relaxed-most-negative-cycle.optimal"
                    || (!matches!(previous_stage, RelaxedMndcStage::PhaseOptimal)
                        && maximum_cost != 0)
                    || event.after.flows != event.before.flows
                {
                    return Err(RelaxedMndcError::TraceVerification);
                }
                if maximum_cost > 0 {
                    let n = u128::try_from(graph.nodes().len())
                        .map_err(|_| RelaxedMndcError::ArithmeticOverflow)?;
                    if maximum_cost
                        .checked_mul(n)
                        .is_none_or(|scaled| scaled >= event.after.epsilon.denominator)
                    {
                        return Err(RelaxedMndcError::TraceVerification);
                    }
                }
            }
            RelaxedMndcStage::Ready => {
                return Err(RelaxedMndcError::TraceVerification);
            }
        }
        previous_stage = event.after.stage;
        current = event.after.clone();
    }
    if current != trace.final_snapshot {
        return Err(RelaxedMndcError::TraceVerification);
    }
    for event in trace.events.iter().rev() {
        if current != event.after {
            return Err(RelaxedMndcError::TraceVerification);
        }
        current = event.before.clone();
    }
    if current != trace.base_snapshot {
        return Err(RelaxedMndcError::TraceVerification);
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if certificate != trace.result.certificate {
        return Err(RelaxedMndcError::TraceVerification);
    }
    Ok(())
}

fn check_feasible_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &RelaxedMndcSnapshot,
) -> Result<(), RelaxedMndcError> {
    let residual = ResidualState::from_flows(graph, &snapshot.flows)?;
    if required_divergence.len() != graph.nodes().len()
        || divergences(graph, residual.flows())? != required_divergence
    {
        return Err(RelaxedMndcError::TraceVerification);
    }
    Ok(())
}

fn verify_snapshot_assignment(
    graph: &FlowNetwork,
    snapshot: &RelaxedMndcSnapshot,
) -> Result<(), RelaxedMndcError> {
    let residual = ResidualState::from_flows(graph, &snapshot.flows)?;
    let matrix = build_assignment_matrix(&residual, snapshot.epsilon, None, None)?;
    let columns = snapshot
        .assignment
        .iter()
        .map(|choice| choice.column.as_usize())
        .collect::<Vec<_>>();
    let solution = AssignmentSolution {
        columns,
        public_choices: snapshot.assignment.clone(),
        value: snapshot
            .assignment_value
            .ok_or(RelaxedMndcError::TraceVerification)?,
        left_duals: snapshot.left_duals.clone(),
        right_duals: snapshot.right_duals.clone(),
    };
    verify_assignment_solution(&matrix, &solution)?;
    let mut scratch = snapshot.metrics;
    let family = decompose_negative_family(&solution, &matrix, &mut scratch)?;
    if family != snapshot.family {
        return Err(RelaxedMndcError::TraceVerification);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(nodes: &[&str], edges: &[(&str, &str, &str, u64, i64)]) -> FlowNetwork {
        let nodes = nodes
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("valid test node id"), 0))
            .collect::<Vec<_>>();
        let edges = edges
            .iter()
            .map(|&(id, from, to, capacity, cost)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("valid test edge id"),
                from: NodeId::parse(from).expect("valid test tail id"),
                to: NodeId::parse(to).expect("valid test head id"),
                lower: 0,
                capacity,
                cost,
            })
            .collect::<Vec<_>>();
        FlowNetwork::new(nodes, edges).expect("valid test graph")
    }

    fn empty_graph(node_count: usize) -> FlowNetwork {
        let nodes = (0..node_count)
            .map(|index| {
                FlowNode::new(
                    NodeId::parse(&format!("n{index:02}")).expect("valid generated node id"),
                    0,
                )
            })
            .collect();
        FlowNetwork::new(nodes, Vec::new()).expect("valid empty graph")
    }

    fn parallel_graph(edge_count: usize) -> FlowNetwork {
        let nodes = ["a", "b"]
            .map(|id| FlowNode::new(NodeId::parse(id).expect("valid node id"), 0))
            .into_iter()
            .collect();
        let edges = (0..edge_count)
            .map(|index| UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("e{index:03}")).expect("valid generated edge id"),
                from: NodeId::parse("a").expect("valid tail id"),
                to: NodeId::parse("b").expect("valid head id"),
                lower: 0,
                capacity: 1,
                cost: 0,
            })
            .collect();
        FlowNetwork::new(nodes, edges).expect("valid parallel graph")
    }

    #[test]
    fn selects_two_node_disjoint_cycles_in_one_assignment() {
        let graph = graph(
            &["a", "b", "c", "d"],
            &[
                ("ab", "a", "b", 2, -4),
                ("ba", "b", "a", 2, 1),
                ("cd", "c", "d", 3, -3),
                ("dc", "d", "c", 3, 0),
            ],
        );
        let target = vec![0; graph.nodes().len()];
        let traced = trace_relaxed_mndc(&graph, &target).expect("relaxed MNDC trace");
        let family = traced
            .events
            .iter()
            .find(|event| event.after.stage == RelaxedMndcStage::SelectFamily)
            .expect("selected family");
        assert_eq!(family.after.family.len(), 2);
        assert_eq!(
            family
                .after
                .family
                .iter()
                .map(|cycle| cycle.arcs.len())
                .sum::<usize>(),
            4
        );
        assert_eq!(traced.result.certificate.total_cost, -15);
        assert_eq!(traced.result.metrics.canceled_families, 1);
        assert_eq!(traced.result.metrics.canceled_cycles, 2);
    }

    #[test]
    fn fast_trace_and_simple_cycle_canceling_agree() {
        let graph = graph(
            &["a", "b", "c", "d"],
            &[
                ("ab", "a", "b", 2, -4),
                ("ba", "b", "a", 2, 1),
                ("bc", "b", "c", 2, 2),
                ("cb", "c", "b", 2, -4),
                ("cd", "c", "d", 3, -3),
                ("dc", "d", "c", 3, 0),
            ],
        );
        let target = vec![0; graph.nodes().len()];
        let fast = solve_relaxed_mndc(&graph, &target).expect("fast");
        let traced = trace_relaxed_mndc(&graph, &target).expect("trace");
        let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.total_cost, oracle.certificate.total_cost);
        check_relaxed_mndc_trace(&graph, &target, &traced).expect("trace checker");
    }

    #[test]
    fn deterministic_small_cost_family_matches_cycle_canceling() {
        const ARCS: [(&str, &str, &str); 5] = [
            ("ab", "a", "b"),
            ("bc", "b", "c"),
            ("ca", "c", "a"),
            ("ac", "a", "c"),
            ("cb", "c", "b"),
        ];
        for encoded in 0_u64..3_u64.pow(u32::try_from(ARCS.len()).expect("small arc count")) {
            let mut digits = encoded;
            let edges = ARCS.map(|(id, from, to)| {
                let digit = digits % 3;
                digits /= 3;
                (id, from, to, 2, i64::try_from(digit).expect("digit") - 1)
            });
            let graph = graph(&["a", "b", "c"], &edges);
            let target = vec![0; graph.nodes().len()];
            let solved = solve_relaxed_mndc(&graph, &target)
                .unwrap_or_else(|error| panic!("relaxed MNDC case {encoded}: {error:?}"));
            let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
            assert_eq!(solved.certificate.total_cost, oracle.certificate.total_cost);
        }
    }

    #[test]
    fn zero_cost_graph_finishes_without_a_scaling_phase() {
        let graph = graph(&["a", "b"], &[("ab", "a", "b", 5, 0)]);
        let target = vec![0; graph.nodes().len()];
        let traced = trace_relaxed_mndc(&graph, &target).expect("trace");
        assert_eq!(traced.result.metrics.scaling_phases, 0);
        assert_eq!(traced.events.len(), 2);
        assert_eq!(traced.final_snapshot.stage, RelaxedMndcStage::Optimal);
    }

    #[test]
    fn checker_rejects_a_forged_assignment_dual() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 2, -2), ("ba", "b", "a", 2, 0)],
        );
        let target = vec![0; graph.nodes().len()];
        let mut traced = trace_relaxed_mndc(&graph, &target).expect("trace");
        let event = traced
            .events
            .iter_mut()
            .find(|event| event.after.stage == RelaxedMndcStage::SelectFamily)
            .expect("selection");
        event.after.left_duals[0] += 1;
        assert!(matches!(
            check_relaxed_mndc_trace(&graph, &target, &traced),
            Err(RelaxedMndcError::TraceVerification | RelaxedMndcError::AssignmentInvariant)
        ));
    }

    #[test]
    fn admission_accepts_exact_limits_and_rejects_the_next_value() {
        let maximum_nodes = empty_graph(RELAXED_MNDC_MAX_NODES);
        solve_relaxed_mndc(&maximum_nodes, &vec![0; RELAXED_MNDC_MAX_NODES])
            .expect("maximum admitted node count");
        let too_many_nodes = empty_graph(RELAXED_MNDC_MAX_NODES + 1);
        assert_eq!(
            solve_relaxed_mndc(&too_many_nodes, &vec![0; RELAXED_MNDC_MAX_NODES + 1]),
            Err(RelaxedMndcError::AdmissionLimit)
        );

        let maximum_edges = parallel_graph(RELAXED_MNDC_MAX_EDGES);
        solve_relaxed_mndc(&maximum_edges, &[0, 0]).expect("maximum admitted edge count");
        let too_many_edges = parallel_graph(RELAXED_MNDC_MAX_EDGES + 1);
        assert_eq!(
            solve_relaxed_mndc(&too_many_edges, &[0, 0]),
            Err(RelaxedMndcError::AdmissionLimit)
        );
    }

    #[test]
    fn cancels_a_negative_self_loop_to_its_capacity() {
        let graph = graph(&["a"], &[("loop", "a", "a", 2, -3)]);
        let traced = trace_relaxed_mndc(&graph, &[0]).expect("negative self-loop");
        assert_eq!(traced.result.flows, [2]);
        assert_eq!(traced.result.certificate.total_cost, -6);
        assert!(traced.events.iter().any(|event| {
            event.after.stage == RelaxedMndcStage::CancelFamily && event.deltas == [2]
        }));
    }

    #[test]
    fn preserves_lower_bounds_and_required_divergence_while_canceling() {
        let nodes = ["s", "t"]
            .map(|id| FlowNode::new(NodeId::parse(id).expect("valid node id"), 0))
            .into_iter()
            .collect();
        let edges = vec![
            UnresolvedFlowEdge {
                id: EdgeId::parse("st").expect("valid edge id"),
                from: NodeId::parse("s").expect("valid node id"),
                to: NodeId::parse("t").expect("valid node id"),
                lower: 2,
                capacity: 5,
                cost: 5,
            },
            UnresolvedFlowEdge {
                id: EdgeId::parse("ts").expect("valid edge id"),
                from: NodeId::parse("t").expect("valid node id"),
                to: NodeId::parse("s").expect("valid node id"),
                lower: 0,
                capacity: 3,
                cost: -7,
            },
        ];
        let graph = FlowNetwork::new(nodes, edges).expect("valid bounded graph");
        let traced = trace_relaxed_mndc(&graph, &[1, -1]).expect("bounded fixed flow");
        assert_eq!(traced.result.flows, [4, 3]);
        assert_eq!(traced.result.certificate.total_cost, -1);
        assert_eq!(
            divergences(&graph, &traced.result.flows).expect("divergence"),
            [1, -1]
        );
    }

    #[test]
    fn checker_rejects_forged_delta_flow_and_epsilon_boundaries() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 2, -2), ("ba", "b", "a", 2, 0)],
        );
        let target = [0, 0];
        let trace = trace_relaxed_mndc(&graph, &target).expect("trace");

        let mut forged_delta = trace.clone();
        let cancel = forged_delta
            .events
            .iter_mut()
            .find(|event| event.after.stage == RelaxedMndcStage::CancelFamily)
            .expect("cancel event");
        cancel.deltas[0] -= 1;
        assert!(check_relaxed_mndc_trace(&graph, &target, &forged_delta).is_err());

        let mut forged_flow = trace.clone();
        let cancel = forged_flow
            .events
            .iter_mut()
            .find(|event| event.after.stage == RelaxedMndcStage::CancelFamily)
            .expect("cancel event");
        cancel.after.flows[0] -= 1;
        assert!(check_relaxed_mndc_trace(&graph, &target, &forged_flow).is_err());

        let mut forged_epsilon = trace;
        let phase = forged_epsilon
            .events
            .iter_mut()
            .find(|event| event.after.stage == RelaxedMndcStage::BeginPhase)
            .expect("phase event");
        phase.after.epsilon.denominator += 1;
        assert!(check_relaxed_mndc_trace(&graph, &target, &forged_epsilon).is_err());
    }

    #[test]
    fn reports_checked_overflow_for_extreme_integral_costs() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 1, i64::MIN), ("ba", "b", "a", 1, 0)],
        );
        assert_eq!(
            solve_relaxed_mndc(&graph, &[0, 0]),
            Err(RelaxedMndcError::ArithmeticOverflow)
        );
    }
}
