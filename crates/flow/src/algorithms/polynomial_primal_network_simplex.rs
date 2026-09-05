//! Orlin's cost-scaling premultiplier primal network simplex.
//!
//! The state machine follows Sections 3 and 4 of Orlin (1997): rooted-tree
//! premultipliers, eligible and awake nodes, epsilon/4 admissibility, and
//! primal fundamental-cycle pivots. Integral right-hand-side perturbation is
//! represented exactly after multiplying all flows by the extended node
//! count; no floating-point tolerance or generic network-simplex pricing rule
//! is used.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::FlowNetwork;

/// Conservative admission for the explicit proof-oriented implementation.
pub const POLYNOMIAL_PRIMAL_SIMPLEX_MAX_NODES: usize = 64;
/// Conservative edge admission for explicit tree and residual scans.
pub const POLYNOMIAL_PRIMAL_SIMPLEX_MAX_EDGES: usize = 512;
/// Deterministic ceiling on source-defined primal pivots.
pub const POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PIVOTS: u64 = 100_000;
/// Deterministic ceiling on residual, tree, and optimality scans.
pub const POLYNOMIAL_PRIMAL_SIMPLEX_MAX_ARC_SCANS: u128 = 20_000_000;
/// Deterministic ceiling on epsilon-scaling phases.
pub const POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PHASES: u64 = 512;

/// Source-defined publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialPrimalSimplexStage {
    /// A perturbed artificial-star basis has been constructed privately.
    Ready,
    /// The basic feasible star and simplex multipliers were published.
    InitializeBasis,
    /// A new epsilon phase initialized `N*` and awake-node cursors.
    BeginScale,
    /// An eligible, awake, epsilon/4-admissible residual arc was selected.
    SelectAdmissible,
    /// One concrete extended arc was inspected by a source search loop.
    InspectResidual,
    /// One primal fundamental-cycle pivot was committed.
    Pivot,
    /// Eligible premultipliers were increased by `min(delta1, delta2)`.
    ModifyPremultipliers,
    /// `N*` became empty and epsilon/2-optimality was established.
    FinishScale,
    /// The unperturbed original flow was independently certified optimal.
    Optimal,
}

/// Lower/tree/upper partition of a primal network-simplex basis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialPrimalBasisState {
    /// Nonbasic at its lower bound.
    Lower,
    /// Basic spanning-tree arc.
    Tree,
    /// Nonbasic at its upper bound.
    Upper,
}

impl PolynomialPrimalBasisState {
    const fn is_tree(self) -> bool {
        matches!(self, Self::Tree)
    }
}

/// Stable reference to one directed residual orientation in the extended graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolynomialPrimalResidualRef {
    /// Original arcs occupy `[0,m)` and artificial star arcs `[m,m+n)`.
    pub arc_index: usize,
    /// `true` is the stored source-to-target orientation.
    pub forward: bool,
}

/// Source loop responsible for one exact extended-arc inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialPrimalScanKind {
    /// Initial negative reduced-cost scan for a new epsilon scale.
    Scale,
    /// Eligible/awake admissible-residual search.
    Admissible,
    /// Tree-path scan while constructing a fundamental cycle.
    FundamentalCycle,
    /// Terminal or inter-scale basis-optimality search.
    Optimality,
}

/// Exact counters from the bounded scaling-premultiplier kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PolynomialPrimalSimplexMetrics {
    /// Completed epsilon phases.
    pub scaling_phases: u64,
    /// Searches for an admissible residual arc.
    pub admissible_searches: u64,
    /// Directed candidates inspected by scale and admissible searches.
    pub admissible_arc_scans: u128,
    /// Full basis-optimality searches, including the terminal search.
    pub optimality_searches: u64,
    /// Non-tree arcs inspected by basis-optimality searches.
    pub optimality_arc_scans: u128,
    /// Calls that changed at least one premultiplier.
    pub premultiplier_updates: u64,
    /// Node premultipliers changed across all update calls.
    pub updated_nodes: u128,
    /// Nodes whose cursor was conceptually reset on reawakening.
    pub reawakened_nodes: u128,
    /// Source-defined primal fundamental-cycle pivots.
    pub pivots: u64,
    /// Pivots exchanging one entering and one leaving tree arc.
    pub basis_exchanges: u64,
    /// Pivots whose entering arc moved directly to its opposite bound.
    pub bound_flips: u64,
    /// Tree arcs inspected while forming fundamental cycles.
    pub cycle_arc_scans: u128,
    /// Explicit rooted-tree reconstructions, including initialization/reroots.
    pub tree_rebuilds: u64,
}

/// Complete reversible source state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialPrimalSimplexSnapshot {
    /// Semantic boundary.
    pub stage: PolynomialPrimalSimplexStage,
    /// One-based epsilon phase, or zero before the first phase.
    pub phase: u64,
    /// Current rooted-tree root; the artificial root has index `n`.
    pub root: usize,
    /// Exact epsilon for the active/just-completed phase.
    pub epsilon: Option<BigRational>,
    /// Exact paper-convention premultipliers for original nodes plus artificial root.
    pub premultipliers: Vec<BigRational>,
    /// Basis state for original arcs followed by artificial star arcs.
    pub basis_states: Vec<PolynomialPrimalBasisState>,
    /// Exact perturbed lower-shifted flow after multiplication by `n+1`.
    pub perturbed_flows: Vec<i128>,
    /// Unperturbed basic flow on original and artificial arcs.
    pub unperturbed_basic_flows: Vec<i128>,
    /// Nodes whose multiplier has not yet changed in this phase.
    pub n_star: Vec<usize>,
    /// Root candidates under the current premultiplier vector.
    pub eligible_nodes: Vec<usize>,
    /// Nodes awake under `N*` or the epsilon/4 grid rule.
    pub awake_nodes: Vec<usize>,
    /// Selected extended residual orientation.
    pub entering: Option<PolynomialPrimalResidualRef>,
    /// Extended arc inspected by the current source search step.
    pub inspected_arc: Option<usize>,
    /// Residual orientation, when the inspected arc has one.
    pub inspected_residual: Option<PolynomialPrimalResidualRef>,
    /// Source loop responsible for the current inspection.
    pub scan_kind: Option<PolynomialPrimalScanKind>,
    /// Unique leaving tree arc; absent for an entering-arc bound flip.
    pub leaving_arc: Option<usize>,
    /// Directed fundamental cycle, entering residual first.
    pub cycle: Vec<PolynomialPrimalResidualRef>,
    /// Exact perturbed augmentation divided by the perturbation scale.
    pub delta: Option<BigRational>,
    /// Exact premultiplier increase at a modify boundary.
    pub potential_shift: Option<BigRational>,
    /// Original bounded flow after independent terminal certification.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact deterministic work counters.
    pub metrics: PolynomialPrimalSimplexMetrics,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialPrimalSimplexTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: PolynomialPrimalSimplexSnapshot,
    /// State after the transition.
    pub after: PolynomialPrimalSimplexSnapshot,
}

/// Certified polynomial primal-network-simplex result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialPrimalSimplexResult {
    /// Original bounded flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent exact minimum-cost certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact source work counters.
    pub metrics: PolynomialPrimalSimplexMetrics,
    /// Number multiplying the symbolic right-hand-side perturbation.
    pub perturbation_scale: i128,
    /// Strict artificial-arc cost used by phase I.
    pub artificial_cost: i128,
    /// Terminal source state.
    pub final_snapshot: PolynomialPrimalSimplexSnapshot,
}

/// Certified result plus every scaling, update, and pivot boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialPrimalSimplexTraceResult {
    /// Same result returned by the fast profile.
    pub result: PolynomialPrimalSimplexResult,
    /// Ready boundary.
    pub base_snapshot: PolynomialPrimalSimplexSnapshot,
    /// Reversible transitions.
    pub events: Vec<PolynomialPrimalSimplexTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: PolynomialPrimalSimplexSnapshot,
}

/// Domain, arithmetic, work, basis, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PolynomialPrimalSimplexError {
    /// Input exceeds the explicit educational admission band.
    #[error("graph exceeds polynomial primal-network-simplex admission limits")]
    AdmissionLimit,
    /// The source perturbation requires at least one original node.
    #[error("polynomial primal network simplex requires a nonempty graph")]
    EmptyGraph,
    /// A deterministic phase, pivot, or scan ceiling was reached.
    #[error("polynomial primal-network-simplex work limit reached")]
    WorkLimit,
    /// No original flow satisfies the requested balances and bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked integer conversion or arithmetic exceeded the declared domain.
    #[error("polynomial primal-network-simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// Tree, perturbed flow, basis partition, or premultiplier invariant failed.
    #[error("polynomial primal-network-simplex basis invariant failed")]
    BasisInvariant,
    /// The exact perturbation failed to produce a unique leaving residual arc.
    #[error("polynomial primal-network-simplex perturbation invariant failed")]
    PerturbationInvariant,
    /// The optimal extended basis retained original-problem artificial flow.
    #[error("polynomial primal-network-simplex terminated with artificial flow")]
    ArtificialFlow,
    /// A public trace differs from the deterministic source transition grammar.
    #[error("polynomial primal-network-simplex trace verification failed")]
    TraceVerification,
}

/// Solves an exact bounded minimum-cost flow with Orlin's Section 4 rule.
///
/// # Errors
///
/// Returns a domain, feasibility, work, arithmetic, basis, or certificate
/// failure.
pub fn solve_polynomial_primal_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PolynomialPrimalSimplexResult, PolynomialPrimalSimplexError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_polynomial_primal_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PolynomialPrimalSimplexResult, PolynomialPrimalSimplexError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every source-defined scale, multiplier update, and primal pivot.
///
/// # Errors
///
/// Returns the same failures as [`solve_polynomial_primal_network_simplex`]
/// plus trace-verification failures.
pub fn trace_polynomial_primal_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PolynomialPrimalSimplexTraceResult, PolynomialPrimalSimplexError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let trace = PolynomialPrimalSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_polynomial_primal_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Traces polynomial primal network simplex while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_polynomial_primal_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PolynomialPrimalSimplexTraceResult, PolynomialPrimalSimplexError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = PolynomialPrimalSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_polynomial_primal_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Validates every public state and reruns the deterministic source kernel.
///
/// # Errors
///
/// Returns [`PolynomialPrimalSimplexError::TraceVerification`] for any
/// mismatch.
pub fn check_polynomial_primal_network_simplex_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &PolynomialPrimalSimplexTraceResult,
) -> Result<(), PolynomialPrimalSimplexError> {
    validate_public_snapshot(graph, required_divergence, &trace.base_snapshot)?;
    let mut current = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != current || event.catalog_id != catalog_id(event.after.stage) {
            return Err(PolynomialPrimalSimplexError::TraceVerification);
        }
        validate_public_snapshot(graph, required_divergence, &event.after)?;
        current = &event.after;
    }
    if current != &trace.final_snapshot || trace.result.final_snapshot != trace.final_snapshot {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    let expected = solve_internal(graph, required_divergence, true)?;
    if expected.base_snapshot != trace.base_snapshot
        || expected.events != trace.events
        || expected.final_snapshot != trace.final_snapshot
        || expected.result != trace.result
    {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct ArcData {
    source: usize,
    target: usize,
    capacity: i128,
    unscaled_capacity: i128,
    cost: i128,
    flow: i128,
    state: PolynomialPrimalBasisState,
}

#[derive(Clone, Debug)]
struct RootedTree {
    root: usize,
    parent: Vec<Option<usize>>,
    parent_arc: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

struct WorkingState<'graph> {
    graph: &'graph FlowNetwork,
    unscaled_balances: Vec<i128>,
    perturbed_balances: Vec<i128>,
    arcs: Vec<ArcData>,
    original_arc_count: usize,
    original_node_count: usize,
    perturbation_scale: i128,
    artificial_cost: i128,
    tree: RootedTree,
    premultipliers: Vec<BigRational>,
    epsilon: Option<BigRational>,
    phase: u64,
    n_star: Vec<bool>,
    metrics: PolynomialPrimalSimplexMetrics,
}

struct InternalRun {
    result: PolynomialPrimalSimplexResult,
    base_snapshot: PolynomialPrimalSimplexSnapshot,
    events: Vec<PolynomialPrimalSimplexTraceEvent>,
    final_snapshot: PolynomialPrimalSimplexSnapshot,
}

struct TraceJournal {
    current: PolynomialPrimalSimplexSnapshot,
    events: Vec<PolynomialPrimalSimplexTraceEvent>,
    record_trace: bool,
}

impl TraceJournal {
    fn capture(
        &mut self,
        work: &WorkingState<'_>,
        stage: PolynomialPrimalSimplexStage,
        detail: SnapshotDetail,
    ) -> Result<(), PolynomialPrimalSimplexError> {
        publish(
            &mut self.current,
            &mut self.events,
            self.record_trace,
            snapshot(work, stage, detail)?,
        );
        Ok(())
    }
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, PolynomialPrimalSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, PolynomialPrimalSimplexError> {
    validate_domain(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let mut work = WorkingState::initialize(graph, required_divergence)?;
    let base_snapshot = snapshot(
        &work,
        PolynomialPrimalSimplexStage::Ready,
        SnapshotDetail::default(),
    )?;
    let mut journal = TraceJournal {
        current: base_snapshot.clone(),
        events: Vec::new(),
        record_trace,
    };
    journal.capture(
        &work,
        PolynomialPrimalSimplexStage::InitializeBasis,
        SnapshotDetail::default(),
    )?;

    while !work.is_basis_optimal(&mut journal)? {
        run_scale(&mut work, &mut journal)?;
    }

    finish_run(&work, graph, required_divergence, base_snapshot, journal)
}

fn run_scale(
    work: &mut WorkingState<'_>,
    journal: &mut TraceJournal,
) -> Result<(), PolynomialPrimalSimplexError> {
    work.start_scale(journal)?;
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::BeginScale,
        SnapshotDetail::default(),
    )?;
    loop {
        if work.n_star.iter().all(|&value| !value) {
            return finish_active_scale(work, journal);
        }
        if let Some(entering) = work.find_admissible(journal)? {
            capture_pivot(work, journal, entering)?;
        } else if let Some(shift) = work.modify_premultipliers()? {
            journal.capture(
                work,
                PolynomialPrimalSimplexStage::ModifyPremultipliers,
                SnapshotDetail {
                    potential_shift: Some(shift),
                    ..SnapshotDetail::default()
                },
            )?;
        } else {
            return finish_active_scale(work, journal);
        }
    }
}

fn capture_pivot(
    work: &mut WorkingState<'_>,
    journal: &mut TraceJournal,
    entering: PolynomialPrimalResidualRef,
) -> Result<(), PolynomialPrimalSimplexError> {
    let cycle = work.prepare_cycle(entering, journal)?;
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::SelectAdmissible,
        SnapshotDetail {
            entering: Some(entering),
            cycle: cycle.refs.clone(),
            ..SnapshotDetail::default()
        },
    )?;
    let pivot = work.pivot(entering, &cycle)?;
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::Pivot,
        SnapshotDetail {
            entering: Some(entering),
            leaving_arc: pivot.leaving,
            cycle: cycle.refs,
            delta: Some(rational_from_scaled(pivot.delta, work.perturbation_scale)),
            ..SnapshotDetail::default()
        },
    )
}

fn finish_active_scale(
    work: &mut WorkingState<'_>,
    journal: &mut TraceJournal,
) -> Result<(), PolynomialPrimalSimplexError> {
    work.finish_scale()?;
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::FinishScale,
        SnapshotDetail::default(),
    )
}

fn finish_run(
    work: &WorkingState<'_>,
    graph: &FlowNetwork,
    required_divergence: &[i128],
    base_snapshot: PolynomialPrimalSimplexSnapshot,
    mut journal: TraceJournal,
) -> Result<InternalRun, PolynomialPrimalSimplexError> {
    let unperturbed = work.reconstruct_unperturbed_basic_flows()?;
    if unperturbed[work.original_arc_count..]
        .iter()
        .any(|&flow| flow != 0)
    {
        return Err(PolynomialPrimalSimplexError::ArtificialFlow);
    }
    let flows = work.original_bounded_flows(&unperturbed)?;
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::Optimal,
        SnapshotDetail {
            certified_flows: Some(flows.clone()),
            ..SnapshotDetail::default()
        },
    )?;
    let final_snapshot = journal.current;
    let result = PolynomialPrimalSimplexResult {
        flows,
        certificate,
        metrics: work.metrics,
        perturbation_scale: work.perturbation_scale,
        artificial_cost: work.artificial_cost,
        final_snapshot: final_snapshot.clone(),
    };
    Ok(InternalRun {
        result,
        base_snapshot,
        events: journal.events,
        final_snapshot,
    })
}

fn validate_domain(graph: &FlowNetwork) -> Result<(), PolynomialPrimalSimplexError> {
    if graph.nodes().is_empty() {
        return Err(PolynomialPrimalSimplexError::EmptyGraph);
    }
    if graph.nodes().len() > POLYNOMIAL_PRIMAL_SIMPLEX_MAX_NODES
        || graph.edges().len() > POLYNOMIAL_PRIMAL_SIMPLEX_MAX_EDGES
    {
        return Err(PolynomialPrimalSimplexError::AdmissionLimit);
    }
    Ok(())
}

struct PerturbedBalances {
    unscaled: Vec<i128>,
    scaled: Vec<i128>,
    scale: i128,
}

fn build_perturbed_balances(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PerturbedBalances, PolynomialPrimalSimplexError> {
    let extended_node_count = graph
        .nodes()
        .len()
        .checked_add(1)
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
    let scale = i128::try_from(extended_node_count)
        .map_err(|_| PolynomialPrimalSimplexError::ArithmeticOverflow)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(crate::model::FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower_flows)?;
    let mut unscaled = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, lower)| {
            required
                .checked_sub(lower)
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    unscaled.push(0);
    if checked_sum(&unscaled)? != 0 {
        return Err(PolynomialPrimalSimplexError::BasisInvariant);
    }
    let scaled = unscaled
        .iter()
        .enumerate()
        .map(|(node, &balance)| {
            let perturbation = if node == 0 { scale - 1 } else { -1 };
            balance
                .checked_mul(scale)
                .and_then(|value| value.checked_add(perturbation))
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if checked_sum(&scaled)? != 0 {
        return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
    }
    Ok(PerturbedBalances {
        unscaled,
        scaled,
        scale,
    })
}

fn artificial_arc_cost(
    graph: &FlowNetwork,
    scale: i128,
) -> Result<i128, PolynomialPrimalSimplexError> {
    let maximum_cost = graph
        .edges()
        .iter()
        .map(|edge| i128::from(edge.cost()).abs())
        .max()
        .unwrap_or(0);
    scale
        .checked_add(1)
        .and_then(|factor| maximum_cost.checked_add(1)?.checked_mul(factor))
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
}

fn build_initial_arcs(
    graph: &FlowNetwork,
    balances: &PerturbedBalances,
    artificial_cost: i128,
) -> Result<Vec<ArcData>, PolynomialPrimalSimplexError> {
    let mut arcs = Vec::with_capacity(graph.edges().len() + graph.nodes().len());
    let mut scaled_original_capacity_sum = 0_i128;
    for edge in graph.edges() {
        let width = i128::from(edge.capacity() - edge.lower());
        let capacity = width
            .checked_mul(balances.scale)
            .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        scaled_original_capacity_sum = scaled_original_capacity_sum
            .checked_add(capacity)
            .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        arcs.push(ArcData {
            source: edge.from().as_usize(),
            target: edge.to().as_usize(),
            capacity,
            unscaled_capacity: width,
            cost: i128::from(edge.cost()),
            flow: 0,
            state: PolynomialPrimalBasisState::Lower,
        });
    }
    append_artificial_star(
        &mut arcs,
        &balances.scaled,
        balances.scale,
        scaled_original_capacity_sum,
        artificial_cost,
        graph.nodes().len(),
    )?;
    Ok(arcs)
}

fn append_artificial_star(
    arcs: &mut Vec<ArcData>,
    perturbed_balances: &[i128],
    scale: i128,
    original_capacity_sum: i128,
    artificial_cost: i128,
    original_node_count: usize,
) -> Result<(), PolynomialPrimalSimplexError> {
    let total_balance = perturbed_balances.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(
            value
                .checked_abs()
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?,
        )
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
    })?;
    let unscaled_capacity = total_balance
        .checked_add(original_capacity_sum)
        .and_then(|value| value.checked_add(scale))
        .and_then(|value| value.checked_add(scale - 1))
        .and_then(|value| value.checked_div(scale))
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
    let capacity = unscaled_capacity
        .checked_mul(scale)
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
    for (node, &balance) in perturbed_balances[..original_node_count].iter().enumerate() {
        let (source, target, flow) =
            artificial_arc_orientation(node, original_node_count, balance)?;
        if flow <= 0 || flow >= capacity {
            return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
        }
        arcs.push(ArcData {
            source,
            target,
            capacity,
            unscaled_capacity,
            cost: artificial_cost,
            flow,
            state: PolynomialPrimalBasisState::Tree,
        });
    }
    Ok(())
}

fn artificial_arc_orientation(
    node: usize,
    root: usize,
    balance: i128,
) -> Result<(usize, usize, i128), PolynomialPrimalSimplexError> {
    if balance > 0 {
        Ok((node, root, balance))
    } else {
        Ok((
            root,
            node,
            balance
                .checked_neg()
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?,
        ))
    }
}

impl<'graph> WorkingState<'graph> {
    fn initialize(
        graph: &'graph FlowNetwork,
        required_divergence: &[i128],
    ) -> Result<Self, PolynomialPrimalSimplexError> {
        if required_divergence.len() != graph.nodes().len() {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        let original_node_count = graph.nodes().len();
        let original_arc_count = graph.edges().len();
        let balances = build_perturbed_balances(graph, required_divergence)?;
        let artificial_cost = artificial_arc_cost(graph, balances.scale)?;
        let arcs = build_initial_arcs(graph, &balances, artificial_cost)?;
        let extended_node_count = original_node_count + 1;
        let artificial_root = original_node_count;

        let mut work = Self {
            graph,
            unscaled_balances: balances.unscaled,
            perturbed_balances: balances.scaled,
            arcs,
            original_arc_count,
            original_node_count,
            perturbation_scale: balances.scale,
            artificial_cost,
            tree: RootedTree {
                root: artificial_root,
                parent: Vec::new(),
                parent_arc: Vec::new(),
                children: Vec::new(),
            },
            premultipliers: Vec::new(),
            epsilon: None,
            phase: 0,
            n_star: vec![false; extended_node_count],
            metrics: PolynomialPrimalSimplexMetrics::default(),
        };
        work.rebuild_tree(artificial_root)?;
        work.install_simplex_multipliers()?;
        work.validate_basis()?;
        Ok(work)
    }

    fn start_scale(
        &mut self,
        journal: &mut TraceJournal,
    ) -> Result<(), PolynomialPrimalSimplexError> {
        if self.phase >= POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PHASES {
            return Err(PolynomialPrimalSimplexError::WorkLimit);
        }
        let epsilon = self.maximum_negative_reduced_cost(journal)?;
        if epsilon.is_zero() {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        self.phase += 1;
        self.metrics.scaling_phases += 1;
        self.epsilon = Some(epsilon);
        self.n_star.fill(true);
        self.validate_basis()
    }

    fn finish_scale(&mut self) -> Result<(), PolynomialPrimalSimplexError> {
        if self.n_star.iter().any(|&value| value) {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        let epsilon = self
            .epsilon
            .as_ref()
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        self.validate_epsilon_optimal(&(epsilon / rational_i128(2)))?;
        self.validate_basis()
    }

    fn find_admissible(
        &mut self,
        journal: &mut TraceJournal,
    ) -> Result<Option<PolynomialPrimalResidualRef>, PolynomialPrimalSimplexError> {
        self.metrics.admissible_searches += 1;
        let eligible = self.eligible_nodes()?;
        let awake = self.awake_nodes();
        let epsilon = self
            .epsilon
            .as_ref()
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        let threshold = -(epsilon / rational_i128(4));
        for node in 0..self.extended_node_count() {
            if !eligible[node] || !awake[node] {
                continue;
            }
            for arc_index in 0..self.arcs.len() {
                self.bump_scan()?;
                let residual = self.non_tree_residual(arc_index)?;
                capture_polynomial_primal_scan(
                    self,
                    journal,
                    PolynomialPrimalScanKind::Admissible,
                    arc_index,
                    residual,
                )?;
                let Some(residual) = residual else {
                    continue;
                };
                let (from, _) = self.residual_endpoints(residual)?;
                if from == node && self.reduced_cost(residual)? < threshold {
                    return Ok(Some(residual));
                }
            }
        }
        Ok(None)
    }

    fn modify_premultipliers(
        &mut self,
    ) -> Result<Option<BigRational>, PolynomialPrimalSimplexError> {
        let eligible = self.eligible_nodes()?;
        let awake_before = self.awake_nodes();
        let selected = eligible
            .iter()
            .enumerate()
            .filter_map(|(node, &value)| value.then_some(node))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        for &node in &selected {
            self.n_star[node] = false;
        }
        if self.n_star.iter().all(|&value| !value) {
            return Ok(None);
        }

        let mut delta_one: Option<BigRational> = None;
        for child in 0..self.extended_node_count() {
            let Some(parent) = self.tree.parent[child] else {
                continue;
            };
            if eligible[child] || !eligible[parent] {
                continue;
            }
            let arc_index =
                self.tree.parent_arc[child].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            let residual = self.residual_between(arc_index, child, parent)?;
            let candidate = -self.reduced_cost(residual)?;
            if candidate <= BigRational::zero() {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            }
            if delta_one
                .as_ref()
                .is_none_or(|current| candidate < *current)
            {
                delta_one = Some(candidate);
            }
        }
        let quarter = self
            .epsilon
            .as_ref()
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?
            / rational_i128(4);
        let delta_two = selected
            .iter()
            .map(|&node| distance_to_next_grid(&self.premultipliers[node], &quarter))
            .min()
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        let shift = delta_one.map_or(delta_two.clone(), |first| first.min(delta_two));
        if shift <= BigRational::zero() {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        for &node in &selected {
            self.premultipliers[node] += shift.clone();
        }
        self.metrics.premultiplier_updates += 1;
        self.metrics.updated_nodes = self
            .metrics
            .updated_nodes
            .checked_add(selected.len() as u128)
            .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        let awake_after = self.awake_nodes();
        self.metrics.reawakened_nodes = self
            .metrics
            .reawakened_nodes
            .checked_add(
                awake_before
                    .iter()
                    .zip(awake_after)
                    .filter(|(before, after)| !**before && *after)
                    .count() as u128,
            )
            .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        self.validate_basis()?;
        Ok(Some(shift))
    }

    fn prepare_cycle(
        &mut self,
        entering: PolynomialPrimalResidualRef,
        journal: &mut TraceJournal,
    ) -> Result<BasicCycle, PolynomialPrimalSimplexError> {
        let (tail, head) = self.residual_endpoints(entering)?;
        let eligible = self.eligible_nodes()?;
        if !eligible[tail] {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        self.rebuild_tree(tail)?;
        let mut path = Vec::new();
        let mut refs = vec![entering];
        let mut node = head;
        while node != tail {
            let parent =
                self.tree.parent[node].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            let arc_index =
                self.tree.parent_arc[node].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            let residual = self.residual_between(arc_index, node, parent)?;
            path.push((node, arc_index, residual));
            refs.push(residual);
            node = parent;
            self.bump_cycle_scan()?;
            capture_polynomial_primal_scan(
                self,
                journal,
                PolynomialPrimalScanKind::FundamentalCycle,
                arc_index,
                Some(residual),
            )?;
        }
        Ok(BasicCycle { path, refs })
    }

    fn pivot(
        &mut self,
        entering: PolynomialPrimalResidualRef,
        cycle: &BasicCycle,
    ) -> Result<PivotResult, PolynomialPrimalSimplexError> {
        if self.metrics.pivots >= POLYNOMIAL_PRIMAL_SIMPLEX_MAX_PIVOTS {
            return Err(PolynomialPrimalSimplexError::WorkLimit);
        }
        let mut candidates = Vec::with_capacity(cycle.path.len() + 1);
        candidates.push((entering.arc_index, None, self.residual_capacity(entering)?));
        for &(child, arc_index, residual) in &cycle.path {
            candidates.push((arc_index, Some(child), self.residual_capacity(residual)?));
        }
        let delta = candidates
            .iter()
            .map(|entry| entry.2)
            .min()
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        if delta <= 0 {
            return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
        }
        let bottlenecks = candidates
            .iter()
            .filter(|entry| entry.2 == delta)
            .collect::<Vec<_>>();
        if bottlenecks.len() != 1 {
            return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
        }
        self.augment(entering, delta)?;
        for &(_, _, residual) in &cycle.path {
            self.augment(residual, delta)?;
        }

        let bottleneck = bottlenecks[0];
        let leaving = bottleneck.1.map(|_| bottleneck.0);
        if let Some(leaving_arc) = leaving {
            let leaving_child = bottleneck
                .1
                .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            self.arcs[entering.arc_index].state = PolynomialPrimalBasisState::Tree;
            self.arcs[leaving_arc].state = if self.arcs[leaving_arc].flow == 0 {
                PolynomialPrimalBasisState::Lower
            } else if self.arcs[leaving_arc].flow == self.arcs[leaving_arc].capacity {
                PolynomialPrimalBasisState::Upper
            } else {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            };
            self.metrics.basis_exchanges += 1;
            self.rebuild_tree(leaving_child)?;
        } else {
            self.arcs[entering.arc_index].state = match self.arcs[entering.arc_index].state {
                PolynomialPrimalBasisState::Lower => PolynomialPrimalBasisState::Upper,
                PolynomialPrimalBasisState::Upper => PolynomialPrimalBasisState::Lower,
                PolynomialPrimalBasisState::Tree => {
                    return Err(PolynomialPrimalSimplexError::BasisInvariant);
                }
            };
            self.metrics.bound_flips += 1;
        }
        self.metrics.pivots += 1;
        self.validate_basis()?;
        Ok(PivotResult { delta, leaving })
    }

    fn is_basis_optimal(
        &mut self,
        journal: &mut TraceJournal,
    ) -> Result<bool, PolynomialPrimalSimplexError> {
        self.metrics.optimality_searches += 1;
        let simplex = self.simplex_multipliers()?;
        for arc_index in 0..self.arcs.len() {
            if self.arcs[arc_index].state.is_tree() {
                continue;
            }
            self.bump_optimality_scan()?;
            let residual = self
                .non_tree_residual(arc_index)?
                .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            capture_polynomial_primal_scan(
                self,
                journal,
                PolynomialPrimalScanKind::Optimality,
                arc_index,
                Some(residual),
            )?;
            if reduced_cost_with(&self.arcs, &simplex, residual)? < BigRational::zero() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn maximum_negative_reduced_cost(
        &mut self,
        journal: &mut TraceJournal,
    ) -> Result<BigRational, PolynomialPrimalSimplexError> {
        let mut maximum = BigRational::zero();
        let residuals = self.positive_residuals();
        for residual in residuals {
            self.bump_scan()?;
            capture_polynomial_primal_scan(
                self,
                journal,
                PolynomialPrimalScanKind::Scale,
                residual.arc_index,
                Some(residual),
            )?;
            let reduced = self.reduced_cost(residual)?;
            if reduced < BigRational::zero() {
                maximum = maximum.max(-reduced);
            }
        }
        Ok(maximum)
    }

    fn validate_epsilon_optimal(
        &self,
        epsilon: &BigRational,
    ) -> Result<(), PolynomialPrimalSimplexError> {
        for residual in self.positive_residuals() {
            if self.reduced_cost(residual)? < -epsilon {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            }
        }
        Ok(())
    }

    fn eligible_nodes(&self) -> Result<Vec<bool>, PolynomialPrimalSimplexError> {
        let mut eligible = vec![false; self.extended_node_count()];
        eligible[self.tree.root] = true;
        let mut stack = vec![self.tree.root];
        while let Some(parent) = stack.pop() {
            for &child in &self.tree.children[parent] {
                let arc_index = self.tree.parent_arc[child]
                    .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
                let residual = self.residual_between(arc_index, child, parent)?;
                if self.reduced_cost(residual)?.is_zero() {
                    eligible[child] = true;
                    stack.push(child);
                }
            }
        }
        Ok(eligible)
    }

    fn awake_nodes(&self) -> Vec<bool> {
        let Some(epsilon) = self.epsilon.as_ref() else {
            return vec![false; self.extended_node_count()];
        };
        let quarter = epsilon / rational_i128(4);
        self.premultipliers
            .iter()
            .enumerate()
            .map(|(node, value)| self.n_star[node] || on_grid(value, &quarter))
            .collect()
    }

    fn validate_basis(&self) -> Result<(), PolynomialPrimalSimplexError> {
        let node_count = self.extended_node_count();
        if self.tree.parent.len() != node_count
            || self.tree.parent_arc.len() != node_count
            || self.tree.children.len() != node_count
            || self.premultipliers.len() != node_count
            || self.n_star.len() != node_count
        {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        if self.arcs.iter().filter(|arc| arc.state.is_tree()).count() + 1 != node_count {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        let mut divergence = vec![0_i128; node_count];
        for arc in &self.arcs {
            if arc.flow < 0 || arc.flow > arc.capacity {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            }
            match arc.state {
                PolynomialPrimalBasisState::Lower if arc.flow != 0 => {
                    return Err(PolynomialPrimalSimplexError::BasisInvariant);
                }
                PolynomialPrimalBasisState::Upper if arc.flow != arc.capacity => {
                    return Err(PolynomialPrimalSimplexError::BasisInvariant);
                }
                PolynomialPrimalBasisState::Tree if arc.flow <= 0 || arc.flow >= arc.capacity => {
                    return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
                }
                PolynomialPrimalBasisState::Lower
                | PolynomialPrimalBasisState::Tree
                | PolynomialPrimalBasisState::Upper => {}
            }
            divergence[arc.source] = divergence[arc.source]
                .checked_add(arc.flow)
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
            divergence[arc.target] = divergence[arc.target]
                .checked_sub(arc.flow)
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        }
        if divergence != self.perturbed_balances {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        for child in 0..node_count {
            let Some(parent) = self.tree.parent[child] else {
                if child != self.tree.root {
                    return Err(PolynomialPrimalSimplexError::BasisInvariant);
                }
                continue;
            };
            let arc_index =
                self.tree.parent_arc[child].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            if !self.arcs[arc_index].state.is_tree()
                || self.reduced_cost(self.residual_between(arc_index, child, parent)?)?
                    > BigRational::zero()
            {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            }
        }
        if let Some(epsilon) = &self.epsilon {
            self.validate_epsilon_optimal(epsilon)?;
        }
        let unperturbed = self.reconstruct_unperturbed_basic_flows()?;
        for (arc, flow) in self.arcs.iter().zip(unperturbed) {
            if flow < 0 || flow > arc.unscaled_capacity {
                return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
            }
        }
        Ok(())
    }

    fn rebuild_tree(&mut self, root: usize) -> Result<(), PolynomialPrimalSimplexError> {
        let node_count = self.extended_node_count();
        if root >= node_count {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        let mut adjacency = vec![Vec::<(usize, usize)>::new(); node_count];
        for (arc_index, arc) in self.arcs.iter().enumerate() {
            if !arc.state.is_tree() {
                continue;
            }
            if arc.source == arc.target {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            }
            adjacency[arc.source].push((arc.target, arc_index));
            adjacency[arc.target].push((arc.source, arc_index));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable();
        }
        let mut parent = vec![None; node_count];
        let mut parent_arc = vec![None; node_count];
        let mut children = vec![Vec::new(); node_count];
        parent[root] = Some(root);
        let mut order = vec![root];
        let mut cursor = 0;
        while cursor < order.len() {
            let node = order[cursor];
            cursor += 1;
            for &(next, arc_index) in &adjacency[node] {
                if parent[next].is_some() {
                    continue;
                }
                parent[next] = Some(node);
                parent_arc[next] = Some(arc_index);
                children[node].push(next);
                order.push(next);
            }
        }
        if order.len() != node_count {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        parent[root] = None;
        self.tree = RootedTree {
            root,
            parent,
            parent_arc,
            children,
        };
        self.metrics.tree_rebuilds += 1;
        Ok(())
    }

    fn install_simplex_multipliers(&mut self) -> Result<(), PolynomialPrimalSimplexError> {
        let mut values = vec![BigRational::zero(); self.extended_node_count()];
        let mut stack = vec![self.tree.root];
        while let Some(parent) = stack.pop() {
            for &child in &self.tree.children[parent] {
                let arc_index = self.tree.parent_arc[child]
                    .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
                let residual = self.residual_between(arc_index, child, parent)?;
                let residual_cost = self.residual_cost(residual)?;
                values[child] = &values[parent] + rational_i128(residual_cost);
                stack.push(child);
            }
        }
        self.premultipliers = values;
        Ok(())
    }

    fn simplex_multipliers(&self) -> Result<Vec<BigRational>, PolynomialPrimalSimplexError> {
        let mut values = vec![BigRational::zero(); self.extended_node_count()];
        let mut stack = vec![self.tree.root];
        while let Some(parent) = stack.pop() {
            for &child in &self.tree.children[parent] {
                let arc_index = self.tree.parent_arc[child]
                    .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
                let residual = self.residual_between(arc_index, child, parent)?;
                values[child] = &values[parent] + rational_i128(self.residual_cost(residual)?);
                stack.push(child);
            }
        }
        Ok(values)
    }

    fn reconstruct_unperturbed_basic_flows(
        &self,
    ) -> Result<Vec<i128>, PolynomialPrimalSimplexError> {
        let node_count = self.extended_node_count();
        let mut flows = vec![0_i128; self.arcs.len()];
        let mut remaining = self.unscaled_balances.clone();
        for (index, arc) in self.arcs.iter().enumerate() {
            flows[index] = match arc.state {
                PolynomialPrimalBasisState::Lower | PolynomialPrimalBasisState::Tree => 0,
                PolynomialPrimalBasisState::Upper => arc.unscaled_capacity,
            };
            if !arc.state.is_tree() {
                remaining[arc.source] = remaining[arc.source]
                    .checked_sub(flows[index])
                    .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
                remaining[arc.target] = remaining[arc.target]
                    .checked_add(flows[index])
                    .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
            }
        }
        let mut degree = vec![0_usize; node_count];
        for arc in &self.arcs {
            if arc.state.is_tree() {
                degree[arc.source] += 1;
                degree[arc.target] += 1;
            }
        }
        let mut leaves = (0..node_count)
            .filter(|&node| node != self.tree.root && degree[node] == 1)
            .collect::<Vec<_>>();
        leaves.sort_unstable_by(|left, right| right.cmp(left));
        while let Some(node) = leaves.pop() {
            if degree[node] != 1 {
                continue;
            }
            let parent =
                self.tree.parent[node].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            let arc_index =
                self.tree.parent_arc[node].ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            let arc = &self.arcs[arc_index];
            let flow = if arc.source == node && arc.target == parent {
                remaining[node]
            } else if arc.source == parent && arc.target == node {
                remaining[node]
                    .checked_neg()
                    .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?
            } else {
                return Err(PolynomialPrimalSimplexError::BasisInvariant);
            };
            if flow < 0 || flow > arc.unscaled_capacity {
                return Err(PolynomialPrimalSimplexError::PerturbationInvariant);
            }
            flows[arc_index] = flow;
            remaining[parent] = remaining[parent]
                .checked_add(remaining[node])
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
            remaining[node] = 0;
            degree[node] = 0;
            degree[parent] = degree[parent]
                .checked_sub(1)
                .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
            if parent != self.tree.root && degree[parent] == 1 {
                leaves.push(parent);
                leaves.sort_unstable_by(|left, right| right.cmp(left));
            }
        }
        if remaining.iter().any(|&value| value != 0) {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        Ok(flows)
    }

    fn original_bounded_flows(
        &self,
        unperturbed: &[i128],
    ) -> Result<Vec<u64>, PolynomialPrimalSimplexError> {
        self.graph
            .edges()
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                i128::from(edge.lower())
                    .checked_add(unperturbed[index])
                    .and_then(|flow| u64::try_from(flow).ok())
                    .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
            })
            .collect()
    }

    fn positive_residuals(&self) -> Vec<PolynomialPrimalResidualRef> {
        let mut residuals = Vec::with_capacity(self.arcs.len() * 2);
        for (arc_index, arc) in self.arcs.iter().enumerate() {
            if arc.flow < arc.capacity {
                residuals.push(PolynomialPrimalResidualRef {
                    arc_index,
                    forward: true,
                });
            }
            if arc.flow > 0 {
                residuals.push(PolynomialPrimalResidualRef {
                    arc_index,
                    forward: false,
                });
            }
        }
        residuals
    }

    fn non_tree_residual(
        &self,
        arc_index: usize,
    ) -> Result<Option<PolynomialPrimalResidualRef>, PolynomialPrimalSimplexError> {
        let arc = self
            .arcs
            .get(arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        Ok(match arc.state {
            PolynomialPrimalBasisState::Lower => Some(PolynomialPrimalResidualRef {
                arc_index,
                forward: true,
            }),
            PolynomialPrimalBasisState::Upper => Some(PolynomialPrimalResidualRef {
                arc_index,
                forward: false,
            }),
            PolynomialPrimalBasisState::Tree => None,
        })
    }

    fn residual_between(
        &self,
        arc_index: usize,
        from: usize,
        to: usize,
    ) -> Result<PolynomialPrimalResidualRef, PolynomialPrimalSimplexError> {
        let arc = self
            .arcs
            .get(arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        if arc.source == from && arc.target == to {
            Ok(PolynomialPrimalResidualRef {
                arc_index,
                forward: true,
            })
        } else if arc.source == to && arc.target == from {
            Ok(PolynomialPrimalResidualRef {
                arc_index,
                forward: false,
            })
        } else {
            Err(PolynomialPrimalSimplexError::BasisInvariant)
        }
    }

    fn residual_endpoints(
        &self,
        residual: PolynomialPrimalResidualRef,
    ) -> Result<(usize, usize), PolynomialPrimalSimplexError> {
        let arc = self
            .arcs
            .get(residual.arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        Ok(if residual.forward {
            (arc.source, arc.target)
        } else {
            (arc.target, arc.source)
        })
    }

    fn residual_cost(
        &self,
        residual: PolynomialPrimalResidualRef,
    ) -> Result<i128, PolynomialPrimalSimplexError> {
        let cost = self
            .arcs
            .get(residual.arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?
            .cost;
        if residual.forward {
            Ok(cost)
        } else {
            cost.checked_neg()
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
        }
    }

    fn residual_capacity(
        &self,
        residual: PolynomialPrimalResidualRef,
    ) -> Result<i128, PolynomialPrimalSimplexError> {
        let arc = self
            .arcs
            .get(residual.arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        if residual.forward {
            arc.capacity
                .checked_sub(arc.flow)
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
        } else {
            Ok(arc.flow)
        }
    }

    fn reduced_cost(
        &self,
        residual: PolynomialPrimalResidualRef,
    ) -> Result<BigRational, PolynomialPrimalSimplexError> {
        reduced_cost_with(&self.arcs, &self.premultipliers, residual)
    }

    fn augment(
        &mut self,
        residual: PolynomialPrimalResidualRef,
        delta: i128,
    ) -> Result<(), PolynomialPrimalSimplexError> {
        let arc = self
            .arcs
            .get_mut(residual.arc_index)
            .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
        arc.flow = if residual.forward {
            arc.flow.checked_add(delta)
        } else {
            arc.flow.checked_sub(delta)
        }
        .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?;
        if arc.flow < 0 || arc.flow > arc.capacity {
            return Err(PolynomialPrimalSimplexError::BasisInvariant);
        }
        Ok(())
    }

    const fn extended_node_count(&self) -> usize {
        self.original_node_count + 1
    }

    fn bump_scan(&mut self) -> Result<(), PolynomialPrimalSimplexError> {
        self.ensure_scan_budget()?;
        self.metrics.admissible_arc_scans += 1;
        Ok(())
    }

    fn bump_optimality_scan(&mut self) -> Result<(), PolynomialPrimalSimplexError> {
        self.ensure_scan_budget()?;
        self.metrics.optimality_arc_scans += 1;
        Ok(())
    }

    fn bump_cycle_scan(&mut self) -> Result<(), PolynomialPrimalSimplexError> {
        self.ensure_scan_budget()?;
        self.metrics.cycle_arc_scans += 1;
        Ok(())
    }

    fn ensure_scan_budget(&self) -> Result<(), PolynomialPrimalSimplexError> {
        if self.metrics.admissible_arc_scans
            + self.metrics.optimality_arc_scans
            + self.metrics.cycle_arc_scans
            >= POLYNOMIAL_PRIMAL_SIMPLEX_MAX_ARC_SCANS
        {
            return Err(PolynomialPrimalSimplexError::WorkLimit);
        }
        Ok(())
    }
}

fn capture_polynomial_primal_scan(
    work: &WorkingState<'_>,
    journal: &mut TraceJournal,
    scan_kind: PolynomialPrimalScanKind,
    inspected_arc: usize,
    inspected_residual: Option<PolynomialPrimalResidualRef>,
) -> Result<(), PolynomialPrimalSimplexError> {
    journal.capture(
        work,
        PolynomialPrimalSimplexStage::InspectResidual,
        SnapshotDetail {
            inspected_arc: Some(inspected_arc),
            inspected_residual,
            scan_kind: Some(scan_kind),
            ..SnapshotDetail::default()
        },
    )
}

struct BasicCycle {
    path: Vec<(usize, usize, PolynomialPrimalResidualRef)>,
    refs: Vec<PolynomialPrimalResidualRef>,
}

struct PivotResult {
    delta: i128,
    leaving: Option<usize>,
}

#[derive(Default)]
struct SnapshotDetail {
    entering: Option<PolynomialPrimalResidualRef>,
    inspected_arc: Option<usize>,
    inspected_residual: Option<PolynomialPrimalResidualRef>,
    scan_kind: Option<PolynomialPrimalScanKind>,
    leaving_arc: Option<usize>,
    cycle: Vec<PolynomialPrimalResidualRef>,
    delta: Option<BigRational>,
    potential_shift: Option<BigRational>,
    certified_flows: Option<Vec<u64>>,
}

fn reduced_cost_with(
    arcs: &[ArcData],
    premultipliers: &[BigRational],
    residual: PolynomialPrimalResidualRef,
) -> Result<BigRational, PolynomialPrimalSimplexError> {
    let arc = arcs
        .get(residual.arc_index)
        .ok_or(PolynomialPrimalSimplexError::BasisInvariant)?;
    let (from, to, cost) = if residual.forward {
        (arc.source, arc.target, arc.cost)
    } else {
        (
            arc.target,
            arc.source,
            arc.cost
                .checked_neg()
                .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)?,
        )
    };
    Ok(rational_i128(cost) - &premultipliers[from] + &premultipliers[to])
}

fn snapshot(
    work: &WorkingState<'_>,
    stage: PolynomialPrimalSimplexStage,
    detail: SnapshotDetail,
) -> Result<PolynomialPrimalSimplexSnapshot, PolynomialPrimalSimplexError> {
    let eligible = work.eligible_nodes()?;
    let awake = work.awake_nodes();
    Ok(PolynomialPrimalSimplexSnapshot {
        stage,
        phase: work.phase,
        root: work.tree.root,
        epsilon: work.epsilon.clone(),
        premultipliers: work.premultipliers.clone(),
        basis_states: work.arcs.iter().map(|arc| arc.state).collect(),
        perturbed_flows: work.arcs.iter().map(|arc| arc.flow).collect(),
        unperturbed_basic_flows: work.reconstruct_unperturbed_basic_flows()?,
        n_star: bool_indices(&work.n_star),
        eligible_nodes: bool_indices(&eligible),
        awake_nodes: bool_indices(&awake),
        entering: detail.entering,
        inspected_arc: detail.inspected_arc,
        inspected_residual: detail.inspected_residual,
        scan_kind: detail.scan_kind,
        leaving_arc: detail.leaving_arc,
        cycle: detail.cycle,
        delta: detail.delta,
        potential_shift: detail.potential_shift,
        certified_flows: detail.certified_flows,
        metrics: work.metrics,
    })
}

fn publish(
    current: &mut PolynomialPrimalSimplexSnapshot,
    events: &mut Vec<PolynomialPrimalSimplexTraceEvent>,
    record_trace: bool,
    after: PolynomialPrimalSimplexSnapshot,
) {
    if record_trace {
        events.push(PolynomialPrimalSimplexTraceEvent {
            catalog_id: catalog_id(after.stage),
            before: current.clone(),
            after: after.clone(),
        });
    }
    *current = after;
}

const fn catalog_id(stage: PolynomialPrimalSimplexStage) -> &'static str {
    match stage {
        PolynomialPrimalSimplexStage::Ready => "polynomial-primal-network-simplex.ready",
        PolynomialPrimalSimplexStage::InitializeBasis => {
            "polynomial-primal-network-simplex.initialize-perturbed-basis"
        }
        PolynomialPrimalSimplexStage::BeginScale => {
            "polynomial-primal-network-simplex.begin-epsilon-scale"
        }
        PolynomialPrimalSimplexStage::SelectAdmissible => {
            "polynomial-primal-network-simplex.select-admissible-arc"
        }
        PolynomialPrimalSimplexStage::InspectResidual => {
            "polynomial-primal-network-simplex.inspect-extended-arc"
        }
        PolynomialPrimalSimplexStage::Pivot => {
            "polynomial-primal-network-simplex.pivot-fundamental-cycle"
        }
        PolynomialPrimalSimplexStage::ModifyPremultipliers => {
            "polynomial-primal-network-simplex.modify-epsilon-premultipliers"
        }
        PolynomialPrimalSimplexStage::FinishScale => {
            "polynomial-primal-network-simplex.finish-epsilon-scale"
        }
        PolynomialPrimalSimplexStage::Optimal => "polynomial-primal-network-simplex.optimal",
    }
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &PolynomialPrimalSimplexSnapshot,
) -> Result<(), PolynomialPrimalSimplexError> {
    let mut work = WorkingState::initialize(graph, required_divergence)?;
    if snapshot.basis_states.len() != work.arcs.len()
        || snapshot.perturbed_flows.len() != work.arcs.len()
        || snapshot.unperturbed_basic_flows.len() != work.arcs.len()
        || snapshot.premultipliers.len() != work.extended_node_count()
        || snapshot.root >= work.extended_node_count()
    {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    for (arc, (&state, &flow)) in work
        .arcs
        .iter_mut()
        .zip(snapshot.basis_states.iter().zip(&snapshot.perturbed_flows))
    {
        arc.state = state;
        arc.flow = flow;
    }
    work.premultipliers.clone_from(&snapshot.premultipliers);
    work.epsilon.clone_from(&snapshot.epsilon);
    work.phase = snapshot.phase;
    work.n_star.fill(false);
    for &node in &snapshot.n_star {
        if node >= work.extended_node_count() || work.n_star[node] {
            return Err(PolynomialPrimalSimplexError::TraceVerification);
        }
        work.n_star[node] = true;
    }
    work.metrics = snapshot.metrics;
    work.rebuild_tree(snapshot.root)?;
    work.metrics = snapshot.metrics;
    work.validate_basis()?;
    if work.reconstruct_unperturbed_basic_flows()? != snapshot.unperturbed_basic_flows
        || bool_indices(&work.eligible_nodes()?) != snapshot.eligible_nodes
        || bool_indices(&work.awake_nodes()) != snapshot.awake_nodes
    {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    validate_selection_grammar(snapshot, work.arcs.len())?;
    if let Some(flows) = &snapshot.certified_flows {
        if snapshot.stage != PolynomialPrimalSimplexStage::Optimal {
            return Err(PolynomialPrimalSimplexError::TraceVerification);
        }
        check_min_cost_flow(graph, required_divergence, flows)?;
    } else if snapshot.stage == PolynomialPrimalSimplexStage::Optimal {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    Ok(())
}

fn validate_selection_grammar(
    snapshot: &PolynomialPrimalSimplexSnapshot,
    arc_count: usize,
) -> Result<(), PolynomialPrimalSimplexError> {
    for residual in snapshot
        .entering
        .iter()
        .chain(snapshot.inspected_residual.iter())
        .chain(snapshot.cycle.iter())
    {
        if residual.arc_index >= arc_count {
            return Err(PolynomialPrimalSimplexError::TraceVerification);
        }
    }
    if snapshot.leaving_arc.is_some_and(|index| index >= arc_count) {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    if snapshot
        .inspected_arc
        .is_some_and(|index| index >= arc_count)
    {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    let selection_stage = matches!(
        snapshot.stage,
        PolynomialPrimalSimplexStage::SelectAdmissible | PolynomialPrimalSimplexStage::Pivot
    );
    let inspection_stage = snapshot.stage == PolynomialPrimalSimplexStage::InspectResidual;
    if selection_stage != (snapshot.entering.is_some() && !snapshot.cycle.is_empty())
        || inspection_stage != (snapshot.inspected_arc.is_some() && snapshot.scan_kind.is_some())
        || snapshot
            .inspected_residual
            .is_some_and(|residual| Some(residual.arc_index) != snapshot.inspected_arc)
        || (snapshot.stage == PolynomialPrimalSimplexStage::Pivot) != snapshot.delta.is_some()
        || (snapshot.stage == PolynomialPrimalSimplexStage::ModifyPremultipliers)
            != snapshot.potential_shift.is_some()
        || (!selection_stage && snapshot.leaving_arc.is_some())
        || (!inspection_stage
            && (snapshot.inspected_arc.is_some()
                || snapshot.inspected_residual.is_some()
                || snapshot.scan_kind.is_some()))
    {
        return Err(PolynomialPrimalSimplexError::TraceVerification);
    }
    Ok(())
}

fn bool_indices(values: &[bool]) -> Vec<usize> {
    values
        .iter()
        .enumerate()
        .filter_map(|(index, &value)| value.then_some(index))
        .collect()
}

fn rational_i128(value: i128) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

fn rational_from_scaled(value: i128, scale: i128) -> BigRational {
    BigRational::new(BigInt::from(value), BigInt::from(scale))
}

fn on_grid(value: &BigRational, grid: &BigRational) -> bool {
    if grid.is_zero() {
        return false;
    }
    let ratio = value / grid;
    ratio.denom() == &BigInt::from(1)
}

fn distance_to_next_grid(value: &BigRational, grid: &BigRational) -> BigRational {
    let ratio = value / grid;
    let denominator = ratio.denom().clone();
    let mut remainder = ratio.numer() % &denominator;
    if remainder.is_negative() {
        remainder += &denominator;
    }
    let steps = if remainder.is_zero() {
        denominator.clone()
    } else {
        &denominator - remainder
    };
    grid * BigRational::new(steps, denominator)
}

fn checked_sum(values: &[i128]) -> Result<i128, PolynomialPrimalSimplexError> {
    values.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(PolynomialPrimalSimplexError::ArithmeticOverflow)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(nodes: &[&str], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        let flow_nodes = nodes
            .iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node id"), 0))
            .collect::<Vec<_>>();
        let flow_edges = edges
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
            .collect::<Vec<_>>();
        FlowNetwork::new(flow_nodes, flow_edges).expect("graph")
    }

    #[test]
    fn scaling_premultiplier_matches_independent_certificate_and_trace() {
        let network = graph(
            &["s", "a", "t"],
            &[
                ("sa", "s", "a", 0, 4, 3),
                ("at", "a", "t", 0, 4, 2),
                ("st", "s", "t", 0, 4, 9),
                ("as", "a", "s", 0, 2, -1),
            ],
        );
        // `FlowNetwork` canonicalizes node order to a,s,t.
        let target = [0, 3, -3];
        let fast = solve_polynomial_primal_network_simplex(&network, &target).expect("fast");
        let traced = trace_polynomial_primal_network_simplex(&network, &target).expect("trace");
        assert_eq!(fast, traced.result);
        // Edge order is likewise canonicalized to as,at,sa,st.
        assert_eq!(fast.flows, vec![0, 3, 3, 0]);
        assert_eq!(fast.certificate.total_cost, 15);
        assert!(traced.events.iter().any(|event| {
            event.after.stage == PolynomialPrimalSimplexStage::ModifyPremultipliers
        }));
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.after.stage == PolynomialPrimalSimplexStage::Pivot })
        );
        let scan_events = traced
            .events
            .iter()
            .filter(|event| event.after.stage == PolynomialPrimalSimplexStage::InspectResidual)
            .collect::<Vec<_>>();
        let expected_scans = fast.metrics.admissible_arc_scans
            + fast.metrics.optimality_arc_scans
            + fast.metrics.cycle_arc_scans;
        assert_eq!(
            u128::try_from(scan_events.len()).expect("scan event count"),
            expected_scans
        );
        for event in scan_events {
            let before = event.before.metrics.admissible_arc_scans
                + event.before.metrics.optimality_arc_scans
                + event.before.metrics.cycle_arc_scans;
            let after = event.after.metrics.admissible_arc_scans
                + event.after.metrics.optimality_arc_scans
                + event.after.metrics.cycle_arc_scans;
            assert_eq!(after, before + 1);
            assert!(event.after.inspected_arc.is_some());
            assert!(event.after.scan_kind.is_some());
        }
    }

    #[test]
    fn exact_rhs_perturbation_keeps_every_tree_arc_strictly_interior() {
        let network = graph(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 0, 2, 0),
                ("bc", "b", "c", 0, 2, 0),
                ("ca", "c", "a", 0, 2, -1),
            ],
        );
        let trace = trace_polynomial_primal_network_simplex(&network, &[0, 0, 0]).expect("trace");
        for snapshot in std::iter::once(&trace.base_snapshot)
            .chain(trace.events.iter().map(|event| &event.after))
        {
            for ((state, &flow), arc) in snapshot
                .basis_states
                .iter()
                .zip(&snapshot.perturbed_flows)
                .zip(
                    WorkingState::initialize(&network, &[0, 0, 0])
                        .expect("work")
                        .arcs,
                )
            {
                if *state == PolynomialPrimalBasisState::Tree {
                    assert!(flow > 0 && flow < arc.capacity);
                }
            }
        }
    }

    #[test]
    fn lower_bounds_parallel_arcs_and_negative_self_loop_are_exact() {
        let network = graph(
            &["s", "t"],
            &[
                ("fixed", "s", "t", 2, 5, 4),
                ("cheap", "s", "t", 0, 4, -2),
                ("loop", "s", "s", 0, 3, -5),
            ],
        );
        let result = solve_polynomial_primal_network_simplex(&network, &[4, -4])
            .expect("polynomial simplex");
        assert_eq!(result.flows, vec![2, 2, 3]);
        assert_eq!(result.certificate.total_cost, -11);
    }

    #[test]
    fn infeasible_and_admission_fail_closed() {
        let infeasible = graph(&["s", "t"], &[("st", "s", "t", 0, 1, 0)]);
        assert!(matches!(
            solve_polynomial_primal_network_simplex(&infeasible, &[2, -2]),
            Err(PolynomialPrimalSimplexError::Feasibility(_))
        ));
        let names = (0..=POLYNOMIAL_PRIMAL_SIMPLEX_MAX_NODES)
            .map(|index| format!("n{index}"))
            .collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let too_large = graph(&refs, &[]);
        assert_eq!(
            solve_polynomial_primal_network_simplex(&too_large, &vec![0; refs.len()]),
            Err(PolynomialPrimalSimplexError::AdmissionLimit)
        );
    }

    #[test]
    fn corrupted_public_trace_is_rejected() {
        let network = graph(
            &["s", "a", "t"],
            &[
                ("sa", "s", "a", 0, 3, 2),
                ("at", "a", "t", 0, 3, 1),
                ("st", "s", "t", 0, 3, 8),
            ],
        );
        let mut trace =
            trace_polynomial_primal_network_simplex(&network, &[0, 2, -2]).expect("trace");
        let pivot = trace
            .events
            .iter_mut()
            .find(|event| event.after.stage == PolynomialPrimalSimplexStage::Pivot)
            .expect("pivot");
        pivot.after.perturbed_flows[0] += 1;
        assert!(matches!(
            check_polynomial_primal_network_simplex_trace(&network, &[0, 2, -2], &trace),
            Err(PolynomialPrimalSimplexError::BasisInvariant
                | PolynomialPrimalSimplexError::PerturbationInvariant
                | PolynomialPrimalSimplexError::TraceVerification)
        ));
    }

    #[test]
    fn rational_grid_distance_is_exact_for_negative_values() {
        let quarter = BigRational::new(BigInt::from(3), BigInt::from(8));
        let value = BigRational::new(BigInt::from(-7), BigInt::from(16));
        assert_eq!(
            distance_to_next_grid(&value, &quarter),
            BigRational::new(BigInt::from(1), BigInt::from(16))
        );
        assert_eq!(distance_to_next_grid(&rational_i128(0), &quarter), quarter);
    }
}
