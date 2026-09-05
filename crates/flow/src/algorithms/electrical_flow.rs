//! Bounded electrical-flow primitive from Christiano et al. §2.3.
//!
//! The paper defines an electrical flow of value `F` as the unique flow that
//! minimizes `sum_e r_e f_e^2` subject to flow conservation.  Its maximum-flow
//! oracle later chooses resistances proportional to `1 / u_e^2`.  This module
//! exposes that source-level primitive for one unit of current with
//! `r_e = 1 / u_e^2`.  Input arcs are arbitrary orientations of undirected
//! resistors; returned currents may therefore be negative and are not integer
//! capacity-feasible maximum-flow values.
//!
//! The visible computation is deterministic Jacobi-preconditioned conjugate
//! gradient on the sink-grounded Laplacian.  A separate arbitrary-precision
//! rational Gauss--Jordan solve checks Kirchhoff conservation, Ohm's law,
//! effective resistance, and the minimum-energy certificate.  The bounded
//! dense realization deliberately does not claim the paper's nearly-linear
//! Laplacian-solver running time.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};
use thiserror::Error;

use crate::model::{EdgeId, FlowNetwork, NodeIndex};

/// Conservative node limit for dense exact cross-checking.
pub const ELECTRICAL_FLOW_MAX_NODES: usize = 24;
/// Conservative edge limit for trace projection and exact arithmetic.
pub const ELECTRICAL_FLOW_MAX_EDGES: usize = 96;
/// Largest admitted capacity; keeps the floating system well scaled.
pub const ELECTRICAL_FLOW_MAX_CAPACITY: u64 = 1_000_000;
/// Relative Euclidean residual tolerance for the visible PCG solve.
pub const ELECTRICAL_FLOW_RELATIVE_TOLERANCE: f64 = 1.0e-10;
/// Maximum PCG iterations per non-ground node.
pub const ELECTRICAL_FLOW_ITERATION_MULTIPLIER: usize = 8;
/// Preserve every small matrix product and geometric progress on larger inputs.
const ELECTRICAL_FLOW_TRACE_SCALAR_PREFIX: u128 = 512;

/// Finite IEEE-754 value with exact, replay-safe bit identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ElectricalScalar(u64);

impl ElectricalScalar {
    fn try_new(value: f64) -> Result<Self, ElectricalFlowError> {
        if !value.is_finite() {
            return Err(ElectricalFlowError::NumericalFailure);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    /// Recovers the finite floating value.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Stable decimal projection used by the JSON scene contract.
    #[must_use]
    pub fn decimal(self) -> String {
        super::stable_scene_decimal(self.get())
    }
}

/// Canonical arbitrary-precision rational witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalExactRational {
    /// Signed reduced numerator.
    pub numerator: BigInt,
    /// Positive reduced denominator.
    pub denominator: BigInt,
}

impl From<&BigRational> for ElectricalExactRational {
    fn from(value: &BigRational) -> Self {
        Self {
            numerator: value.numer().clone(),
            denominator: value.denom().clone(),
        }
    }
}

/// Source-level boundary of the bounded primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElectricalFlowStage {
    /// Valid input is visible; no linear system has been published.
    Ready,
    /// Conductances and the sink-grounded Laplacian were assembled.
    AssembleLaplacian,
    /// Zero potential, residual, and Jacobi search direction were initialized.
    InitializeConjugateGradient,
    /// One complete preconditioned conjugate-gradient iteration committed.
    ConjugateGradientIteration,
    /// Oriented currents and per-edge energy were recovered by Ohm's law.
    RecoverCurrents,
    /// An independent exact rational solve agreed with the visible state.
    CheckExactReference,
    /// The primitive certificate is complete; this is not a max-flow result.
    Complete,
}

/// One arbitrarily oriented undirected resistor at a public boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalEdgeState {
    /// Stable original edge identity.
    pub edge: EdgeId,
    /// Exact integer conductance `u_e^2`.
    pub conductance: u128,
    /// Oriented potential drop from stored tail to stored head.
    pub voltage_drop: ElectricalScalar,
    /// Signed current relative to the stored orientation.
    pub current: ElectricalScalar,
    /// Absolute source capacity congestion `|f_e| / u_e`.
    pub congestion: ElectricalScalar,
    /// Edge energy `r_e f_e^2`.
    pub energy: ElectricalScalar,
}

/// Exact bounded work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElectricalFlowMetrics {
    /// Weighted Laplacian assemblies.
    pub laplacian_assemblies: u64,
    /// Grounded matrix dimension.
    pub grounded_dimension: u64,
    /// Completed preconditioned conjugate-gradient iterations.
    pub conjugate_gradient_iterations: u64,
    /// Dense grounded matrix-vector products.
    pub matrix_vector_products: u64,
    /// Scalar multiply-accumulates performed by dense matrix-vector products.
    pub matrix_scalar_products: u128,
    /// Original edges inspected while assembling or recovering state.
    pub edge_scans: u128,
    /// Exact rational elimination pivots.
    pub exact_elimination_pivots: u64,
    /// Independent terminal certificate checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete numerical state at one reversible boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalFlowSnapshot {
    /// Node potentials in canonical node order; the sink is grounded at zero.
    pub potentials: Vec<ElectricalScalar>,
    /// Grounded linear-system residual projected to canonical node order.
    pub residuals: Vec<ElectricalScalar>,
    /// Current PCG search direction projected to canonical node order.
    pub search_directions: Vec<ElectricalScalar>,
    /// Canonical original-edge electrical state.
    pub edges: Vec<ElectricalEdgeState>,
    /// Completed PCG iteration count.
    pub iteration: u64,
    /// Euclidean norm of the grounded residual.
    pub residual_l2: ElectricalScalar,
    /// Sum of all recovered edge energies.
    pub total_energy: ElectricalScalar,
    /// Unit-current effective resistance `phi(s) - phi(t)`.
    pub effective_resistance: ElectricalScalar,
    /// Exact effective resistance after the reference solve.
    pub exact_effective_resistance: Option<ElectricalExactRational>,
    /// Largest potential/current error against the exact reference.
    pub maximum_absolute_error: Option<ElectricalScalar>,
    /// Whether the visible PCG residual met the closed tolerance.
    pub converged: bool,
    /// Source-level boundary.
    pub stage: ElectricalFlowStage,
    /// Exact work counters.
    pub metrics: ElectricalFlowMetrics,
}

/// One reversible numerical transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalFlowTraceEvent {
    /// Stable event identity.
    pub catalog_id: &'static str,
    /// Grounded matrix row/column nodes touched by this source operation.
    pub active_nodes: Vec<NodeIndex>,
    /// State before the atomic transition.
    pub before: ElectricalFlowSnapshot,
    /// State after the atomic transition.
    pub after: ElectricalFlowSnapshot,
}

/// Independently checked primitive result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalFlowResult {
    /// Terminal numerical state.
    pub final_snapshot: ElectricalFlowSnapshot,
    /// Exact unit-current effective resistance.
    pub exact_effective_resistance: ElectricalExactRational,
    /// Exact work counters.
    pub metrics: ElectricalFlowMetrics,
}

/// Result plus every visible Laplacian/PCG/certificate boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElectricalFlowTraceResult {
    /// Same checked primitive result as the fast profile.
    pub result: ElectricalFlowResult,
    /// Ready boundary before matrix assembly.
    pub base_snapshot: ElectricalFlowSnapshot,
    /// Canonical reversible transitions.
    pub events: Vec<ElectricalFlowTraceEvent>,
    /// Terminal boundary, equal to `result.final_snapshot`.
    pub final_snapshot: ElectricalFlowSnapshot,
}

/// Admission, graph-contract, numerical, exact-oracle, or replay failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ElectricalFlowError {
    /// Input exceeds the conservative dense interactive band.
    #[error("electrical-flow input exceeds admission limits")]
    AdmissionLimit,
    /// The directed graph cannot serve as the source-defined resistor model.
    #[error(
        "electrical-flow requires zero lower bounds, costs, supplies, and positive bounded capacities"
    )]
    GraphRequirement,
    /// Source and sink are equal or disconnected in the undirected resistor graph.
    #[error("electrical-flow terminals must be distinct and connected")]
    DisconnectedTerminals,
    /// PCG produced a nonfinite or nonpositive-definite intermediate value.
    #[error("electrical-flow numerical invariant failed")]
    NumericalFailure,
    /// The deterministic PCG ceiling was reached before convergence.
    #[error("electrical-flow conjugate gradient did not converge within the bounded ceiling")]
    NonConvergence,
    /// The independent rational system was singular or malformed.
    #[error("electrical-flow exact rational reference solve failed")]
    ExactReferenceFailure,
    /// KCL, Ohm, energy, or exact-reference agreement failed.
    #[error("electrical-flow certificate failed")]
    CertificateFailure,
    /// A supplied trace differs from deterministic replay.
    #[error("electrical-flow trace verification failed")]
    TraceVerification,
}

/// Solves one bounded unit-current electrical-flow primitive.
///
/// # Errors
///
/// Rejects unsupported/oversized resistor graphs, disconnected terminals,
/// numerical non-convergence, or any independent certificate disagreement.
pub fn solve_electrical_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<ElectricalFlowResult, ElectricalFlowError> {
    run_internal(graph, source, sink, false).map(|run| run.result)
}

/// Records every matrix, PCG iteration, recovery, and certificate boundary.
///
/// # Errors
///
/// Returns the same errors as [`solve_electrical_flow`] or replay failure.
pub fn trace_electrical_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<ElectricalFlowTraceResult, ElectricalFlowError> {
    let run = run_internal(graph, source, sink, true)?;
    let trace = ElectricalFlowTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_electrical_flow_trace(graph, source, sink, &trace)?;
    Ok(trace)
}

/// Checks continuity, stage identity, the terminal certificate, and replay.
///
/// # Errors
///
/// Rejects any malformed boundary or disagreement with a fresh deterministic
/// source-level run.
pub fn check_electrical_flow_trace(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    trace: &ElectricalFlowTraceResult,
) -> Result<(), ElectricalFlowError> {
    validate_graph(graph, source, sink)?;
    validate_base(graph, &trace.base_snapshot)?;
    if trace.events.len() < 6 {
        return Err(ElectricalFlowError::TraceVerification);
    }
    let mut previous = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != previous {
            return Err(ElectricalFlowError::TraceVerification);
        }
        previous = &event.after;
    }
    if previous != &trace.final_snapshot
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.result.metrics != trace.final_snapshot.metrics
        || trace.result.exact_effective_resistance
            != trace
                .final_snapshot
                .exact_effective_resistance
                .clone()
                .ok_or(ElectricalFlowError::TraceVerification)?
    {
        return Err(ElectricalFlowError::TraceVerification);
    }
    let stages = trace
        .events
        .iter()
        .map(|event| event.after.stage)
        .collect::<Vec<_>>();
    if stages.first() != Some(&ElectricalFlowStage::AssembleLaplacian)
        || stages.get(1) != Some(&ElectricalFlowStage::InitializeConjugateGradient)
        || stages
            .iter()
            .skip(2)
            .take(stages.len().saturating_sub(5))
            .any(|stage| *stage != ElectricalFlowStage::ConjugateGradientIteration)
        || stages.get(stages.len() - 3) != Some(&ElectricalFlowStage::RecoverCurrents)
        || stages.get(stages.len() - 2) != Some(&ElectricalFlowStage::CheckExactReference)
        || stages.last() != Some(&ElectricalFlowStage::Complete)
    {
        return Err(ElectricalFlowError::TraceVerification);
    }
    validate_terminal(graph, source, sink, &trace.result)?;
    let replay = run_internal(graph, source, sink, true)?;
    if replay.base_snapshot != trace.base_snapshot
        || replay.events != trace.events
        || replay.result != trace.result
    {
        return Err(ElectricalFlowError::TraceVerification);
    }
    Ok(())
}

struct InternalRun {
    result: ElectricalFlowResult,
    base_snapshot: ElectricalFlowSnapshot,
    events: Vec<ElectricalFlowTraceEvent>,
}

struct Recorder {
    current: ElectricalFlowSnapshot,
    events: Vec<ElectricalFlowTraceEvent>,
    enabled: bool,
}

type DenseGroundedSystem = (Vec<Vec<f64>>, Vec<f64>, u128);

impl Recorder {
    fn emit<F>(&mut self, catalog_id: &'static str, update: F) -> Result<(), ElectricalFlowError>
    where
        F: FnOnce(&mut ElectricalFlowSnapshot) -> Result<(), ElectricalFlowError>,
    {
        self.emit_with_nodes(catalog_id, Vec::new(), update)
    }

    fn emit_with_nodes<F>(
        &mut self,
        catalog_id: &'static str,
        active_nodes: Vec<NodeIndex>,
        update: F,
    ) -> Result<(), ElectricalFlowError>
    where
        F: FnOnce(&mut ElectricalFlowSnapshot) -> Result<(), ElectricalFlowError>,
    {
        let before = self.enabled.then(|| self.current.clone());
        update(&mut self.current)?;
        self.current.metrics.state_transitions = self
            .current
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(ElectricalFlowError::NumericalFailure)?;
        if self.enabled {
            self.events.push(ElectricalFlowTraceEvent {
                catalog_id,
                active_nodes,
                before: before.ok_or(ElectricalFlowError::TraceVerification)?,
                after: self.current.clone(),
            });
        }
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    record_events: bool,
) -> Result<InternalRun, ElectricalFlowError> {
    validate_graph(graph, source, sink)?;
    let node_count = graph.nodes().len();
    let grounded = grounded_nodes(node_count, sink.as_usize());
    let grounded_index = inverse_indices(node_count, &grounded);
    let dimension = grounded.len();
    let zero_nodes = scalar_vector(node_count, 0.0)?;
    let zero_edges = graph
        .edges()
        .iter()
        .map(|edge| {
            Ok(ElectricalEdgeState {
                edge: edge.id().clone(),
                conductance: u128::from(edge.capacity()) * u128::from(edge.capacity()),
                voltage_drop: ElectricalScalar::try_new(0.0)?,
                current: ElectricalScalar::try_new(0.0)?,
                congestion: ElectricalScalar::try_new(0.0)?,
                energy: ElectricalScalar::try_new(0.0)?,
            })
        })
        .collect::<Result<Vec<_>, ElectricalFlowError>>()?;
    let base_snapshot = ElectricalFlowSnapshot {
        potentials: zero_nodes.clone(),
        residuals: zero_nodes.clone(),
        search_directions: zero_nodes,
        edges: zero_edges,
        iteration: 0,
        residual_l2: ElectricalScalar::try_new(0.0)?,
        total_energy: ElectricalScalar::try_new(0.0)?,
        effective_resistance: ElectricalScalar::try_new(0.0)?,
        exact_effective_resistance: None,
        maximum_absolute_error: None,
        converged: false,
        stage: ElectricalFlowStage::Ready,
        metrics: ElectricalFlowMetrics::default(),
    };
    let mut recorder = Recorder {
        current: base_snapshot.clone(),
        events: Vec::new(),
        enabled: record_events,
    };

    let (matrix, diagonal, assembly_scans) =
        assemble_grounded_laplacian(graph, &grounded_index, sink.as_usize(), dimension)?;
    recorder.emit("electrical-flow.assemble-laplacian", |snapshot| {
        snapshot.stage = ElectricalFlowStage::AssembleLaplacian;
        snapshot.metrics.laplacian_assemblies = 1;
        snapshot.metrics.grounded_dimension =
            u64::try_from(dimension).map_err(|_| ElectricalFlowError::NumericalFailure)?;
        snapshot.metrics.edge_scans = assembly_scans;
        Ok(())
    })?;

    let source_grounded =
        grounded_index[source.as_usize()].ok_or(ElectricalFlowError::DisconnectedTerminals)?;
    let mut potentials = vec![0.0_f64; dimension];
    let mut residual = vec![0.0_f64; dimension];
    residual[source_grounded] = 1.0;
    let mut preconditioned = jacobi_precondition(&residual, &diagonal)?;
    let mut direction = preconditioned.clone();
    let mut rz = dot(&residual, &preconditioned)?;
    recorder.emit("electrical-flow.initialize-cg", |snapshot| {
        snapshot.stage = ElectricalFlowStage::InitializeConjugateGradient;
        snapshot.residuals = project_grounded(&residual, &grounded, node_count)?;
        snapshot.search_directions = project_grounded(&direction, &grounded, node_count)?;
        snapshot.residual_l2 = ElectricalScalar::try_new(norm(&residual)?)?;
        Ok(())
    })?;

    let iteration_limit = dimension
        .checked_mul(ELECTRICAL_FLOW_ITERATION_MULTIPLIER)
        .ok_or(ElectricalFlowError::NumericalFailure)?;
    let dense_product_width = u128::try_from(dimension)
        .ok()
        .and_then(|dimension| dimension.checked_mul(dimension))
        .ok_or(ElectricalFlowError::NumericalFailure)?;
    let mut converged = norm(&residual)? <= ELECTRICAL_FLOW_RELATIVE_TOLERANCE;
    for iteration in 1..=iteration_limit {
        if converged {
            break;
        }
        let product = matrix_vector_product(
            &matrix,
            &direction,
            recorder.current.metrics.matrix_scalar_products,
        )?;
        for checkpoint in &product.checkpoints {
            let mut active_nodes = vec![
                NodeIndex::try_from_usize(grounded[checkpoint.row])
                    .ok_or(ElectricalFlowError::NumericalFailure)?,
                NodeIndex::try_from_usize(grounded[checkpoint.column])
                    .ok_or(ElectricalFlowError::NumericalFailure)?,
            ];
            active_nodes.dedup();
            recorder.emit_with_nodes(
                "electrical-flow.matrix-scalar-product",
                active_nodes,
                |snapshot| {
                    snapshot.stage = ElectricalFlowStage::ConjugateGradientIteration;
                    snapshot.metrics.matrix_scalar_products = checkpoint.completed;
                    Ok(())
                },
            )?;
        }
        let denominator = dot(&direction, &product.values)?;
        if denominator <= 0.0 || rz <= 0.0 {
            return Err(ElectricalFlowError::NumericalFailure);
        }
        let alpha = rz / denominator;
        axpy(&mut potentials, alpha, &direction)?;
        axpy(&mut residual, -alpha, &product.values)?;
        let residual_norm = norm(&residual)?;
        converged = residual_norm <= ELECTRICAL_FLOW_RELATIVE_TOLERANCE;
        if converged {
            direction.fill(0.0);
        } else {
            preconditioned = jacobi_precondition(&residual, &diagonal)?;
            let next_rz = dot(&residual, &preconditioned)?;
            if next_rz < 0.0 {
                return Err(ElectricalFlowError::NumericalFailure);
            }
            let beta = next_rz / rz;
            for (direction_value, &preconditioned_value) in
                direction.iter_mut().zip(&preconditioned)
            {
                *direction_value = preconditioned_value + beta * *direction_value;
                ensure_finite(*direction_value)?;
            }
            rz = next_rz;
        }
        recorder.emit("electrical-flow.cg-iteration", |snapshot| {
            snapshot.stage = ElectricalFlowStage::ConjugateGradientIteration;
            snapshot.iteration =
                u64::try_from(iteration).map_err(|_| ElectricalFlowError::NumericalFailure)?;
            snapshot.potentials = project_grounded(&potentials, &grounded, node_count)?;
            snapshot.residuals = project_grounded(&residual, &grounded, node_count)?;
            snapshot.search_directions = project_grounded(&direction, &grounded, node_count)?;
            snapshot.residual_l2 = ElectricalScalar::try_new(residual_norm)?;
            snapshot.converged = converged;
            snapshot.metrics.conjugate_gradient_iterations = snapshot.iteration;
            snapshot.metrics.matrix_vector_products = snapshot.iteration;
            snapshot.metrics.matrix_scalar_products = u128::from(snapshot.iteration)
                .checked_mul(dense_product_width)
                .ok_or(ElectricalFlowError::NumericalFailure)?;
            Ok(())
        })?;
    }
    if !converged {
        return Err(ElectricalFlowError::NonConvergence);
    }

    let projected_potentials = project_grounded(&potentials, &grounded, node_count)?;
    let (edge_states, total_energy, recovery_scans) =
        recover_edge_states(graph, &projected_potentials)?;
    let effective_resistance =
        projected_potentials[source.as_usize()].get() - projected_potentials[sink.as_usize()].get();
    recorder.emit("electrical-flow.recover-currents", |snapshot| {
        snapshot.stage = ElectricalFlowStage::RecoverCurrents;
        snapshot.edges.clone_from(&edge_states);
        snapshot.total_energy = ElectricalScalar::try_new(total_energy)?;
        snapshot.effective_resistance = ElectricalScalar::try_new(effective_resistance)?;
        snapshot.metrics.edge_scans = snapshot
            .metrics
            .edge_scans
            .checked_add(recovery_scans)
            .ok_or(ElectricalFlowError::NumericalFailure)?;
        Ok(())
    })?;

    let exact = exact_reference(graph, source, sink, &grounded, &grounded_index)?;
    let maximum_error = compare_reference(&recorder.current, &exact)?;
    recorder.emit("electrical-flow.check-exact-reference", |snapshot| {
        snapshot.stage = ElectricalFlowStage::CheckExactReference;
        snapshot.exact_effective_resistance =
            Some(ElectricalExactRational::from(&exact.effective_resistance));
        snapshot.maximum_absolute_error = Some(ElectricalScalar::try_new(maximum_error)?);
        snapshot.metrics.exact_elimination_pivots =
            u64::try_from(exact.pivots).map_err(|_| ElectricalFlowError::NumericalFailure)?;
        snapshot.metrics.certificate_checks = 1;
        Ok(())
    })?;
    recorder.emit("electrical-flow.complete-primitive", |snapshot| {
        snapshot.stage = ElectricalFlowStage::Complete;
        Ok(())
    })?;

    let exact_effective_resistance = ElectricalExactRational::from(&exact.effective_resistance);
    let metrics = recorder.current.metrics;
    let result = ElectricalFlowResult {
        final_snapshot: recorder.current,
        exact_effective_resistance,
        metrics,
    };
    validate_terminal(graph, source, sink, &result)?;
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn validate_graph(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), ElectricalFlowError> {
    if graph.nodes().len() < 2
        || graph.nodes().len() > ELECTRICAL_FLOW_MAX_NODES
        || graph.edges().is_empty()
        || graph.edges().len() > ELECTRICAL_FLOW_MAX_EDGES
    {
        return Err(ElectricalFlowError::AdmissionLimit);
    }
    if source == sink
        || source.as_usize() >= graph.nodes().len()
        || sink.as_usize() >= graph.nodes().len()
    {
        return Err(ElectricalFlowError::DisconnectedTerminals);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph.edges().iter().any(|edge| {
            edge.lower() != 0
                || edge.cost() != 0
                || edge.capacity() == 0
                || edge.capacity() > ELECTRICAL_FLOW_MAX_CAPACITY
                || edge.from() == edge.to()
        })
    {
        return Err(ElectricalFlowError::GraphRequirement);
    }
    let mut seen = vec![false; graph.nodes().len()];
    let mut stack = vec![source.as_usize()];
    seen[source.as_usize()] = true;
    while let Some(node) = stack.pop() {
        for edge in graph.edges() {
            let neighbor = if edge.from().as_usize() == node {
                Some(edge.to().as_usize())
            } else if edge.to().as_usize() == node {
                Some(edge.from().as_usize())
            } else {
                None
            };
            if let Some(neighbor) = neighbor.filter(|&neighbor| !seen[neighbor]) {
                seen[neighbor] = true;
                stack.push(neighbor);
            }
        }
    }
    if !seen[sink.as_usize()] || seen.iter().any(|value| !value) {
        return Err(ElectricalFlowError::DisconnectedTerminals);
    }
    Ok(())
}

fn validate_base(
    graph: &FlowNetwork,
    snapshot: &ElectricalFlowSnapshot,
) -> Result<(), ElectricalFlowError> {
    if snapshot.stage != ElectricalFlowStage::Ready
        || snapshot.potentials.len() != graph.nodes().len()
        || snapshot.residuals.len() != graph.nodes().len()
        || snapshot.search_directions.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.iteration != 0
        || snapshot.converged
        || snapshot.exact_effective_resistance.is_some()
        || snapshot.maximum_absolute_error.is_some()
        || snapshot.metrics != ElectricalFlowMetrics::default()
        || snapshot.potentials.iter().any(|value| value.get() != 0.0)
        || snapshot.residuals.iter().any(|value| value.get() != 0.0)
        || snapshot
            .search_directions
            .iter()
            .any(|value| value.get() != 0.0)
    {
        return Err(ElectricalFlowError::TraceVerification);
    }
    Ok(())
}

fn grounded_nodes(node_count: usize, sink: usize) -> Vec<usize> {
    (0..node_count).filter(|&node| node != sink).collect()
}

fn inverse_indices(node_count: usize, grounded: &[usize]) -> Vec<Option<usize>> {
    let mut inverse = vec![None; node_count];
    for (index, &node) in grounded.iter().enumerate() {
        inverse[node] = Some(index);
    }
    inverse
}

fn scalar_vector(length: usize, value: f64) -> Result<Vec<ElectricalScalar>, ElectricalFlowError> {
    (0..length)
        .map(|_| ElectricalScalar::try_new(value))
        .collect()
}

fn project_grounded(
    values: &[f64],
    grounded: &[usize],
    node_count: usize,
) -> Result<Vec<ElectricalScalar>, ElectricalFlowError> {
    if values.len() != grounded.len() {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    let mut projected = scalar_vector(node_count, 0.0)?;
    for (&node, &value) in grounded.iter().zip(values) {
        projected[node] = ElectricalScalar::try_new(value)?;
    }
    Ok(projected)
}

fn assemble_grounded_laplacian(
    graph: &FlowNetwork,
    inverse: &[Option<usize>],
    sink: usize,
    dimension: usize,
) -> Result<DenseGroundedSystem, ElectricalFlowError> {
    let mut matrix = vec![vec![0.0_f64; dimension]; dimension];
    for edge in graph.edges() {
        let capacity = capacity_f64(edge.capacity())?;
        let conductance = capacity * capacity;
        ensure_finite(conductance)?;
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        if from != sink {
            let row = inverse[from].ok_or(ElectricalFlowError::NumericalFailure)?;
            matrix[row][row] += conductance;
        }
        if to != sink {
            let row = inverse[to].ok_or(ElectricalFlowError::NumericalFailure)?;
            matrix[row][row] += conductance;
        }
        if from != sink && to != sink {
            let left = inverse[from].ok_or(ElectricalFlowError::NumericalFailure)?;
            let right = inverse[to].ok_or(ElectricalFlowError::NumericalFailure)?;
            matrix[left][right] -= conductance;
            matrix[right][left] -= conductance;
        }
    }
    let diagonal = matrix
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let value = row[index];
            if value <= 0.0 || !value.is_finite() {
                Err(ElectricalFlowError::NumericalFailure)
            } else {
                Ok(value)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((matrix, diagonal, graph.edges().len() as u128))
}

fn jacobi_precondition(
    residual: &[f64],
    diagonal: &[f64],
) -> Result<Vec<f64>, ElectricalFlowError> {
    if residual.len() != diagonal.len() {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    residual
        .iter()
        .zip(diagonal)
        .map(|(&residual, &diagonal)| {
            let value = residual / diagonal;
            ensure_finite(value)?;
            Ok(value)
        })
        .collect()
}

struct MatrixProduct {
    values: Vec<f64>,
    checkpoints: Vec<MatrixScalarCheckpoint>,
}

struct MatrixScalarCheckpoint {
    row: usize,
    column: usize,
    completed: u128,
}

fn matrix_vector_product(
    matrix: &[Vec<f64>],
    vector: &[f64],
    completed_before: u128,
) -> Result<MatrixProduct, ElectricalFlowError> {
    if matrix.len() != vector.len() || matrix.iter().any(|row| row.len() != vector.len()) {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    let mut values = Vec::with_capacity(matrix.len());
    let mut checkpoints = Vec::new();
    let mut completed = completed_before;
    for (row_index, row) in matrix.iter().enumerate() {
        let mut value = 0.0_f64;
        for (column_index, (&coefficient, &entry)) in row.iter().zip(vector).enumerate() {
            value += coefficient * entry;
            ensure_finite(value)?;
            completed = completed
                .checked_add(1)
                .ok_or(ElectricalFlowError::NumericalFailure)?;
            if completed <= ELECTRICAL_FLOW_TRACE_SCALAR_PREFIX || completed.is_power_of_two() {
                checkpoints.push(MatrixScalarCheckpoint {
                    row: row_index,
                    column: column_index,
                    completed,
                });
            }
        }
        values.push(value);
    }
    Ok(MatrixProduct {
        values,
        checkpoints,
    })
}

fn dot(left: &[f64], right: &[f64]) -> Result<f64, ElectricalFlowError> {
    if left.len() != right.len() {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    let value = left
        .iter()
        .zip(right)
        .map(|(&left, &right)| left * right)
        .sum::<f64>();
    ensure_finite(value)?;
    Ok(value)
}

fn norm(vector: &[f64]) -> Result<f64, ElectricalFlowError> {
    let squared = dot(vector, vector)?;
    if squared < 0.0 {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    let value = squared.sqrt();
    ensure_finite(value)?;
    Ok(value)
}

fn axpy(target: &mut [f64], scalar: f64, vector: &[f64]) -> Result<(), ElectricalFlowError> {
    if target.len() != vector.len() {
        return Err(ElectricalFlowError::NumericalFailure);
    }
    for (target, &vector) in target.iter_mut().zip(vector) {
        *target += scalar * vector;
        ensure_finite(*target)?;
    }
    Ok(())
}

fn ensure_finite(value: f64) -> Result<(), ElectricalFlowError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ElectricalFlowError::NumericalFailure)
    }
}

fn recover_edge_states(
    graph: &FlowNetwork,
    potentials: &[ElectricalScalar],
) -> Result<(Vec<ElectricalEdgeState>, f64, u128), ElectricalFlowError> {
    let mut total_energy = 0.0_f64;
    let mut states = Vec::with_capacity(graph.edges().len());
    for edge in graph.edges() {
        let capacity = capacity_f64(edge.capacity())?;
        let conductance = capacity * capacity;
        let voltage_drop =
            potentials[edge.from().as_usize()].get() - potentials[edge.to().as_usize()].get();
        let current = conductance * voltage_drop;
        let congestion = current.abs() / capacity;
        let energy = current * current / conductance;
        ensure_finite(voltage_drop)?;
        ensure_finite(current)?;
        ensure_finite(congestion)?;
        ensure_finite(energy)?;
        total_energy += energy;
        ensure_finite(total_energy)?;
        states.push(ElectricalEdgeState {
            edge: edge.id().clone(),
            conductance: u128::from(edge.capacity()) * u128::from(edge.capacity()),
            voltage_drop: ElectricalScalar::try_new(voltage_drop)?,
            current: ElectricalScalar::try_new(current)?,
            congestion: ElectricalScalar::try_new(congestion)?,
            energy: ElectricalScalar::try_new(energy)?,
        });
    }
    Ok((states, total_energy, graph.edges().len() as u128))
}

fn capacity_f64(capacity: u64) -> Result<f64, ElectricalFlowError> {
    let capacity = u32::try_from(capacity).map_err(|_| ElectricalFlowError::NumericalFailure)?;
    Ok(f64::from(capacity))
}

struct ExactReference {
    potentials: Vec<BigRational>,
    currents: Vec<BigRational>,
    effective_resistance: BigRational,
    pivots: usize,
}

fn exact_reference(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    grounded: &[usize],
    inverse: &[Option<usize>],
) -> Result<ExactReference, ElectricalFlowError> {
    let dimension = grounded.len();
    let mut matrix = vec![vec![BigRational::zero(); dimension]; dimension];
    for edge in graph.edges() {
        let conductance = BigRational::from_integer(BigInt::from(
            u128::from(edge.capacity()) * u128::from(edge.capacity()),
        ));
        let from = edge.from().as_usize();
        let to = edge.to().as_usize();
        if from != sink.as_usize() {
            let row = inverse[from].ok_or(ElectricalFlowError::ExactReferenceFailure)?;
            matrix[row][row] += &conductance;
        }
        if to != sink.as_usize() {
            let row = inverse[to].ok_or(ElectricalFlowError::ExactReferenceFailure)?;
            matrix[row][row] += &conductance;
        }
        if from != sink.as_usize() && to != sink.as_usize() {
            let left = inverse[from].ok_or(ElectricalFlowError::ExactReferenceFailure)?;
            let right = inverse[to].ok_or(ElectricalFlowError::ExactReferenceFailure)?;
            matrix[left][right] -= &conductance;
            matrix[right][left] -= &conductance;
        }
    }
    let mut rhs = vec![BigRational::zero(); dimension];
    rhs[inverse[source.as_usize()].ok_or(ElectricalFlowError::ExactReferenceFailure)?] =
        BigRational::one();
    let (grounded_potentials, pivots) = gauss_jordan(matrix, rhs)?;
    let mut potentials = vec![BigRational::zero(); graph.nodes().len()];
    for (&node, potential) in grounded.iter().zip(grounded_potentials) {
        potentials[node] = potential;
    }
    let currents = graph
        .edges()
        .iter()
        .map(|edge| {
            let conductance = BigRational::from_integer(BigInt::from(
                u128::from(edge.capacity()) * u128::from(edge.capacity()),
            ));
            conductance * (&potentials[edge.from().as_usize()] - &potentials[edge.to().as_usize()])
        })
        .collect::<Vec<_>>();
    let effective_resistance = &potentials[source.as_usize()] - &potentials[sink.as_usize()];
    let exact_energy =
        graph
            .edges()
            .iter()
            .zip(&currents)
            .fold(BigRational::zero(), |energy, (edge, current)| {
                let conductance = BigRational::from_integer(BigInt::from(
                    u128::from(edge.capacity()) * u128::from(edge.capacity()),
                ));
                energy + current * current / conductance
            });
    if exact_energy != effective_resistance {
        return Err(ElectricalFlowError::ExactReferenceFailure);
    }
    let mut divergence = vec![BigRational::zero(); graph.nodes().len()];
    for (edge, current) in graph.edges().iter().zip(&currents) {
        divergence[edge.from().as_usize()] += current;
        divergence[edge.to().as_usize()] -= current;
    }
    for (node, value) in divergence.iter().enumerate() {
        let expected = if node == source.as_usize() {
            BigRational::one()
        } else if node == sink.as_usize() {
            -BigRational::one()
        } else {
            BigRational::zero()
        };
        if *value != expected {
            return Err(ElectricalFlowError::ExactReferenceFailure);
        }
    }
    Ok(ExactReference {
        potentials,
        currents,
        effective_resistance,
        pivots,
    })
}

fn gauss_jordan(
    mut matrix: Vec<Vec<BigRational>>,
    mut rhs: Vec<BigRational>,
) -> Result<(Vec<BigRational>, usize), ElectricalFlowError> {
    let dimension = rhs.len();
    if matrix.len() != dimension || matrix.iter().any(|row| row.len() != dimension) {
        return Err(ElectricalFlowError::ExactReferenceFailure);
    }
    let mut pivots = 0_usize;
    for column in 0..dimension {
        let pivot = (column..dimension)
            .find(|&row| !matrix[row][column].is_zero())
            .ok_or(ElectricalFlowError::ExactReferenceFailure)?;
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let divisor = matrix[column][column].clone();
        for entry in &mut matrix[column] {
            *entry /= &divisor;
        }
        rhs[column] /= divisor;
        let pivot_row = matrix[column].clone();
        let pivot_rhs = rhs[column].clone();
        for row in 0..dimension {
            if row == column || matrix[row][column].is_zero() {
                continue;
            }
            let factor = matrix[row][column].clone();
            for (entry, pivot_entry) in matrix[row].iter_mut().zip(&pivot_row) {
                *entry -= &factor * pivot_entry;
            }
            rhs[row] -= factor * &pivot_rhs;
        }
        pivots += 1;
    }
    Ok((rhs, pivots))
}

fn compare_reference(
    snapshot: &ElectricalFlowSnapshot,
    exact: &ExactReference,
) -> Result<f64, ElectricalFlowError> {
    if snapshot.potentials.len() != exact.potentials.len()
        || snapshot.edges.len() != exact.currents.len()
    {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    let mut maximum_error = 0.0_f64;
    let mut maximum_exact = 0.0_f64;
    for (actual, exact) in snapshot.potentials.iter().zip(&exact.potentials) {
        let exact = exact
            .to_f64()
            .ok_or(ElectricalFlowError::ExactReferenceFailure)?;
        maximum_error = maximum_error.max((actual.get() - exact).abs());
        maximum_exact = maximum_exact.max(exact.abs());
    }
    for (actual, exact) in snapshot.edges.iter().zip(&exact.currents) {
        let exact = exact
            .to_f64()
            .ok_or(ElectricalFlowError::ExactReferenceFailure)?;
        maximum_error = maximum_error.max((actual.current.get() - exact).abs());
        maximum_exact = maximum_exact.max(exact.abs());
    }
    let tolerance = 1.0e-8 * (1.0 + maximum_exact);
    if maximum_error > tolerance {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    Ok(maximum_error)
}

fn validate_terminal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    result: &ElectricalFlowResult,
) -> Result<(), ElectricalFlowError> {
    let snapshot = &result.final_snapshot;
    if snapshot.stage != ElectricalFlowStage::Complete
        || !snapshot.converged
        || snapshot.metrics != result.metrics
        || snapshot.metrics.laplacian_assemblies != 1
        || snapshot.metrics.grounded_dimension != (graph.nodes().len() - 1) as u64
        || snapshot.metrics.conjugate_gradient_iterations == 0
        || snapshot.metrics.matrix_vector_products != snapshot.metrics.conjugate_gradient_iterations
        || snapshot.metrics.matrix_scalar_products
            != u128::from(snapshot.metrics.matrix_vector_products)
                .checked_mul(u128::from(snapshot.metrics.grounded_dimension).pow(2))
                .ok_or(ElectricalFlowError::NumericalFailure)?
        || snapshot.metrics.edge_scans != (graph.edges().len() * 2) as u128
        || snapshot.metrics.exact_elimination_pivots != (graph.nodes().len() - 1) as u64
        || snapshot.metrics.certificate_checks != 1
        || snapshot.potentials.len() != graph.nodes().len()
        || snapshot.edges.len() != graph.edges().len()
        || snapshot.exact_effective_resistance.as_ref() != Some(&result.exact_effective_resistance)
        || snapshot.maximum_absolute_error.is_none()
        || snapshot.residual_l2.get() > ELECTRICAL_FLOW_RELATIVE_TOLERANCE
    {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    let exact = exact_reference(
        graph,
        source,
        sink,
        &grounded_nodes(graph.nodes().len(), sink.as_usize()),
        &inverse_indices(
            graph.nodes().len(),
            &grounded_nodes(graph.nodes().len(), sink.as_usize()),
        ),
    )?;
    if ElectricalExactRational::from(&exact.effective_resistance)
        != result.exact_effective_resistance
    {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    let measured_error = compare_reference(snapshot, &exact)?;
    let published_error = snapshot
        .maximum_absolute_error
        .ok_or(ElectricalFlowError::CertificateFailure)?
        .get();
    if measured_error.to_bits() != published_error.to_bits() {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    let energy_error = (snapshot.total_energy.get() - snapshot.effective_resistance.get()).abs();
    if energy_error > 1.0e-8 * (1.0 + snapshot.effective_resistance.get().abs()) {
        return Err(ElectricalFlowError::CertificateFailure);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn edge(id: &str, from: &str, to: &str, capacity: u64) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower: 0,
            capacity,
            cost: 0,
        }
    }

    fn graph(nodes: &[&str], edges: Vec<UnresolvedFlowEdge>) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
                .collect(),
            edges,
        )
        .expect("graph")
    }

    #[test]
    fn parallel_paths_split_current_by_squared_capacity() {
        let graph = graph(
            &["s", "a", "b", "t"],
            vec![
                edge("sa", "s", "a", 1),
                edge("at", "a", "t", 1),
                edge("sb", "s", "b", 2),
                edge("bt", "b", "t", 2),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("t");
        let result = solve_electrical_flow(&graph, source, sink).expect("electrical flow");
        let currents = result
            .final_snapshot
            .edges
            .iter()
            .map(|edge| (edge.edge.as_str(), edge.current.get()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!((currents["sa"] - 0.2).abs() < 1.0e-9);
        assert!((currents["at"] - 0.2).abs() < 1.0e-9);
        assert!((currents["sb"] - 0.8).abs() < 1.0e-9);
        assert!((currents["bt"] - 0.8).abs() < 1.0e-9);
        assert_eq!(result.exact_effective_resistance.numerator, BigInt::from(2));
        assert_eq!(
            result.exact_effective_resistance.denominator,
            BigInt::from(5)
        );
    }

    #[test]
    fn arbitrary_orientation_produces_negative_current_and_exact_energy() {
        let graph = graph(
            &["s", "a", "t"],
            vec![edge("backward", "a", "s", 1), edge("forward", "a", "t", 1)],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("t");
        let trace = trace_electrical_flow(&graph, source, sink).expect("trace");
        assert!(trace.result.final_snapshot.edges[0].current.get() < 0.0);
        assert_eq!(
            trace.result.exact_effective_resistance.numerator,
            BigInt::from(2)
        );
        assert_eq!(
            trace.result.exact_effective_resistance.denominator,
            BigInt::from(1)
        );
        assert_eq!(
            trace.events.last().expect("complete").after.stage,
            ElectricalFlowStage::Complete
        );
    }

    #[test]
    fn trace_is_reversible_and_checker_rejects_forgery() {
        let graph = graph(
            &["s", "a", "t"],
            vec![
                edge("sa", "s", "a", 3),
                edge("at", "a", "t", 2),
                edge("st", "s", "t", 1),
            ],
        );
        let source = graph
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("s");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("t");
        let trace = trace_electrical_flow(&graph, source, sink).expect("trace");
        let fast = solve_electrical_flow(&graph, source, sink).expect("fast");
        assert_eq!(trace.result, fast);
        let mut cursor = trace.final_snapshot.clone();
        for event in trace.events.iter().rev() {
            assert_eq!(event.after, cursor);
            cursor = event.before.clone();
        }
        assert_eq!(cursor, trace.base_snapshot);

        let mut forged = trace;
        forged.events[0].catalog_id = "electrical-flow.forged";
        assert_eq!(
            check_electrical_flow_trace(&graph, source, sink, &forged),
            Err(ElectricalFlowError::TraceVerification)
        );
    }

    #[test]
    fn admission_and_graph_contract_fail_closed() {
        let disconnected = graph(&["s", "a", "t"], vec![edge("sa", "s", "a", 1)]);
        let source = disconnected
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("s");
        let sink = disconnected
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("t");
        assert_eq!(
            solve_electrical_flow(&disconnected, source, sink),
            Err(ElectricalFlowError::DisconnectedTerminals)
        );

        let zero_capacity = graph(&["s", "t"], vec![edge("zero", "s", "t", 0)]);
        let source = zero_capacity
            .node_index(&NodeId::parse("s").expect("s"))
            .expect("s");
        let sink = zero_capacity
            .node_index(&NodeId::parse("t").expect("t"))
            .expect("t");
        assert_eq!(
            solve_electrical_flow(&zero_capacity, source, sink),
            Err(ElectricalFlowError::GraphRequirement)
        );
    }

    #[test]
    fn deterministic_small_connected_graphs_match_exact_reference() {
        for mask in 0_u8..64 {
            let mut edges = vec![
                edge("sa", "s", "a", u64::from(mask % 3) + 1),
                edge("at", "a", "t", u64::from((mask / 3) % 3) + 1),
            ];
            if mask & 1 != 0 {
                edges.push(edge("st", "s", "t", 2));
            }
            if mask & 2 != 0 {
                edges.push(edge("as", "a", "s", 1));
            }
            let graph = graph(&["s", "a", "t"], edges);
            let source = graph
                .node_index(&NodeId::parse("s").expect("s"))
                .expect("s");
            let sink = graph
                .node_index(&NodeId::parse("t").expect("t"))
                .expect("t");
            let result = solve_electrical_flow(&graph, source, sink).expect("certified");
            assert!(
                result
                    .final_snapshot
                    .maximum_absolute_error
                    .expect("error")
                    .get()
                    <= 1.0e-8
            );
        }
    }
}
