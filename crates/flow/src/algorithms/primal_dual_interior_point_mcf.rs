//! Bounded integer primal-dual interior-point minimum-cost flow.
//!
//! This is a small-input realization of Becker, Karrenbauer, and Mehlhorn's
//! integer path-following method.  It keeps the paper's capacitated-to-
//! uncapacitated reduction, integer central-path iterates, sticky arc deletion
//! and contraction, randomized fundamental-cycle centering, proxy test, dual
//! crossover, and admissible-network recovery.  The implementation deliberately
//! admits only graphs small enough to enumerate the minimum-condition spanning
//! forest exactly; the source's asymptotic low-stretch-tree construction is not
//! replaced by a presentation-only shortcut.

use std::collections::{BTreeSet, VecDeque};

use num_bigint::{BigInt, BigUint, Sign};
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Maximum original nodes admitted by the bounded exact realization.
pub const PRIMAL_DUAL_IPM_MCF_MAX_NODES: usize = 6;
/// Maximum original edges admitted by the bounded exact realization.
pub const PRIMAL_DUAL_IPM_MCF_MAX_EDGES: usize = 5;
/// Maximum absolute original capacity admitted by the bounded realization.
pub const PRIMAL_DUAL_IPM_MCF_MAX_CAPACITY: u64 = 32;
/// Maximum absolute original unit cost admitted by the bounded realization.
pub const PRIMAL_DUAL_IPM_MCF_MAX_COST: u64 = 32;
/// Maximum arcs after the paper's capacity-removal and initialization reduction.
pub const PRIMAL_DUAL_IPM_MCF_MAX_AUXILIARY_ARCS: usize = 15;
/// Maximum subsets inspected when selecting an exact minimum-condition forest.
pub const PRIMAL_DUAL_IPM_MCF_MAX_FOREST_SUBSETS: u64 = 32_768;
/// Guard on short-step path-following iterations.
pub const PRIMAL_DUAL_IPM_MCF_MAX_OUTER_ITERATIONS: u64 = 512;
/// Guard on randomized fundamental-cycle updates.
pub const PRIMAL_DUAL_IPM_MCF_MAX_CYCLE_UPDATES: u64 = 65_536;
/// Guard on public trace events. This covers every admitted forest subset and
/// every admitted fundamental-cycle update without post-hoc compression.
pub const PRIMAL_DUAL_IPM_MCF_MAX_TRACE_EVENTS: usize = 131_072;
/// Reproducible default seed for the randomized centering solver.
pub const PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED: u64 = 0x6a09_e667_f3bc_c909;

/// Capacity-reduction node kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrimalDualIpmNodeKind {
    /// A node from the original input network.
    Original(NodeIndex),
    /// The capacity node introduced for one normalized original arc.
    Capacity(EdgeId),
}

/// Capacity-reduction arc kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimalDualIpmArcKind {
    /// Cost-carrying tail-to-capacity-node arc.
    Upper,
    /// Zero-cost head-to-capacity-node complement arc.
    Lower,
    /// High-cost initialization arc carrying the tree-solution discrepancy.
    Artificial,
}

/// Public algorithm stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimalDualIpmStage {
    /// Validated original input before source transformations.
    Ready,
    /// Lower bounds, negative costs, and common gcds were normalized.
    NormalizeInput,
    /// Finite capacities were replaced by two uncapacitated arcs.
    BuildCapacityReduction,
    /// Appendix-A artificial arcs produced an integer central point.
    InitializeCentralPoint,
    /// Tiny primal/dual coordinates were deleted/contracted in the sticky minor.
    BuildMinor,
    /// The short-step parameter was decreased exactly.
    DecreaseMu,
    /// An exact minimum-condition spanning forest was selected.
    BuildLowStretchForest,
    /// One exact candidate subset was inspected while selecting the forest.
    InspectForestSubset,
    /// One fundamental cycle was sampled by the paper's resistance weights.
    SampleFundamentalCycle,
    /// The rounded cycle correction was applied.
    CenteringCycleUpdate,
    /// The new iterate passed the one-norm centrality bound.
    Centered,
    /// The active minor passed the proxy inequality.
    ProxyReached,
    /// One nested crossover cut grew across a zero reduced-cost arc.
    CrossoverGrowCut,
    /// Crossover potentials were checked against the original auxiliary costs.
    RestoreOriginalDual,
    /// A feasible optimum was recovered in the zero-reduced-cost network.
    RecoverAdmissibleFlow,
    /// The original graph passed the independent exact MCF checker.
    CheckCertificate,
    /// Certified optimum.
    Optimal,
}

/// One auxiliary node projected at an algorithm boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmNodeState {
    /// Stable auxiliary-node ordinal.
    pub node: usize,
    /// Original or per-edge capacity-node identity.
    pub kind: PrimalDualIpmNodeKind,
    /// Current dual potential on the scaled auxiliary instance.
    pub potential: BigInt,
    /// Current sticky-contraction component.
    pub component: usize,
    /// Whether crossover has already inserted this node into its growing set.
    pub in_crossover_set: bool,
}

/// One auxiliary arc projected at an algorithm boundary.
// These flags are independent visual layers (minor membership, tree
// membership, deletion, and contraction), not an encoded state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmArcState {
    /// Stable auxiliary-arc ordinal.
    pub arc: usize,
    /// Original edge represented by this arc.
    pub original_edge: EdgeId,
    /// Source auxiliary node.
    pub from: usize,
    /// Destination auxiliary node.
    pub to: usize,
    /// Reduction role.
    pub kind: PrimalDualIpmArcKind,
    /// Scaled integer primal coordinate.
    pub flow: BigInt,
    /// Scaled integer dual slack.
    pub slack: BigInt,
    /// Current centering resistance `ceil(s/x)` when the arc is in the minor.
    pub resistance: Option<BigInt>,
    /// Sticky primal deletion flag.
    pub deleted: bool,
    /// Sticky dual contraction flag.
    pub contracted: bool,
    /// Whether this arc is present between distinct minor components.
    pub in_minor: bool,
    /// Whether it belongs to the selected low-stretch forest.
    pub in_tree: bool,
    /// Signed orientation in the currently sampled fundamental cycle.
    pub active_cycle_sign: i8,
}

/// Exact bounded-work counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrimalDualIpmMetrics {
    /// Short-step path-following iterations.
    pub outer_iterations: u64,
    /// Completed centering calls.
    pub centering_steps: u64,
    /// Candidate forest subsets inspected.
    pub forest_subsets: u64,
    /// Random u64 words consumed.
    pub random_draws: u64,
    /// Sampled fundamental cycles.
    pub sampled_cycles: u64,
    /// Nonzero rounded cycle corrections.
    pub cycle_updates: u64,
    /// Sticky primal deletions.
    pub deleted_arcs: u64,
    /// Sticky dual contractions.
    pub contracted_arcs: u64,
    /// Nested-cut potential shifts during crossover.
    pub crossover_shifts: u64,
    /// Augmentations in the final admissible-network recovery.
    pub recovery_augmentations: u64,
    /// Independent terminal certificate checks.
    pub certificate_checks: u64,
    /// Public state transitions.
    pub state_transitions: u64,
}

/// Complete public boundary state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmSnapshot {
    /// Current source phase.
    pub stage: PrimalDualIpmStage,
    /// Scaled auxiliary nodes.
    pub nodes: Vec<PrimalDualIpmNodeState>,
    /// Scaled auxiliary arcs.
    pub arcs: Vec<PrimalDualIpmArcState>,
    /// Current path-following parameter.
    pub mu: BigInt,
    /// Demand/supply granularity used for the integer grid.
    pub beta: BigInt,
    /// Cost granularity used for the integer grid.
    pub gamma: BigInt,
    /// Active-minor complementarity gap.
    pub proxy_gap: BigInt,
    /// Exact sum `sum |x_a s_a - mu|` on the active minor.
    pub centrality_numerator: BigInt,
    /// Selected off-tree auxiliary arc, if any.
    pub sampled_arc: Option<usize>,
    /// Latest rounded fundamental-cycle correction.
    pub cycle_alpha: BigInt,
    /// Exact selected-forest tree condition number.
    pub tree_condition_number: Option<BigRational>,
    /// Auxiliary arcs in the candidate forest subset currently being inspected.
    pub forest_candidate_arcs: Vec<usize>,
    /// Recovered original-edge flows, once available.
    pub final_flows: Vec<Option<u64>>,
    /// Exact counters.
    pub metrics: PrimalDualIpmMetrics,
}

/// One deterministic/reproducible trace transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmTraceEvent {
    /// Stable event vocabulary.
    pub catalog_id: &'static str,
    /// State before the operation.
    pub before: PrimalDualIpmSnapshot,
    /// State after the operation.
    pub after: PrimalDualIpmSnapshot,
}

/// Certified integer-IPM result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmResult {
    /// Original-edge flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independently reconstructed MCF certificate.
    pub certificate: MinCostFlowCertificate,
    /// Reproducible centering seed.
    pub seed: u64,
    /// Final source-specific boundary.
    pub final_snapshot: PrimalDualIpmSnapshot,
    /// Exact bounded-work counters.
    pub metrics: PrimalDualIpmMetrics,
}

/// Result plus every reversible source boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimalDualIpmTraceResult {
    /// Certified result.
    pub result: PrimalDualIpmResult,
    /// Boundary before source transformations.
    pub base_snapshot: PrimalDualIpmSnapshot,
    /// Complete ordered event stream.
    pub events: Vec<PrimalDualIpmTraceEvent>,
    /// Boundary after certification.
    pub final_snapshot: PrimalDualIpmSnapshot,
}

/// Construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PrimalDualIpmError {
    /// Input exceeds the declared bounded research band.
    #[error("integer primal-dual IPM input exceeds admission limits")]
    AdmissionLimit,
    /// The requested balance vector is malformed.
    #[error("integer primal-dual IPM requires a balanced divergence vector")]
    InvalidDivergence,
    /// The source algorithm assumes a feasible instance.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A source reduction or integer-grid identity failed.
    #[error("integer primal-dual IPM reduction invariant failed")]
    ReductionInvariant,
    /// No low-stretch forest or liftable fundamental cycle existed.
    #[error("integer primal-dual IPM minor/forest invariant failed")]
    ForestInvariant,
    /// A checked conversion back to the public numeric domain failed.
    #[error("integer primal-dual IPM arithmetic exceeded the public domain")]
    ArithmeticOverflow,
    /// The bounded randomized centering guard was exhausted.
    #[error("integer primal-dual IPM centering did not converge within its bounded guard")]
    NonConvergence,
    /// Proxy crossover could not construct an original-cost feasible dual tree.
    #[error("integer primal-dual IPM crossover invariant failed")]
    CrossoverInvariant,
    /// The zero-reduced-cost recovery network was infeasible.
    #[error("integer primal-dual IPM admissible-network recovery failed")]
    RecoveryInvariant,
    /// Independent minimum-cost certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Public trace did not equal a complete seeded re-execution.
    #[error("integer primal-dual IPM trace verification failed")]
    TraceVerification,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NormalizedSign {
    Forward,
    Complement,
}

#[derive(Clone, Debug)]
struct NormalizedArc {
    original: usize,
    original_edge: EdgeId,
    from: usize,
    to: usize,
    capacity: BigInt,
    cost: BigInt,
    sign: NormalizedSign,
}

#[derive(Clone, Debug)]
struct NormalizedProblem {
    node_count: usize,
    arcs: Vec<NormalizedArc>,
    target: Vec<BigInt>,
    base_flows: Vec<BigInt>,
    common_flow_gcd: BigInt,
}

#[derive(Clone, Debug)]
struct AuxiliaryNode {
    kind: PrimalDualIpmNodeKind,
}

#[derive(Clone, Debug)]
struct AuxiliaryArc {
    original_edge: EdgeId,
    from: usize,
    to: usize,
    kind: PrimalDualIpmArcKind,
    cost: BigInt,
}

#[derive(Clone, Debug)]
struct AuxiliaryProblem {
    nodes: Vec<AuxiliaryNode>,
    arcs: Vec<AuxiliaryArc>,
    demand: Vec<BigInt>,
    normalized_to_upper: Vec<usize>,
    normalized_to_lower: Vec<usize>,
    beta: BigInt,
    gamma: BigInt,
    initial_mu: BigInt,
}

struct AuxiliaryInitialization {
    problem: AuxiliaryProblem,
    primal: Vec<BigInt>,
    potentials: Vec<BigInt>,
    slacks: Vec<BigInt>,
}

#[derive(Clone, Debug)]
struct KernelState {
    normalized: NormalizedProblem,
    auxiliary: Option<AuxiliaryProblem>,
    x: Vec<BigInt>,
    y: Vec<BigInt>,
    s: Vec<BigInt>,
    deleted: Vec<bool>,
    contracted: Vec<bool>,
    absorbed: Vec<bool>,
    contraction_forest: Vec<usize>,
    components: Vec<usize>,
    active_arcs: Vec<usize>,
    tree_arcs: BTreeSet<usize>,
    active_cycle: Vec<(usize, i8)>,
    resistances: Vec<Option<BigInt>>,
    crossover_set: Vec<bool>,
    mu: BigInt,
    proxy_gap: BigInt,
    centrality_numerator: BigInt,
    sampled_arc: Option<usize>,
    cycle_alpha: BigInt,
    tree_condition_number: Option<BigRational>,
    forest_candidate_arcs: Vec<usize>,
    final_flows: Vec<Option<u64>>,
    metrics: PrimalDualIpmMetrics,
    stage: PrimalDualIpmStage,
    rng: ExactRng,
}

#[derive(Clone, Debug)]
struct ExactRng {
    state: u64,
}

impl ExactRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    fn next_u64(&mut self, metrics: &mut PrimalDualIpmMetrics) -> Result<u64, PrimalDualIpmError> {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        metrics.random_draws = metrics
            .random_draws
            .checked_add(1)
            .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
        Ok(value)
    }

    fn below(
        &mut self,
        upper: &BigInt,
        metrics: &mut PrimalDualIpmMetrics,
    ) -> Result<BigInt, PrimalDualIpmError> {
        let bound = upper
            .to_biguint()
            .filter(|value| !value.is_zero())
            .ok_or(PrimalDualIpmError::ForestInvariant)?;
        let bits = bound.bits();
        let words = bits.div_ceil(64);
        loop {
            let mut candidate = BigUint::zero();
            for word in 0..words {
                candidate |= BigUint::from(self.next_u64(metrics)?) << (word * 64);
            }
            let excess = words * 64 - bits;
            if excess > 0 {
                candidate &= (BigUint::one() << bits) - BigUint::one();
            }
            if candidate < bound {
                return Ok(BigInt::from_biguint(Sign::Plus, candidate));
            }
        }
    }
}

struct Recorder<'a> {
    graph: &'a FlowNetwork,
    state: KernelState,
    current: PrimalDualIpmSnapshot,
    events: Vec<PrimalDualIpmTraceEvent>,
    enabled: bool,
}

impl Recorder<'_> {
    fn emit(
        &mut self,
        catalog_id: &'static str,
        stage: PrimalDualIpmStage,
    ) -> Result<(), PrimalDualIpmError> {
        self.state.stage = stage;
        self.state.metrics.state_transitions = self
            .state
            .metrics
            .state_transitions
            .checked_add(1)
            .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
        if !self.enabled {
            return Ok(());
        }
        let before = self.current.clone();
        let after = project_snapshot(self.graph, &self.state);
        if self.events.len() >= PRIMAL_DUAL_IPM_MCF_MAX_TRACE_EVENTS {
            return Err(PrimalDualIpmError::AdmissionLimit);
        }
        self.events.push(PrimalDualIpmTraceEvent {
            catalog_id,
            before,
            after: after.clone(),
        });
        self.current = after;
        Ok(())
    }
}

struct InternalRun {
    result: PrimalDualIpmResult,
    base_snapshot: PrimalDualIpmSnapshot,
    events: Vec<PrimalDualIpmTraceEvent>,
}

/// Solves a feasible bounded minimum-cost-flow instance with the default seed.
///
/// # Errors
///
/// Rejects admission, feasibility, source-invariant, convergence, recovery, or
/// independent certificate failures.
pub fn solve_primal_dual_interior_point_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PrimalDualIpmResult, PrimalDualIpmError> {
    solve_primal_dual_interior_point_mcf_with_seed(
        graph,
        required_divergence,
        PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED,
    )
}

/// Solves the default-seed execution while reporting its feasibility precheck
/// to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_primal_dual_interior_point_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PrimalDualIpmResult, PrimalDualIpmError> {
    run_internal_with_feasibility(
        graph,
        required_divergence,
        PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves with a reproducible fundamental-cycle sampling seed.
///
/// # Errors
///
/// Returns the same failures as [`solve_primal_dual_interior_point_mcf`].
pub fn solve_primal_dual_interior_point_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<PrimalDualIpmResult, PrimalDualIpmError> {
    run_internal(graph, required_divergence, seed, false).map(|run| run.result)
}

/// Records the complete default-seed source execution.
///
/// # Errors
///
/// Returns the same failures as [`solve_primal_dual_interior_point_mcf`].
pub fn trace_primal_dual_interior_point_mcf(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PrimalDualIpmTraceResult, PrimalDualIpmError> {
    trace_primal_dual_interior_point_mcf_with_seed(
        graph,
        required_divergence,
        PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED,
    )
}

/// Records the complete seeded source execution.
///
/// # Errors
///
/// Returns the same failures as [`solve_primal_dual_interior_point_mcf`].
pub fn trace_primal_dual_interior_point_mcf_with_seed(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
) -> Result<PrimalDualIpmTraceResult, PrimalDualIpmError> {
    let run = run_internal(graph, required_divergence, seed, true)?;
    let trace = PrimalDualIpmTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_primal_dual_interior_point_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Records the default-seed execution while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_primal_dual_interior_point_mcf_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PrimalDualIpmTraceResult, PrimalDualIpmError> {
    let run = run_internal_with_feasibility(
        graph,
        required_divergence,
        PRIMAL_DUAL_IPM_MCF_DEFAULT_SEED,
        true,
        feasibility,
    )?;
    let trace = PrimalDualIpmTraceResult {
        final_snapshot: run.result.final_snapshot.clone(),
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
    };
    check_primal_dual_interior_point_mcf_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Independently checks every public boundary and terminal certificate.
///
/// # Errors
///
/// Rejects altered, omitted, reordered, or disconnected source states.
pub fn check_primal_dual_interior_point_mcf_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &PrimalDualIpmTraceResult,
) -> Result<(), PrimalDualIpmError> {
    validate_admission(graph, required_divergence)?;
    if trace.base_snapshot != expected_primal_dual_ipm_base(graph)
        || trace.final_snapshot.stage != PrimalDualIpmStage::Optimal
        || trace.final_snapshot != trace.result.final_snapshot
        || trace.final_snapshot.metrics != trace.result.metrics
        || trace.events.is_empty()
        || trace.events.len() > PRIMAL_DUAL_IPM_MCF_MAX_TRACE_EVENTS
        || trace.final_snapshot.final_flows.len() != graph.edges().len()
    {
        return Err(PrimalDualIpmError::TraceVerification);
    }
    let mut cursor = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != cursor
            || !valid_primal_dual_ipm_event(event)
            || event.after.metrics.state_transitions
                != event.before.metrics.state_transitions.saturating_add(1)
            || event
                .after
                .nodes
                .iter()
                .enumerate()
                .any(|(index, state)| state.node != index)
            || event.after.arcs.iter().enumerate().any(|(index, state)| {
                state.arc != index || !matches!(state.active_cycle_sign, -1..=1)
            })
        {
            return Err(PrimalDualIpmError::TraceVerification);
        }
        cursor = &event.after;
    }
    let certificate = check_min_cost_flow(graph, required_divergence, &trace.result.flows)?;
    if cursor != &trace.final_snapshot
        || certificate != trace.result.certificate
        || trace.events.len() as u64 != trace.final_snapshot.metrics.state_transitions
        || trace
            .final_snapshot
            .final_flows
            .iter()
            .zip(&trace.result.flows)
            .any(|(state, flow)| *state != Some(*flow))
    {
        return Err(PrimalDualIpmError::TraceVerification);
    }
    Ok(())
}

fn expected_primal_dual_ipm_base(graph: &FlowNetwork) -> PrimalDualIpmSnapshot {
    PrimalDualIpmSnapshot {
        stage: PrimalDualIpmStage::Ready,
        nodes: graph
            .node_indices()
            .map(|node| PrimalDualIpmNodeState {
                node: node.as_usize(),
                kind: PrimalDualIpmNodeKind::Original(node),
                potential: BigInt::zero(),
                component: node.as_usize(),
                in_crossover_set: false,
            })
            .collect(),
        arcs: Vec::new(),
        mu: BigInt::zero(),
        beta: BigInt::one(),
        gamma: BigInt::one(),
        proxy_gap: BigInt::zero(),
        centrality_numerator: BigInt::zero(),
        sampled_arc: None,
        cycle_alpha: BigInt::zero(),
        tree_condition_number: None,
        forest_candidate_arcs: Vec::new(),
        final_flows: vec![None; graph.edges().len()],
        metrics: PrimalDualIpmMetrics::default(),
    }
}

#[allow(clippy::too_many_lines, clippy::unnested_or_patterns)]
fn valid_primal_dual_ipm_event(event: &PrimalDualIpmTraceEvent) -> bool {
    let catalog_matches_stage = matches!(
        (event.catalog_id, event.after.stage),
        (
            "primal-dual-interior-point-mcf.normalize-input",
            PrimalDualIpmStage::NormalizeInput
        ) | (
            "primal-dual-interior-point-mcf.build-capacity-reduction",
            PrimalDualIpmStage::BuildCapacityReduction
        ) | (
            "primal-dual-interior-point-mcf.initialize-central-point",
            PrimalDualIpmStage::InitializeCentralPoint
        ) | (
            "primal-dual-interior-point-mcf.build-minor",
            PrimalDualIpmStage::BuildMinor
        ) | (
            "primal-dual-interior-point-mcf.decrease-mu",
            PrimalDualIpmStage::DecreaseMu
        ) | (
            "primal-dual-interior-point-mcf.inspect-forest-subset",
            PrimalDualIpmStage::InspectForestSubset
        ) | (
            "primal-dual-interior-point-mcf.build-low-stretch-forest",
            PrimalDualIpmStage::BuildLowStretchForest
        ) | (
            "primal-dual-interior-point-mcf.sample-fundamental-cycle",
            PrimalDualIpmStage::SampleFundamentalCycle
        ) | (
            "primal-dual-interior-point-mcf.centering-cycle-update",
            PrimalDualIpmStage::CenteringCycleUpdate
        ) | (
            "primal-dual-interior-point-mcf.centered",
            PrimalDualIpmStage::Centered
        ) | (
            "primal-dual-interior-point-mcf.proxy-reached",
            PrimalDualIpmStage::ProxyReached
        ) | (
            "primal-dual-interior-point-mcf.crossover-grow-cut",
            PrimalDualIpmStage::CrossoverGrowCut
        ) | (
            "primal-dual-interior-point-mcf.restore-original-dual",
            PrimalDualIpmStage::RestoreOriginalDual
        ) | (
            "primal-dual-interior-point-mcf.recover-admissible-flow",
            PrimalDualIpmStage::RecoverAdmissibleFlow
        ) | (
            "primal-dual-interior-point-mcf.check-certificate",
            PrimalDualIpmStage::CheckCertificate
        ) | (
            "primal-dual-interior-point-mcf.optimal",
            PrimalDualIpmStage::Optimal
        )
    );
    let stage_transition = matches!(
        (event.before.stage, event.after.stage),
        (
            PrimalDualIpmStage::Ready,
            PrimalDualIpmStage::NormalizeInput
        ) | (
            PrimalDualIpmStage::NormalizeInput,
            PrimalDualIpmStage::BuildCapacityReduction
        ) | (
            PrimalDualIpmStage::BuildCapacityReduction,
            PrimalDualIpmStage::InitializeCentralPoint
        ) | (
            PrimalDualIpmStage::InitializeCentralPoint | PrimalDualIpmStage::Centered,
            PrimalDualIpmStage::BuildMinor
        ) | (
            PrimalDualIpmStage::BuildMinor,
            PrimalDualIpmStage::DecreaseMu
        ) | (
            PrimalDualIpmStage::DecreaseMu,
            PrimalDualIpmStage::InspectForestSubset | PrimalDualIpmStage::BuildLowStretchForest
        ) | (
            PrimalDualIpmStage::InspectForestSubset,
            PrimalDualIpmStage::InspectForestSubset | PrimalDualIpmStage::BuildLowStretchForest
        ) | (
            PrimalDualIpmStage::BuildLowStretchForest,
            PrimalDualIpmStage::SampleFundamentalCycle | PrimalDualIpmStage::Centered
        ) | (
            PrimalDualIpmStage::SampleFundamentalCycle,
            PrimalDualIpmStage::CenteringCycleUpdate
        ) | (
            PrimalDualIpmStage::CenteringCycleUpdate,
            PrimalDualIpmStage::SampleFundamentalCycle | PrimalDualIpmStage::Centered
        ) | (
            PrimalDualIpmStage::Centered,
            PrimalDualIpmStage::ProxyReached
        ) | (
            PrimalDualIpmStage::ProxyReached | PrimalDualIpmStage::CrossoverGrowCut,
            PrimalDualIpmStage::CrossoverGrowCut | PrimalDualIpmStage::RestoreOriginalDual
        ) | (
            PrimalDualIpmStage::RestoreOriginalDual,
            PrimalDualIpmStage::RecoverAdmissibleFlow
        ) | (
            PrimalDualIpmStage::RecoverAdmissibleFlow,
            PrimalDualIpmStage::CheckCertificate
        ) | (
            PrimalDualIpmStage::CheckCertificate,
            PrimalDualIpmStage::Optimal
        )
    );
    let forest_observation = if event.after.stage == PrimalDualIpmStage::InspectForestSubset {
        event.after.metrics.forest_subsets == event.before.metrics.forest_subsets.saturating_add(1)
            && !event
                .after
                .forest_candidate_arcs
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            && event
                .after
                .forest_candidate_arcs
                .iter()
                .all(|&arc| arc < event.after.arcs.len())
    } else {
        event.after.metrics.forest_subsets == event.before.metrics.forest_subsets
    };
    let candidate_scope = if event.after.stage == PrimalDualIpmStage::InspectForestSubset {
        true
    } else {
        event.after.forest_candidate_arcs.is_empty()
    };
    catalog_matches_stage && stage_transition && forest_observation && candidate_scope
}

fn validate_admission(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<(), PrimalDualIpmError> {
    if graph.nodes().len() > PRIMAL_DUAL_IPM_MCF_MAX_NODES
        || graph.edges().len() > PRIMAL_DUAL_IPM_MCF_MAX_EDGES
        || required_divergence.len() != graph.nodes().len()
        || graph.edges().iter().any(|edge| {
            edge.capacity() > PRIMAL_DUAL_IPM_MCF_MAX_CAPACITY
                || edge.cost().unsigned_abs() > PRIMAL_DUAL_IPM_MCF_MAX_COST
        })
    {
        return Err(PrimalDualIpmError::AdmissionLimit);
    }
    if required_divergence
        .iter()
        .try_fold(0_i128, |sum, value| sum.checked_add(*value))
        != Some(0)
    {
        return Err(PrimalDualIpmError::InvalidDivergence);
    }
    Ok(())
}

fn gcd_big(mut left: BigInt, mut right: BigInt) -> BigInt {
    left = left.abs();
    right = right.abs();
    while !right.is_zero() {
        let remainder = &left % &right;
        left = right;
        right = remainder;
    }
    left
}

fn gcd_values<'a>(values: impl IntoIterator<Item = &'a BigInt>) -> BigInt {
    values
        .into_iter()
        .fold(BigInt::zero(), |value, item| gcd_big(value, item.clone()))
}

fn normalize_problem(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<NormalizedProblem, PrimalDualIpmError> {
    let lower = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    let mut target = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, current)| BigInt::from(required - current))
        .collect::<Vec<_>>();
    let mut base_flows = graph
        .edges()
        .iter()
        .map(|edge| BigInt::from(edge.lower()))
        .collect::<Vec<_>>();
    let mut arcs = Vec::new();
    for (original, edge) in graph.edges().iter().enumerate() {
        let width = edge.capacity() - edge.lower();
        if width == 0 {
            continue;
        }
        if edge.from() == edge.to() {
            if edge.cost() < 0 {
                base_flows[original] += BigInt::from(width);
            }
            continue;
        }
        if edge.cost() >= 0 {
            arcs.push(NormalizedArc {
                original,
                original_edge: edge.id().clone(),
                from: edge.from().as_usize(),
                to: edge.to().as_usize(),
                capacity: BigInt::from(width),
                cost: BigInt::from(edge.cost()),
                sign: NormalizedSign::Forward,
            });
        } else {
            base_flows[original] += BigInt::from(width);
            let amount = BigInt::from(width);
            target[edge.from().as_usize()] -= &amount;
            target[edge.to().as_usize()] += &amount;
            arcs.push(NormalizedArc {
                original,
                original_edge: edge.id().clone(),
                from: edge.to().as_usize(),
                to: edge.from().as_usize(),
                capacity: amount,
                cost: BigInt::from(edge.cost()).abs(),
                sign: NormalizedSign::Complement,
            });
        }
    }

    let common_flow_gcd = gcd_values(target.iter().chain(arcs.iter().map(|arc| &arc.capacity)));
    let common_flow_gcd = if common_flow_gcd.is_zero() {
        BigInt::one()
    } else {
        common_flow_gcd
    };
    for value in &mut target {
        *value /= &common_flow_gcd;
    }
    for arc in &mut arcs {
        arc.capacity /= &common_flow_gcd;
    }
    let common_cost_gcd = gcd_values(arcs.iter().map(|arc| &arc.cost));
    let common_cost_gcd = if common_cost_gcd.is_zero() {
        BigInt::one()
    } else {
        common_cost_gcd
    };
    for arc in &mut arcs {
        arc.cost /= &common_cost_gcd;
    }
    if target.iter().fold(BigInt::zero(), |sum, value| sum + value) != BigInt::zero() {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    Ok(NormalizedProblem {
        node_count: graph.nodes().len(),
        arcs,
        target,
        base_flows,
        common_flow_gcd,
    })
}

#[derive(Clone, Debug)]
struct Dsu {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl Dsu {
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            rank: vec![0; count],
        }
    }

    fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            self.parent[node] = self.find(self.parent[node]);
        }
        self.parent[node]
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let mut left = self.find(left);
        let mut right = self.find(right);
        if left == right {
            return false;
        }
        if self.rank[left] < self.rank[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left;
        if self.rank[left] == self.rank[right] {
            self.rank[left] += 1;
        }
        true
    }
}

fn tree_solution(problem: &NormalizedProblem) -> Result<Vec<BigInt>, PrimalDualIpmError> {
    let mut dsu = Dsu::new(problem.node_count);
    let mut tree = Vec::new();
    for (index, arc) in problem.arcs.iter().enumerate() {
        if dsu.union(arc.from, arc.to) {
            tree.push(index);
        }
    }
    let mut component_sum = vec![BigInt::zero(); problem.node_count];
    for (node, target) in problem.target.iter().enumerate() {
        let root = dsu.find(node);
        component_sum[root] += target;
    }
    if component_sum.iter().any(|value| !value.is_zero()) {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    let mut adjacency = vec![Vec::new(); problem.node_count];
    for &edge in &tree {
        let arc = &problem.arcs[edge];
        adjacency[arc.from].push((arc.to, edge));
        adjacency[arc.to].push((arc.from, edge));
    }
    let mut values = vec![BigInt::zero(); problem.arcs.len()];
    let mut visited = vec![false; problem.node_count];
    for root in 0..problem.node_count {
        if visited[root] {
            continue;
        }
        let mut parent = vec![None; problem.node_count];
        let mut parent_edge = vec![None; problem.node_count];
        let mut order = vec![root];
        visited[root] = true;
        let mut cursor = 0;
        while cursor < order.len() {
            let node = order[cursor];
            cursor += 1;
            for &(next, edge) in &adjacency[node] {
                if visited[next] {
                    continue;
                }
                visited[next] = true;
                parent[next] = Some(node);
                parent_edge[next] = Some(edge);
                order.push(next);
            }
        }
        let mut subtotal = problem.target.clone();
        for &node in order.iter().rev() {
            let Some(parent_node) = parent[node] else {
                if !subtotal[node].is_zero() {
                    return Err(PrimalDualIpmError::ReductionInvariant);
                }
                continue;
            };
            let edge = parent_edge[node].ok_or(PrimalDualIpmError::ReductionInvariant)?;
            let arc = &problem.arcs[edge];
            values[edge] = if arc.from == node {
                subtotal[node].clone()
            } else {
                -subtotal[node].clone()
            };
            let child = subtotal[node].clone();
            subtotal[parent_node] += child;
        }
    }
    Ok(values)
}

fn ceil_div_positive(
    numerator: &BigInt,
    denominator: &BigInt,
) -> Result<BigInt, PrimalDualIpmError> {
    if numerator.is_negative() || denominator <= &BigInt::zero() {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    Ok((numerator + denominator - BigInt::one()) / denominator)
}

fn lcm_positive(left: &BigInt, right: &BigInt) -> Result<BigInt, PrimalDualIpmError> {
    if left <= &BigInt::zero() || right <= &BigInt::zero() {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    Ok((left / gcd_big(left.clone(), right.clone())) * right)
}

#[allow(clippy::too_many_lines)]
fn build_auxiliary(
    problem: &NormalizedProblem,
) -> Result<AuxiliaryInitialization, PrimalDualIpmError> {
    let tree_values = tree_solution(problem)?;
    let artificial_count = problem
        .arcs
        .iter()
        .zip(&tree_values)
        .filter(|(arc, value)| (*value).clone() * BigInt::from(2_u8) != arc.capacity)
        .count();
    let auxiliary_arc_count = problem
        .arcs
        .len()
        .checked_mul(2)
        .and_then(|value| value.checked_add(artificial_count))
        .ok_or(PrimalDualIpmError::AdmissionLimit)?;
    if auxiliary_arc_count > PRIMAL_DUAL_IPM_MCF_MAX_AUXILIARY_ARCS {
        return Err(PrimalDualIpmError::AdmissionLimit);
    }
    let arc_count_big = BigInt::from(auxiliary_arc_count.max(1));
    let beta = BigInt::from(256_u16) * &arc_count_big * &arc_count_big * &arc_count_big;
    let max_capacity = problem
        .arcs
        .iter()
        .map(|arc| arc.capacity.clone())
        .max()
        .unwrap_or_else(BigInt::one);
    let half_balance = problem
        .target
        .iter()
        .map(BigInt::abs)
        .fold(BigInt::zero(), |sum, value| sum + value)
        / 2;
    let max_problem_magnitude = max_capacity.max(half_balance).max(BigInt::one());
    let max_cost = problem
        .arcs
        .iter()
        .map(|arc| arc.cost.clone())
        .max()
        .unwrap_or_else(BigInt::one)
        .max(BigInt::one());
    let gamma = BigInt::from(32_768_u32)
        * &arc_count_big
        * &arc_count_big
        * &arc_count_big
        * &arc_count_big
        * &beta
        * &max_problem_magnitude
        * &max_cost;
    let central_product = &beta * &gamma * &max_problem_magnitude * &max_cost;
    let required_centralization = BigInt::from(24_u8) * &arc_count_big * &central_product;
    let mut common_multiple = gamma.clone();
    for (index, arc) in problem.arcs.iter().enumerate() {
        let scaled_capacity = &arc.capacity * &beta;
        common_multiple = lcm_positive(&common_multiple, &(&scaled_capacity * &gamma))?;
        let discrepancy = (&tree_values[index] * &beta) - (&scaled_capacity / BigInt::from(2_u8));
        if !discrepancy.is_zero() {
            common_multiple = lcm_positive(&common_multiple, &(discrepancy.abs() * &gamma))?;
        }
    }
    let centralization_parameter =
        ceil_div_positive(&required_centralization, &common_multiple)? * common_multiple;
    let initial_mu = &centralization_parameter + &central_product;

    let mut nodes = (0..problem.node_count)
        .map(|node| {
            Ok(AuxiliaryNode {
                kind: PrimalDualIpmNodeKind::Original(
                    NodeIndex::try_from_usize(node).ok_or(PrimalDualIpmError::AdmissionLimit)?,
                ),
            })
        })
        .collect::<Result<Vec<_>, PrimalDualIpmError>>()?;
    let mut arcs = Vec::with_capacity(auxiliary_arc_count);
    let mut demand = problem
        .target
        .iter()
        .map(|value| -value * &beta)
        .collect::<Vec<_>>();
    let mut initial_primal = Vec::with_capacity(auxiliary_arc_count);
    let mut initial_potentials = vec![BigInt::zero(); problem.node_count];
    let mut normalized_to_upper = Vec::with_capacity(problem.arcs.len());
    let mut normalized_to_lower = Vec::with_capacity(problem.arcs.len());

    for (index, arc) in problem.arcs.iter().enumerate() {
        let capacity_node = nodes.len();
        nodes.push(AuxiliaryNode {
            kind: PrimalDualIpmNodeKind::Capacity(arc.original_edge.clone()),
        });
        let scaled_capacity = &arc.capacity * &beta;
        demand[arc.to] -= &scaled_capacity;
        demand.push(scaled_capacity.clone());
        let dual: BigInt = -(&centralization_parameter * BigInt::from(2_u8) / &scaled_capacity);
        initial_potentials.push(dual);
        let half: BigInt = &scaled_capacity / BigInt::from(2_u8);
        normalized_to_upper.push(arcs.len());
        arcs.push(AuxiliaryArc {
            original_edge: arc.original_edge.clone(),
            from: arc.from,
            to: capacity_node,
            kind: PrimalDualIpmArcKind::Upper,
            cost: &arc.cost * &gamma,
        });
        initial_primal.push(half.clone());
        normalized_to_lower.push(arcs.len());
        arcs.push(AuxiliaryArc {
            original_edge: arc.original_edge.clone(),
            from: arc.to,
            to: capacity_node,
            kind: PrimalDualIpmArcKind::Lower,
            cost: BigInt::zero(),
        });
        initial_primal.push(half);

        let discrepancy_twice: BigInt = &tree_values[index] * BigInt::from(2_u8) - &arc.capacity;
        if !discrepancy_twice.is_zero() {
            let discrepancy: BigInt =
                (&tree_values[index] * &beta) - (&scaled_capacity / BigInt::from(2_u8));
            let (from, to, amount) = if discrepancy.is_positive() {
                (arc.from, arc.to, discrepancy)
            } else {
                (arc.to, arc.from, -discrepancy)
            };
            let cost = ceil_div_positive(&centralization_parameter, &amount)?;
            arcs.push(AuxiliaryArc {
                original_edge: arc.original_edge.clone(),
                from,
                to,
                kind: PrimalDualIpmArcKind::Artificial,
                cost,
            });
            initial_primal.push(amount);
        }
    }
    let initial_slacks = arcs
        .iter()
        .map(|arc| &arc.cost - (&initial_potentials[arc.to] - &initial_potentials[arc.from]))
        .collect::<Vec<_>>();
    if initial_primal.iter().any(|value| value <= &BigInt::zero())
        || initial_slacks.iter().any(|value| value <= &BigInt::zero())
        || incidence(&arcs, nodes.len(), &initial_primal) != demand
        || centrality(
            &initial_primal,
            &initial_slacks,
            &initial_mu,
            &(0..arcs.len()).collect::<Vec<_>>(),
        ) > &initial_mu / 8
    {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    Ok(AuxiliaryInitialization {
        problem: AuxiliaryProblem {
            nodes,
            arcs,
            demand,
            normalized_to_upper,
            normalized_to_lower,
            beta,
            gamma,
            initial_mu,
        },
        primal: initial_primal,
        potentials: initial_potentials,
        slacks: initial_slacks,
    })
}

fn incidence(arcs: &[AuxiliaryArc], node_count: usize, values: &[BigInt]) -> Vec<BigInt> {
    let mut result = vec![BigInt::zero(); node_count];
    for (arc, value) in arcs.iter().zip(values) {
        result[arc.from] -= value;
        result[arc.to] += value;
    }
    result
}

fn centrality(x: &[BigInt], s: &[BigInt], mu: &BigInt, active: &[usize]) -> BigInt {
    active
        .iter()
        .map(|&arc| (&x[arc] * &s[arc] - mu).abs())
        .fold(BigInt::zero(), |sum, value| sum + value)
}

fn project_snapshot(graph: &FlowNetwork, state: &KernelState) -> PrimalDualIpmSnapshot {
    let Some(auxiliary) = state.auxiliary.as_ref() else {
        return PrimalDualIpmSnapshot {
            stage: state.stage,
            nodes: graph
                .node_indices()
                .map(|node| PrimalDualIpmNodeState {
                    node: node.as_usize(),
                    kind: PrimalDualIpmNodeKind::Original(node),
                    potential: BigInt::zero(),
                    component: node.as_usize(),
                    in_crossover_set: false,
                })
                .collect(),
            arcs: Vec::new(),
            mu: BigInt::zero(),
            beta: BigInt::one(),
            gamma: BigInt::one(),
            proxy_gap: BigInt::zero(),
            centrality_numerator: BigInt::zero(),
            sampled_arc: None,
            cycle_alpha: BigInt::zero(),
            tree_condition_number: None,
            forest_candidate_arcs: Vec::new(),
            final_flows: state.final_flows.clone(),
            metrics: state.metrics,
        };
    };
    let active = state.active_arcs.iter().copied().collect::<BTreeSet<_>>();
    let cycle = state
        .active_cycle
        .iter()
        .copied()
        .collect::<std::collections::BTreeMap<_, _>>();
    let nodes = auxiliary
        .nodes
        .iter()
        .enumerate()
        .map(|(node, item)| PrimalDualIpmNodeState {
            node,
            kind: item.kind.clone(),
            potential: state.y[node].clone(),
            component: state.components.get(node).copied().unwrap_or(node),
            in_crossover_set: state.crossover_set.get(node).copied().unwrap_or(false),
        })
        .collect();
    let arcs = auxiliary
        .arcs
        .iter()
        .enumerate()
        .map(|(arc, item)| PrimalDualIpmArcState {
            arc,
            original_edge: item.original_edge.clone(),
            from: item.from,
            to: item.to,
            kind: item.kind,
            flow: state.x[arc].clone(),
            slack: state.s[arc].clone(),
            resistance: state.resistances.get(arc).cloned().flatten(),
            deleted: state.deleted[arc],
            contracted: state.contracted[arc] || state.absorbed[arc],
            in_minor: active.contains(&arc),
            in_tree: state.tree_arcs.contains(&arc),
            active_cycle_sign: cycle.get(&arc).copied().unwrap_or(0),
        })
        .collect();
    PrimalDualIpmSnapshot {
        stage: state.stage,
        nodes,
        arcs,
        mu: state.mu.clone(),
        beta: auxiliary.beta.clone(),
        gamma: auxiliary.gamma.clone(),
        proxy_gap: state.proxy_gap.clone(),
        centrality_numerator: state.centrality_numerator.clone(),
        sampled_arc: state.sampled_arc,
        cycle_alpha: state.cycle_alpha.clone(),
        tree_condition_number: state.tree_condition_number.clone(),
        forest_candidate_arcs: state.forest_candidate_arcs.clone(),
        final_flows: state.final_flows.clone(),
        metrics: state.metrics,
    }
}

#[allow(clippy::too_many_lines)]
fn run_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
    record_events: bool,
) -> Result<InternalRun, PrimalDualIpmError> {
    let mut feasibility = FeasibilityExecution::untracked();
    run_internal_with_feasibility(
        graph,
        required_divergence,
        seed,
        record_events,
        &mut feasibility,
    )
}

#[allow(clippy::too_many_lines)]
fn run_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    seed: u64,
    record_events: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, PrimalDualIpmError> {
    validate_admission(graph, required_divergence)?;
    // The source algorithm assumes feasibility.  This primitive is used only
    // as a rejecting precondition; its flow is never used as an IPM iterate.
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let normalized = normalize_problem(graph, required_divergence)?;
    let state = KernelState {
        normalized,
        auxiliary: None,
        x: Vec::new(),
        y: Vec::new(),
        s: Vec::new(),
        deleted: Vec::new(),
        contracted: Vec::new(),
        absorbed: Vec::new(),
        contraction_forest: Vec::new(),
        components: Vec::new(),
        active_arcs: Vec::new(),
        tree_arcs: BTreeSet::new(),
        active_cycle: Vec::new(),
        resistances: Vec::new(),
        crossover_set: Vec::new(),
        mu: BigInt::zero(),
        proxy_gap: BigInt::zero(),
        centrality_numerator: BigInt::zero(),
        sampled_arc: None,
        cycle_alpha: BigInt::zero(),
        tree_condition_number: None,
        forest_candidate_arcs: Vec::new(),
        final_flows: vec![None; graph.edges().len()],
        metrics: PrimalDualIpmMetrics::default(),
        stage: PrimalDualIpmStage::Ready,
        rng: ExactRng::new(seed),
    };
    let base_snapshot = project_snapshot(graph, &state);
    let mut recorder = Recorder {
        graph,
        state,
        current: base_snapshot.clone(),
        events: Vec::new(),
        enabled: record_events,
    };
    recorder.emit(
        "primal-dual-interior-point-mcf.normalize-input",
        PrimalDualIpmStage::NormalizeInput,
    )?;

    let initialization = build_auxiliary(&recorder.state.normalized)?;
    let auxiliary = initialization.problem;
    let auxiliary_nodes = auxiliary.nodes.len();
    let auxiliary_arcs = auxiliary.arcs.len();
    recorder.state.auxiliary = Some(auxiliary);
    recorder.state.x = vec![BigInt::zero(); auxiliary_arcs];
    recorder.state.y = vec![BigInt::zero(); auxiliary_nodes];
    recorder.state.s = vec![BigInt::zero(); auxiliary_arcs];
    recorder.state.deleted = vec![false; auxiliary_arcs];
    recorder.state.contracted = vec![false; auxiliary_arcs];
    recorder.state.absorbed = vec![false; auxiliary_arcs];
    recorder.state.components = (0..auxiliary_nodes).collect();
    recorder.state.resistances = vec![None; auxiliary_arcs];
    recorder.state.crossover_set = vec![false; auxiliary_nodes];
    recorder.emit(
        "primal-dual-interior-point-mcf.build-capacity-reduction",
        PrimalDualIpmStage::BuildCapacityReduction,
    )?;

    recorder.state.x = initialization.primal;
    recorder.state.y = initialization.potentials;
    recorder.state.s = initialization.slacks;
    recorder.state.mu = recorder
        .state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?
        .initial_mu
        .clone();
    recorder.state.active_arcs = (0..auxiliary_arcs).collect();
    recorder.state.proxy_gap = complementarity_gap(
        &recorder.state.x,
        &recorder.state.s,
        &recorder.state.active_arcs,
    );
    recorder.state.centrality_numerator = centrality(
        &recorder.state.x,
        &recorder.state.s,
        &recorder.state.mu,
        &recorder.state.active_arcs,
    );
    recorder.emit(
        "primal-dual-interior-point-mcf.initialize-central-point",
        PrimalDualIpmStage::InitializeCentralPoint,
    )?;

    loop {
        if recorder.state.metrics.outer_iterations >= PRIMAL_DUAL_IPM_MCF_MAX_OUTER_ITERATIONS {
            return Err(PrimalDualIpmError::NonConvergence);
        }
        build_minor(&mut recorder.state)?;
        recorder.emit(
            "primal-dual-interior-point-mcf.build-minor",
            PrimalDualIpmStage::BuildMinor,
        )?;

        let next_mu = decrease_mu_exact(
            &recorder.state.mu,
            recorder
                .state
                .auxiliary
                .as_ref()
                .ok_or(PrimalDualIpmError::ReductionInvariant)?
                .arcs
                .len()
                .max(1),
        )?;
        if next_mu >= recorder.state.mu {
            return Err(PrimalDualIpmError::NonConvergence);
        }
        recorder.state.mu = next_mu;
        recorder.state.metrics.outer_iterations += 1;
        recorder.state.centrality_numerator = centrality(
            &recorder.state.x,
            &recorder.state.s,
            &recorder.state.mu,
            &recorder.state.active_arcs,
        );
        recorder.emit(
            "primal-dual-interior-point-mcf.decrease-mu",
            PrimalDualIpmStage::DecreaseMu,
        )?;

        center_minor(&mut recorder)?;
        recorder.state.proxy_gap = complementarity_gap(
            &recorder.state.x,
            &recorder.state.s,
            &recorder.state.active_arcs,
        );
        let auxiliary = recorder
            .state
            .auxiliary
            .as_ref()
            .ok_or(PrimalDualIpmError::ReductionInvariant)?;
        if &recorder.state.proxy_gap * 81 < &auxiliary.beta * &auxiliary.gamma * 4 {
            recorder.emit(
                "primal-dual-interior-point-mcf.proxy-reached",
                PrimalDualIpmStage::ProxyReached,
            )?;
            break;
        }
    }

    crossover(&mut recorder)?;
    let (normalized_flows, recovery_augmentations) = recover_admissible_flow(&recorder.state)?;
    recorder.state.metrics.recovery_augmentations = recovery_augmentations;
    let flows = lift_original_flows(&recorder.state.normalized, &normalized_flows)?;
    recorder.state.final_flows = flows.iter().copied().map(Some).collect();
    recorder.emit(
        "primal-dual-interior-point-mcf.recover-admissible-flow",
        PrimalDualIpmStage::RecoverAdmissibleFlow,
    )?;

    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    recorder.state.metrics.certificate_checks += 1;
    recorder.emit(
        "primal-dual-interior-point-mcf.check-certificate",
        PrimalDualIpmStage::CheckCertificate,
    )?;
    recorder.emit(
        "primal-dual-interior-point-mcf.optimal",
        PrimalDualIpmStage::Optimal,
    )?;
    let final_snapshot = if recorder.enabled {
        recorder.current.clone()
    } else {
        project_snapshot(graph, &recorder.state)
    };
    let result = PrimalDualIpmResult {
        flows,
        certificate,
        seed,
        final_snapshot,
        metrics: recorder.state.metrics,
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events: recorder.events,
    })
}

fn complementarity_gap(x: &[BigInt], s: &[BigInt], active: &[usize]) -> BigInt {
    active
        .iter()
        .map(|&arc| &x[arc] * &s[arc])
        .fold(BigInt::zero(), |sum, value| sum + value)
}

fn build_minor(state: &mut KernelState) -> Result<(), PrimalDualIpmError> {
    let auxiliary = state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?;
    let m = BigInt::from(auxiliary.arcs.len().max(1));
    for arc in 0..auxiliary.arcs.len() {
        if state.deleted[arc] || state.contracted[arc] {
            continue;
        }
        let delete = &state.x[arc] * &m * 9 < &auxiliary.beta * 7;
        let contract = &state.s[arc] * &m * 9 < &auxiliary.gamma * 7;
        if delete && contract {
            return Err(PrimalDualIpmError::ReductionInvariant);
        }
        if delete {
            state.deleted[arc] = true;
            state.metrics.deleted_arcs += 1;
        } else if contract {
            state.contracted[arc] = true;
            state.metrics.contracted_arcs += 1;
        }
    }
    let mut dsu = Dsu::new(auxiliary.nodes.len());
    state.contraction_forest.clear();
    for (arc, item) in auxiliary.arcs.iter().enumerate() {
        if state.contracted[arc] && dsu.union(item.from, item.to) {
            state.contraction_forest.push(arc);
        }
    }
    let mut minimum = vec![usize::MAX; auxiliary.nodes.len()];
    for node in 0..auxiliary.nodes.len() {
        let root = dsu.find(node);
        minimum[root] = minimum[root].min(node);
    }
    state.components = (0..auxiliary.nodes.len())
        .map(|node| {
            let root = dsu.find(node);
            minimum[root]
        })
        .collect();
    state.active_arcs.clear();
    state.resistances.fill(None);
    for arc in 0..auxiliary.arcs.len() {
        if state.deleted[arc] || state.contracted[arc] {
            continue;
        }
        if state.x[arc] <= BigInt::zero() || state.s[arc] <= BigInt::zero() {
            return Err(PrimalDualIpmError::ReductionInvariant);
        }
        let resistance = ceil_div_positive(&state.s[arc], &state.x[arc])?.max(BigInt::one());
        state.resistances[arc] = Some(resistance);
        state.active_arcs.push(arc);
    }
    state.tree_arcs.clear();
    state.active_cycle.clear();
    state.sampled_arc = None;
    state.cycle_alpha = BigInt::zero();
    state.tree_condition_number = None;
    state.proxy_gap = complementarity_gap(&state.x, &state.s, &state.active_arcs);
    state.centrality_numerator = centrality(&state.x, &state.s, &state.mu, &state.active_arcs);
    Ok(())
}

fn decrease_mu_exact(mu: &BigInt, arc_count: usize) -> Result<BigInt, PrimalDualIpmError> {
    if mu <= &BigInt::zero() {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    let target_square = mu * mu;
    let coefficient = BigInt::from(64_usize * arc_count);
    let mut low = BigInt::zero();
    let mut high = mu / 8 + BigInt::one();
    while &low + 1 < high {
        let middle = (&low + &high) / 2;
        if &coefficient * &middle * &middle <= target_square {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(mu - low)
}

#[derive(Clone, Debug)]
struct MinorEdge {
    arc: usize,
    from: usize,
    to: usize,
    resistance: BigInt,
}

#[derive(Clone, Debug)]
struct ForestChoice {
    tree_positions: BTreeSet<usize>,
    condition: BigRational,
}

struct SampledCycle {
    position: usize,
    edges: Vec<(usize, i8)>,
    resistance: BigInt,
}

struct CenteringCandidate {
    primal: Vec<BigInt>,
    potentials: Vec<BigInt>,
    slacks: Vec<BigInt>,
    deviation: BigInt,
}

#[allow(clippy::too_many_lines)]
fn select_low_stretch_forest(
    recorder: &mut Recorder<'_>,
) -> Result<(Vec<MinorEdge>, ForestChoice), PrimalDualIpmError> {
    let auxiliary = recorder
        .state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?
        .clone();
    let edges = recorder
        .state
        .active_arcs
        .iter()
        .map(|&arc| {
            let item = &auxiliary.arcs[arc];
            Ok(MinorEdge {
                arc,
                from: recorder.state.components[item.from],
                to: recorder.state.components[item.to],
                resistance: recorder.state.resistances[arc]
                    .clone()
                    .ok_or(PrimalDualIpmError::ForestInvariant)?,
            })
        })
        .collect::<Result<Vec<_>, PrimalDualIpmError>>()?;
    let subset_count = 1_u64
        .checked_shl(u32::try_from(edges.len()).map_err(|_| PrimalDualIpmError::AdmissionLimit)?)
        .ok_or(PrimalDualIpmError::AdmissionLimit)?;
    if subset_count > PRIMAL_DUAL_IPM_MCF_MAX_FOREST_SUBSETS {
        return Err(PrimalDualIpmError::AdmissionLimit);
    }
    let mut all_components = recorder.state.components.clone();
    all_components.sort_unstable();
    all_components.dedup();
    let mut full = Dsu::new(auxiliary.nodes.len());
    for edge in &edges {
        full.union(edge.from, edge.to);
    }
    let full_components = all_components
        .iter()
        .map(|&node| full.find(node))
        .collect::<BTreeSet<_>>()
        .len();
    let required_edges = all_components.len().saturating_sub(full_components);
    let mut best: Option<(u64, ForestChoice)> = None;
    for mask in 0..subset_count {
        recorder.state.forest_candidate_arcs = edges
            .iter()
            .enumerate()
            .filter_map(|(position, edge)| (mask & (1_u64 << position) != 0).then_some(edge.arc))
            .collect();
        recorder.state.metrics.forest_subsets = recorder
            .state
            .metrics
            .forest_subsets
            .checked_add(1)
            .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
        recorder.emit(
            "primal-dual-interior-point-mcf.inspect-forest-subset",
            PrimalDualIpmStage::InspectForestSubset,
        )?;
        if mask.count_ones() as usize != required_edges {
            continue;
        }
        let mut dsu = Dsu::new(auxiliary.nodes.len());
        let mut tree_positions = BTreeSet::new();
        let mut acyclic = true;
        for (position, edge) in edges.iter().enumerate() {
            if mask & (1_u64 << position) == 0 {
                continue;
            }
            if !dsu.union(edge.from, edge.to) {
                acyclic = false;
                break;
            }
            tree_positions.insert(position);
        }
        if !acyclic {
            continue;
        }
        let selected_components = all_components
            .iter()
            .map(|&node| dsu.find(node))
            .collect::<BTreeSet<_>>()
            .len();
        if selected_components != full_components {
            continue;
        }
        let mut condition = BigRational::zero();
        for (position, edge) in edges.iter().enumerate() {
            if tree_positions.contains(&position) {
                continue;
            }
            let path = tree_path(&edges, &tree_positions, edge.to, edge.from)?;
            let cycle_resistance = path.iter().fold(edge.resistance.clone(), |sum, (tree, _)| {
                sum + &edges[*tree].resistance
            });
            condition += BigRational::new(cycle_resistance, edge.resistance.clone());
        }
        let choice = ForestChoice {
            tree_positions,
            condition,
        };
        if best
            .as_ref()
            .is_none_or(|(_, current)| choice.condition < current.condition)
        {
            best = Some((mask, choice));
        }
    }
    let (_, choice) = best.ok_or(PrimalDualIpmError::ForestInvariant)?;
    recorder.state.forest_candidate_arcs.clear();
    Ok((edges, choice))
}

fn tree_path(
    edges: &[MinorEdge],
    tree: &BTreeSet<usize>,
    start: usize,
    target: usize,
) -> Result<Vec<(usize, i8)>, PrimalDualIpmError> {
    if start == target {
        return Ok(Vec::new());
    }
    let max_node = edges
        .iter()
        .map(|edge| edge.from.max(edge.to))
        .max()
        .unwrap_or(start.max(target));
    let mut predecessor = vec![None; max_node + 1];
    let mut queue = VecDeque::from([start]);
    predecessor[start] = Some((start, usize::MAX, 0_i8));
    while let Some(node) = queue.pop_front() {
        for &position in tree {
            let edge = &edges[position];
            let (next, sign) = if edge.from == node {
                (edge.to, 1)
            } else if edge.to == node {
                (edge.from, -1)
            } else {
                continue;
            };
            if predecessor[next].is_none() {
                predecessor[next] = Some((node, position, sign));
                queue.push_back(next);
            }
        }
    }
    if predecessor.get(target).and_then(|value| *value).is_none() {
        return Err(PrimalDualIpmError::ForestInvariant);
    }
    let mut path = Vec::new();
    let mut node = target;
    while node != start {
        let (previous, edge, sign) =
            predecessor[node].ok_or(PrimalDualIpmError::ForestInvariant)?;
        path.push((edge, sign));
        node = previous;
    }
    path.reverse();
    Ok(path)
}

fn tree_potentials(
    node_count: usize,
    edges: &[MinorEdge],
    tree: &BTreeSet<usize>,
    phi: &[BigInt],
) -> Vec<BigInt> {
    let mut adjacency = vec![Vec::new(); node_count];
    for &position in tree {
        let edge = &edges[position];
        adjacency[edge.from].push((edge.to, position, 1_i8));
        adjacency[edge.to].push((edge.from, position, -1_i8));
    }
    let mut potentials = vec![BigInt::zero(); node_count];
    let mut visited = vec![false; node_count];
    for root in 0..node_count {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &(next, position, sign) in &adjacency[node] {
                if visited[next] {
                    continue;
                }
                visited[next] = true;
                let voltage = &edges[position].resistance * &phi[position];
                potentials[next] = if sign > 0 {
                    &potentials[node] + voltage
                } else {
                    &potentials[node] - voltage
                };
                queue.push_back(next);
            }
        }
    }
    potentials
}

fn round_nearest(numerator: &BigInt, denominator: &BigInt) -> Result<BigInt, PrimalDualIpmError> {
    if denominator <= &BigInt::zero() {
        return Err(PrimalDualIpmError::ForestInvariant);
    }
    let sign = numerator.sign();
    let absolute = numerator.abs();
    let quotient = &absolute / denominator;
    let remainder = &absolute % denominator;
    let rounded = if remainder * 2 >= *denominator {
        quotient + 1
    } else {
        quotient
    };
    Ok(match sign {
        Sign::Minus => -rounded,
        _ => rounded,
    })
}

fn sample_off_tree(
    recorder: &mut Recorder<'_>,
    edges: &[MinorEdge],
    choice: &ForestChoice,
) -> Result<SampledCycle, PrimalDualIpmError> {
    let off_tree = (0..edges.len())
        .filter(|position| !choice.tree_positions.contains(position))
        .collect::<Vec<_>>();
    if off_tree.is_empty() {
        return Err(PrimalDualIpmError::ForestInvariant);
    }
    let mut common = BigInt::one();
    let mut cycle_resistances = Vec::with_capacity(off_tree.len());
    let mut paths = Vec::with_capacity(off_tree.len());
    for &position in &off_tree {
        let edge = &edges[position];
        common = lcm_positive(&common, &edge.resistance)?;
        let path = tree_path(edges, &choice.tree_positions, edge.to, edge.from)?;
        let cycle_resistance = path.iter().fold(edge.resistance.clone(), |sum, (tree, _)| {
            sum + &edges[*tree].resistance
        });
        paths.push(path);
        cycle_resistances.push(cycle_resistance);
    }
    let weights = off_tree
        .iter()
        .enumerate()
        .map(|(index, &position)| {
            &cycle_resistances[index] * (&common / &edges[position].resistance)
        })
        .collect::<Vec<_>>();
    let total = weights
        .iter()
        .fold(BigInt::zero(), |sum, weight| sum + weight);
    let draw = {
        let KernelState { rng, metrics, .. } = &mut recorder.state;
        rng.below(&total, metrics)?
    };
    let mut prefix = BigInt::zero();
    let mut selected = None;
    for (index, weight) in weights.iter().enumerate() {
        prefix += weight;
        if draw < prefix {
            selected = Some(index);
            break;
        }
    }
    let selected = selected.ok_or(PrimalDualIpmError::ForestInvariant)?;
    let position = off_tree[selected];
    let mut cycle = vec![(position, 1_i8)];
    cycle.extend(paths[selected].iter().copied());
    Ok(SampledCycle {
        position,
        edges: cycle,
        resistance: cycle_resistances[selected].clone(),
    })
}

fn centering_candidates(
    state: &KernelState,
    edges: &[MinorEdge],
    choice: &ForestChoice,
    base_x: &[BigInt],
    phi_zero: &[BigInt],
    phi: &[BigInt],
) -> Result<CenteringCandidate, PrimalDualIpmError> {
    let auxiliary = state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?;
    let component_potential =
        tree_potentials(auxiliary.nodes.len(), edges, &choice.tree_positions, phi);
    let lifted_potential = (0..auxiliary.nodes.len())
        .map(|node| component_potential[state.components[node]].clone())
        .collect::<Vec<_>>();
    let candidate_y = state
        .y
        .iter()
        .zip(&lifted_potential)
        .map(|(value, delta)| value + delta)
        .collect::<Vec<_>>();
    let candidate_s = auxiliary
        .arcs
        .iter()
        .map(|arc| &arc.cost - (&candidate_y[arc.to] - &candidate_y[arc.from]))
        .collect::<Vec<_>>();
    let mut candidate_x = base_x.to_vec();
    for (position, edge) in edges.iter().enumerate() {
        candidate_x[edge.arc] = &base_x[edge.arc] + &phi[position] - &phi_zero[position];
    }
    if candidate_s.iter().any(|value| value <= &BigInt::zero())
        || state
            .active_arcs
            .iter()
            .any(|&arc| candidate_x[arc] <= BigInt::zero())
    {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    let centrality = centrality(&candidate_x, &candidate_s, &state.mu, &state.active_arcs);
    Ok(CenteringCandidate {
        primal: candidate_x,
        potentials: candidate_y,
        slacks: candidate_s,
        deviation: centrality,
    })
}

// This function intentionally keeps one complete centering-call transaction in
// view: forest choice, source sampling, candidate projection, and circulation lift.
#[allow(clippy::too_many_lines)]
fn center_minor(recorder: &mut Recorder<'_>) -> Result<(), PrimalDualIpmError> {
    let (edges, choice) = select_low_stretch_forest(recorder)?;
    recorder.state.tree_arcs = choice
        .tree_positions
        .iter()
        .map(|&position| edges[position].arc)
        .collect();
    recorder.state.tree_condition_number = Some(choice.condition.clone());
    recorder.emit(
        "primal-dual-interior-point-mcf.build-low-stretch-forest",
        PrimalDualIpmStage::BuildLowStretchForest,
    )?;

    let base_x = recorder.state.x.clone();
    let base_y = recorder.state.y.clone();
    let base_s = recorder.state.s.clone();
    let base_proxy_gap = complementarity_gap(&base_x, &base_s, &recorder.state.active_arcs);
    let base_centrality = centrality(
        &base_x,
        &base_s,
        &recorder.state.mu,
        &recorder.state.active_arcs,
    );
    let phi_zero = edges
        .iter()
        .map(|edge| {
            let quotient = ceil_div_positive(&recorder.state.mu, &base_s[edge.arc])?;
            Ok(&base_x[edge.arc] - quotient)
        })
        .collect::<Result<Vec<_>, PrimalDualIpmError>>()?;
    let mut phi = phi_zero.clone();
    let mut candidate =
        centering_candidates(&recorder.state, &edges, &choice, &base_x, &phi_zero, &phi)?;
    let threshold = &recorder.state.mu / 8;
    while candidate.deviation >= threshold {
        if recorder.state.metrics.sampled_cycles >= PRIMAL_DUAL_IPM_MCF_MAX_CYCLE_UPDATES {
            return Err(PrimalDualIpmError::NonConvergence);
        }
        let sampled = sample_off_tree(recorder, &edges, &choice)?;
        let voltage = sampled
            .edges
            .iter()
            .fold(BigInt::zero(), |sum, (position, sign)| {
                let term = &edges[*position].resistance * &phi[*position];
                if *sign > 0 { sum + term } else { sum - term }
            });
        let negative_voltage = -voltage;
        let alpha = round_nearest(&negative_voltage, &sampled.resistance)?;
        recorder.state.sampled_arc = Some(edges[sampled.position].arc);
        recorder.state.active_cycle = sampled
            .edges
            .iter()
            .map(|(position, sign)| (edges[*position].arc, *sign))
            .collect();
        recorder.state.cycle_alpha.clone_from(&alpha);
        recorder.state.metrics.sampled_cycles += 1;
        recorder.emit(
            "primal-dual-interior-point-mcf.sample-fundamental-cycle",
            PrimalDualIpmStage::SampleFundamentalCycle,
        )?;
        if !alpha.is_zero() {
            for &(position, sign) in &sampled.edges {
                if sign > 0 {
                    phi[position] += &alpha;
                } else {
                    phi[position] -= &alpha;
                }
            }
            recorder.state.metrics.cycle_updates += 1;
        }
        candidate = centering_candidates(
            &KernelState {
                y: base_y.clone(),
                ..recorder.state.clone()
            },
            &edges,
            &choice,
            &base_x,
            &phi_zero,
            &phi,
        )?;
        recorder.state.x.clone_from(&candidate.primal);
        recorder.state.y.clone_from(&candidate.potentials);
        recorder.state.s.clone_from(&candidate.slacks);
        recorder
            .state
            .centrality_numerator
            .clone_from(&candidate.deviation);
        recorder.state.proxy_gap = complementarity_gap(
            &recorder.state.x,
            &recorder.state.s,
            &recorder.state.active_arcs,
        );
        recorder.emit(
            "primal-dual-interior-point-mcf.centering-cycle-update",
            PrimalDualIpmStage::CenteringCycleUpdate,
        )?;
        // Candidate projection is relative to the centering-call base, even
        // though the recorder now exposes the candidate state.
        recorder.state.y.clone_from(&base_y);
        recorder.state.s.clone_from(&base_s);
        recorder.state.x.clone_from(&base_x);
        recorder.state.proxy_gap.clone_from(&base_proxy_gap);
        recorder
            .state
            .centrality_numerator
            .clone_from(&base_centrality);
    }
    lift_primal_circulation(&mut candidate.primal, &base_x, &recorder.state, &edges)?;
    if incidence(
        &recorder
            .state
            .auxiliary
            .as_ref()
            .ok_or(PrimalDualIpmError::ReductionInvariant)?
            .arcs,
        candidate.potentials.len(),
        &candidate.primal,
    ) != recorder
        .state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?
        .demand
        || candidate
            .primal
            .iter()
            .any(|value| value <= &BigInt::zero())
        || candidate
            .slacks
            .iter()
            .any(|value| value <= &BigInt::zero())
    {
        return Err(PrimalDualIpmError::ReductionInvariant);
    }
    recorder.state.x = candidate.primal;
    recorder.state.y = candidate.potentials;
    recorder.state.s = candidate.slacks;
    recorder.state.centrality_numerator = candidate.deviation;
    recorder.state.proxy_gap = complementarity_gap(
        &recorder.state.x,
        &recorder.state.s,
        &recorder.state.active_arcs,
    );
    recorder.state.metrics.centering_steps += 1;
    recorder.emit(
        "primal-dual-interior-point-mcf.centered",
        PrimalDualIpmStage::Centered,
    )?;
    Ok(())
}

fn lift_primal_circulation(
    candidate_x: &mut [BigInt],
    base_x: &[BigInt],
    state: &KernelState,
    edges: &[MinorEdge],
) -> Result<(), PrimalDualIpmError> {
    let auxiliary = state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::ReductionInvariant)?;
    let mut imbalance = vec![BigInt::zero(); auxiliary.nodes.len()];
    for edge in edges {
        let delta = &candidate_x[edge.arc] - &base_x[edge.arc];
        let arc = &auxiliary.arcs[edge.arc];
        imbalance[arc.from] += &delta;
        imbalance[arc.to] -= &delta;
    }
    let mut adjacency = vec![Vec::new(); auxiliary.nodes.len()];
    for &arc in &state.contraction_forest {
        let item = &auxiliary.arcs[arc];
        adjacency[item.from].push((item.to, arc));
        adjacency[item.to].push((item.from, arc));
    }
    let mut visited = vec![false; auxiliary.nodes.len()];
    for root in 0..auxiliary.nodes.len() {
        if visited[root] {
            continue;
        }
        let mut parent = vec![None; auxiliary.nodes.len()];
        let mut parent_arc = vec![None; auxiliary.nodes.len()];
        let mut order = vec![root];
        visited[root] = true;
        let mut cursor = 0;
        while cursor < order.len() {
            let node = order[cursor];
            cursor += 1;
            for &(next, arc) in &adjacency[node] {
                if visited[next] {
                    continue;
                }
                visited[next] = true;
                parent[next] = Some(node);
                parent_arc[next] = Some(arc);
                order.push(next);
            }
        }
        let mut subtotal = imbalance.clone();
        for &node in order.iter().rev() {
            let Some(parent_node) = parent[node] else {
                if !subtotal[node].is_zero() {
                    return Err(PrimalDualIpmError::ForestInvariant);
                }
                continue;
            };
            let arc = parent_arc[node].ok_or(PrimalDualIpmError::ForestInvariant)?;
            let item = &auxiliary.arcs[arc];
            let correction = if item.from == node {
                -subtotal[node].clone()
            } else {
                subtotal[node].clone()
            };
            candidate_x[arc] += correction;
            let child = subtotal[node].clone();
            subtotal[parent_node] += child;
        }
    }
    Ok(())
}

// The nested-cut construction and original-cost tree restoration form one
// auditable crossover transaction, so the source steps stay adjacent here.
#[allow(clippy::too_many_lines)]
fn crossover(recorder: &mut Recorder<'_>) -> Result<(), PrimalDualIpmError> {
    let auxiliary = recorder
        .state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::CrossoverInvariant)?
        .clone();
    let deleted_values = recorder
        .state
        .x
        .iter()
        .enumerate()
        .map(|(arc, value)| {
            if recorder.state.deleted[arc] {
                value.clone()
            } else {
                BigInt::zero()
            }
        })
        .collect::<Vec<_>>();
    let deleted_incidence = incidence(&auxiliary.arcs, auxiliary.nodes.len(), &deleted_values);
    let perturbed_demand = auxiliary
        .demand
        .iter()
        .zip(deleted_incidence)
        .map(|(demand, removed)| demand - removed)
        .collect::<Vec<_>>();
    let perturbed_cost = auxiliary
        .arcs
        .iter()
        .enumerate()
        .map(|(arc, item)| {
            if recorder.state.contracted[arc] {
                &item.cost - &recorder.state.s[arc]
            } else {
                item.cost.clone()
            }
        })
        .collect::<Vec<_>>();
    let mut potentials = recorder.state.y.clone();
    let mut slacks = reduced_slacks(&auxiliary.arcs, &perturbed_cost, &potentials);
    if slacks.iter().any(BigInt::is_negative) {
        return Err(PrimalDualIpmError::CrossoverInvariant);
    }
    for (arc, slack) in slacks.iter().enumerate() {
        let expected = if recorder.state.contracted[arc] {
            BigInt::zero()
        } else {
            recorder.state.s[arc].clone()
        };
        if *slack != expected {
            return Err(PrimalDualIpmError::CrossoverInvariant);
        }
    }

    let components = undirected_components(&auxiliary.arcs, auxiliary.nodes.len());
    recorder.state.crossover_set.fill(false);
    let mut crossover_tree = BTreeSet::new();
    let mut roots = components.clone();
    roots.sort_unstable();
    roots.dedup();
    for root_component in roots {
        let members = (0..auxiliary.nodes.len())
            .filter(|&node| components[node] == root_component)
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        let root = members[0];
        recorder.state.crossover_set[root] = true;
        while members
            .iter()
            .any(|&node| !recorder.state.crossover_set[node])
        {
            let balance = members
                .iter()
                .filter(|&&node| recorder.state.crossover_set[node])
                .map(|&node| perturbed_demand[node].clone())
                .fold(BigInt::zero(), |sum, value| sum + value);
            let increase = balance >= BigInt::zero();
            let mut candidate = crossing_candidate(
                &auxiliary.arcs,
                &slacks,
                &recorder.state.crossover_set,
                root_component,
                &components,
                increase,
            );
            let mut actual_increase = increase;
            if candidate.is_none() && balance.is_zero() {
                actual_increase = !increase;
                candidate = crossing_candidate(
                    &auxiliary.arcs,
                    &slacks,
                    &recorder.state.crossover_set,
                    root_component,
                    &components,
                    actual_increase,
                );
            }
            let (arc, outside, delta) = candidate.ok_or(PrimalDualIpmError::CrossoverInvariant)?;
            for node in &members {
                if recorder.state.crossover_set[*node] {
                    if actual_increase {
                        potentials[*node] += &delta;
                    } else {
                        potentials[*node] -= &delta;
                    }
                }
            }
            slacks = reduced_slacks(&auxiliary.arcs, &perturbed_cost, &potentials);
            if slacks.iter().any(BigInt::is_negative) || !slacks[arc].is_zero() {
                return Err(PrimalDualIpmError::CrossoverInvariant);
            }
            recorder.state.crossover_set[outside] = true;
            crossover_tree.insert(arc);
            recorder.state.tree_arcs.clone_from(&crossover_tree);
            recorder.state.y.clone_from(&potentials);
            recorder.state.s.clone_from(&slacks);
            recorder.state.sampled_arc = Some(arc);
            recorder.state.metrics.crossover_shifts += 1;
            recorder.emit(
                "primal-dual-interior-point-mcf.crossover-grow-cut",
                PrimalDualIpmStage::CrossoverGrowCut,
            )?;
        }
    }
    recorder.state.y =
        original_cost_tree_potentials(&auxiliary.arcs, auxiliary.nodes.len(), &crossover_tree)?;
    recorder.state.s = reduced_slacks(
        &auxiliary.arcs,
        &auxiliary
            .arcs
            .iter()
            .map(|arc| arc.cost.clone())
            .collect::<Vec<_>>(),
        &recorder.state.y,
    );
    if recorder.state.s.iter().any(BigInt::is_negative) {
        return Err(PrimalDualIpmError::CrossoverInvariant);
    }
    recorder.emit(
        "primal-dual-interior-point-mcf.restore-original-dual",
        PrimalDualIpmStage::RestoreOriginalDual,
    )?;
    Ok(())
}

fn original_cost_tree_potentials(
    arcs: &[AuxiliaryArc],
    node_count: usize,
    tree: &BTreeSet<usize>,
) -> Result<Vec<BigInt>, PrimalDualIpmError> {
    let mut adjacency = vec![Vec::new(); node_count];
    for &arc in tree {
        let item = arcs
            .get(arc)
            .ok_or(PrimalDualIpmError::CrossoverInvariant)?;
        adjacency[item.from].push((item.to, arc, 1_i8));
        adjacency[item.to].push((item.from, arc, -1_i8));
    }
    let mut potentials = vec![BigInt::zero(); node_count];
    let mut visited = vec![false; node_count];
    for root in 0..node_count {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut queue = VecDeque::from([root]);
        while let Some(node) = queue.pop_front() {
            for &(next, arc, sign) in &adjacency[node] {
                if visited[next] {
                    continue;
                }
                visited[next] = true;
                potentials[next] = if sign > 0 {
                    &potentials[node] + &arcs[arc].cost
                } else {
                    &potentials[node] - &arcs[arc].cost
                };
                queue.push_back(next);
            }
        }
    }
    Ok(potentials)
}

fn reduced_slacks(arcs: &[AuxiliaryArc], costs: &[BigInt], potentials: &[BigInt]) -> Vec<BigInt> {
    arcs.iter()
        .zip(costs)
        .map(|(arc, cost)| cost - (&potentials[arc.to] - &potentials[arc.from]))
        .collect()
}

fn undirected_components(arcs: &[AuxiliaryArc], node_count: usize) -> Vec<usize> {
    let mut dsu = Dsu::new(node_count);
    for arc in arcs {
        if arc.from != arc.to {
            dsu.union(arc.from, arc.to);
        }
    }
    let mut minimum = vec![usize::MAX; node_count];
    for node in 0..node_count {
        let root = dsu.find(node);
        minimum[root] = minimum[root].min(node);
    }
    (0..node_count)
        .map(|node| {
            let root = dsu.find(node);
            minimum[root]
        })
        .collect()
}

fn crossing_candidate(
    arcs: &[AuxiliaryArc],
    slacks: &[BigInt],
    inside: &[bool],
    component: usize,
    components: &[usize],
    increase: bool,
) -> Option<(usize, usize, BigInt)> {
    arcs.iter()
        .enumerate()
        .filter_map(|(arc, item)| {
            if item.from == item.to
                || components[item.from] != component
                || components[item.to] != component
            {
                return None;
            }
            if increase && !inside[item.from] && inside[item.to] {
                Some((arc, item.from, slacks[arc].clone()))
            } else if !increase && inside[item.from] && !inside[item.to] {
                Some((arc, item.to, slacks[arc].clone()))
            } else {
                None
            }
        })
        .min_by(|left, right| left.2.cmp(&right.2).then_with(|| left.0.cmp(&right.0)))
}

#[derive(Clone, Debug)]
struct RecoveryArc {
    to: usize,
    reverse: usize,
    capacity: u64,
}

#[derive(Clone, Copy, Debug)]
struct RecoveryHandle {
    from: usize,
    slot: usize,
    initial: u64,
}

#[derive(Clone, Debug)]
struct RecoveryGraph {
    adjacency: Vec<Vec<RecoveryArc>>,
}

impl RecoveryGraph {
    fn new(nodes: usize) -> Self {
        Self {
            adjacency: vec![Vec::new(); nodes],
        }
    }

    fn add_edge(&mut self, from: usize, to: usize, capacity: u64) -> RecoveryHandle {
        let forward = self.adjacency[from].len();
        let reverse = self.adjacency[to].len();
        self.adjacency[from].push(RecoveryArc {
            to,
            reverse,
            capacity,
        });
        self.adjacency[to].push(RecoveryArc {
            to: from,
            reverse: forward,
            capacity: 0,
        });
        RecoveryHandle {
            from,
            slot: forward,
            initial: capacity,
        }
    }

    fn used(&self, handle: RecoveryHandle) -> u64 {
        handle.initial - self.adjacency[handle.from][handle.slot].capacity
    }

    fn max_flow(&mut self, source: usize, sink: usize) -> Result<(u64, u64), PrimalDualIpmError> {
        let mut total = 0_u64;
        let mut augmentations = 0_u64;
        loop {
            let mut level = vec![usize::MAX; self.adjacency.len()];
            level[source] = 0;
            let mut queue = VecDeque::from([source]);
            while let Some(node) = queue.pop_front() {
                for arc in &self.adjacency[node] {
                    if arc.capacity > 0 && level[arc.to] == usize::MAX {
                        level[arc.to] = level[node] + 1;
                        queue.push_back(arc.to);
                    }
                }
            }
            if level[sink] == usize::MAX {
                break;
            }
            let mut current = vec![0; self.adjacency.len()];
            loop {
                let pushed = self.dfs(source, sink, u64::MAX, &level, &mut current)?;
                if pushed == 0 {
                    break;
                }
                total = total
                    .checked_add(pushed)
                    .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
                augmentations = augmentations
                    .checked_add(1)
                    .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
            }
        }
        Ok((total, augmentations))
    }

    fn dfs(
        &mut self,
        node: usize,
        sink: usize,
        limit: u64,
        level: &[usize],
        current: &mut [usize],
    ) -> Result<u64, PrimalDualIpmError> {
        if node == sink {
            return Ok(limit);
        }
        while current[node] < self.adjacency[node].len() {
            let slot = current[node];
            let arc = self.adjacency[node][slot].clone();
            if arc.capacity > 0 && level[arc.to] == level[node] + 1 {
                let pushed = self.dfs(arc.to, sink, limit.min(arc.capacity), level, current)?;
                if pushed > 0 {
                    self.adjacency[node][slot].capacity -= pushed;
                    self.adjacency[arc.to][arc.reverse].capacity = self.adjacency[arc.to]
                        [arc.reverse]
                        .capacity
                        .checked_add(pushed)
                        .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
                    return Ok(pushed);
                }
            }
            current[node] += 1;
        }
        Ok(0)
    }
}

fn recover_admissible_flow(state: &KernelState) -> Result<(Vec<BigInt>, u64), PrimalDualIpmError> {
    let auxiliary = state
        .auxiliary
        .as_ref()
        .ok_or(PrimalDualIpmError::RecoveryInvariant)?;
    let mut fixed = vec![None; state.normalized.arcs.len()];
    for (normalized, fixed_value) in fixed.iter_mut().enumerate() {
        let upper = auxiliary.normalized_to_upper[normalized];
        let lower = auxiliary.normalized_to_lower[normalized];
        let upper_positive = state.s[upper].is_positive();
        let lower_positive = state.s[lower].is_positive();
        *fixed_value = match (upper_positive, lower_positive) {
            (true, false) => Some(0_u64),
            (false, true) => Some(
                state.normalized.arcs[normalized]
                    .capacity
                    .to_u64()
                    .ok_or(PrimalDualIpmError::ArithmeticOverflow)?,
            ),
            (false, false) => None,
            (true, true) => return Err(PrimalDualIpmError::RecoveryInvariant),
        };
    }
    let node_count = state.normalized.node_count;
    let source = node_count;
    let sink = node_count + 1;
    let mut graph = RecoveryGraph::new(node_count + 2);
    let mut handles = vec![None; state.normalized.arcs.len()];
    let mut flows = vec![BigInt::zero(); state.normalized.arcs.len()];
    let mut current = vec![BigInt::zero(); node_count];
    for (index, arc) in state.normalized.arcs.iter().enumerate() {
        if let Some(value) = fixed[index] {
            flows[index] = BigInt::from(value);
            current[arc.from] += BigInt::from(value);
            current[arc.to] -= BigInt::from(value);
        } else {
            let capacity = arc
                .capacity
                .to_u64()
                .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
            handles[index] = Some(graph.add_edge(arc.from, arc.to, capacity));
        }
    }
    let remaining = state
        .normalized
        .target
        .iter()
        .zip(current)
        .map(|(target, value)| target - value)
        .collect::<Vec<_>>();
    let mut required = 0_u64;
    for (node, value) in remaining.iter().enumerate() {
        if value.is_positive() {
            let amount = value
                .to_u64()
                .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
            graph.add_edge(source, node, amount);
            required = required
                .checked_add(amount)
                .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
        } else if value.is_negative() {
            let amount = (-value)
                .to_u64()
                .ok_or(PrimalDualIpmError::ArithmeticOverflow)?;
            graph.add_edge(node, sink, amount);
        }
    }
    let (routed, augmentations) = graph.max_flow(source, sink)?;
    if routed != required {
        return Err(PrimalDualIpmError::RecoveryInvariant);
    }
    for (index, handle) in handles.into_iter().enumerate() {
        if let Some(handle) = handle {
            flows[index] = BigInt::from(graph.used(handle));
        }
    }
    let mut actual = vec![BigInt::zero(); node_count];
    for (arc, value) in state.normalized.arcs.iter().zip(&flows) {
        actual[arc.from] += value;
        actual[arc.to] -= value;
    }
    if actual != state.normalized.target {
        return Err(PrimalDualIpmError::RecoveryInvariant);
    }
    Ok((flows, augmentations))
}

fn lift_original_flows(
    problem: &NormalizedProblem,
    normalized_flows: &[BigInt],
) -> Result<Vec<u64>, PrimalDualIpmError> {
    let mut flows = problem.base_flows.clone();
    for (arc, value) in problem.arcs.iter().zip(normalized_flows) {
        let amount = value * &problem.common_flow_gcd;
        match arc.sign {
            NormalizedSign::Forward => flows[arc.original] += amount,
            NormalizedSign::Complement => flows[arc.original] -= amount,
        }
    }
    flows
        .into_iter()
        .map(|value| value.to_u64().ok_or(PrimalDualIpmError::ArithmeticOverflow))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

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
    fn solves_positive_cost_fixed_flow_through_integer_ipm() {
        let graph = network(
            &["a", "s", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("st", "s", "t", 0, 2, 4),
                ("at", "a", "t", 0, 3, 1),
            ],
        );
        let target = vec![0, 3, -3];
        let result = solve_primal_dual_interior_point_mcf(&graph, &target).expect("IPM");
        assert_eq!(result.flows, vec![3, 3, 0]);
        assert_eq!(result.certificate.total_cost, 6);
        assert!(result.metrics.centering_steps > 0);
        assert!(result.metrics.crossover_shifts > 0);
    }

    #[test]
    fn normalizes_lower_bounds_negative_costs_and_self_loops() {
        let graph = network(
            &["s", "t"],
            &[
                ("loop", "s", "s", 1, 4, -3),
                ("neg", "s", "t", 1, 4, -2),
                ("back", "t", "s", 0, 2, 5),
            ],
        );
        let result = solve_primal_dual_interior_point_mcf(&graph, &[2, -2]).expect("IPM");
        assert_eq!(result.flows, vec![0, 4, 2]);
        assert_eq!(result.certificate.total_cost, -16);
    }

    #[test]
    fn seeded_trace_is_reproducible_and_rechecked() {
        let graph = network(
            &["a", "s", "t"],
            &[
                ("sa", "s", "a", 0, 2, 1),
                ("at", "a", "t", 0, 2, 1),
                ("st", "s", "t", 0, 2, 3),
            ],
        );
        let target = vec![0, 2, -2];
        let left =
            trace_primal_dual_interior_point_mcf_with_seed(&graph, &target, 7).expect("left trace");
        let right = trace_primal_dual_interior_point_mcf_with_seed(&graph, &target, 7)
            .expect("right trace");
        assert_eq!(left, right);
        check_primal_dual_interior_point_mcf_trace(&graph, &target, &left).expect("recheck");
        let subset_events = left
            .events
            .iter()
            .filter(|event| event.after.stage == PrimalDualIpmStage::InspectForestSubset)
            .collect::<Vec<_>>();
        assert_eq!(
            u64::try_from(subset_events.len()).expect("subset event count"),
            left.result.metrics.forest_subsets
        );
        assert!(!subset_events.is_empty());
        for event in subset_events {
            assert_eq!(
                event.after.metrics.forest_subsets,
                event.before.metrics.forest_subsets + 1
            );
            assert_eq!(
                event.catalog_id,
                "primal-dual-interior-point-mcf.inspect-forest-subset"
            );
        }
    }

    #[test]
    fn one_edge_visual_fixture_keeps_every_source_phase_in_a_bounded_trace() {
        let graph = network(&["s", "t"], &[("st", "s", "t", 0, 2, 1)]);
        let trace = trace_primal_dual_interior_point_mcf_with_seed(&graph, &[2, -2], 17)
            .expect("one-edge trace");
        assert!(
            trace.events.len() <= PRIMAL_DUAL_IPM_MCF_MAX_TRACE_EVENTS,
            "one-edge visual trace has {} events",
            trace.events.len()
        );
        let subset_event_count = trace
            .events
            .iter()
            .filter(|event| event.after.stage == PrimalDualIpmStage::InspectForestSubset)
            .count();
        assert_eq!(
            u64::try_from(subset_event_count).expect("subset event count"),
            trace.result.metrics.forest_subsets
        );
        for stage in [
            PrimalDualIpmStage::BuildCapacityReduction,
            PrimalDualIpmStage::BuildLowStretchForest,
            PrimalDualIpmStage::SampleFundamentalCycle,
            PrimalDualIpmStage::CenteringCycleUpdate,
            PrimalDualIpmStage::CrossoverGrowCut,
            PrimalDualIpmStage::RecoverAdmissibleFlow,
            PrimalDualIpmStage::Optimal,
        ] {
            assert!(
                trace.events.iter().any(|event| event.after.stage == stage),
                "missing {stage:?}"
            );
        }
    }

    #[test]
    fn optimizes_a_negative_cost_circulation_without_a_terminal_adapter() {
        let graph = network(
            &["a", "b"],
            &[("ab", "a", "b", 0, 2, -2), ("ba", "b", "a", 0, 2, 0)],
        );
        let result = solve_primal_dual_interior_point_mcf(&graph, &[0, 0]).expect("circulation");
        assert_eq!(result.flows, vec![2, 2]);
        assert_eq!(result.certificate.total_cost, -4);
    }

    #[test]
    fn preserves_common_flow_and_cost_gcd_scaling() {
        let graph = network(
            &["s", "t"],
            &[("cheap", "s", "t", 0, 4, 6), ("dear", "s", "t", 0, 4, 12)],
        );
        let result = solve_primal_dual_interior_point_mcf(&graph, &[4, -4]).expect("scaled IPM");
        assert_eq!(result.flows, vec![4, 0]);
        assert_eq!(result.certificate.total_cost, 24);
        let snapshot = &result.final_snapshot;
        assert!(&snapshot.proxy_gap * 81 < &snapshot.beta * &snapshot.gamma * 4);
        assert_eq!(snapshot.stage, PrimalDualIpmStage::Optimal);
    }

    #[test]
    fn different_centering_seeds_keep_the_same_certified_optimum() {
        let graph = network(
            &["a", "s", "t"],
            &[
                ("sa", "s", "a", 0, 3, 1),
                ("at", "a", "t", 0, 3, 1),
                ("st", "s", "t", 0, 3, 4),
            ],
        );
        let target = [0, 3, -3];
        let left =
            solve_primal_dual_interior_point_mcf_with_seed(&graph, &target, 11).expect("left seed");
        let right = solve_primal_dual_interior_point_mcf_with_seed(&graph, &target, 29)
            .expect("right seed");
        assert_eq!(left.flows, right.flows);
        assert_eq!(left.certificate, right.certificate);
    }

    #[test]
    fn rejects_infeasible_and_oversized_inputs() {
        let graph = network(&["s", "t"], &[("e", "s", "t", 0, 1, 1)]);
        assert!(matches!(
            solve_primal_dual_interior_point_mcf(&graph, &[2, -2]),
            Err(PrimalDualIpmError::Feasibility(_))
        ));
        let oversized = network(
            &["a", "b", "c", "d", "e", "f", "g"],
            &[("ab", "a", "b", 0, 1, 0)],
        );
        assert_eq!(
            solve_primal_dual_interior_point_mcf(&oversized, &[0; 7]),
            Err(PrimalDualIpmError::AdmissionLimit)
        );
    }

    #[test]
    fn trace_checker_rejects_a_mutated_boundary() {
        let graph = network(&["s", "t"], &[("e", "s", "t", 0, 2, 1)]);
        let mut trace = trace_primal_dual_interior_point_mcf(&graph, &[2, -2]).expect("trace");
        trace.events[0].after.mu += 1;
        assert_eq!(
            check_primal_dual_interior_point_mcf_trace(&graph, &[2, -2], &trace),
            Err(PrimalDualIpmError::TraceVerification)
        );
        let mut catalog_trace =
            trace_primal_dual_interior_point_mcf(&graph, &[2, -2]).expect("trace");
        catalog_trace.events[0].catalog_id = "primal-dual-interior-point-mcf.optimal";
        assert_eq!(
            check_primal_dual_interior_point_mcf_trace(&graph, &[2, -2], &catalog_trace),
            Err(PrimalDualIpmError::TraceVerification)
        );
        let mut base_trace = trace_primal_dual_interior_point_mcf(&graph, &[2, -2]).expect("trace");
        base_trace.base_snapshot.mu += 1;
        base_trace.events[0].before.mu += 1;
        assert_eq!(
            check_primal_dual_interior_point_mcf_trace(&graph, &[2, -2], &base_trace),
            Err(PrimalDualIpmError::TraceVerification)
        );
    }
}
