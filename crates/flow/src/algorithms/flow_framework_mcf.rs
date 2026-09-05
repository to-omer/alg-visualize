//! Bounded atomic coordinator for the source minimum-cost-flow Flow Framework.
//!
//! The public solver joins the source initial-point construction, shared
//! alpha-power kernel, resumable topology-aware minimum-ratio-cycle session,
//! the additive-half final-point gate, and Kang--Payor rounding. Its bounded
//! exhaustive oracle certifies only the scalar optimum value; no precomputed
//! optimum flow is retained or projected into the source state.

#![allow(clippy::cast_precision_loss)]

use num_bigint::{BigInt, ToBigInt};
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive, Zero};
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowModelError, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};

use super::alpha_power_ipm::{AlphaPowerIpmEvaluation, evaluate_alpha_power_ipm};
use super::{
    CostedFlowRoundingError, CostedFlowRoundingResult, DynamicCoreGraphStageEdge,
    DynamicCoreGraphStageUpdate, DynamicMinRatioCycleConfig, DynamicMinRatioCycleError,
    DynamicMinRatioCycleMetrics, DynamicMinRatioCycleOperation, DynamicMinRatioCycleResponse,
    DynamicMinRatioCycleSession, DynamicMinRatioCycleTraceResult, DynamicMwuCollectionBridgeConfig,
    DynamicTreeChainEpochRuntimeState, ShiftedTreeChainEdge, ShiftedTreeChainGraph,
    check_dynamic_min_ratio_cycle_trace, initialize_dynamic_min_ratio_cycle_runtime,
    round_costed_flow, trace_dynamic_min_ratio_cycle,
};

/// Original-node admission for the bounded optimal-value oracle.
pub const FLOW_FRAMEWORK_MCF_MAX_NODES: usize = 6;
/// Original-edge admission for the bounded optimal-value oracle.
pub const FLOW_FRAMEWORK_MCF_MAX_EDGES: usize = 8;
/// Augmented edge admission inherited from the dynamic LSF realization.
pub const FLOW_FRAMEWORK_MCF_MAX_AUGMENTED_EDGES: usize = 12;
/// Maximum original capacity.
pub const FLOW_FRAMEWORK_MCF_MAX_CAPACITY: u64 = 8;
/// Maximum absolute original cost.
pub const FLOW_FRAMEWORK_MCF_MAX_COST: u64 = 32;
/// Maximum integer assignments streamed only to certify the scalar `F*`.
pub const FLOW_FRAMEWORK_MCF_MAX_ASSIGNMENTS: u64 = 100_000;
/// Maximum source iterations in one bounded complete solve.
pub const FLOW_FRAMEWORK_MCF_MAX_ITERATIONS: u64 = 1_024;
/// Maximum fully materialized source iterations in one trace.
pub const FLOW_FRAMEWORK_MCF_MAX_TRACE_ITERATIONS: u64 = 128;

const QUANTIZATION_DENOMINATOR: i64 = 1_000_000_000_000;
const DYNAMIC_LEVELS: usize = 2;
const DYNAMIC_BRANCHES: usize = 2;
const SOURCE_RATIO_THRESHOLD_DENOMINATOR: i64 = 16;
const DETECTION_DENOMINATOR: i64 = 1_000_000_000;
const POTENTIAL_TOLERANCE: f64 = 1.0e-8;

/// Replay-safe finite IEEE-754 projection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FlowFrameworkMcfScalar(u64);

impl FlowFrameworkMcfScalar {
    fn new(value: f64) -> Result<Self, FlowFrameworkMcfError> {
        if !value.is_finite() {
            return Err(FlowFrameworkMcfError::NumericalInvariant);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    /// Returns the finite scalar.
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

/// One fully checked outer-loop iteration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfIteration {
    /// One-based completed source iteration.
    pub iteration: u64,
    /// Whether the exact current flow was absorbed into a fresh dynamic epoch.
    pub reinitialized: bool,
    /// Slots returned and reset by `Detect` before the source query.
    pub detected_edges: Vec<usize>,
    /// Coordinates whose quantized gradient or length actually changed.
    pub refreshed_edges: Vec<usize>,
    /// Potential before the accepted circulation.
    pub potential_before: FlowFrameworkMcfScalar,
    /// Potential after the accepted circulation.
    pub potential_after: FlowFrameworkMcfScalar,
    /// Original-cost gap before the accepted circulation.
    pub gap_before: FlowFrameworkMcfScalar,
    /// Original-cost gap after the accepted circulation.
    pub gap_after: FlowFrameworkMcfScalar,
    /// Exact augmented-cost gap before the accepted circulation.
    pub exact_gap_before: BigRational,
    /// Exact augmented-cost gap after the accepted circulation.
    pub exact_gap_after: BigRational,
    /// Exact-ratio target `kappa^2 / 50` used by Algorithm 2.
    pub target_progress: BigRational,
    /// Accepted exact maintained ratio.
    pub accepted_ratio: BigRational,
    /// Exact augmented flow after the source step.
    pub augmented_flow: Vec<BigRational>,
    /// Exact flow in original canonical edge order after the source step.
    pub original_flow: Vec<BigRational>,
    /// Accepted circulation coefficients in original canonical edge order.
    pub original_cycle: Vec<BigRational>,
}

/// An integral no-worse-cost rounding that independently reaches `F*`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfRoundedSolution {
    /// Integral flow in original canonical edge order.
    pub flows: Vec<u64>,
    /// Certified original optimum.
    pub total_cost: i128,
    /// Complete checked augmented rounding result.
    pub augmented_rounding: CostedFlowRoundingResult,
    /// Independent certificate on the original graph.
    pub certificate: MinCostFlowCertificate,
}

/// Explicit reason why the bounded coordinator published an exact result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowFrameworkMcfTermination {
    /// The source additive-half final-point gate was followed by checked rounding.
    SourceAdditiveHalfGap,
}

/// Complete bounded fast execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfResult {
    /// Number of accepted source iterations before the source final-point gate.
    pub iterations: u64,
    /// Source final-point rule used before deterministic integer rounding.
    pub termination: FlowFrameworkMcfTermination,
    /// Final source state immediately before successful exact rounding.
    pub final_snapshot: FlowFrameworkMcfSnapshot,
    /// Last accepted source iteration, absent when initial rounding already succeeds.
    pub last_iteration: Option<FlowFrameworkMcfIteration>,
    /// Dynamic edge inspections across every completed source iteration.
    pub dynamic_edge_inspections: u128,
    /// Independently certified integral optimum.
    pub solution: FlowFrameworkMcfRoundedSolution,
}

/// One source iteration with its independently checkable Algorithm 2 transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfTraceIteration {
    /// Outer source-step projection.
    pub source: FlowFrameworkMcfIteration,
    /// Fresh exact runtime at this bounded periodic-reinitialization boundary.
    pub initial_runtime: DynamicTreeChainEpochRuntimeState,
    /// Algorithm 2 configuration used by this iteration.
    pub config: DynamicMinRatioCycleConfig,
    /// Exact public operations, in source call order.
    pub operations: Vec<DynamicMinRatioCycleOperation>,
    /// Fully checked topology/query/flow transcript.
    pub dynamic_trace: DynamicMinRatioCycleTraceResult,
}

/// Complete bounded source transcript and certified result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfTraceResult {
    /// Initial source midpoint/auxiliary state.
    pub base_snapshot: FlowFrameworkMcfSnapshot,
    /// Accepted source iterations in order.
    pub iterations: Vec<FlowFrameworkMcfTraceIteration>,
    /// Final exact result.
    pub result: FlowFrameworkMcfResult,
}

/// Read-only current outer-loop projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfSnapshot {
    /// Completed source iterations.
    pub iterations: u64,
    /// Exact augmented flow.
    pub augmented_flow: Vec<BigRational>,
    /// Scalar optimum value certified by the bounded oracle.
    pub optimum_cost: i128,
    /// Augmented-node divergence requirements in canonical order.
    pub augmented_nodes: Vec<FlowFrameworkMcfAugmentedNodeState>,
    /// Augmented graph and exact current point in canonical edge order.
    pub augmented_edges: Vec<FlowFrameworkMcfAugmentedEdgeState>,
    /// Exact flow in original canonical edge order.
    pub original_flow: Vec<BigRational>,
    /// Current original-cost gap to the certified `F*`.
    pub gap: FlowFrameworkMcfScalar,
    /// Exact augmented-cost gap to the certified `F*`.
    pub exact_gap: BigRational,
    /// Current alpha-power potential.
    pub potential: FlowFrameworkMcfScalar,
    /// Most recent detected slots.
    pub detected_edges: Vec<usize>,
}

/// One augmented node in the exact source final-point contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfAugmentedNodeState {
    /// Stable node identity, including the source auxiliary node when present.
    pub node_id: String,
    /// Required outgoing-minus-incoming divergence.
    pub required_divergence: i128,
}

/// One augmented edge in the exact source final-point contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowFrameworkMcfAugmentedEdgeState {
    /// Stable edge identity.
    pub edge_id: String,
    /// Stable tail-node identity.
    pub from: String,
    /// Stable head-node identity.
    pub to: String,
    /// Integral lower bound.
    pub lower: u64,
    /// Integral upper bound.
    pub capacity: u64,
    /// Integral unit cost.
    pub cost: i64,
    /// Exact current source point.
    pub flow: BigRational,
    /// Whether this is an auxiliary edge introduced by the initial-point construction.
    pub auxiliary: bool,
}

/// Explicit bounded coordinator failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FlowFrameworkMcfError {
    /// The original instance is malformed or exceeds the explicit band.
    #[error("Flow Framework MCF input exceeds its explicit admission band")]
    AdmissionLimit,
    /// Feasibility construction failed with an independently checkable witness.
    #[error("Flow Framework MCF feasibility failed: {0}")]
    Feasibility(#[from] FeasibilityError),
    /// Checked integer arithmetic overflowed.
    #[error("Flow Framework MCF arithmetic overflow")]
    ArithmeticOverflow,
    /// The source initial-point graph could not be represented.
    #[error("Flow Framework MCF model construction failed: {0}")]
    Model(#[from] FlowModelError),
    /// The dynamic minimum-ratio-cycle stack rejected an operation.
    #[error("Flow Framework MCF dynamic stack failed: {0}")]
    Dynamic(#[from] DynamicMinRatioCycleError),
    /// Exact costed-flow rounding failed.
    #[error("Flow Framework MCF rounding failed: {0}")]
    Rounding(#[from] CostedFlowRoundingError),
    /// The final independent MCF certificate failed.
    #[error("Flow Framework MCF certificate failed: {0}")]
    Certificate(#[from] CertificateError),
    /// A finite, strict-interior, approximation, or potential invariant failed.
    #[error("Flow Framework MCF numerical invariant failed")]
    NumericalInvariant,
    /// The requested bounded solve ended before the source final-point gate.
    #[error("Flow Framework MCF iteration limit reached")]
    IterationLimit,
    /// A supplied source transcript is not the exact independent composition.
    #[error("Flow Framework MCF trace verification failed")]
    TraceVerification,
}

#[derive(Clone)]
struct InitialPoint {
    graph: FlowNetwork,
    required: Vec<i128>,
    flow: Vec<BigRational>,
    original_to_augmented: Vec<usize>,
    auxiliary_edges: Vec<usize>,
}

struct DynamicInitialization {
    rows: Vec<DynamicCoreGraphStageEdge>,
    runtime: DynamicTreeChainEpochRuntimeState,
    config: DynamicMinRatioCycleConfig,
}

/// Private-state, operation-atomic bounded Flow Framework coordinator.
#[derive(Clone)]
pub struct FlowFrameworkMcfSession {
    original: FlowNetwork,
    original_required: Vec<i128>,
    optimum_cost: i128,
    initial: InitialPoint,
    rows: Vec<DynamicCoreGraphStageEdge>,
    dynamic: DynamicMinRatioCycleSession,
    completed_dynamic_edge_inspections: u128,
    iterations: u64,
    detected_edges: Vec<usize>,
}

impl FlowFrameworkMcfSession {
    /// Builds the source midpoint/auxiliary initial point and checked dynamic hierarchy.
    ///
    /// # Errors
    ///
    /// Rejects out-of-band, infeasible, non-strict, or dynamically unsupported input.
    pub fn new(
        graph: &FlowNetwork,
        required_divergence: &[i128],
    ) -> Result<Self, FlowFrameworkMcfError> {
        let mut feasibility = FeasibilityExecution::untracked();
        Self::new_with_feasibility(graph, required_divergence, &mut feasibility)
    }

    fn new_with_feasibility(
        graph: &FlowNetwork,
        required_divergence: &[i128],
        feasibility: &mut FeasibilityExecution,
    ) -> Result<Self, FlowFrameworkMcfError> {
        let (initial, optimum_cost) =
            prepare_initial_problem_with_feasibility(graph, required_divergence, feasibility)?;
        let evaluation = evaluate(&initial, &initial.flow, optimum_cost)?;
        let initialization = dynamic_initialization(&initial, &evaluation)?;
        let dynamic =
            DynamicMinRatioCycleSession::new(&initialization.runtime, &initialization.config)?;
        Ok(Self {
            original: graph.clone(),
            original_required: required_divergence.to_vec(),
            optimum_cost,
            initial,
            rows: initialization.rows,
            dynamic,
            completed_dynamic_edge_inspections: 0,
            iterations: 0,
            detected_edges: Vec::new(),
        })
    }

    /// Returns a checked immutable projection of the current outer-loop state.
    ///
    /// # Errors
    ///
    /// Returns a numerical failure if exact rational state cannot be projected.
    pub fn snapshot(&self) -> Result<FlowFrameworkMcfSnapshot, FlowFrameworkMcfError> {
        let flow = self.current_flow();
        let evaluation = evaluate(&self.initial, &flow, self.optimum_cost)?;
        let exact_gap = exact_augmented_cost_gap(&self.initial, &flow, self.optimum_cost)?;
        let (augmented_nodes, augmented_edges) = augmented_source_state(&self.initial, &flow)?;
        Ok(FlowFrameworkMcfSnapshot {
            iterations: self.iterations,
            original_flow: project_original_flow(&self.initial, &flow)?,
            augmented_flow: flow,
            optimum_cost: self.optimum_cost,
            augmented_nodes,
            augmented_edges,
            gap: FlowFrameworkMcfScalar::new(evaluation.gap)?,
            exact_gap,
            potential: FlowFrameworkMcfScalar::new(evaluation.potential)?,
            detected_edges: self.detected_edges.clone(),
        })
    }

    /// Executes one `Detect -> coordinate refresh -> Query -> source step` atomically.
    ///
    /// # Errors
    ///
    /// Any component or potential failure leaves this session unchanged.
    pub fn step(&mut self) -> Result<FlowFrameworkMcfIteration, FlowFrameworkMcfError> {
        let mut candidate = self.clone();
        let iteration = candidate.step_candidate()?;
        *self = candidate;
        Ok(iteration)
    }

    /// Runs Kang--Payor rounding only after the source additive-half final point.
    ///
    /// # Errors
    ///
    /// Returns a rounding or independent-certificate failure.
    pub fn round_if_source_ready(
        &self,
    ) -> Result<Option<FlowFrameworkMcfRoundedSolution>, FlowFrameworkMcfError> {
        round_if_source_ready_state(
            &self.original,
            &self.original_required,
            &self.initial,
            self.optimum_cost,
            &self.current_flow(),
        )
    }

    fn step_candidate(&mut self) -> Result<FlowFrameworkMcfIteration, FlowFrameworkMcfError> {
        let reinitialized = self.reinitialize_if_due()?;
        let Some(DynamicMinRatioCycleResponse::Detect {
            edges: detected, ..
        }) = self.dynamic.apply(DynamicMinRatioCycleOperation::Detect)?
        else {
            return Err(FlowFrameworkMcfError::NumericalInvariant);
        };
        let before_flow = self.current_flow();
        let before = evaluate(&self.initial, &before_flow, self.optimum_cost)?;
        let next_rows = rows_from_evaluation(&self.initial.graph, &before)?;
        let updates = attribute_updates(&self.rows, &next_rows);
        let refreshed_edges = updates
            .iter()
            .filter_map(|update| match update {
                DynamicCoreGraphStageUpdate::ReplaceAttributes { after, .. } => Some(after.edge),
                _ => None,
            })
            .collect::<Vec<_>>();
        self.dynamic
            .apply(DynamicMinRatioCycleOperation::SourceProgressUpdate { updates })?;
        let after_flow = self.current_flow();
        let after = evaluate(&self.initial, &after_flow, self.optimum_cost)?;
        let accepted = self
            .dynamic
            .snapshot()
            .last_candidate
            .as_ref()
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?;
        let target_progress =
            &accepted.ratio * &accepted.ratio / BigRational::from_integer(BigInt::from(50));
        let target = target_progress
            .to_f64()
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?;
        let actual_decrease = before.potential - after.potential;
        let tolerance = POTENTIAL_TOLERANCE * before.potential.abs().max(1.0);
        if actual_decrease + tolerance < target / 10.0 || after.gap >= before.gap {
            return Err(FlowFrameworkMcfError::NumericalInvariant);
        }
        self.rows = next_rows;
        self.iterations = self
            .iterations
            .checked_add(1)
            .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)?;
        self.detected_edges.clone_from(&detected);
        Ok(FlowFrameworkMcfIteration {
            iteration: self.iterations,
            reinitialized,
            detected_edges: detected,
            refreshed_edges,
            potential_before: FlowFrameworkMcfScalar::new(before.potential)?,
            potential_after: FlowFrameworkMcfScalar::new(after.potential)?,
            gap_before: FlowFrameworkMcfScalar::new(before.gap)?,
            gap_after: FlowFrameworkMcfScalar::new(after.gap)?,
            exact_gap_before: exact_augmented_cost_gap(
                &self.initial,
                &before_flow,
                self.optimum_cost,
            )?,
            exact_gap_after: exact_augmented_cost_gap(
                &self.initial,
                &after_flow,
                self.optimum_cost,
            )?,
            target_progress,
            accepted_ratio: accepted.ratio.clone(),
            original_flow: project_original_flow(&self.initial, &after_flow)?,
            original_cycle: project_original_coefficients(&self.initial, &accepted.coefficients)?,
            augmented_flow: after_flow,
        })
    }

    fn current_flow(&self) -> Vec<BigRational> {
        self.initial
            .flow
            .iter()
            .zip(&self.dynamic.snapshot().flow)
            .map(|(initial, movement)| initial + movement)
            .collect()
    }

    fn reinitialize_if_due(&mut self) -> Result<bool, FlowFrameworkMcfError> {
        if self.iterations == 0 {
            return Ok(false);
        }
        self.completed_dynamic_edge_inspections = self
            .completed_dynamic_edge_inspections
            .checked_add(dynamic_edge_inspections(&self.dynamic.snapshot().metrics)?)
            .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)?;
        self.initial.flow = self.current_flow();
        let evaluation = evaluate(&self.initial, &self.initial.flow, self.optimum_cost)?;
        let initialization = dynamic_initialization(&self.initial, &evaluation)?;
        self.dynamic =
            DynamicMinRatioCycleSession::new(&initialization.runtime, &initialization.config)?;
        self.rows = initialization.rows;
        self.detected_edges.clear();
        Ok(true)
    }

    fn dynamic_edge_inspections(&self) -> Result<u128, FlowFrameworkMcfError> {
        self.completed_dynamic_edge_inspections
            .checked_add(dynamic_edge_inspections(&self.dynamic.snapshot().metrics)?)
            .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)
    }
}

fn dynamic_edge_inspections(
    metrics: &DynamicMinRatioCycleMetrics,
) -> Result<u128, FlowFrameworkMcfError> {
    u128::from(metrics.intermediate_edge_inspections)
        .checked_add(u128::from(metrics.terminal_edge_inspections))
        .and_then(|total| total.checked_add(u128::from(metrics.detection_edge_scans)))
        .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)
}

fn prepare_initial_problem(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<(InitialPoint, i128), FlowFrameworkMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    prepare_initial_problem_with_feasibility(graph, required_divergence, &mut feasibility)
}

fn prepare_initial_problem_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<(InitialPoint, i128), FlowFrameworkMcfError> {
    validate_original(graph, required_divergence)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let optimum_cost = minimum_feasible_cost(graph, required_divergence)?;
    let initial = build_source_initial_point(graph, required_divergence)?;
    if initial.graph.edges().len() > FLOW_FRAMEWORK_MCF_MAX_AUGMENTED_EDGES {
        return Err(FlowFrameworkMcfError::AdmissionLimit);
    }
    Ok((initial, optimum_cost))
}

/// Exact deterministic final-point threshold from the source theorem.
#[must_use]
pub fn flow_framework_mcf_stopping_gap() -> BigRational {
    BigRational::new(BigInt::from(1), BigInt::from(2))
}

fn exact_augmented_cost_gap(
    initial: &InitialPoint,
    flow: &[BigRational],
    optimum_cost: i128,
) -> Result<BigRational, FlowFrameworkMcfError> {
    if flow.len() != initial.graph.edges().len() {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    let cost = initial.graph.edges().iter().zip(flow).try_fold(
        BigRational::zero(),
        |sum, (edge, amount)| {
            if amount < &BigRational::from_integer(BigInt::from(edge.lower()))
                || amount > &BigRational::from_integer(BigInt::from(edge.capacity()))
            {
                return Err(FlowFrameworkMcfError::NumericalInvariant);
            }
            Ok(sum + amount * BigInt::from(edge.cost()))
        },
    )?;
    let gap = cost - BigRational::from_integer(BigInt::from(optimum_cost));
    if gap.is_negative() {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(gap)
}

fn round_if_source_ready_state(
    original: &FlowNetwork,
    original_required: &[i128],
    initial: &InitialPoint,
    optimum_cost: i128,
    flow: &[BigRational],
) -> Result<Option<FlowFrameworkMcfRoundedSolution>, FlowFrameworkMcfError> {
    if exact_augmented_cost_gap(initial, flow, optimum_cost)? > flow_framework_mcf_stopping_gap() {
        return Ok(None);
    }
    let rounded = round_costed_flow(&initial.graph, &initial.required, flow)?;
    if rounded.total_cost != optimum_cost {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    if initial
        .auxiliary_edges
        .iter()
        .any(|&edge| rounded.flows[edge] != 0)
    {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    let flows = initial
        .original_to_augmented
        .iter()
        .map(|&edge| rounded.flows[edge])
        .collect::<Vec<_>>();
    let certificate = check_min_cost_flow(original, original_required, &flows)?;
    if integral_cost(original, &flows)? != optimum_cost {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(Some(FlowFrameworkMcfRoundedSolution {
        flows,
        total_cost: optimum_cost,
        augmented_rounding: rounded,
        certificate,
    }))
}

/// Runs the bounded source Flow Framework through its additive-half final point.
///
/// # Errors
///
/// Rejects a zero/out-of-band iteration budget, any component invariant
/// failure, or a run that does not reach that point within the supplied budget.
pub fn execute_flow_framework_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
) -> Result<FlowFrameworkMcfResult, FlowFrameworkMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    execute_flow_framework_mcf_with_feasibility(
        graph,
        required_divergence,
        maximum_iterations,
        &mut feasibility,
    )
}

/// Runs the bounded source Flow Framework while reporting its feasibility
/// precheck to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn execute_flow_framework_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
    feasibility: &mut FeasibilityExecution,
) -> Result<FlowFrameworkMcfResult, FlowFrameworkMcfError> {
    if maximum_iterations == 0 || maximum_iterations > FLOW_FRAMEWORK_MCF_MAX_ITERATIONS {
        return Err(FlowFrameworkMcfError::AdmissionLimit);
    }
    let mut session =
        FlowFrameworkMcfSession::new_with_feasibility(graph, required_divergence, feasibility)?;
    if let Some(solution) = session.round_if_source_ready()? {
        return Ok(FlowFrameworkMcfResult {
            iterations: 0,
            termination: FlowFrameworkMcfTermination::SourceAdditiveHalfGap,
            final_snapshot: session.snapshot()?,
            last_iteration: None,
            dynamic_edge_inspections: 0,
            solution,
        });
    }
    for _ in 0..maximum_iterations {
        let last_iteration = session.step()?;
        if let Some(solution) = session.round_if_source_ready()? {
            return Ok(FlowFrameworkMcfResult {
                iterations: session.iterations,
                termination: FlowFrameworkMcfTermination::SourceAdditiveHalfGap,
                final_snapshot: session.snapshot()?,
                last_iteration: Some(last_iteration),
                dynamic_edge_inspections: session.dynamic_edge_inspections()?,
                solution,
            });
        }
    }
    Err(FlowFrameworkMcfError::IterationLimit)
}

/// Records every bounded source iteration and its nested Algorithm 2 transcript.
///
/// # Errors
///
/// Rejects out-of-band input/budgets, component failures, or a run that cannot
/// reach the additive-half final point within the supplied trace budget.
pub fn trace_flow_framework_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
) -> Result<FlowFrameworkMcfTraceResult, FlowFrameworkMcfError> {
    let mut feasibility = FeasibilityExecution::untracked();
    trace_flow_framework_mcf_with_feasibility(
        graph,
        required_divergence,
        maximum_iterations,
        &mut feasibility,
    )
}

/// Records the bounded Flow Framework while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_flow_framework_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
    feasibility: &mut FeasibilityExecution,
) -> Result<FlowFrameworkMcfTraceResult, FlowFrameworkMcfError> {
    if maximum_iterations == 0 || maximum_iterations > FLOW_FRAMEWORK_MCF_MAX_TRACE_ITERATIONS {
        return Err(FlowFrameworkMcfError::AdmissionLimit);
    }
    let (mut initial, optimum_cost) =
        prepare_initial_problem_with_feasibility(graph, required_divergence, feasibility)?;
    let mut current = initial.flow.clone();
    let base_snapshot = source_snapshot(&initial, &current, optimum_cost, 0, Vec::new())?;
    if let Some(solution) =
        round_if_source_ready_state(graph, required_divergence, &initial, optimum_cost, &current)?
    {
        return finish_flow_framework_mcf_trace(
            graph,
            required_divergence,
            maximum_iterations,
            base_snapshot.clone(),
            Vec::new(),
            base_snapshot,
            solution,
        );
    }

    let mut iterations = Vec::new();
    for iteration in 1..=maximum_iterations {
        initial.flow.clone_from(&current);
        let before = evaluate(&initial, &current, optimum_cost)?;
        let initialization = dynamic_initialization(&initial, &before)?;
        let operations = source_operations();
        let dynamic_trace = trace_dynamic_min_ratio_cycle(
            &initialization.runtime,
            &initialization.config,
            &operations,
        )?;
        let detected = detected_response(&dynamic_trace)?;
        let after_flow = add_movement(&current, &dynamic_trace.result.final_snapshot.flow)?;
        let after = evaluate(&initial, &after_flow, optimum_cost)?;
        let accepted = dynamic_trace
            .result
            .final_snapshot
            .last_candidate
            .as_ref()
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?;
        let target_progress =
            &accepted.ratio * &accepted.ratio / BigRational::from_integer(BigInt::from(50));
        validate_source_progress(&before, &after, &target_progress)?;
        let source = FlowFrameworkMcfIteration {
            iteration,
            reinitialized: iteration > 1,
            detected_edges: detected,
            refreshed_edges: Vec::new(),
            potential_before: FlowFrameworkMcfScalar::new(before.potential)?,
            potential_after: FlowFrameworkMcfScalar::new(after.potential)?,
            gap_before: FlowFrameworkMcfScalar::new(before.gap)?,
            gap_after: FlowFrameworkMcfScalar::new(after.gap)?,
            exact_gap_before: exact_augmented_cost_gap(&initial, &current, optimum_cost)?,
            exact_gap_after: exact_augmented_cost_gap(&initial, &after_flow, optimum_cost)?,
            target_progress,
            accepted_ratio: accepted.ratio.clone(),
            original_flow: project_original_flow(&initial, &after_flow)?,
            original_cycle: project_original_coefficients(&initial, &accepted.coefficients)?,
            augmented_flow: after_flow.clone(),
        };
        iterations.push(FlowFrameworkMcfTraceIteration {
            source,
            initial_runtime: initialization.runtime,
            config: initialization.config,
            operations,
            dynamic_trace,
        });
        current = after_flow;
        initial.flow.clone_from(&current);
        if let Some(solution) = round_if_source_ready_state(
            graph,
            required_divergence,
            &initial,
            optimum_cost,
            &current,
        )? {
            let final_snapshot =
                source_snapshot(&initial, &current, optimum_cost, iteration, Vec::new())?;
            return finish_flow_framework_mcf_trace(
                graph,
                required_divergence,
                maximum_iterations,
                base_snapshot,
                iterations,
                final_snapshot,
                solution,
            );
        }
    }
    Err(FlowFrameworkMcfError::IterationLimit)
}

fn finish_flow_framework_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
    base_snapshot: FlowFrameworkMcfSnapshot,
    iterations: Vec<FlowFrameworkMcfTraceIteration>,
    final_snapshot: FlowFrameworkMcfSnapshot,
    solution: FlowFrameworkMcfRoundedSolution,
) -> Result<FlowFrameworkMcfTraceResult, FlowFrameworkMcfError> {
    let completed =
        u64::try_from(iterations.len()).map_err(|_| FlowFrameworkMcfError::ArithmeticOverflow)?;
    let dynamic_edge_inspections = iterations.iter().try_fold(0_u128, |total, iteration| {
        total
            .checked_add(dynamic_edge_inspections(
                &iteration.dynamic_trace.result.final_snapshot.metrics,
            )?)
            .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)
    })?;
    let last_iteration = iterations.last().map(|value| value.source.clone());
    let trace = FlowFrameworkMcfTraceResult {
        base_snapshot,
        result: FlowFrameworkMcfResult {
            iterations: completed,
            termination: FlowFrameworkMcfTermination::SourceAdditiveHalfGap,
            final_snapshot,
            last_iteration,
            dynamic_edge_inspections,
            solution,
        },
        iterations,
    };
    check_flow_framework_mcf_trace(graph, required_divergence, maximum_iterations, &trace)?;
    Ok(trace)
}

/// Independently reconstructs a complete source transcript.
///
/// # Errors
///
/// Returns trace verification failure for any altered runtime, operation,
/// nested event, source scalar, flow, stopping point, rounding, or certificate.
pub fn check_flow_framework_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
    trace: &FlowFrameworkMcfTraceResult,
) -> Result<(), FlowFrameworkMcfError> {
    audit_flow_framework_mcf_trace(graph, required_divergence, maximum_iterations, trace)
        .map_err(|_| FlowFrameworkMcfError::TraceVerification)
}

#[allow(
    clippy::too_many_lines,
    reason = "one independent audit replays every nested source iteration and terminal certificate"
)]
fn audit_flow_framework_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    maximum_iterations: u64,
    trace: &FlowFrameworkMcfTraceResult,
) -> Result<(), FlowFrameworkMcfError> {
    if maximum_iterations == 0
        || maximum_iterations > FLOW_FRAMEWORK_MCF_MAX_TRACE_ITERATIONS
        || trace.iterations.len() as u64 > maximum_iterations
        || trace.result.iterations != trace.iterations.len() as u64
    {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    let (mut initial, optimum_cost) = prepare_initial_problem(graph, required_divergence)?;
    let mut current = initial.flow.clone();
    let expected_base = source_snapshot(&initial, &current, optimum_cost, 0, Vec::new())?;
    if trace.base_snapshot != expected_base {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }

    for (offset, recorded) in trace.iterations.iter().enumerate() {
        if exact_augmented_cost_gap(&initial, &current, optimum_cost)?
            <= flow_framework_mcf_stopping_gap()
        {
            return Err(FlowFrameworkMcfError::TraceVerification);
        }
        initial.flow.clone_from(&current);
        let before = evaluate(&initial, &current, optimum_cost)?;
        let expected_initialization = dynamic_initialization(&initial, &before)?;
        let expected_operations = source_operations();
        if recorded.initial_runtime != expected_initialization.runtime
            || recorded.config != expected_initialization.config
            || recorded.operations != expected_operations
        {
            return Err(FlowFrameworkMcfError::TraceVerification);
        }
        check_dynamic_min_ratio_cycle_trace(
            &expected_initialization.runtime,
            &expected_initialization.config,
            &expected_operations,
            &recorded.dynamic_trace,
        )?;
        let detected = detected_response(&recorded.dynamic_trace)?;
        let after_flow =
            add_movement(&current, &recorded.dynamic_trace.result.final_snapshot.flow)?;
        let after = evaluate(&initial, &after_flow, optimum_cost)?;
        let accepted = recorded
            .dynamic_trace
            .result
            .final_snapshot
            .last_candidate
            .as_ref()
            .ok_or(FlowFrameworkMcfError::TraceVerification)?;
        let target_progress =
            &accepted.ratio * &accepted.ratio / BigRational::from_integer(BigInt::from(50));
        validate_source_progress(&before, &after, &target_progress)?;
        let expected_source = FlowFrameworkMcfIteration {
            iteration: offset as u64 + 1,
            reinitialized: offset > 0,
            detected_edges: detected,
            refreshed_edges: Vec::new(),
            potential_before: FlowFrameworkMcfScalar::new(before.potential)?,
            potential_after: FlowFrameworkMcfScalar::new(after.potential)?,
            gap_before: FlowFrameworkMcfScalar::new(before.gap)?,
            gap_after: FlowFrameworkMcfScalar::new(after.gap)?,
            exact_gap_before: exact_augmented_cost_gap(&initial, &current, optimum_cost)?,
            exact_gap_after: exact_augmented_cost_gap(&initial, &after_flow, optimum_cost)?,
            target_progress,
            accepted_ratio: accepted.ratio.clone(),
            original_flow: project_original_flow(&initial, &after_flow)?,
            original_cycle: project_original_coefficients(&initial, &accepted.coefficients)?,
            augmented_flow: after_flow.clone(),
        };
        if recorded.source != expected_source {
            return Err(FlowFrameworkMcfError::TraceVerification);
        }
        current = after_flow;
        initial.flow.clone_from(&current);
    }

    let solution =
        round_if_source_ready_state(graph, required_divergence, &initial, optimum_cost, &current)?
            .ok_or(FlowFrameworkMcfError::TraceVerification)?;
    if trace.result.solution != solution {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    if trace.result.termination != FlowFrameworkMcfTermination::SourceAdditiveHalfGap {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    let expected_final = source_snapshot(
        &initial,
        &current,
        optimum_cost,
        trace.result.iterations,
        Vec::new(),
    )?;
    if trace.result.final_snapshot != expected_final {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    if trace.result.last_iteration
        != trace
            .iterations
            .last()
            .map(|iteration| iteration.source.clone())
    {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    let expected_dynamic_edge_inspections =
        trace
            .iterations
            .iter()
            .try_fold(0_u128, |total, iteration| {
                total
                    .checked_add(dynamic_edge_inspections(
                        &iteration.dynamic_trace.result.final_snapshot.metrics,
                    )?)
                    .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)
            })?;
    if trace.result.dynamic_edge_inspections != expected_dynamic_edge_inspections {
        return Err(FlowFrameworkMcfError::TraceVerification);
    }
    Ok(())
}

fn source_operations() -> Vec<DynamicMinRatioCycleOperation> {
    vec![
        DynamicMinRatioCycleOperation::Detect,
        DynamicMinRatioCycleOperation::SourceProgressUpdate {
            updates: Vec::new(),
        },
    ]
}

fn detected_response(
    trace: &DynamicMinRatioCycleTraceResult,
) -> Result<Vec<usize>, FlowFrameworkMcfError> {
    let [DynamicMinRatioCycleResponse::Detect { edges, .. }] = trace.result.responses.as_slice()
    else {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    };
    Ok(edges.clone())
}

fn add_movement(
    base: &[BigRational],
    movement: &[BigRational],
) -> Result<Vec<BigRational>, FlowFrameworkMcfError> {
    if base.len() != movement.len() {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(base
        .iter()
        .zip(movement)
        .map(|(base, movement)| base + movement)
        .collect())
}

fn source_snapshot(
    initial: &InitialPoint,
    flow: &[BigRational],
    optimum_cost: i128,
    iterations: u64,
    detected_edges: Vec<usize>,
) -> Result<FlowFrameworkMcfSnapshot, FlowFrameworkMcfError> {
    let evaluation = evaluate(initial, flow, optimum_cost)?;
    let (augmented_nodes, augmented_edges) = augmented_source_state(initial, flow)?;
    Ok(FlowFrameworkMcfSnapshot {
        iterations,
        original_flow: project_original_flow(initial, flow)?,
        augmented_flow: flow.to_vec(),
        optimum_cost,
        augmented_nodes,
        augmented_edges,
        gap: FlowFrameworkMcfScalar::new(evaluation.gap)?,
        exact_gap: exact_augmented_cost_gap(initial, flow, optimum_cost)?,
        potential: FlowFrameworkMcfScalar::new(evaluation.potential)?,
        detected_edges,
    })
}

fn augmented_source_state(
    initial: &InitialPoint,
    flow: &[BigRational],
) -> Result<
    (
        Vec<FlowFrameworkMcfAugmentedNodeState>,
        Vec<FlowFrameworkMcfAugmentedEdgeState>,
    ),
    FlowFrameworkMcfError,
> {
    if flow.len() != initial.graph.edges().len()
        || initial.required.len() != initial.graph.nodes().len()
    {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    let augmented_nodes = initial
        .graph
        .nodes()
        .iter()
        .zip(&initial.required)
        .map(
            |(node, &required_divergence)| FlowFrameworkMcfAugmentedNodeState {
                node_id: node.id().as_str().to_owned(),
                required_divergence,
            },
        )
        .collect();
    let augmented_edges = initial
        .graph
        .edges()
        .iter()
        .zip(flow)
        .enumerate()
        .map(
            |(index, (edge, amount))| FlowFrameworkMcfAugmentedEdgeState {
                edge_id: edge.id().as_str().to_owned(),
                from: initial.graph.nodes()[edge.from().as_usize()]
                    .id()
                    .as_str()
                    .to_owned(),
                to: initial.graph.nodes()[edge.to().as_usize()]
                    .id()
                    .as_str()
                    .to_owned(),
                lower: edge.lower(),
                capacity: edge.capacity(),
                cost: edge.cost(),
                flow: amount.clone(),
                auxiliary: initial.auxiliary_edges.contains(&index),
            },
        )
        .collect();
    Ok((augmented_nodes, augmented_edges))
}

fn project_original_flow(
    initial: &InitialPoint,
    augmented_flow: &[BigRational],
) -> Result<Vec<BigRational>, FlowFrameworkMcfError> {
    initial
        .original_to_augmented
        .iter()
        .map(|&edge| {
            augmented_flow
                .get(edge)
                .cloned()
                .ok_or(FlowFrameworkMcfError::NumericalInvariant)
        })
        .collect()
}

fn project_original_coefficients(
    initial: &InitialPoint,
    augmented_coefficients: &[BigInt],
) -> Result<Vec<BigRational>, FlowFrameworkMcfError> {
    initial
        .original_to_augmented
        .iter()
        .map(|&edge| {
            augmented_coefficients
                .get(edge)
                .cloned()
                .map(BigRational::from_integer)
                .ok_or(FlowFrameworkMcfError::NumericalInvariant)
        })
        .collect()
}

fn validate_source_progress(
    before: &AlphaPowerIpmEvaluation,
    after: &AlphaPowerIpmEvaluation,
    target_progress: &BigRational,
) -> Result<(), FlowFrameworkMcfError> {
    let target = target_progress
        .to_f64()
        .ok_or(FlowFrameworkMcfError::NumericalInvariant)?;
    let actual_decrease = before.potential - after.potential;
    let tolerance = POTENTIAL_TOLERANCE * before.potential.abs().max(1.0);
    if actual_decrease + tolerance < target / 10.0 || after.gap >= before.gap {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(())
}

fn dynamic_initialization(
    initial: &InitialPoint,
    evaluation: &AlphaPowerIpmEvaluation,
) -> Result<DynamicInitialization, FlowFrameworkMcfError> {
    let rows = rows_from_evaluation(&initial.graph, evaluation)?;
    let root = shifted_graph(&initial.graph, &rows);
    let bridge = DynamicMwuCollectionBridgeConfig {
        branches: DYNAMIC_BRANCHES,
        maximum_node_count: 8,
        stable_edge_slots: initial.graph.edges().len(),
    };
    let level_configs = vec![bridge; DYNAMIC_LEVELS];
    let runtime = initialize_dynamic_min_ratio_cycle_runtime(&root, &level_configs)?;
    let threshold =
        quantize_positive(evaluation.alpha / SOURCE_RATIO_THRESHOLD_DENOMINATOR as f64)?;
    let config = DynamicMinRatioCycleConfig {
        level_configs,
        terminal_branches: DYNAMIC_BRANCHES,
        psi: 2,
        kappa_alpha: threshold,
        epsilon: BigRational::new(BigInt::from(1), BigInt::from(DETECTION_DENOMINATOR)),
        rebuild_after_updates: vec![64; DYNAMIC_LEVELS],
    };
    Ok(DynamicInitialization {
        rows,
        runtime,
        config,
    })
}

fn validate_original(graph: &FlowNetwork, required: &[i128]) -> Result<(), FlowFrameworkMcfError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > FLOW_FRAMEWORK_MCF_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > FLOW_FRAMEWORK_MCF_MAX_EDGES
        || required.len() != graph.nodes().len()
        || graph.edges().iter().any(|edge| {
            edge.capacity() > FLOW_FRAMEWORK_MCF_MAX_CAPACITY
                || edge.cost().unsigned_abs() > FLOW_FRAMEWORK_MCF_MAX_COST
                || edge.lower() == edge.capacity()
                || edge.from() == edge.to()
        })
        || required
            .iter()
            .try_fold(0_i128, |sum, value| sum.checked_add(*value))
            != Some(0)
    {
        return Err(FlowFrameworkMcfError::AdmissionLimit);
    }
    let assignments = graph.edges().iter().try_fold(1_u64, |count, edge| {
        count.checked_mul(edge.capacity() - edge.lower() + 1)
    });
    if assignments.is_none_or(|count| count > FLOW_FRAMEWORK_MCF_MAX_ASSIGNMENTS) {
        return Err(FlowFrameworkMcfError::AdmissionLimit);
    }
    Ok(())
}

fn minimum_feasible_cost(
    graph: &FlowNetwork,
    required: &[i128],
) -> Result<i128, FlowFrameworkMcfError> {
    fn recurse(
        graph: &FlowNetwork,
        required: &[i128],
        index: usize,
        current: &mut [u64],
        minimum: &mut Option<i128>,
    ) -> Result<(), FlowFrameworkMcfError> {
        if index == graph.edges().len() {
            if divergences(graph, current).map_err(|_| FlowFrameworkMcfError::ArithmeticOverflow)?
                == required
            {
                let cost = integral_cost(graph, current)?;
                *minimum = Some(minimum.map_or(cost, |value| value.min(cost)));
            }
            return Ok(());
        }
        let edge = &graph.edges()[index];
        for value in edge.lower()..=edge.capacity() {
            current[index] = value;
            recurse(graph, required, index + 1, current, minimum)?;
        }
        Ok(())
    }

    let mut current = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let mut minimum = None;
    recurse(graph, required, 0, &mut current, &mut minimum)?;
    minimum.ok_or(FlowFrameworkMcfError::NumericalInvariant)
}

fn build_source_initial_point(
    graph: &FlowNetwork,
    required: &[i128],
) -> Result<InitialPoint, FlowFrameworkMcfError> {
    let auxiliary_id = unique_node_id(graph, "flow-framework-aux")?;
    let mut unresolved = graph
        .edges()
        .iter()
        .map(|edge| UnresolvedFlowEdge {
            id: edge.id().clone(),
            from: graph.nodes()[edge.from().as_usize()].id().clone(),
            to: graph.nodes()[edge.to().as_usize()].id().clone(),
            lower: edge.lower(),
            capacity: edge.capacity(),
            cost: edge.cost(),
        })
        .collect::<Vec<_>>();
    let midpoint = graph
        .edges()
        .iter()
        .map(|edge| {
            BigRational::new(
                BigInt::from(edge.lower() + edge.capacity()),
                BigInt::from(2),
            )
        })
        .collect::<Vec<_>>();
    let mut routed = vec![BigRational::zero(); graph.nodes().len()];
    for (edge, amount) in graph.edges().iter().zip(&midpoint) {
        routed[edge.from().as_usize()] += amount;
        routed[edge.to().as_usize()] -= amount;
    }
    let scale = source_scale(graph, required)?;
    let mut auxiliary_rows = Vec::new();
    for (node, routed) in routed.iter().enumerate() {
        let difference = routed - BigRational::from_integer(BigInt::from(required[node]));
        if difference.is_zero() {
            continue;
        }
        let edge_id = unique_edge_id(graph, &unresolved, &format!("flow-framework-aux-{node}"))?;
        let amount = difference.abs();
        let doubled = (&amount * BigInt::from(2))
            .to_integer()
            .to_u64()
            .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)?;
        let node_id = graph.nodes()[node].id().clone();
        let (from, to) = if difference.is_positive() {
            (auxiliary_id.clone(), node_id)
        } else {
            (node_id, auxiliary_id.clone())
        };
        unresolved.push(UnresolvedFlowEdge {
            id: edge_id.clone(),
            from,
            to,
            lower: 0,
            capacity: doubled,
            cost: scale,
        });
        auxiliary_rows.push((edge_id, amount));
    }
    let mut nodes = graph.nodes().to_vec();
    if !auxiliary_rows.is_empty() {
        nodes.push(FlowNode::new(auxiliary_id, 0));
    }
    let augmented = FlowNetwork::new(nodes, unresolved)?;
    let mut flow = vec![BigRational::zero(); augmented.edges().len()];
    let mut original_to_augmented = Vec::with_capacity(graph.edges().len());
    for (edge, amount) in graph.edges().iter().zip(midpoint) {
        let index = augmented
            .edge_index(edge.id())
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?
            .as_usize();
        flow[index] = amount;
        original_to_augmented.push(index);
    }
    let mut auxiliary_edges = Vec::with_capacity(auxiliary_rows.len());
    for (edge, amount) in auxiliary_rows {
        let index = augmented
            .edge_index(&edge)
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?
            .as_usize();
        flow[index] = amount;
        auxiliary_edges.push(index);
    }
    let mut augmented_required = vec![0_i128; augmented.nodes().len()];
    for (node, &value) in graph.nodes().iter().zip(required) {
        let index = augmented
            .node_index(node.id())
            .ok_or(FlowFrameworkMcfError::NumericalInvariant)?
            .as_usize();
        augmented_required[index] = value;
    }
    if rational_divergence(&augmented, &flow) != rational_required(&augmented_required) {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(InitialPoint {
        graph: augmented,
        required: augmented_required,
        flow,
        original_to_augmented,
        auxiliary_edges,
    })
}

fn source_scale(graph: &FlowNetwork, required: &[i128]) -> Result<i64, FlowFrameworkMcfError> {
    let maximum_requirement = required
        .iter()
        .map(|value| u64::try_from(value.unsigned_abs()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| FlowFrameworkMcfError::ArithmeticOverflow)?
        .into_iter()
        .max();
    let u = graph
        .edges()
        .iter()
        .map(|edge| edge.capacity().max(edge.cost().unsigned_abs()))
        .chain(maximum_requirement)
        .max()
        .unwrap_or(1)
        .max(1);
    let value = 4_i128
        .checked_mul(graph.edges().len() as i128)
        .and_then(|value| value.checked_mul(i128::from(u)))
        .and_then(|value| value.checked_mul(i128::from(u)))
        .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)?;
    i64::try_from(value).map_err(|_| FlowFrameworkMcfError::ArithmeticOverflow)
}

fn unique_node_id(graph: &FlowNetwork, stem: &str) -> Result<NodeId, FlowModelError> {
    for suffix in 0..=graph.nodes().len() {
        let value = if suffix == 0 {
            stem.to_owned()
        } else {
            format!("{stem}-{suffix}")
        };
        let id = NodeId::parse(&value)?;
        if graph.node_index(&id).is_none() {
            return Ok(id);
        }
    }
    Err(FlowModelError::DuplicateNode)
}

fn unique_edge_id(
    graph: &FlowNetwork,
    unresolved: &[UnresolvedFlowEdge],
    stem: &str,
) -> Result<EdgeId, FlowModelError> {
    for suffix in 0..=unresolved.len() {
        let value = if suffix == 0 {
            stem.to_owned()
        } else {
            format!("{stem}-{suffix}")
        };
        let id = EdgeId::parse(&value)?;
        if graph.edge_index(&id).is_none() && unresolved.iter().all(|edge| edge.id != id) {
            return Ok(id);
        }
    }
    Err(FlowModelError::DuplicateEdge)
}

fn evaluate(
    initial: &InitialPoint,
    flow: &[BigRational],
    optimum_cost: i128,
) -> Result<AlphaPowerIpmEvaluation, FlowFrameworkMcfError> {
    let projected = flow
        .iter()
        .map(|value| {
            value
                .to_f64()
                .ok_or(FlowFrameworkMcfError::NumericalInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    evaluate_alpha_power_ipm(
        &initial.graph,
        &vec![true; initial.graph.edges().len()],
        &projected,
        optimum_cost as f64,
    )
    .map_err(|_| FlowFrameworkMcfError::NumericalInvariant)
}

fn rows_from_evaluation(
    graph: &FlowNetwork,
    evaluation: &AlphaPowerIpmEvaluation,
) -> Result<Vec<DynamicCoreGraphStageEdge>, FlowFrameworkMcfError> {
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            Ok(DynamicCoreGraphStageEdge {
                edge: edge_index,
                from: edge.from().as_usize(),
                to: edge.to().as_usize(),
                length: quantize_positive(evaluation.lengths[edge_index])?,
                gradient: quantize(evaluation.gradients[edge_index])?,
            })
        })
        .collect()
}

fn shifted_graph(graph: &FlowNetwork, rows: &[DynamicCoreGraphStageEdge]) -> ShiftedTreeChainGraph {
    ShiftedTreeChainGraph {
        node_count: graph.nodes().len(),
        edges: rows
            .iter()
            .map(|row| ShiftedTreeChainEdge {
                source_edge: row.edge,
                from: row.from,
                to: row.to,
                length: row.length.clone(),
                gradient: row.gradient.clone(),
            })
            .collect(),
    }
}

fn attribute_updates(
    before: &[DynamicCoreGraphStageEdge],
    after: &[DynamicCoreGraphStageEdge],
) -> Vec<DynamicCoreGraphStageUpdate> {
    before
        .iter()
        .zip(after)
        .filter(|(before, after)| {
            before.length != after.length || before.gradient != after.gradient
        })
        .map(
            |(before, after)| DynamicCoreGraphStageUpdate::ReplaceAttributes {
                before: before.clone(),
                after: after.clone(),
            },
        )
        .collect()
}

fn quantize(value: f64) -> Result<BigRational, FlowFrameworkMcfError> {
    if !value.is_finite() {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    let scaled = (value * QUANTIZATION_DENOMINATOR as f64).round();
    let numerator = scaled
        .to_bigint()
        .ok_or(FlowFrameworkMcfError::NumericalInvariant)?;
    Ok(BigRational::new(
        numerator,
        BigInt::from(QUANTIZATION_DENOMINATOR),
    ))
}

fn quantize_positive(value: f64) -> Result<BigRational, FlowFrameworkMcfError> {
    let value = quantize(value)?;
    if value <= BigRational::zero() {
        return Err(FlowFrameworkMcfError::NumericalInvariant);
    }
    Ok(value)
}

fn rational_divergence(graph: &FlowNetwork, flow: &[BigRational]) -> Vec<BigRational> {
    let mut divergence = vec![BigRational::zero(); graph.nodes().len()];
    for (edge, amount) in graph.edges().iter().zip(flow) {
        divergence[edge.from().as_usize()] += amount;
        divergence[edge.to().as_usize()] -= amount;
    }
    divergence
}

fn rational_required(required: &[i128]) -> Vec<BigRational> {
    required
        .iter()
        .map(|value| BigRational::from_integer(BigInt::from(*value)))
        .collect()
}

fn integral_cost(graph: &FlowNetwork, flow: &[u64]) -> Result<i128, FlowFrameworkMcfError> {
    graph
        .edges()
        .iter()
        .zip(flow)
        .try_fold(0_i128, |sum, (edge, &amount)| {
            i128::from(amount)
                .checked_mul(i128::from(edge.cost()))
                .and_then(|term| sum.checked_add(term))
                .ok_or(FlowFrameworkMcfError::ArithmeticOverflow)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network() -> FlowNetwork {
        let nodes = ["a", "s", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edges = [
            ("sa", "s", "a", 1),
            ("at", "a", "t", 1),
            ("st", "s", "t", 5),
        ]
        .into_iter()
        .map(|(id, from, to, cost)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower: 0,
            capacity: 3,
            cost,
        })
        .collect();
        FlowNetwork::new(nodes, edges).expect("network")
    }

    #[test]
    fn one_atomic_iteration_uses_detect_refresh_query_and_source_progress() {
        let graph = network();
        let mut session = FlowFrameworkMcfSession::new(&graph, &[0, 3, -3]).expect("session");
        let before = session.snapshot().expect("before");
        let iteration = session.step().expect("step");
        let after = session.snapshot().expect("after");
        assert_eq!(iteration.iteration, 1);
        assert!(!iteration.reinitialized);
        assert!(iteration.detected_edges.is_empty());
        assert!(iteration.refreshed_edges.is_empty());
        assert!(iteration.potential_after.get() < iteration.potential_before.get());
        assert!(after.gap.get() < before.gap.get());
        assert!(iteration.exact_gap_after < iteration.exact_gap_before);
        assert!(after.exact_gap > flow_framework_mcf_stopping_gap());
        assert!(session.round_if_source_ready().expect("rounding").is_none());
    }

    #[test]
    fn source_initial_point_adds_strict_auxiliary_midpoint_edges_when_needed() {
        let graph = network();
        let initial = build_source_initial_point(&graph, &[0, 2, -2]).expect("initial");
        assert!(!initial.auxiliary_edges.is_empty());
        assert_eq!(
            rational_divergence(&initial.graph, &initial.flow),
            rational_required(&initial.required)
        );
        for (&edge, amount) in initial.auxiliary_edges.iter().zip(
            initial
                .auxiliary_edges
                .iter()
                .map(|&edge| &initial.flow[edge]),
        ) {
            let row = &initial.graph.edges()[edge];
            assert!(amount > &BigRational::zero());
            assert!(amount < &BigRational::from_integer(BigInt::from(row.capacity())));
        }
    }

    #[test]
    fn repeated_atomic_iterations_reach_an_independently_checked_optimum() {
        let graph = network();
        let mut session = FlowFrameworkMcfSession::new(&graph, &[0, 3, -3]).expect("session");
        for _ in 0..128 {
            if let Some(solution) = session.round_if_source_ready().expect("rounding") {
                assert_eq!(solution.flows, vec![3, 3, 0]);
                assert_eq!(solution.total_cost, 6);
                assert!(
                    session.snapshot().expect("final snapshot").exact_gap
                        <= flow_framework_mcf_stopping_gap()
                );
                return;
            }
            session
                .step()
                .unwrap_or_else(|error| panic!("step {}: {error:?}", session.iterations + 1));
        }
        panic!("bounded source iterations did not reach the exact optimum");
    }

    #[test]
    fn complete_fast_driver_reaches_the_same_certified_optimum() {
        let graph = network();
        let result = execute_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("complete run");
        assert!(result.iterations > 1);
        assert_eq!(
            result.termination,
            FlowFrameworkMcfTermination::SourceAdditiveHalfGap
        );
        assert!(result.final_snapshot.exact_gap <= flow_framework_mcf_stopping_gap());
        assert_eq!(result.final_snapshot.optimum_cost, 6);
        assert_eq!(
            result
                .final_snapshot
                .augmented_edges
                .iter()
                .map(|edge| edge.flow.clone())
                .collect::<Vec<_>>(),
            result.final_snapshot.augmented_flow
        );
        assert_eq!(
            result
                .final_snapshot
                .augmented_nodes
                .iter()
                .map(|node| node.required_divergence)
                .collect::<Vec<_>>(),
            rational_divergence(
                &build_source_initial_point(&graph, &[0, 3, -3])
                    .expect("source point")
                    .graph,
                &result.final_snapshot.augmented_flow,
            )
            .into_iter()
            .map(|value| value.to_integer().to_i128().expect("integral divergence"))
            .collect::<Vec<_>>()
        );
        assert_eq!(result.solution.flows, vec![3, 3, 0]);
        assert_eq!(result.solution.total_cost, 6);
    }

    #[test]
    fn traced_driver_matches_fast_and_checks_every_nested_iteration() {
        let graph = network();
        let fast = execute_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("fast");
        let traced = trace_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("trace");
        assert_eq!(traced.result, fast);
        assert_eq!(traced.iterations.len() as u64, fast.iterations);
        assert!(
            traced
                .iterations
                .iter()
                .all(|iteration| iteration.source.exact_gap_before
                    > flow_framework_mcf_stopping_gap()
                    && iteration.source.exact_gap_after < iteration.source.exact_gap_before)
        );
        assert!(
            traced
                .iterations
                .iter()
                .all(|iteration| !iteration.dynamic_trace.events.is_empty())
        );
        check_flow_framework_mcf_trace(&graph, &[0, 3, -3], 128, &traced).expect("check");
    }

    #[test]
    fn exact_optimal_rounding_does_not_bypass_the_source_final_point() {
        let graph = network();
        let mut session = FlowFrameworkMcfSession::new(&graph, &[0, 3, -3]).expect("session");
        let mut premature_rounding_iteration = None;
        for _ in 0..128 {
            let flow = session.current_flow();
            let gap = exact_augmented_cost_gap(&session.initial, &flow, session.optimum_cost)
                .expect("exact gap");
            let raw_rounding =
                round_costed_flow(&session.initial.graph, &session.initial.required, &flow)
                    .expect("raw rounding");
            if raw_rounding.total_cost == session.optimum_cost
                && gap > flow_framework_mcf_stopping_gap()
            {
                premature_rounding_iteration = Some(session.iterations);
                assert!(
                    session
                        .round_if_source_ready()
                        .expect("source-gated rounding")
                        .is_none()
                );
                break;
            }
            session.step().expect("source step");
        }
        let premature_rounding_iteration =
            premature_rounding_iteration.expect("fixture must expose premature exact rounding");
        let result = execute_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("complete run");
        assert!(result.iterations > premature_rounding_iteration);
    }

    #[test]
    fn outer_trace_checker_rejects_a_changed_source_scalar() {
        let graph = network();
        let mut traced = trace_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("trace");
        let first = traced.iterations.first_mut().expect("source iteration");
        first.source.potential_after =
            FlowFrameworkMcfScalar::new(first.source.potential_after.get() + 1.0)
                .expect("finite corruption");
        assert_eq!(
            check_flow_framework_mcf_trace(&graph, &[0, 3, -3], 128, &traced),
            Err(FlowFrameworkMcfError::TraceVerification)
        );
    }

    #[test]
    fn outer_trace_checker_rejects_a_changed_exact_gap() {
        let graph = network();
        let mut traced = trace_flow_framework_mcf(&graph, &[0, 3, -3], 128).expect("trace");
        let first = traced.iterations.first_mut().expect("source iteration");
        first.source.exact_gap_after += BigRational::from_integer(BigInt::from(1));
        assert_eq!(
            check_flow_framework_mcf_trace(&graph, &[0, 3, -3], 128, &traced),
            Err(FlowFrameworkMcfError::TraceVerification)
        );
    }

    #[test]
    fn strict_interior_and_public_iteration_bands_fail_closed() {
        let graph = network();
        assert_eq!(
            execute_flow_framework_mcf(&graph, &[0, 3, -3], 0),
            Err(FlowFrameworkMcfError::AdmissionLimit)
        );
        let nodes = ["s", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edges = vec![UnresolvedFlowEdge {
            id: EdgeId::parse("fixed").expect("edge"),
            from: NodeId::parse("s").expect("tail"),
            to: NodeId::parse("t").expect("head"),
            lower: 1,
            capacity: 1,
            cost: 0,
        }];
        let fixed = FlowNetwork::new(nodes, edges).expect("fixed network");
        assert!(matches!(
            FlowFrameworkMcfSession::new(&fixed, &[1, -1]),
            Err(FlowFrameworkMcfError::AdmissionLimit)
        ));
    }
}
