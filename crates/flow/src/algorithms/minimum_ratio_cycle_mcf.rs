//! Bounded source-faithful minimum-ratio-cycle MCF progress primitive.
//!
//! Section 4 of Chen et al. defines an alpha-power potential, its gradient
//! and edge lengths, then reduces one potential-reduction IPM step to
//!
//! `min g^T delta / ||diag(l) delta||_1` subject to `B^T delta = 0`.
//!
//! The paper answers that query approximately with a sophisticated dynamic
//! data structure.  This interactive small-input realization instead checks
//! every signed simple cycle exactly, applies the source scaling
//! `eta * g^T delta = -kappa^2 / 50`, and verifies the promised potential
//! decrease.  A structurally independent DFS cycle oracle checks the selected
//! direction.  Exact feasible-flow enumeration is used only to construct and
//! audit a strict relative-interior point and the value F*.  This module is one
//! progress primitive, not an end-to-end almost-linear MCF solver.

#![allow(clippy::cast_precision_loss)]

use std::cmp::Ordering;
use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::divergences;
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

use super::alpha_power_ipm::{apply_alpha_power_source_step, evaluate_alpha_power_ipm};

/// Maximum original nodes admitted by the bounded primitive.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_NODES: usize = 6;
/// Maximum original edges admitted by the bounded primitive.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES: usize = 8;
/// Maximum original edge capacity.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_CAPACITY: u64 = 8;
/// Maximum absolute original edge cost.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_COST: u64 = 32;
/// Maximum integer assignments inspected while constructing the exact face.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_ASSIGNMENTS: u64 = 100_000;
/// Maximum ternary vectors inspected by the visible cycle oracle.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_VECTORS: u64 = 6_561;
/// Maximum independent DFS edge expansions.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_DFS_EXPANSIONS: u64 = 500_000;
/// Maximum reversible trace boundaries.
pub const MINIMUM_RATIO_CYCLE_MCF_MAX_TRACE_EVENTS: usize = 8_192;

/// Maximum real ternary-vector evaluations represented by one inspection
/// checkpoint when no candidate-cycle boundary occurs first.
const MINIMUM_RATIO_CYCLE_MCF_VECTOR_CHECKPOINT_STRIDE: u64 = 8;

const NUMERICAL_TOLERANCE: f64 = 1.0e-10;

/// Replay-safe finite IEEE-754 value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MinimumRatioCycleMcfScalar(u64);

impl MinimumRatioCycleMcfScalar {
    fn try_new(value: f64) -> Result<Self, MinimumRatioCycleMcfError> {
        if !value.is_finite() {
            return Err(MinimumRatioCycleMcfError::NumericalFailure);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    /// Recovers the finite value.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Stable decimal scene projection.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// One signed original edge in the selected circulation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfArc {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// `1` follows the stored orientation and `-1` opposes it.
    pub sign: i8,
}

/// One source or bounded-audit publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinimumRatioCycleMcfStage {
    /// Valid input before exact face construction.
    Ready,
    /// Bounded integer feasible flows and the exact optimum value were found.
    EnumerateFeasibleSet,
    /// Coordinates fixed throughout the feasible affine face were contracted.
    ContractFixedFace,
    /// The average feasible flow installed a strict relative-interior point.
    InitializeStrictInterior,
    /// The source alpha-power potential was evaluated.
    EvaluatePotential,
    /// Source gradient and length vectors were evaluated.
    MapGradientLength,
    /// A deterministic active-edge spanning forest was built.
    BuildSpanningForest,
    /// One geometrically spaced active-edge sign vector is being inspected.
    InspectVector,
    /// One signed simple-cycle ratio was evaluated.
    EvaluateCycle,
    /// The exact incumbent direction changed.
    UpdateBest,
    /// The selected vector passed incidence conservation checks.
    VerifyCycleSpace,
    /// The source-scaled circulation step was applied.
    ApplySourceStep,
    /// The source potential-decrease inequality was checked.
    MeasurePotentialDecrease,
    /// An independent DFS cycle oracle agreed with the visible oracle.
    CheckDfsOracle,
    /// The progress primitive is complete; no terminal MCF claim is made.
    Complete,
}

/// Original-node projection of forest and cycle-space state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfNodeState {
    /// Canonical original node identity.
    pub node: NodeIndex,
    /// Deterministic active-edge component ordinal.
    pub component: usize,
    /// Canonical spanning-forest parent.
    pub parent: Option<NodeIndex>,
    /// Spanning-forest depth.
    pub depth: usize,
    /// Signed incidence balance of the visible candidate.
    pub candidate_balance: i32,
    /// Whether the node belongs to the visible candidate.
    pub on_candidate: bool,
    /// Whether the node belongs to the selected cycle.
    pub on_selected: bool,
}

/// Original-edge projection of the source potential subproblem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Whether every exact feasible flow fixes this coordinate.
    pub fixed_on_face: bool,
    /// Initial strict relative-interior flow.
    pub initial_flow: MinimumRatioCycleMcfScalar,
    /// Flow after the source-scaled step.
    pub updated_flow: MinimumRatioCycleMcfScalar,
    /// Initial residual to the original lower bound.
    pub lower_slack: MinimumRatioCycleMcfScalar,
    /// Initial residual to the original upper bound.
    pub upper_slack: MinimumRatioCycleMcfScalar,
    /// Source gradient `g_e = [nabla Phi(f)]_e`.
    pub gradient: MinimumRatioCycleMcfScalar,
    /// Source length `l_e`.
    pub length: MinimumRatioCycleMcfScalar,
    /// Membership in the deterministic active-edge spanning forest.
    pub tree_edge: bool,
    /// Visible candidate sign in `{-1,0,1}`.
    pub candidate_sign: i8,
    /// Selected exact-oracle sign in `{-1,0,1}`.
    pub selected_sign: i8,
    /// Visible contribution `g_e delta_e`.
    pub numerator_contribution: MinimumRatioCycleMcfScalar,
    /// Visible contribution `l_e |delta_e|`.
    pub denominator_contribution: MinimumRatioCycleMcfScalar,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfMetrics {
    /// Integer assignments inspected while constructing the feasible face.
    pub enumerated_assignments: u64,
    /// Feasible integer flows retained.
    pub feasible_flows: u64,
    /// Active-edge scans used to build and root the forest.
    pub forest_edge_scans: u64,
    /// Active cycle-space dimension `m - n + components`.
    pub fundamental_cycles: u64,
    /// Ternary vectors inspected by the visible oracle.
    pub enumerated_vectors: u64,
    /// Connected degree-two circulations evaluated.
    pub simple_cycles: u64,
    /// Ratio comparisons.
    pub ratio_comparisons: u64,
    /// Incumbent changes.
    pub best_updates: u64,
    /// Independent DFS edge expansions.
    pub dfs_expansions: u64,
    /// Applied source progress steps, either zero or one.
    pub source_steps: u64,
    /// Independent cycle and potential checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete public state at one atomic boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfSnapshot {
    /// Current source/audit phase.
    pub stage: MinimumRatioCycleMcfStage,
    /// Original-node projections.
    pub nodes: Vec<MinimumRatioCycleMcfNodeState>,
    /// Original-edge projections.
    pub edges: Vec<MinimumRatioCycleMcfEdgeState>,
    /// Source exponent `1/(1000 log(mU))` on the contracted active face.
    pub alpha: MinimumRatioCycleMcfScalar,
    /// Exact optimum objective value used as `F*`.
    pub optimum_cost: i128,
    /// Initial fractional objective value.
    pub initial_cost: MinimumRatioCycleMcfScalar,
    /// Current fractional objective value.
    pub current_cost: MinimumRatioCycleMcfScalar,
    /// Current objective gap `c^T f - F*`.
    pub cost_gap: MinimumRatioCycleMcfScalar,
    /// Potential at the strict relative-interior point.
    pub potential_before: MinimumRatioCycleMcfScalar,
    /// Potential at the current point.
    pub current_potential: MinimumRatioCycleMcfScalar,
    /// Visible candidate ratio.
    pub candidate_ratio: Option<MinimumRatioCycleMcfScalar>,
    /// Selected exact minimum ratio.
    pub best_ratio: Option<MinimumRatioCycleMcfScalar>,
    /// Certified quality parameter used by the source update.
    pub kappa: MinimumRatioCycleMcfScalar,
    /// Source step multiplier.
    pub eta: MinimumRatioCycleMcfScalar,
    /// `||L eta delta||_1`.
    pub weighted_step_norm: MinimumRatioCycleMcfScalar,
    /// Measured potential decrease.
    pub potential_decrease: MinimumRatioCycleMcfScalar,
    /// Guaranteed decrease `kappa^2/500`.
    pub guaranteed_decrease: MinimumRatioCycleMcfScalar,
    /// Whether the initial feasible face was already cost-optimal.
    pub stationary: bool,
    /// Number of selected original edges.
    pub selected_edge_count: usize,
    /// Largest absolute visible incidence imbalance.
    pub maximum_absolute_balance: u32,
    /// Exact work counters.
    pub metrics: MinimumRatioCycleMcfMetrics,
}

/// One reversible transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfTraceEvent {
    /// Stable event identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: MinimumRatioCycleMcfSnapshot,
    /// State after the transition.
    pub after: MinimumRatioCycleMcfSnapshot,
}

/// Independently checked one-step primitive result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfResult {
    /// Selected signed simple cycle; empty for a stationary or acyclic face.
    pub cycle: Vec<MinimumRatioCycleMcfArc>,
    /// Selected exact-oracle ratio.
    pub ratio: Option<MinimumRatioCycleMcfScalar>,
    /// Initial strict relative-interior flow.
    pub initial_flow: Vec<MinimumRatioCycleMcfScalar>,
    /// Flow after at most one source-scaled step.
    pub updated_flow: Vec<MinimumRatioCycleMcfScalar>,
    /// Terminal public state.
    pub final_snapshot: MinimumRatioCycleMcfSnapshot,
    /// Exact bounded work counters.
    pub metrics: MinimumRatioCycleMcfMetrics,
}

/// Result plus the complete bounded trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimumRatioCycleMcfTraceResult {
    /// Independently checked primitive result.
    pub result: MinimumRatioCycleMcfResult,
    /// Ready boundary.
    pub base_snapshot: MinimumRatioCycleMcfSnapshot,
    /// Ordered reversible transitions.
    pub events: Vec<MinimumRatioCycleMcfTraceEvent>,
    /// Terminal boundary.
    pub final_snapshot: MinimumRatioCycleMcfSnapshot,
}

/// Admission, feasibility, source-invariant, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MinimumRatioCycleMcfError {
    /// Input exceeds the declared bounded research band.
    #[error("minimum-ratio-cycle MCF input exceeds admission limits")]
    AdmissionLimit,
    /// Requested divergence is malformed.
    #[error("minimum-ratio-cycle MCF requires a balanced divergence vector")]
    InvalidDivergence,
    /// The bounded exact feasible-set scaffold found no flow.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A finite source potential, gradient, length, ratio, or step invariant failed.
    #[error("minimum-ratio-cycle MCF numerical invariant failed")]
    NumericalFailure,
    /// The selected signed vector was not a simple circulation.
    #[error("minimum-ratio-cycle MCF cycle-space invariant failed")]
    CycleInvariant,
    /// The source step did not satisfy its feasibility or decrease guarantee.
    #[error("minimum-ratio-cycle MCF source progress guarantee failed")]
    ProgressInvariant,
    /// The independent DFS oracle selected a different cycle.
    #[error("minimum-ratio-cycle MCF exact cycle oracles disagree")]
    OracleDisagreement,
    /// Checked integer arithmetic overflowed.
    #[error("minimum-ratio-cycle MCF arithmetic overflow")]
    ArithmeticOverflow,
    /// A deterministic work budget was exhausted.
    #[error("minimum-ratio-cycle MCF work budget exhausted")]
    WorkLimit,
    /// Trace state differs from a full re-execution.
    #[error("minimum-ratio-cycle MCF trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Debug)]
struct Candidate {
    signs: Vec<i8>,
    numerator: f64,
    denominator: f64,
    ratio: f64,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.signs == other.signs
            && self.numerator.to_bits() == other.numerator.to_bits()
            && self.denominator.to_bits() == other.denominator.to_bits()
            && self.ratio.to_bits() == other.ratio.to_bits()
    }
}

#[derive(Clone, Debug)]
struct KernelState {
    fixed: Vec<bool>,
    initial_flow: Vec<f64>,
    updated_flow: Vec<f64>,
    gradient: Vec<f64>,
    length: Vec<f64>,
    tree_edges: Vec<bool>,
    nodes: Vec<MinimumRatioCycleMcfNodeState>,
    candidate: Option<Candidate>,
    inspection_signs: Option<Vec<i8>>,
    best: Option<Candidate>,
    alpha: f64,
    optimum_cost: i128,
    initial_cost: f64,
    current_cost: f64,
    cost_gap: f64,
    potential_before: f64,
    current_potential: f64,
    kappa: f64,
    eta: f64,
    weighted_step_norm: f64,
    potential_decrease: f64,
    guaranteed_decrease: f64,
    stationary: bool,
    metrics: MinimumRatioCycleMcfMetrics,
    stage: MinimumRatioCycleMcfStage,
}

struct Recorder<'a> {
    graph: &'a FlowNetwork,
    state: KernelState,
    current: MinimumRatioCycleMcfSnapshot,
    events: Vec<MinimumRatioCycleMcfTraceEvent>,
    enabled: bool,
}

impl Recorder<'_> {
    fn emit(
        &mut self,
        catalog_id: &'static str,
        stage: MinimumRatioCycleMcfStage,
    ) -> Result<(), MinimumRatioCycleMcfError> {
        self.state.stage = stage;
        self.state.metrics.state_transitions = self
            .state
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
        let after = project_snapshot(self.graph, &self.state)?;
        if self.enabled {
            if self.events.len() >= MINIMUM_RATIO_CYCLE_MCF_MAX_TRACE_EVENTS {
                return Err(MinimumRatioCycleMcfError::WorkLimit);
            }
            self.events.push(MinimumRatioCycleMcfTraceEvent {
                catalog_id,
                before: self.current.clone(),
                after: after.clone(),
            });
        }
        self.current = after;
        Ok(())
    }
}

struct InternalRun {
    result: MinimumRatioCycleMcfResult,
    base_snapshot: MinimumRatioCycleMcfSnapshot,
    events: Vec<MinimumRatioCycleMcfTraceEvent>,
}

/// Executes one exact bounded source progress primitive.
///
/// # Errors
///
/// Returns an error for an out-of-band or infeasible input, a non-finite
/// source quantity, an exhausted work guard, or a failed independent check.
pub fn solve_minimum_ratio_cycle_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<MinimumRatioCycleMcfResult, MinimumRatioCycleMcfError> {
    run_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_minimum_ratio_cycle_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumRatioCycleMcfResult, MinimumRatioCycleMcfError> {
    run_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every bounded source and exact-audit boundary.
///
/// # Errors
///
/// Returns any primitive failure or a full replay mismatch.
pub fn trace_minimum_ratio_cycle_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<MinimumRatioCycleMcfTraceResult, MinimumRatioCycleMcfError> {
    let run = run_internal(graph, required_divergence, true)?;
    let trace = MinimumRatioCycleMcfTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_minimum_ratio_cycle_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Records the bounded primitive while explicitly publishing any feasibility
/// precheck performed by the source run.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_minimum_ratio_cycle_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<MinimumRatioCycleMcfTraceResult, MinimumRatioCycleMcfError> {
    let run = run_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = MinimumRatioCycleMcfTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_minimum_ratio_cycle_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Independently reconstructs the published transition and source-step invariants.
///
/// # Errors
///
/// Returns [`MinimumRatioCycleMcfError::TraceVerification`] when any boundary,
/// event identity, metric, result, or transition link differs.
pub fn check_minimum_ratio_cycle_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &MinimumRatioCycleMcfTraceResult,
) -> Result<(), MinimumRatioCycleMcfError> {
    validate_admission(graph, required_divergence)?;
    if trace.events.is_empty()
        || trace.base_snapshot.stage != MinimumRatioCycleMcfStage::Ready
        || trace.final_snapshot.stage != MinimumRatioCycleMcfStage::Complete
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.result.metrics != trace.final_snapshot.metrics
        || trace.events.len() > MINIMUM_RATIO_CYCLE_MCF_MAX_TRACE_EVENTS
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor
            || event.catalog_id != trace_stage_catalog_id(event.after.stage)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
        {
            return Err(MinimumRatioCycleMcfError::TraceVerification);
        }
        cursor = &event.after;
    }
    if cursor != &trace.final_snapshot
        || trace.final_snapshot.metrics.state_transitions != trace.events.len() as u64
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    check_published_minimum_ratio_cycle_result(graph, required_divergence, &trace.result)
}

fn trace_stage_catalog_id(stage: MinimumRatioCycleMcfStage) -> &'static str {
    use MinimumRatioCycleMcfStage as Stage;
    match stage {
        Stage::Ready => "minimum-ratio-cycle-mcf.ready",
        Stage::EnumerateFeasibleSet => "minimum-ratio-cycle-mcf.enumerate-feasible-set",
        Stage::ContractFixedFace => "minimum-ratio-cycle-mcf.contract-fixed-face",
        Stage::InitializeStrictInterior => "minimum-ratio-cycle-mcf.initialize-strict-interior",
        Stage::EvaluatePotential => "minimum-ratio-cycle-mcf.evaluate-potential",
        Stage::MapGradientLength => "minimum-ratio-cycle-mcf.map-gradient-length",
        Stage::BuildSpanningForest => "minimum-ratio-cycle-mcf.build-spanning-forest",
        Stage::InspectVector => "minimum-ratio-cycle-mcf.inspect-vector-checkpoint",
        Stage::EvaluateCycle => "minimum-ratio-cycle-mcf.evaluate-cycle",
        Stage::UpdateBest => "minimum-ratio-cycle-mcf.update-best",
        Stage::VerifyCycleSpace => "minimum-ratio-cycle-mcf.verify-cycle-space",
        Stage::ApplySourceStep => "minimum-ratio-cycle-mcf.apply-source-step",
        Stage::MeasurePotentialDecrease => "minimum-ratio-cycle-mcf.measure-potential-decrease",
        Stage::CheckDfsOracle => "minimum-ratio-cycle-mcf.check-dfs-oracle",
        Stage::Complete => "minimum-ratio-cycle-mcf.complete-primitive",
    }
}

fn trace_close(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1.0e-8 * (1.0 + left.abs().max(right.abs()))
}

#[allow(clippy::too_many_lines)]
fn check_published_minimum_ratio_cycle_result(
    graph: &FlowNetwork,
    required: &[i128],
    result: &MinimumRatioCycleMcfResult,
) -> Result<(), MinimumRatioCycleMcfError> {
    let snapshot = &result.final_snapshot;
    if result.initial_flow.len() != graph.edges().len()
        || result.updated_flow.len() != graph.edges().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.nodes.len() != graph.nodes().len()
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let initial = result
        .initial_flow
        .iter()
        .map(|value| value.get())
        .collect::<Vec<_>>();
    let updated = result
        .updated_flow
        .iter()
        .map(|value| value.get())
        .collect::<Vec<_>>();
    let mut initial_balance = vec![0.0_f64; graph.nodes().len()];
    let mut updated_balance = vec![0.0_f64; graph.nodes().len()];
    let mut selected_balance = vec![0_i32; graph.nodes().len()];
    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;
    let mut cycle = Vec::new();
    for (index, (edge, state)) in graph.edges().iter().zip(&snapshot.edges).enumerate() {
        let lower = edge.lower() as f64;
        let upper = edge.capacity() as f64;
        if state.edge != *edge.id()
            || !trace_close(state.initial_flow.get(), initial[index])
            || !trace_close(state.updated_flow.get(), updated[index])
            || initial[index] < lower - NUMERICAL_TOLERANCE
            || initial[index] > upper + NUMERICAL_TOLERANCE
            || updated[index] < lower - NUMERICAL_TOLERANCE
            || updated[index] > upper + NUMERICAL_TOLERANCE
            || !trace_close(state.lower_slack.get(), initial[index] - lower)
            || !trace_close(state.upper_slack.get(), upper - initial[index])
            || !matches!(state.selected_sign, -1..=1)
        {
            return Err(MinimumRatioCycleMcfError::TraceVerification);
        }
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        initial_balance[from] += initial[index];
        initial_balance[to] -= initial[index];
        updated_balance[from] += updated[index];
        updated_balance[to] -= updated[index];
        if state.selected_sign != 0 {
            cycle.push(MinimumRatioCycleMcfArc {
                edge: edge.id().clone(),
                sign: state.selected_sign,
            });
            selected_balance[from] += i32::from(state.selected_sign);
            selected_balance[to] -= i32::from(state.selected_sign);
            numerator += state.gradient.get() * f64::from(state.selected_sign);
            denominator += state.length.get();
        }
    }
    if initial_balance
        .iter()
        .zip(required)
        .any(|(actual, expected)| !trace_close(*actual, *expected as f64))
        || updated_balance
            .iter()
            .zip(required)
            .any(|(actual, expected)| !trace_close(*actual, *expected as f64))
        || selected_balance.iter().any(|balance| *balance != 0)
        || cycle != result.cycle
        || cycle.len() != snapshot.selected_edge_count
        || snapshot.maximum_absolute_balance != 0
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let initial_cost = graph
        .edges()
        .iter()
        .zip(&initial)
        .map(|(edge, flow)| edge.cost() as f64 * flow)
        .sum::<f64>();
    let current_cost = graph
        .edges()
        .iter()
        .zip(&updated)
        .map(|(edge, flow)| edge.cost() as f64 * flow)
        .sum::<f64>();
    if !trace_close(snapshot.initial_cost.get(), initial_cost)
        || !trace_close(snapshot.current_cost.get(), current_cost)
        || !trace_close(
            snapshot.cost_gap.get(),
            current_cost - snapshot.optimum_cost as f64,
        )
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    if snapshot.stationary {
        if !cycle.is_empty()
            || result.ratio.is_some()
            || snapshot.best_ratio.is_some()
            || snapshot.metrics.source_steps != 0
            || initial
                .iter()
                .zip(&updated)
                .any(|(left, right)| !trace_close(*left, *right))
        {
            return Err(MinimumRatioCycleMcfError::TraceVerification);
        }
        return Ok(());
    }
    if denominator <= 0.0 || numerator >= 0.0 {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let ratio = numerator / denominator;
    if result
        .ratio
        .is_none_or(|value| !trace_close(value.get(), ratio))
        || snapshot
            .best_ratio
            .is_none_or(|value| !trace_close(value.get(), ratio))
        || snapshot.metrics.source_steps != 1
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let active = snapshot
        .edges
        .iter()
        .map(|state| !state.fixed_on_face)
        .collect::<Vec<_>>();
    let before = evaluate_alpha_power_ipm(graph, &active, &initial, snapshot.optimum_cost as f64)
        .map_err(|_| MinimumRatioCycleMcfError::TraceVerification)?;
    let after = evaluate_alpha_power_ipm(graph, &active, &updated, snapshot.optimum_cost as f64)
        .map_err(|_| MinimumRatioCycleMcfError::TraceVerification)?;
    if !trace_close(snapshot.alpha.get(), before.alpha)
        || !trace_close(snapshot.potential_before.get(), before.potential)
        || !trace_close(snapshot.current_potential.get(), after.potential)
        || snapshot
            .edges
            .iter()
            .zip(before.gradients.iter().zip(&before.lengths))
            .any(|(state, (gradient, length))| {
                !trace_close(state.gradient.get(), *gradient)
                    || !trace_close(state.length.get(), *length)
            })
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    let kappa = (-ratio).min(0.99);
    let eta = -kappa * kappa / (50.0 * numerator);
    let weighted_norm = eta.abs() * denominator;
    let guaranteed = kappa * kappa / 500.0;
    for (index, state) in snapshot.edges.iter().enumerate() {
        let expected = initial[index] + eta * f64::from(state.selected_sign);
        if !trace_close(updated[index], expected) {
            return Err(MinimumRatioCycleMcfError::TraceVerification);
        }
    }
    let decrease = before.potential - after.potential;
    if !trace_close(snapshot.kappa.get(), kappa)
        || !trace_close(snapshot.eta.get(), eta)
        || !trace_close(snapshot.weighted_step_norm.get(), weighted_norm)
        || !trace_close(snapshot.guaranteed_decrease.get(), guaranteed)
        || !trace_close(snapshot.potential_decrease.get(), decrease)
        || decrease + 1.0e-8 * before.potential.abs().max(1.0) < guaranteed
    {
        return Err(MinimumRatioCycleMcfError::TraceVerification);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    required: &[i128],
    record_events: bool,
) -> Result<InternalRun, MinimumRatioCycleMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(graph, required, record_events, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required: &[i128],
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, MinimumRatioCycleMcfError> {
    validate_admission(graph, required)?;
    let mut metrics = MinimumRatioCycleMcfMetrics::default();
    let feasible = enumerate_feasible(graph, required, &mut metrics, feasibility)?;
    let edge_count = graph.edges().len();
    let mut fixed = vec![true; edge_count];
    let first = feasible
        .first()
        .ok_or(MinimumRatioCycleMcfError::NumericalFailure)?;
    for index in 0..edge_count {
        fixed[index] = feasible.iter().all(|flow| flow[index] == first[index]);
    }
    let count = feasible.len() as f64;
    let initial_flow = (0..edge_count)
        .map(|index| feasible.iter().map(|flow| flow[index] as f64).sum::<f64>() / count)
        .collect::<Vec<_>>();
    let optimum_cost = feasible
        .iter()
        .map(|flow| exact_cost(graph, flow))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(MinimumRatioCycleMcfError::NumericalFailure)?;
    let active_edges = fixed.iter().filter(|&&value| !value).count();
    let u = graph
        .edges()
        .iter()
        .map(FlowEdge::capacity)
        .max()
        .unwrap_or(1)
        .max(2);
    let scale = (active_edges.max(1) as f64 * u as f64).max(2.0);
    let alpha = 1.0 / (1_000.0 * scale.ln());
    let initial_cost = fractional_cost(graph, &initial_flow)?;
    let gap = initial_cost - optimum_cost as f64;
    if gap < -NUMERICAL_TOLERANCE {
        return Err(MinimumRatioCycleMcfError::NumericalFailure);
    }
    let stationary = gap <= NUMERICAL_TOLERANCE;
    let nodes = base_nodes(graph);
    let state = KernelState {
        fixed,
        updated_flow: initial_flow.clone(),
        initial_flow,
        gradient: vec![0.0; edge_count],
        length: vec![0.0; edge_count],
        tree_edges: vec![false; edge_count],
        nodes,
        candidate: None,
        inspection_signs: None,
        best: None,
        alpha,
        optimum_cost,
        initial_cost,
        current_cost: initial_cost,
        cost_gap: gap.max(0.0),
        potential_before: 0.0,
        current_potential: 0.0,
        kappa: 0.0,
        eta: 0.0,
        weighted_step_norm: 0.0,
        potential_decrease: 0.0,
        guaranteed_decrease: 0.0,
        stationary,
        metrics,
        stage: MinimumRatioCycleMcfStage::Ready,
    };
    // Feasible-face enumeration is real bounded work performed before the
    // first source mutation. Keep the ready snapshot at zero work so the first
    // recorded boundary can publish every assignment evaluation instead of
    // hiding the entire oracle inside initialization.
    let mut ready_state = state.clone();
    ready_state.metrics.enumerated_assignments = 0;
    let base_snapshot = project_snapshot(graph, &ready_state)?;
    let mut recorder = Recorder {
        graph,
        state,
        current: base_snapshot.clone(),
        events: Vec::new(),
        enabled: record_events,
    };
    recorder.emit(
        "minimum-ratio-cycle-mcf.enumerate-feasible-set",
        MinimumRatioCycleMcfStage::EnumerateFeasibleSet,
    )?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.contract-fixed-face",
        MinimumRatioCycleMcfStage::ContractFixedFace,
    )?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.initialize-strict-interior",
        MinimumRatioCycleMcfStage::InitializeStrictInterior,
    )?;

    if stationary || active_edges == 0 {
        recorder.state.stationary = true;
        recorder.emit(
            "minimum-ratio-cycle-mcf.evaluate-potential",
            MinimumRatioCycleMcfStage::EvaluatePotential,
        )?;
        recorder.emit(
            "minimum-ratio-cycle-mcf.complete-primitive",
            MinimumRatioCycleMcfStage::Complete,
        )?;
        return finish_run(recorder, base_snapshot);
    }

    let potential = potential(graph, &recorder.state, &recorder.state.initial_flow)?;
    recorder.state.potential_before = potential;
    recorder.state.current_potential = potential;
    recorder.emit(
        "minimum-ratio-cycle-mcf.evaluate-potential",
        MinimumRatioCycleMcfStage::EvaluatePotential,
    )?;
    map_gradient_length(graph, &mut recorder.state)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.map-gradient-length",
        MinimumRatioCycleMcfStage::MapGradientLength,
    )?;
    build_spanning_forest(graph, &mut recorder.state)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.build-spanning-forest",
        MinimumRatioCycleMcfStage::BuildSpanningForest,
    )?;
    enumerate_visible_cycles(&mut recorder)?;
    recorder.state.candidate = None;
    recorder.state.metrics.certificate_checks = recorder
        .state
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.verify-cycle-space",
        MinimumRatioCycleMcfStage::VerifyCycleSpace,
    )?;
    apply_source_step(&mut recorder)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.apply-source-step",
        MinimumRatioCycleMcfStage::ApplySourceStep,
    )?;
    check_potential_decrease(&mut recorder)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.measure-potential-decrease",
        MinimumRatioCycleMcfStage::MeasurePotentialDecrease,
    )?;
    let mut expansions = 0_u64;
    let dfs_best = enumerate_best_with_dfs(graph, &recorder.state, &mut expansions)?;
    if dfs_best != recorder.state.best {
        return Err(MinimumRatioCycleMcfError::OracleDisagreement);
    }
    recorder.state.metrics.dfs_expansions = expansions;
    recorder.state.metrics.certificate_checks = recorder
        .state
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.check-dfs-oracle",
        MinimumRatioCycleMcfStage::CheckDfsOracle,
    )?;
    recorder.emit(
        "minimum-ratio-cycle-mcf.complete-primitive",
        MinimumRatioCycleMcfStage::Complete,
    )?;
    finish_run(recorder, base_snapshot)
}

fn finish_run(
    recorder: Recorder<'_>,
    base_snapshot: MinimumRatioCycleMcfSnapshot,
) -> Result<InternalRun, MinimumRatioCycleMcfError> {
    let cycle = recorder
        .state
        .best
        .as_ref()
        .map(|candidate| cycle_arcs(recorder.graph, &candidate.signs))
        .unwrap_or_default();
    let ratio = recorder
        .state
        .best
        .as_ref()
        .map(|candidate| scalar(candidate.ratio))
        .transpose()?;
    let initial_flow = scalars(&recorder.state.initial_flow)?;
    let updated_flow = scalars(&recorder.state.updated_flow)?;
    let metrics = recorder.state.metrics;
    let result = MinimumRatioCycleMcfResult {
        cycle,
        ratio,
        initial_flow,
        updated_flow,
        final_snapshot: recorder.current,
        metrics,
    };
    validate_terminal(recorder.graph, &recorder.state, &result)?;
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn validate_admission(
    graph: &FlowNetwork,
    required: &[i128],
) -> Result<(), MinimumRatioCycleMcfError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > MINIMUM_RATIO_CYCLE_MCF_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > MINIMUM_RATIO_CYCLE_MCF_MAX_EDGES
        || required.len() != graph.nodes().len()
        || graph.edges().iter().any(|edge| {
            edge.capacity() > MINIMUM_RATIO_CYCLE_MCF_MAX_CAPACITY
                || edge.cost().unsigned_abs() > MINIMUM_RATIO_CYCLE_MCF_MAX_COST
                || edge.from() == edge.to()
        })
    {
        return Err(MinimumRatioCycleMcfError::AdmissionLimit);
    }
    if required
        .iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(*value))
        != Some(0)
    {
        return Err(MinimumRatioCycleMcfError::InvalidDivergence);
    }
    let assignments = graph
        .edges()
        .iter()
        .try_fold(1_u64, |count, edge| {
            count.checked_mul(edge.capacity() - edge.lower() + 1)
        })
        .ok_or(MinimumRatioCycleMcfError::AdmissionLimit)?;
    if assignments > MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_ASSIGNMENTS {
        return Err(MinimumRatioCycleMcfError::AdmissionLimit);
    }
    Ok(())
}

fn enumerate_feasible(
    graph: &FlowNetwork,
    required: &[i128],
    metrics: &mut MinimumRatioCycleMcfMetrics,
    feasibility: &mut FeasibilityExecution,
) -> Result<Vec<Vec<u64>>, MinimumRatioCycleMcfError> {
    let mut feasible = Vec::new();
    let mut current = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    enumerate_coordinate(graph, required, 0, &mut current, &mut feasible, metrics)?;
    if feasible.is_empty() {
        feasibility.find_feasible_flow(graph, required, FeasibilityUse::PrecheckOnly)?;
        return Err(MinimumRatioCycleMcfError::NumericalFailure);
    }
    metrics.feasible_flows =
        u64::try_from(feasible.len()).map_err(|_| MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    Ok(feasible)
}

fn enumerate_coordinate(
    graph: &FlowNetwork,
    required: &[i128],
    index: usize,
    current: &mut [u64],
    feasible: &mut Vec<Vec<u64>>,
    metrics: &mut MinimumRatioCycleMcfMetrics,
) -> Result<(), MinimumRatioCycleMcfError> {
    if index == graph.edges().len() {
        metrics.enumerated_assignments = metrics
            .enumerated_assignments
            .checked_add(1)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
        if divergences(graph, current).map_err(|_| MinimumRatioCycleMcfError::CycleInvariant)?
            == required
        {
            feasible.push(current.to_vec());
        }
        return Ok(());
    }
    let edge = &graph.edges()[index];
    for value in edge.lower()..=edge.capacity() {
        current[index] = value;
        enumerate_coordinate(graph, required, index + 1, current, feasible, metrics)?;
    }
    Ok(())
}

fn exact_cost(graph: &FlowNetwork, flow: &[u64]) -> Result<i128, MinimumRatioCycleMcfError> {
    graph
        .edges()
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (edge, &value)| {
            i128::from(value)
                .checked_mul(i128::from(edge.cost()))
                .and_then(|term| sum.checked_add(term))
                .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)
        })
}

fn fractional_cost(graph: &FlowNetwork, flow: &[f64]) -> Result<f64, MinimumRatioCycleMcfError> {
    let value = graph
        .edges()
        .iter()
        .zip(flow)
        .map(|(edge, &amount)| edge.cost() as f64 * amount)
        .sum::<f64>();
    finite(value)
}

fn potential(
    graph: &FlowNetwork,
    state: &KernelState,
    flow: &[f64],
) -> Result<f64, MinimumRatioCycleMcfError> {
    let active = state.fixed.iter().map(|&fixed| !fixed).collect::<Vec<_>>();
    let evaluation = evaluate_alpha_power_ipm(graph, &active, flow, state.optimum_cost as f64)
        .map_err(|_| MinimumRatioCycleMcfError::NumericalFailure)?;
    if (evaluation.alpha - state.alpha).abs() > NUMERICAL_TOLERANCE {
        return Err(MinimumRatioCycleMcfError::NumericalFailure);
    }
    finite(evaluation.potential)
}

fn map_gradient_length(
    graph: &FlowNetwork,
    state: &mut KernelState,
) -> Result<(), MinimumRatioCycleMcfError> {
    let active = state.fixed.iter().map(|&fixed| !fixed).collect::<Vec<_>>();
    let evaluation = evaluate_alpha_power_ipm(
        graph,
        &active,
        &state.initial_flow,
        state.optimum_cost as f64,
    )
    .map_err(|_| MinimumRatioCycleMcfError::NumericalFailure)?;
    if (evaluation.alpha - state.alpha).abs() > NUMERICAL_TOLERANCE
        || (evaluation.objective - state.initial_cost).abs() > NUMERICAL_TOLERANCE
    {
        return Err(MinimumRatioCycleMcfError::NumericalFailure);
    }
    state.gradient = evaluation.gradients;
    state.length = evaluation.lengths;
    Ok(())
}

fn build_spanning_forest(
    graph: &FlowNetwork,
    state: &mut KernelState,
) -> Result<(), MinimumRatioCycleMcfError> {
    let node_count = graph.nodes().len();
    let mut union = UnionFind::new(node_count);
    let mut scans = 0_u64;
    for (index, edge) in graph.edges().iter().enumerate() {
        if state.fixed[index] {
            continue;
        }
        scans = checked_increment(scans)?;
        if union.join(edge.from().as_usize(), edge.to().as_usize()) {
            state.tree_edges[index] = true;
        }
    }
    let mut adjacency = vec![Vec::<usize>::new(); node_count];
    for (index, edge) in graph.edges().iter().enumerate() {
        if !state.tree_edges[index] {
            continue;
        }
        adjacency[edge.from().as_usize()].push(edge.to().as_usize());
        adjacency[edge.to().as_usize()].push(edge.from().as_usize());
        scans = scans
            .checked_add(2)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
    }
    let mut seen = vec![false; node_count];
    let mut component = 0_usize;
    for root in 0..node_count {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        state.nodes[root].component = component;
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &next in &adjacency[node] {
                scans = checked_increment(scans)?;
                if !seen[next] {
                    seen[next] = true;
                    state.nodes[next].component = component;
                    state.nodes[next].parent = Some(
                        NodeIndex::try_from_usize(node)
                            .expect("admitted node index fits model index"),
                    );
                    state.nodes[next].depth = state.nodes[node]
                        .depth
                        .checked_add(1)
                        .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
                    queue.push_back(next);
                }
            }
        }
        component = component
            .checked_add(1)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    }
    let active = state.fixed.iter().filter(|&&value| !value).count();
    let dimension = active
        .checked_add(component)
        .and_then(|value| value.checked_sub(node_count))
        .ok_or(MinimumRatioCycleMcfError::CycleInvariant)?;
    state.metrics.forest_edge_scans = scans;
    state.metrics.fundamental_cycles =
        u64::try_from(dimension).map_err(|_| MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    Ok(())
}

fn enumerate_visible_cycles(recorder: &mut Recorder<'_>) -> Result<(), MinimumRatioCycleMcfError> {
    let active = recorder.state.fixed.iter().filter(|&&value| !value).count();
    let vector_count = checked_pow3(active)?;
    if vector_count > MINIMUM_RATIO_CYCLE_MCF_MAX_ENUMERATED_VECTORS {
        return Err(MinimumRatioCycleMcfError::WorkLimit);
    }
    let active_indices = recorder
        .state
        .fixed
        .iter()
        .enumerate()
        .filter_map(|(index, &fixed)| (!fixed).then_some(index))
        .collect::<Vec<_>>();
    for code in 1..vector_count {
        recorder.state.metrics.enumerated_vectors = code;
        let active_signs = decode_ternary(code, active);
        let mut signs = vec![0_i8; recorder.graph.edges().len()];
        for (&index, &sign) in active_indices.iter().zip(&active_signs) {
            signs[index] = sign;
        }
        if code <= 16
            || code.is_multiple_of(MINIMUM_RATIO_CYCLE_MCF_VECTOR_CHECKPOINT_STRIDE)
            || code + 1 == vector_count
        {
            recorder.state.candidate = None;
            recorder.state.inspection_signs = Some(signs.clone());
            recorder.emit(
                "minimum-ratio-cycle-mcf.inspect-vector-checkpoint",
                MinimumRatioCycleMcfStage::InspectVector,
            )?;
        }
        if active_signs.iter().find(|&&sign| sign != 0) != Some(&1) {
            continue;
        }
        let Some(candidate) = candidate_from_signs(recorder.graph, &recorder.state, signs)? else {
            continue;
        };
        recorder.state.metrics.simple_cycles =
            checked_increment(recorder.state.metrics.simple_cycles)?;
        recorder.state.inspection_signs = None;
        recorder.state.candidate = Some(candidate.clone());
        recorder.emit(
            "minimum-ratio-cycle-mcf.evaluate-cycle",
            MinimumRatioCycleMcfStage::EvaluateCycle,
        )?;
        let replace = match &recorder.state.best {
            None => true,
            Some(current) => {
                recorder.state.metrics.ratio_comparisons =
                    checked_increment(recorder.state.metrics.ratio_comparisons)?;
                candidate_order(&candidate, current) == Ordering::Less
            }
        };
        if replace {
            recorder.state.metrics.best_updates =
                checked_increment(recorder.state.metrics.best_updates)?;
            recorder.state.best = Some(candidate);
            recorder.emit(
                "minimum-ratio-cycle-mcf.update-best",
                MinimumRatioCycleMcfStage::UpdateBest,
            )?;
        }
    }
    recorder.state.inspection_signs = None;
    recorder.state.metrics.enumerated_vectors = vector_count.saturating_sub(1);
    Ok(())
}

fn checked_pow3(exponent: usize) -> Result<u64, MinimumRatioCycleMcfError> {
    (0..exponent).try_fold(1_u64, |value, _| {
        value
            .checked_mul(3)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)
    })
}

fn decode_ternary(mut code: u64, count: usize) -> Vec<i8> {
    let mut signs = vec![0_i8; count];
    for sign in &mut signs {
        *sign = match code % 3 {
            0 => 0,
            1 => 1,
            _ => -1,
        };
        code /= 3;
    }
    signs
}

fn candidate_from_signs(
    graph: &FlowNetwork,
    state: &KernelState,
    mut signs: Vec<i8>,
) -> Result<Option<Candidate>, MinimumRatioCycleMcfError> {
    if signs.len() != graph.edges().len() || signs.iter().all(|&sign| sign == 0) {
        return Ok(None);
    }
    let node_count = graph.nodes().len();
    let mut balance = vec![0_i32; node_count];
    let mut degree = vec![0_u8; node_count];
    let mut adjacency = vec![Vec::<usize>::new(); node_count];
    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;
    for (index, (&sign, edge)) in signs.iter().zip(graph.edges()).enumerate() {
        if sign == 0 {
            continue;
        }
        if state.fixed[index] || !matches!(sign, -1 | 1) {
            return Err(MinimumRatioCycleMcfError::CycleInvariant);
        }
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        balance[from] -= i32::from(sign);
        balance[to] += i32::from(sign);
        degree[from] = degree[from]
            .checked_add(1)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
        degree[to] = degree[to]
            .checked_add(1)
            .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
        adjacency[from].push(to);
        adjacency[to].push(from);
        numerator += state.gradient[index] * f64::from(sign);
        denominator += state.length[index];
    }
    if balance.iter().any(|&value| value != 0)
        || degree.iter().any(|&value| value != 0 && value != 2)
    {
        return Ok(None);
    }
    let Some(start) = degree.iter().position(|&value| value != 0) else {
        return Ok(None);
    };
    let mut seen = vec![false; node_count];
    seen[start] = true;
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for &next in &adjacency[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    if degree
        .iter()
        .enumerate()
        .any(|(node, &value)| value != 0 && !seen[node])
    {
        return Ok(None);
    }
    if !(denominator > 0.0 && numerator.is_finite()) {
        return Err(MinimumRatioCycleMcfError::NumericalFailure);
    }
    if numerator > 0.0 {
        for sign in &mut signs {
            *sign = -*sign;
        }
        numerator = -numerator;
    } else if numerator == 0.0 {
        let reversed = signs.iter().map(|sign| -*sign).collect::<Vec<_>>();
        if reversed < signs {
            signs = reversed;
        }
    }
    let ratio = finite(numerator / denominator)?;
    Ok(Some(Candidate {
        signs,
        numerator,
        denominator,
        ratio,
    }))
}

fn candidate_order(left: &Candidate, right: &Candidate) -> Ordering {
    left.ratio
        .total_cmp(&right.ratio)
        .then_with(|| left.signs.cmp(&right.signs))
}

fn apply_source_step(recorder: &mut Recorder<'_>) -> Result<(), MinimumRatioCycleMcfError> {
    let Some(best) = recorder.state.best.clone() else {
        recorder.state.stationary = true;
        return Ok(());
    };
    let quality = -best.ratio;
    if quality <= NUMERICAL_TOLERANCE {
        recorder.state.stationary = true;
        return Ok(());
    }
    let active = recorder
        .state
        .fixed
        .iter()
        .map(|&fixed| !fixed)
        .collect::<Vec<_>>();
    let direction = best
        .signs
        .iter()
        .map(|&sign| f64::from(sign))
        .collect::<Vec<_>>();
    let step = apply_alpha_power_source_step(
        recorder.graph,
        &active,
        &recorder.state.initial_flow,
        recorder.state.optimum_cost as f64,
        &direction,
    )
    .map_err(|_| MinimumRatioCycleMcfError::ProgressInvariant)?;
    let comparison_scale = quality.abs().max(1.0);
    if (step.ratio - quality).abs() > NUMERICAL_TOLERANCE * comparison_scale
        || (step.gradient_dot - best.numerator).abs()
            > NUMERICAL_TOLERANCE * best.numerator.abs().max(1.0)
        || (step.weighted_length - best.denominator).abs()
            > NUMERICAL_TOLERANCE * best.denominator.abs().max(1.0)
    {
        return Err(MinimumRatioCycleMcfError::ProgressInvariant);
    }
    recorder.state.updated_flow = step.updated_flow;
    recorder.state.kappa = step.kappa;
    recorder.state.eta = step.multiplier;
    recorder.state.weighted_step_norm = step.weighted_step_norm;
    recorder.state.guaranteed_decrease = step.guaranteed_decrease;
    recorder.state.current_cost = step.after.objective;
    recorder.state.cost_gap = step.after.gap;
    recorder.state.metrics.source_steps = 1;
    Ok(())
}

fn check_potential_decrease(recorder: &mut Recorder<'_>) -> Result<(), MinimumRatioCycleMcfError> {
    if recorder.state.stationary {
        return Ok(());
    }
    let after = potential(
        recorder.graph,
        &recorder.state,
        &recorder.state.updated_flow,
    )?;
    let decrease = recorder.state.potential_before - after;
    recorder.state.current_potential = after;
    recorder.state.potential_decrease = decrease;
    recorder.state.metrics.certificate_checks = recorder
        .state
        .metrics
        .certificate_checks
        .checked_add(1)
        .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)?;
    let tolerance = 1.0e-8 * recorder.state.potential_before.abs().max(1.0);
    if decrease + tolerance < recorder.state.guaranteed_decrease
        || recorder.state.weighted_step_norm > recorder.state.kappa / 25.0 + tolerance
        || recorder.state.cost_gap <= 0.0
    {
        return Err(MinimumRatioCycleMcfError::ProgressInvariant);
    }
    Ok(())
}

fn enumerate_best_with_dfs(
    graph: &FlowNetwork,
    state: &KernelState,
    expansions: &mut u64,
) -> Result<Option<Candidate>, MinimumRatioCycleMcfError> {
    let mut cycles = BTreeSet::<Vec<i8>>::new();
    for start in 0..graph.nodes().len() {
        let mut visited = vec![false; graph.nodes().len()];
        visited[start] = true;
        let mut used_edges = vec![false; graph.edges().len()];
        let mut signs = vec![0_i8; graph.edges().len()];
        dfs_cycles(
            graph,
            state,
            start,
            start,
            &mut visited,
            &mut used_edges,
            &mut signs,
            0,
            &mut cycles,
            expansions,
        )?;
    }
    let mut best = None;
    for signs in cycles {
        let candidate = candidate_from_signs(graph, state, signs)?
            .ok_or(MinimumRatioCycleMcfError::CycleInvariant)?;
        if best
            .as_ref()
            .is_none_or(|current| candidate_order(&candidate, current) == Ordering::Less)
        {
            best = Some(candidate);
        }
    }
    Ok(best)
}

#[allow(clippy::too_many_arguments)]
fn dfs_cycles(
    graph: &FlowNetwork,
    state: &KernelState,
    start: usize,
    current: usize,
    visited: &mut [bool],
    used_edges: &mut [bool],
    signs: &mut [i8],
    path_edges: usize,
    cycles: &mut BTreeSet<Vec<i8>>,
    expansions: &mut u64,
) -> Result<(), MinimumRatioCycleMcfError> {
    for (edge_index, edge) in graph.edges().iter().enumerate() {
        if state.fixed[edge_index] || used_edges[edge_index] {
            continue;
        }
        let (next, sign) = if edge.from().as_usize() == current {
            (edge.to().as_usize(), 1_i8)
        } else if edge.to().as_usize() == current {
            (edge.from().as_usize(), -1_i8)
        } else {
            continue;
        };
        *expansions = checked_increment(*expansions)?;
        if *expansions > MINIMUM_RATIO_CYCLE_MCF_MAX_DFS_EXPANSIONS {
            return Err(MinimumRatioCycleMcfError::WorkLimit);
        }
        if next == start && path_edges >= 1 {
            signs[edge_index] = sign;
            let canonical = canonical_sign_vector(signs);
            cycles.insert(canonical);
            signs[edge_index] = 0;
            continue;
        }
        if visited[next] {
            continue;
        }
        used_edges[edge_index] = true;
        signs[edge_index] = sign;
        visited[next] = true;
        dfs_cycles(
            graph,
            state,
            start,
            next,
            visited,
            used_edges,
            signs,
            path_edges + 1,
            cycles,
            expansions,
        )?;
        visited[next] = false;
        signs[edge_index] = 0;
        used_edges[edge_index] = false;
    }
    Ok(())
}

fn canonical_sign_vector(signs: &[i8]) -> Vec<i8> {
    let forward = signs.to_vec();
    let reverse = signs.iter().map(|sign| -*sign).collect::<Vec<_>>();
    forward.min(reverse)
}

fn cycle_arcs(graph: &FlowNetwork, signs: &[i8]) -> Vec<MinimumRatioCycleMcfArc> {
    graph
        .edges()
        .iter()
        .zip(signs)
        .filter(|(_, sign)| **sign != 0)
        .map(|(edge, &sign)| MinimumRatioCycleMcfArc {
            edge: edge.id().clone(),
            sign,
        })
        .collect()
}

fn project_snapshot(
    graph: &FlowNetwork,
    state: &KernelState,
) -> Result<MinimumRatioCycleMcfSnapshot, MinimumRatioCycleMcfError> {
    let candidate_signs = state.inspection_signs.clone().unwrap_or_else(|| {
        state.candidate.as_ref().map_or_else(
            || vec![0; graph.edges().len()],
            |candidate| candidate.signs.clone(),
        )
    });
    let selected_signs = state.best.as_ref().map_or_else(
        || vec![0; graph.edges().len()],
        |candidate| candidate.signs.clone(),
    );
    let mut balances = vec![0_i32; graph.nodes().len()];
    let edges = graph
        .edges()
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let sign = candidate_signs[index];
            balances[edge.from().as_usize()] -= i32::from(sign);
            balances[edge.to().as_usize()] += i32::from(sign);
            Ok(MinimumRatioCycleMcfEdgeState {
                edge: edge.id().clone(),
                fixed_on_face: state.fixed[index],
                initial_flow: scalar(state.initial_flow[index])?,
                updated_flow: scalar(state.updated_flow[index])?,
                lower_slack: scalar(state.initial_flow[index] - edge.lower() as f64)?,
                upper_slack: scalar(edge.capacity() as f64 - state.initial_flow[index])?,
                gradient: scalar(state.gradient[index])?,
                length: scalar(state.length[index])?,
                tree_edge: state.tree_edges[index],
                candidate_sign: sign,
                selected_sign: selected_signs[index],
                numerator_contribution: scalar(state.gradient[index] * f64::from(sign))?,
                denominator_contribution: scalar(if sign == 0 {
                    0.0
                } else {
                    state.length[index]
                })?,
            })
        })
        .collect::<Result<Vec<_>, MinimumRatioCycleMcfError>>()?;
    let mut nodes = state.nodes.clone();
    for (index, node) in nodes.iter_mut().enumerate() {
        node.candidate_balance = balances[index];
        node.on_candidate = graph
            .edges()
            .iter()
            .zip(&candidate_signs)
            .any(|(edge, &sign)| {
                sign != 0 && (edge.from().as_usize() == index || edge.to().as_usize() == index)
            });
        node.on_selected = graph
            .edges()
            .iter()
            .zip(&selected_signs)
            .any(|(edge, &sign)| {
                sign != 0 && (edge.from().as_usize() == index || edge.to().as_usize() == index)
            });
    }
    Ok(MinimumRatioCycleMcfSnapshot {
        stage: state.stage,
        nodes,
        edges,
        alpha: scalar(state.alpha)?,
        optimum_cost: state.optimum_cost,
        initial_cost: scalar(state.initial_cost)?,
        current_cost: scalar(state.current_cost)?,
        cost_gap: scalar(state.cost_gap)?,
        potential_before: scalar(state.potential_before)?,
        current_potential: scalar(state.current_potential)?,
        candidate_ratio: state
            .candidate
            .as_ref()
            .map(|candidate| scalar(candidate.ratio))
            .transpose()?,
        best_ratio: state
            .best
            .as_ref()
            .map(|candidate| scalar(candidate.ratio))
            .transpose()?,
        kappa: scalar(state.kappa)?,
        eta: scalar(state.eta)?,
        weighted_step_norm: scalar(state.weighted_step_norm)?,
        potential_decrease: scalar(state.potential_decrease)?,
        guaranteed_decrease: scalar(state.guaranteed_decrease)?,
        stationary: state.stationary,
        selected_edge_count: selected_signs.iter().filter(|&&sign| sign != 0).count(),
        maximum_absolute_balance: balances
            .iter()
            .map(|value| value.unsigned_abs())
            .max()
            .unwrap_or(0),
        metrics: state.metrics,
    })
}

fn validate_terminal(
    graph: &FlowNetwork,
    state: &KernelState,
    result: &MinimumRatioCycleMcfResult,
) -> Result<(), MinimumRatioCycleMcfError> {
    if result.final_snapshot.stage != MinimumRatioCycleMcfStage::Complete
        || result.metrics != result.final_snapshot.metrics
        || result.initial_flow != scalars(&state.initial_flow)?
        || result.updated_flow != scalars(&state.updated_flow)?
        || result.cycle
            != state
                .best
                .as_ref()
                .map(|candidate| cycle_arcs(graph, &candidate.signs))
                .unwrap_or_default()
        || result.ratio
            != state
                .best
                .as_ref()
                .map(|candidate| scalar(candidate.ratio))
                .transpose()?
    {
        return Err(MinimumRatioCycleMcfError::CycleInvariant);
    }
    if !state.stationary {
        let best = state
            .best
            .as_ref()
            .ok_or(MinimumRatioCycleMcfError::CycleInvariant)?;
        let checked = candidate_from_signs(graph, state, best.signs.clone())?
            .ok_or(MinimumRatioCycleMcfError::CycleInvariant)?;
        if checked != *best || result.final_snapshot.maximum_absolute_balance != 0 {
            return Err(MinimumRatioCycleMcfError::CycleInvariant);
        }
    }
    Ok(())
}

fn base_nodes(graph: &FlowNetwork) -> Vec<MinimumRatioCycleMcfNodeState> {
    graph
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, _)| MinimumRatioCycleMcfNodeState {
            node: NodeIndex::try_from_usize(index).expect("admitted node index"),
            component: index,
            parent: None,
            depth: 0,
            candidate_balance: 0,
            on_candidate: false,
            on_selected: false,
        })
        .collect()
}

fn scalar(value: f64) -> Result<MinimumRatioCycleMcfScalar, MinimumRatioCycleMcfError> {
    MinimumRatioCycleMcfScalar::try_new(value)
}

fn scalars(values: &[f64]) -> Result<Vec<MinimumRatioCycleMcfScalar>, MinimumRatioCycleMcfError> {
    values.iter().copied().map(scalar).collect()
}

fn finite(value: f64) -> Result<f64, MinimumRatioCycleMcfError> {
    if value.is_finite() {
        Ok(if value == 0.0 { 0.0 } else { value })
    } else {
        Err(MinimumRatioCycleMcfError::NumericalFailure)
    }
}

fn checked_increment(value: u64) -> Result<u64, MinimumRatioCycleMcfError> {
    value
        .checked_add(1)
        .ok_or(MinimumRatioCycleMcfError::ArithmeticOverflow)
}

#[derive(Clone, Debug)]
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            self.parent[node] = self.find(self.parent[node]);
        }
        self.parent[node]
    }

    fn join(&mut self, left: usize, right: usize) -> bool {
        let left_root = self.find(left);
        let right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        match self.rank[left_root].cmp(&self.rank[right_root]) {
            Ordering::Less => self.parent[left_root] = right_root,
            Ordering::Greater => self.parent[right_root] = left_root,
            Ordering::Equal => {
                self.parent[right_root] = left_root;
                self.rank[left_root] = self.rank[left_root].saturating_add(1);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    fn network(node_ids: &[&str], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        let nodes = node_ids
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect();
        let edges = edges
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
            .collect();
        FlowNetwork::new(nodes, edges).expect("network")
    }

    #[test]
    fn applies_source_step_on_parallel_route_cycle() {
        let graph = network(
            &["a", "s", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 3, 1),
                ("st", "s", "t", 0, 3, 5),
            ],
        );
        let result = solve_minimum_ratio_cycle_mcf(&graph, &[0, 3, -3]).expect("primitive");
        assert_eq!(result.cycle.len(), 3);
        assert_eq!(result.metrics.source_steps, 1);
        assert!(result.final_snapshot.potential_decrease.get() > 0.0);
        assert!(
            result.final_snapshot.potential_decrease.get()
                >= result.final_snapshot.guaranteed_decrease.get()
        );
        assert_eq!(result.final_snapshot.maximum_absolute_balance, 0);
    }

    #[test]
    fn trace_replays_and_matches_fast_result() {
        let graph = network(
            &["s", "t"],
            &[("cheap", "s", "t", 0, 2, 1), ("costly", "s", "t", 0, 2, 4)],
        );
        let trace = trace_minimum_ratio_cycle_mcf(&graph, &[2, -2]).expect("trace");
        assert!(trace.events.iter().any(|event| {
            event.after.stage == MinimumRatioCycleMcfStage::MeasurePotentialDecrease
        }));
        check_minimum_ratio_cycle_mcf_trace(&graph, &[2, -2], &trace).expect("replay");
        for event in &trace.events {
            let delta = event
                .after
                .metrics
                .enumerated_vectors
                .checked_sub(event.before.metrics.enumerated_vectors)
                .expect("vector evaluations are monotone");
            if delta == 0 {
                continue;
            }
            assert!(matches!(
                event.after.stage,
                MinimumRatioCycleMcfStage::InspectVector | MinimumRatioCycleMcfStage::EvaluateCycle
            ));
            assert!(delta <= MINIMUM_RATIO_CYCLE_MCF_VECTOR_CHECKPOINT_STRIDE);
        }
        let verification = trace
            .events
            .iter()
            .find(|event| event.after.stage == MinimumRatioCycleMcfStage::VerifyCycleSpace)
            .expect("cycle-space verification boundary");
        assert_eq!(
            verification.after.metrics.enumerated_vectors,
            verification.before.metrics.enumerated_vectors,
            "verification must not hide the final vector-evaluation block"
        );
        assert_eq!(
            trace.result,
            solve_minimum_ratio_cycle_mcf(&graph, &[2, -2]).expect("fast")
        );
    }

    #[test]
    fn reports_stationary_singleton_face_without_inventing_a_step() {
        let graph = network(&["s", "t"], &[("st", "s", "t", 0, 2, 1)]);
        let result = solve_minimum_ratio_cycle_mcf(&graph, &[2, -2]).expect("stationary");
        assert!(result.cycle.is_empty());
        assert!(result.final_snapshot.stationary);
        assert_eq!(result.metrics.source_steps, 0);
    }

    #[test]
    fn reports_stationary_cost_flat_active_face_without_mapping_an_undefined_gradient() {
        let graph = network(
            &["s", "t"],
            &[("left", "s", "t", 0, 2, 1), ("right", "s", "t", 0, 2, 1)],
        );
        let trace = trace_minimum_ratio_cycle_mcf(&graph, &[1, -1]).expect("stationary face");
        let snapshot = &trace.result.final_snapshot;
        assert!(snapshot.stationary);
        assert!(snapshot.edges.iter().all(|edge| {
            !edge.fixed_on_face
                && edge.gradient.get() == 0.0
                && edge.length.get() == 0.0
                && !edge.tree_edge
        }));
        assert_eq!(snapshot.metrics.source_steps, 0);
        assert_eq!(snapshot.metrics.enumerated_vectors, 0);
        check_minimum_ratio_cycle_mcf_trace(&graph, &[1, -1], &trace).expect("replay");
    }

    #[test]
    fn rejects_oversized_assignment_space_and_tampered_trace() {
        let graph = network(
            &["s", "t"],
            &[("a", "s", "t", 0, 2, 1), ("b", "s", "t", 0, 2, 2)],
        );
        let mut trace = trace_minimum_ratio_cycle_mcf(&graph, &[2, -2]).expect("trace");
        trace.events[0].after.metrics.feasible_flows += 1;
        assert_eq!(
            check_minimum_ratio_cycle_mcf_trace(&graph, &[2, -2], &trace),
            Err(MinimumRatioCycleMcfError::TraceVerification)
        );

        let oversized = network(
            &["s", "t"],
            &[
                ("a", "s", "t", 0, 8, 0),
                ("b", "s", "t", 0, 8, 0),
                ("c", "s", "t", 0, 8, 0),
                ("d", "s", "t", 0, 8, 0),
                ("e", "s", "t", 0, 8, 0),
                ("f", "s", "t", 0, 8, 0),
            ],
        );
        assert_eq!(
            solve_minimum_ratio_cycle_mcf(&oversized, &[1, -1]),
            Err(MinimumRatioCycleMcfError::AdmissionLimit)
        );
    }
}
