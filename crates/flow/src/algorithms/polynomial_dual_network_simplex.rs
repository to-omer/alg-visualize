//! Orlin–Plotkin–Tardos polynomial dual network simplex.
//!
//! This is the `Scaling-Simplex` algorithm from Orlin, Plotkin, and Tardos
//! (1993), Section 3.2 and Figure 3.  It maintains a dual-feasible tree and an
//! auxiliary tree-supported pseudoflow.  Dyadic scaling augmentations are sent
//! from an active node back to the root; `Make-Good` then performs only dual
//! simplex pivots until every zero-flow downward tree arc has disappeared.
//!
//! The source algorithm is for uncapacitated transshipment.  As with the
//! natural dual-network-simplex descriptor, this project accepts a finite
//! encoding only when every shifted capacity is at least the total supply, so
//! no upper bound can bind in an original feasible tree flow.  The auxiliary
//! pseudoflow is intentionally kept separate from the bounded model flow.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Conservative node limit for explicit rooted-tree reconstruction.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_NODES: usize = 64;
/// Conservative edge limit for explicit pricing scans.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_EDGES: usize = 512;
/// Deterministic ceiling on source-defined scaling phases.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_PHASES: u64 = 512;
/// Deterministic ceiling on tree augmentations.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_AUGMENTATIONS: u64 = 100_000;
/// Deterministic ceiling on `Make-Good` dual pivots.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_PIVOTS: u64 = 100_000;
/// Deterministic ceiling on tree, cut, and pricing scans.
pub const POLYNOMIAL_DUAL_SIMPLEX_MAX_ARC_SCANS: u128 = 20_000_000;

/// One directed traversal of an original tree edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolynomialDualResidualRef {
    /// Canonical original-edge index.
    pub edge_index: usize,
    /// `true` traverses the stored source-to-target orientation.
    pub forward: bool,
}

/// Source-defined publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialDualSimplexStage {
    /// Domain checks passed; no tree has been published.
    Ready,
    /// One original arc was inspected while building the initial dual tree.
    InspectInitialArc,
    /// The shortest-path arborescence and dual prices were installed.
    InitializeTree,
    /// The initial root-to-every-node auxiliary pseudoflow was installed.
    InitializePseudoflow,
    /// A new dyadic scaling phase began.
    BeginScale,
    /// One original arc was inspected while constructing a tree path.
    InspectAugmentationArc,
    /// A node with excess strictly greater than `delta` was selected.
    SelectActive,
    /// `delta` auxiliary flow was sent from the active node to the root.
    AugmentToRoot,
    /// `Make-Good` selected the bad-node set and first bad leaving arc.
    SelectBadArc,
    /// One original arc was inspected while pricing the bad-node cut.
    InspectEnteringArc,
    /// The minimum reduced-cost arc leaving the bad-node set was selected.
    SelectEntering,
    /// One source-defined dual-simplex basis exchange was committed.
    PivotMakeGood,
    /// No node had excess above `delta`; the phase completed.
    FinishScale,
    /// The integral basic tree flow was independently certified optimal.
    Optimal,
}

/// Exact counters from the bounded scaling-simplex kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PolynomialDualSimplexMetrics {
    /// Completed dyadic scaling phases.
    pub scaling_phases: u64,
    /// Active-node searches, including terminal searches in each phase.
    pub active_searches: u64,
    /// Tree-supported `delta` augmentations.
    pub augmentations: u64,
    /// Original arcs inspected while constructing the initial dual tree.
    pub initial_arc_scans: u128,
    /// Tree arcs inspected while constructing augmentation paths.
    pub augmentation_arc_scans: u128,
    /// Full rooted-tree reconstructions.
    pub tree_rebuilds: u64,
    /// Searches for zero-flow downward arcs.
    pub bad_arc_searches: u64,
    /// Original arcs inspected while pricing the bad-node cut.
    pub entering_arc_scans: u128,
    /// `Make-Good` dual-simplex pivots.
    pub pivots: u64,
    /// Pivots whose entering reduced cost was zero.
    pub zero_price_pivots: u64,
    /// Nodes whose potential changed across all pivots.
    pub price_updates: u128,
}

/// Complete reversible scaling-simplex state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialDualSimplexSnapshot {
    /// Semantic boundary.
    pub stage: PolynomialDualSimplexStage,
    /// Canonical root node (always node zero in this implementation).
    pub root: usize,
    /// One-based active phase, or zero before scaling starts.
    pub phase: u64,
    /// Numerator of the exact dyadic scaling parameter.
    pub delta_numerator: i128,
    /// Shared positive denominator for auxiliary flows and excesses.
    pub scale_denominator: i128,
    /// Original edges forming the undirected spanning-tree basis.
    pub tree_edges: Vec<EdgeId>,
    /// Auxiliary tree-supported pseudoflow numerators.
    pub pseudoflow_numerators: Vec<i128>,
    /// Integral basic flow induced by the current tree and balances.
    pub basic_flows: Vec<i128>,
    /// Exact auxiliary node-excess numerators.
    pub excess_numerators: Vec<i128>,
    /// Exact dual node prices.
    pub potentials: Vec<i128>,
    /// Exact original-edge reduced costs.
    pub reduced_costs: Vec<i128>,
    /// Selected active node.
    pub active_node: Option<NodeIndex>,
    /// Original edge inspected by the current source primitive.
    pub inspected_edge: Option<EdgeId>,
    /// Directed active-to-root tree path.
    pub augment_path: Vec<PolynomialDualResidualRef>,
    /// Zero-flow downward tree arcs under the current root.
    pub bad_edges: Vec<EdgeId>,
    /// Nodes whose root path contains a bad edge.
    pub bad_nodes: Vec<NodeIndex>,
    /// First bad arc on the root-to-entering-tail path.
    pub leaving_edge: Option<EdgeId>,
    /// Minimum reduced-cost arc leaving the bad-node set.
    pub entering_edge: Option<EdgeId>,
    /// Head-side cut of the selected leaving arc.
    pub pivot_cut: Vec<NodeIndex>,
    /// Reduced cost used for the exact cut-price shift.
    pub pivot_price_delta: Option<i128>,
    /// Independently certified original bounded flow at termination.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact deterministic work counters.
    pub metrics: PolynomialDualSimplexMetrics,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialDualSimplexTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: PolynomialDualSimplexSnapshot,
    /// State after the transition.
    pub after: PolynomialDualSimplexSnapshot,
}

/// Certified polynomial dual-network-simplex result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialDualSimplexResult {
    /// Original bounded flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent exact primal/dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact source work counters.
    pub metrics: PolynomialDualSimplexMetrics,
    /// Terminal source state.
    pub final_snapshot: PolynomialDualSimplexSnapshot,
}

/// Certified result plus every scaling, augmentation, and pivot boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolynomialDualSimplexTraceResult {
    /// Same result returned by the fast profile.
    pub result: PolynomialDualSimplexResult,
    /// Ready boundary.
    pub base_snapshot: PolynomialDualSimplexSnapshot,
    /// Reversible source transitions.
    pub events: Vec<PolynomialDualSimplexTraceEvent>,
    /// Independently certified terminal boundary.
    pub final_snapshot: PolynomialDualSimplexSnapshot,
}

/// Domain, arithmetic, work, basis, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PolynomialDualSimplexError {
    /// Input exceeds the explicit educational admission band.
    #[error("graph exceeds polynomial dual-network-simplex admission limits")]
    AdmissionLimit,
    /// A deterministic phase, augmentation, pivot, or scan ceiling was reached.
    #[error("polynomial dual-network-simplex work limit reached")]
    WorkLimit,
    /// The source algorithm requires a strongly connected positive-width graph.
    #[error("polynomial dual network simplex requires a strongly connected positive-width graph")]
    StrongConnectivity,
    /// Finite project capacities do not encode uncapacitated arcs.
    #[error("polynomial dual network simplex requires nonbinding transshipment capacities")]
    CapacityBound,
    /// A negative forward cycle makes the source transshipment unbounded.
    #[error("polynomial dual-network-simplex transshipment is unbounded")]
    Unbounded,
    /// Requested balances are infeasible in the original bounded model.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic overflowed.
    #[error("polynomial dual-network-simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// Tree, pseudoflow, excess, cut, or dual invariant failed.
    #[error("polynomial dual-network-simplex invariant failed")]
    Invariant,
    /// A public trace did not match the deterministic source grammar.
    #[error("polynomial dual-network-simplex trace verification failed")]
    TraceVerification,
}

/// Solves the nonbinding-capacity encoding using source Figure 3.
///
/// # Errors
///
/// Returns a domain, feasibility, work, arithmetic, invariant, or certificate
/// failure.
pub fn solve_polynomial_dual_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PolynomialDualSimplexResult, PolynomialDualSimplexError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_polynomial_dual_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PolynomialDualSimplexResult, PolynomialDualSimplexError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every source-defined scaling-simplex transition.
///
/// # Errors
///
/// Returns the same failures as [`solve_polynomial_dual_network_simplex`].
pub fn trace_polynomial_dual_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PolynomialDualSimplexTraceResult, PolynomialDualSimplexError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let trace = PolynomialDualSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_polynomial_dual_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Traces polynomial dual network simplex while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_polynomial_dual_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PolynomialDualSimplexTraceResult, PolynomialDualSimplexError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = PolynomialDualSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_polynomial_dual_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Replays and validates every public source boundary.
///
/// # Errors
///
/// Returns [`PolynomialDualSimplexError::TraceVerification`] for any drift.
pub fn check_polynomial_dual_network_simplex_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &PolynomialDualSimplexTraceResult,
) -> Result<(), PolynomialDualSimplexError> {
    let required_variable = validate_source_domain(graph, required_divergence)?;
    validate_snapshot(graph, &required_variable, &trace.base_snapshot)
        .map_err(|_| PolynomialDualSimplexError::TraceVerification)?;
    let mut current = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != current || event.catalog_id != catalog_id(event.after.stage) {
            return Err(PolynomialDualSimplexError::TraceVerification);
        }
        validate_snapshot(graph, &required_variable, &event.after)
            .map_err(|_| PolynomialDualSimplexError::TraceVerification)?;
        current = &event.after;
    }
    if current != &trace.final_snapshot || trace.result.final_snapshot != trace.final_snapshot {
        return Err(PolynomialDualSimplexError::TraceVerification);
    }
    let expected = solve_internal(graph, required_divergence, true)?;
    if expected.base_snapshot != trace.base_snapshot
        || expected.events != trace.events
        || expected.final_snapshot != trace.final_snapshot
        || expected.result != trace.result
    {
        return Err(PolynomialDualSimplexError::TraceVerification);
    }
    Ok(())
}

struct InternalRun {
    result: PolynomialDualSimplexResult,
    base_snapshot: PolynomialDualSimplexSnapshot,
    events: Vec<PolynomialDualSimplexTraceEvent>,
    final_snapshot: PolynomialDualSimplexSnapshot,
}

struct WorkingState<'graph> {
    graph: &'graph FlowNetwork,
    required_variable: Vec<i128>,
    root: usize,
    phase: u64,
    denominator: i128,
    delta: i128,
    tree: Vec<bool>,
    pseudoflow: Vec<i128>,
    basic_flows: Vec<i128>,
    potentials: Vec<i128>,
    metrics: PolynomialDualSimplexMetrics,
}

#[derive(Clone, Copy)]
struct ArcScanCheckpoint {
    edge_index: usize,
    metrics: PolynomialDualSimplexMetrics,
}

const POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_PREFIX: u128 = 512;
const POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_BLOCK: u128 = 256;

const fn should_publish_arc_scan(scan: u128) -> bool {
    scan <= POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_PREFIX
        || (scan - POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_PREFIX)
            .is_multiple_of(POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_BLOCK)
}

fn flush_final_arc_scan_checkpoint(
    checkpoints: &mut Vec<ArcScanCheckpoint>,
    edge_index: Option<usize>,
    metrics: PolynomialDualSimplexMetrics,
) {
    if let Some(edge_index) = edge_index
        && checkpoints
            .last()
            .is_none_or(|checkpoint| checkpoint.metrics != metrics)
    {
        checkpoints.push(ArcScanCheckpoint {
            edge_index,
            metrics,
        });
    }
}

fn total_arc_scans(metrics: PolynomialDualSimplexMetrics) -> Option<u128> {
    metrics
        .initial_arc_scans
        .checked_add(metrics.augmentation_arc_scans)?
        .checked_add(metrics.entering_arc_scans)
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, PolynomialDualSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, PolynomialDualSimplexError> {
    let required_variable =
        validate_source_domain_with_feasibility(graph, required_divergence, feasibility)?;
    let denominator = dyadic_denominator(graph.nodes().len())?;
    let initial_delta = initial_delta(&required_variable)?
        .checked_mul(denominator)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    let mut work = WorkingState {
        graph,
        required_variable,
        root: 0,
        phase: 0,
        denominator,
        delta: initial_delta,
        tree: vec![false; graph.edges().len()],
        pseudoflow: vec![0; graph.edges().len()],
        basic_flows: vec![0; graph.edges().len()],
        potentials: vec![0; graph.nodes().len()],
        metrics: PolynomialDualSimplexMetrics::default(),
    };
    let base_snapshot = snapshot(
        &work,
        PolynomialDualSimplexStage::Ready,
        Selection::default(),
    )?;
    let mut current = base_snapshot.clone();
    let mut events = Vec::new();

    initialize_scaling_simplex(&mut work, &mut current, &mut events, record_trace)?;
    run_scaling_phases(&mut work, &mut current, &mut events, record_trace)?;

    validate_work(&work)?;
    if work.basic_flows.iter().any(|&flow| flow < 0) {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let flows = recover_original_flows(graph, &work.basic_flows)?;
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        snapshot(
            &work,
            PolynomialDualSimplexStage::Optimal,
            Selection {
                certified_flows: Some(flows.clone()),
                ..Selection::default()
            },
        )?,
    );
    let final_snapshot = current;
    let result = PolynomialDualSimplexResult {
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

#[derive(Clone, Debug, Default)]
struct Selection {
    active: Option<usize>,
    augment_path: Vec<PolynomialDualResidualRef>,
    leaving: Option<usize>,
    entering: Option<usize>,
    pivot_cut: Vec<bool>,
    pivot_price_delta: Option<i128>,
    certified_flows: Option<Vec<u64>>,
}

fn initialize_scaling_simplex(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
) -> Result<(), PolynomialDualSimplexError> {
    let (tree, potentials, scans, checkpoints) = initial_dual_tree(work.graph)?;
    publish_arc_scan_checkpoints(
        work,
        current,
        events,
        record_trace,
        PolynomialDualSimplexStage::InspectInitialArc,
        checkpoints,
    )?;
    work.tree = tree;
    work.potentials = potentials;
    work.basic_flows = reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)?;
    work.metrics.initial_arc_scans = scans;
    work.metrics.tree_rebuilds = 1;
    validate_work(work)?;
    publish(
        current,
        events,
        record_trace,
        snapshot(
            work,
            PolynomialDualSimplexStage::InitializeTree,
            Selection::default(),
        )?,
    );

    install_initial_pseudoflow(work, current, events, record_trace)?;
    validate_work(work)?;
    publish(
        current,
        events,
        record_trace,
        snapshot(
            work,
            PolynomialDualSimplexStage::InitializePseudoflow,
            Selection::default(),
        )?,
    );
    Ok(())
}

fn run_scaling_phases(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
) -> Result<(), PolynomialDualSimplexError> {
    while scale_is_above_terminal(work)? {
        if work.phase >= POLYNOMIAL_DUAL_SIMPLEX_MAX_PHASES {
            return Err(PolynomialDualSimplexError::WorkLimit);
        }
        work.phase = work
            .phase
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::BeginScale,
                Selection::default(),
            )?,
        );
        augment_active_nodes(work, current, events, record_trace)?;
        work.metrics.scaling_phases = work
            .metrics
            .scaling_phases
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::FinishScale,
                Selection::default(),
            )?,
        );
        work.delta /= 2;
    }
    Ok(())
}

fn augment_active_nodes(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
) -> Result<(), PolynomialDualSimplexError> {
    loop {
        work.metrics.active_searches = work
            .metrics
            .active_searches
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        let excesses = excess_numerators(work)?;
        let Some(active) = (0..work.graph.nodes().len())
            .find(|&node| node != work.root && excesses[node] > work.delta)
        else {
            return Ok(());
        };
        if work.metrics.augmentations >= POLYNOMIAL_DUAL_SIMPLEX_MAX_AUGMENTATIONS {
            return Err(PolynomialDualSimplexError::WorkLimit);
        }
        let mut checkpoints = Vec::new();
        let path = tree_path(
            work,
            active,
            work.root,
            record_trace.then_some(&mut checkpoints),
        )?;
        publish_arc_scan_checkpoints(
            work,
            current,
            events,
            record_trace,
            PolynomialDualSimplexStage::InspectAugmentationArc,
            checkpoints,
        )?;
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::SelectActive,
                Selection {
                    active: Some(active),
                    augment_path: path.clone(),
                    ..Selection::default()
                },
            )?,
        );
        augment_path(work, &path)?;
        work.metrics.augmentations = work
            .metrics
            .augmentations
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        validate_work(work)?;
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::AugmentToRoot,
                Selection {
                    active: Some(active),
                    augment_path: path,
                    ..Selection::default()
                },
            )?,
        );
        make_good(work, current, events, record_trace)?;
    }
}

fn make_good(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
) -> Result<(), PolynomialDualSimplexError> {
    loop {
        work.metrics.bad_arc_searches = work
            .metrics
            .bad_arc_searches
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        let rooted = rooted_tree(work.graph, &work.tree, work.root)?;
        let bad = bad_state(work.graph, &work.tree, &work.pseudoflow, &rooted)?;
        if bad.edges.is_empty() {
            return Ok(());
        }
        if work.metrics.pivots >= POLYNOMIAL_DUAL_SIMPLEX_MAX_PIVOTS {
            return Err(PolynomialDualSimplexError::WorkLimit);
        }
        let reduced = reduced_costs(work.graph, &work.potentials)?;
        let mut checkpoints = Vec::new();
        let (entering, price_delta) = select_entering_bad_cut(
            work,
            &bad.nodes,
            &reduced,
            record_trace.then_some(&mut checkpoints),
        )?;
        publish_arc_scan_checkpoints(
            work,
            current,
            events,
            record_trace,
            PolynomialDualSimplexStage::InspectEnteringArc,
            checkpoints,
        )?;
        let tail = work.graph.edges()[entering].from().as_usize();
        let leaving = first_bad_on_root_path(&rooted, &bad.edge_mask, tail)?;
        let cut = head_side_cut(work.graph, &work.tree, leaving)?;
        if cut != bad.nodes {
            return Err(PolynomialDualSimplexError::Invariant);
        }
        let selection = Selection {
            leaving: Some(leaving),
            entering: None,
            pivot_cut: cut.clone(),
            ..Selection::default()
        };
        publish(
            current,
            events,
            record_trace,
            snapshot(work, PolynomialDualSimplexStage::SelectBadArc, selection)?,
        );
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::SelectEntering,
                Selection {
                    leaving: Some(leaving),
                    entering: Some(entering),
                    pivot_cut: cut.clone(),
                    pivot_price_delta: Some(price_delta),
                    ..Selection::default()
                },
            )?,
        );

        commit_make_good_pivot(work, leaving, entering, &cut, price_delta)?;
        publish(
            current,
            events,
            record_trace,
            snapshot(
                work,
                PolynomialDualSimplexStage::PivotMakeGood,
                Selection {
                    leaving: Some(leaving),
                    entering: Some(entering),
                    pivot_cut: cut,
                    pivot_price_delta: Some(price_delta),
                    ..Selection::default()
                },
            )?,
        );
    }
}

fn commit_make_good_pivot(
    work: &mut WorkingState<'_>,
    leaving: usize,
    entering: usize,
    cut: &[bool],
    price_delta: i128,
) -> Result<(), PolynomialDualSimplexError> {
    if work.pseudoflow[leaving] != 0 || work.pseudoflow[entering] != 0 {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    work.tree[leaving] = false;
    work.tree[entering] = true;
    for (node, &inside) in cut.iter().enumerate() {
        if inside {
            work.potentials[node] = work.potentials[node]
                .checked_sub(price_delta)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            work.metrics.price_updates = work
                .metrics
                .price_updates
                .checked_add(1)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        }
    }
    work.basic_flows = reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)?;
    work.metrics.tree_rebuilds = work
        .metrics
        .tree_rebuilds
        .checked_add(1)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    work.metrics.pivots = work
        .metrics
        .pivots
        .checked_add(1)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    if price_delta == 0 {
        work.metrics.zero_price_pivots = work
            .metrics
            .zero_price_pivots
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    }
    validate_work(work)
}

fn install_initial_pseudoflow(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
) -> Result<(), PolynomialDualSimplexError> {
    let mut paths = Vec::with_capacity(work.graph.nodes().len().saturating_sub(1));
    for node in 0..work.graph.nodes().len() {
        if node == work.root {
            continue;
        }
        let mut checkpoints = Vec::new();
        let path = tree_path(
            work,
            work.root,
            node,
            record_trace.then_some(&mut checkpoints),
        )?;
        publish_arc_scan_checkpoints(
            work,
            current,
            events,
            record_trace,
            PolynomialDualSimplexStage::InspectAugmentationArc,
            checkpoints,
        )?;
        paths.push(path);
    }
    for path in paths {
        for reference in path {
            if !reference.forward {
                return Err(PolynomialDualSimplexError::Invariant);
            }
            work.pseudoflow[reference.edge_index] = work.pseudoflow[reference.edge_index]
                .checked_add(work.delta)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn augment_path(
    work: &mut WorkingState<'_>,
    path: &[PolynomialDualResidualRef],
) -> Result<(), PolynomialDualSimplexError> {
    for reference in path {
        let value = &mut work.pseudoflow[reference.edge_index];
        if reference.forward {
            *value = value
                .checked_add(work.delta)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        } else {
            if *value < work.delta {
                return Err(PolynomialDualSimplexError::Invariant);
            }
            *value = value
                .checked_sub(work.delta)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn tree_path(
    work: &mut WorkingState<'_>,
    start: usize,
    target: usize,
    mut checkpoints: Option<&mut Vec<ArcScanCheckpoint>>,
) -> Result<Vec<PolynomialDualResidualRef>, PolynomialDualSimplexError> {
    let node_count = work.graph.nodes().len();
    let mut predecessor: Vec<Option<(usize, bool, usize)>> = vec![None; node_count];
    let mut queue = VecDeque::from([start]);
    let mut last_inspected_edge = None;
    predecessor[start] = Some((usize::MAX, true, start));
    while let Some(node) = queue.pop_front() {
        if node == target {
            break;
        }
        for (index, edge) in work.graph.edges().iter().enumerate() {
            last_inspected_edge = Some(index);
            work.metrics.augmentation_arc_scans = work
                .metrics
                .augmentation_arc_scans
                .checked_add(1)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            enforce_scan_limit(work)?;
            if should_publish_arc_scan(work.metrics.augmentation_arc_scans)
                && let Some(checkpoints) = checkpoints.as_deref_mut()
            {
                checkpoints.push(ArcScanCheckpoint {
                    edge_index: index,
                    metrics: work.metrics,
                });
            }
            if !work.tree[index] {
                continue;
            }
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            let candidate = if from == node {
                Some((to, true))
            } else if to == node {
                Some((from, false))
            } else {
                None
            };
            if let Some((next, forward)) =
                candidate.filter(|(next, _)| predecessor[*next].is_none())
            {
                predecessor[next] = Some((index, forward, node));
                queue.push_back(next);
            }
        }
    }
    if let Some(checkpoints) = checkpoints {
        flush_final_arc_scan_checkpoint(checkpoints, last_inspected_edge, work.metrics);
    }
    if predecessor[target].is_none() {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let mut path = Vec::new();
    let mut node = target;
    while node != start {
        let (edge_index, forward, previous) =
            predecessor[node].ok_or(PolynomialDualSimplexError::Invariant)?;
        path.push(PolynomialDualResidualRef {
            edge_index,
            forward,
        });
        node = previous;
    }
    path.reverse();
    Ok(path)
}

#[derive(Clone)]
struct RootedTree {
    parent: Vec<Option<usize>>,
    parent_edge: Vec<Option<usize>>,
}

fn rooted_tree(
    graph: &FlowNetwork,
    tree: &[bool],
    root: usize,
) -> Result<RootedTree, PolynomialDualSimplexError> {
    validate_tree(graph, tree)?;
    let mut parent = vec![None; graph.nodes().len()];
    let mut parent_edge = vec![None; graph.nodes().len()];
    let mut queue = VecDeque::from([root]);
    parent[root] = Some(root);
    while let Some(node) = queue.pop_front() {
        for (index, edge) in graph.edges().iter().enumerate() {
            if !tree[index] {
                continue;
            }
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            let next = if from == node {
                Some(to)
            } else if to == node {
                Some(from)
            } else {
                None
            };
            if let Some(next) = next.filter(|&next| parent[next].is_none()) {
                parent[next] = Some(node);
                parent_edge[next] = Some(index);
                queue.push_back(next);
            }
        }
    }
    if parent.iter().any(Option::is_none) {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(RootedTree {
        parent,
        parent_edge,
    })
}

struct BadState {
    edges: Vec<usize>,
    edge_mask: Vec<bool>,
    nodes: Vec<bool>,
}

fn bad_state(
    graph: &FlowNetwork,
    tree: &[bool],
    pseudoflow: &[i128],
    rooted: &RootedTree,
) -> Result<BadState, PolynomialDualSimplexError> {
    let mut edge_mask = vec![false; graph.edges().len()];
    let mut edges = Vec::new();
    for node in 0..graph.nodes().len() {
        let Some(edge_index) = rooted.parent_edge[node] else {
            continue;
        };
        let parent = rooted.parent[node].ok_or(PolynomialDualSimplexError::Invariant)?;
        let edge = &graph.edges()[edge_index];
        let downward = edge.from().as_usize() == parent && edge.to().as_usize() == node;
        if downward && pseudoflow[edge_index] == 0 {
            edge_mask[edge_index] = true;
            edges.push(edge_index);
        }
    }
    let mut nodes = vec![false; graph.nodes().len()];
    for node in 0..graph.nodes().len() {
        let mut cursor = node;
        while rooted.parent[cursor] != Some(cursor) {
            let edge = rooted.parent_edge[cursor].ok_or(PolynomialDualSimplexError::Invariant)?;
            if edge_mask[edge] {
                nodes[node] = true;
                break;
            }
            cursor = rooted.parent[cursor].ok_or(PolynomialDualSimplexError::Invariant)?;
        }
    }
    if edges.iter().any(|&index| !tree[index]) {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(BadState {
        edges,
        edge_mask,
        nodes,
    })
}

fn first_bad_on_root_path(
    rooted: &RootedTree,
    bad_edges: &[bool],
    node: usize,
) -> Result<usize, PolynomialDualSimplexError> {
    let mut reversed = Vec::new();
    let mut cursor = node;
    while rooted.parent[cursor] != Some(cursor) {
        let edge = rooted.parent_edge[cursor].ok_or(PolynomialDualSimplexError::Invariant)?;
        reversed.push(edge);
        cursor = rooted.parent[cursor].ok_or(PolynomialDualSimplexError::Invariant)?;
    }
    reversed
        .into_iter()
        .rev()
        .find(|&edge| bad_edges[edge])
        .ok_or(PolynomialDualSimplexError::Invariant)
}

fn select_entering_bad_cut(
    work: &mut WorkingState<'_>,
    bad_nodes: &[bool],
    reduced_costs: &[i128],
    mut checkpoints: Option<&mut Vec<ArcScanCheckpoint>>,
) -> Result<(usize, i128), PolynomialDualSimplexError> {
    let mut best = None;
    let mut last_inspected_edge = None;
    for (index, edge) in work.graph.edges().iter().enumerate() {
        last_inspected_edge = Some(index);
        work.metrics.entering_arc_scans = work
            .metrics
            .entering_arc_scans
            .checked_add(1)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        enforce_scan_limit(work)?;
        if should_publish_arc_scan(work.metrics.entering_arc_scans)
            && let Some(checkpoints) = checkpoints.as_deref_mut()
        {
            checkpoints.push(ArcScanCheckpoint {
                edge_index: index,
                metrics: work.metrics,
            });
        }
        if bad_nodes[edge.from().as_usize()] && !bad_nodes[edge.to().as_usize()] {
            let candidate = (reduced_costs[index], index);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
    }
    if let Some(checkpoints) = checkpoints {
        flush_final_arc_scan_checkpoint(checkpoints, last_inspected_edge, work.metrics);
    }
    let (delta, index) = best.ok_or(PolynomialDualSimplexError::StrongConnectivity)?;
    if delta < 0 {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok((index, delta))
}

fn validate_source_domain(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    validate_source_domain_with_feasibility(graph, required_divergence, &mut feasibility)
}

fn validate_source_domain_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > POLYNOMIAL_DUAL_SIMPLEX_MAX_NODES
        || graph.edges().len() > POLYNOMIAL_DUAL_SIMPLEX_MAX_EDGES
    {
        return Err(PolynomialDualSimplexError::AdmissionLimit);
    }
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    if lower_divergence.len() != required_divergence.len() {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let required_variable = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, actual)| {
            required
                .checked_sub(actual)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if required_variable.iter().try_fold(0_i128, |sum, &value| {
        sum.checked_add(value)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
    })? != 0
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let total_supply = required_variable.iter().try_fold(0_u128, |total, &value| {
        if value > 0 {
            total
                .checked_add(value.unsigned_abs())
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
        } else {
            Ok(total)
        }
    })?;
    for edge in graph.edges() {
        if u128::from(edge.capacity() - edge.lower()) < total_supply {
            return Err(PolynomialDualSimplexError::CapacityBound);
        }
    }
    if !is_strongly_connected(graph) {
        return Err(PolynomialDualSimplexError::StrongConnectivity);
    }
    if has_negative_forward_cycle(graph)? {
        return Err(PolynomialDualSimplexError::Unbounded);
    }
    Ok(required_variable)
}

fn initial_delta(required: &[i128]) -> Result<i128, PolynomialDualSimplexError> {
    let maximum = required.iter().try_fold(0_u128, |current, value| {
        Ok::<_, PolynomialDualSimplexError>(current.max(value.unsigned_abs()))
    })?;
    let target = maximum
        .checked_add(1)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    let delta = target
        .checked_next_power_of_two()
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    i128::try_from(delta).map_err(|_| PolynomialDualSimplexError::ArithmeticOverflow)
}

fn dyadic_denominator(node_count: usize) -> Result<i128, PolynomialDualSimplexError> {
    let target = node_count
        .checked_mul(2)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    let denominator = target
        .checked_next_power_of_two()
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    i128::try_from(denominator).map_err(|_| PolynomialDualSimplexError::ArithmeticOverflow)
}

fn scale_is_above_terminal(work: &WorkingState<'_>) -> Result<bool, PolynomialDualSimplexError> {
    let factor = i128::try_from(
        work.graph
            .nodes()
            .len()
            .checked_mul(2)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?,
    )
    .map_err(|_| PolynomialDualSimplexError::ArithmeticOverflow)?;
    Ok(work
        .delta
        .checked_mul(factor)
        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?
        > work.denominator)
}

type InitialDualTree = (Vec<bool>, Vec<i128>, u128, Vec<ArcScanCheckpoint>);

fn initial_dual_tree(graph: &FlowNetwork) -> Result<InitialDualTree, PolynomialDualSimplexError> {
    let node_count = graph.nodes().len();
    let mut distances = vec![None; node_count];
    let mut predecessor = vec![None; node_count];
    distances[0] = Some(0_i128);
    let mut scans = 0_u128;
    let mut checkpoints = Vec::new();
    let mut last_inspected_edge = None;
    for _ in 1..node_count {
        let mut changed = false;
        for (index, edge) in graph.edges().iter().enumerate() {
            last_inspected_edge = Some(index);
            scans = scans
                .checked_add(1)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            if scans > POLYNOMIAL_DUAL_SIMPLEX_MAX_ARC_SCANS {
                return Err(PolynomialDualSimplexError::WorkLimit);
            }
            if should_publish_arc_scan(scans) {
                checkpoints.push(ArcScanCheckpoint {
                    edge_index: index,
                    metrics: PolynomialDualSimplexMetrics {
                        initial_arc_scans: scans,
                        ..PolynomialDualSimplexMetrics::default()
                    },
                });
            }
            if edge.capacity() == edge.lower() {
                continue;
            }
            let Some(from_distance) = distances[edge.from().as_usize()] else {
                continue;
            };
            let candidate = from_distance
                .checked_add(i128::from(edge.cost()))
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            let to = edge.to().as_usize();
            if distances[to].is_none_or(|current| candidate < current) {
                distances[to] = Some(candidate);
                predecessor[to] = Some(index);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    flush_final_arc_scan_checkpoint(
        &mut checkpoints,
        last_inspected_edge,
        PolynomialDualSimplexMetrics {
            initial_arc_scans: scans,
            ..PolynomialDualSimplexMetrics::default()
        },
    );
    let potentials = distances
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(PolynomialDualSimplexError::StrongConnectivity)?;
    let mut tree = vec![false; graph.edges().len()];
    for edge in predecessor.into_iter().skip(1) {
        tree[edge.ok_or(PolynomialDualSimplexError::Invariant)?] = true;
    }
    validate_tree(graph, &tree)?;
    let reduced = reduced_costs(graph, &potentials)?;
    if reduced.iter().any(|&cost| cost < 0)
        || reduced
            .iter()
            .zip(&tree)
            .any(|(&cost, &in_tree)| in_tree && cost != 0)
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let rooted = rooted_tree(graph, &tree, 0)?;
    for node in 1..node_count {
        let edge_index = rooted.parent_edge[node].ok_or(PolynomialDualSimplexError::Invariant)?;
        let edge = &graph.edges()[edge_index];
        if edge.from().as_usize() != rooted.parent[node].unwrap_or(usize::MAX)
            || edge.to().as_usize() != node
        {
            return Err(PolynomialDualSimplexError::Invariant);
        }
    }
    Ok((tree, potentials, scans, checkpoints))
}

fn reconstruct_basic_flows(
    graph: &FlowNetwork,
    required: &[i128],
    tree: &[bool],
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    validate_tree(graph, tree)?;
    let mut flows = vec![0_i128; graph.edges().len()];
    for (index, edge) in graph.edges().iter().enumerate() {
        if !tree[index] {
            continue;
        }
        let cut = tree_side(graph, tree, index, edge.to().as_usize())?;
        let balance = required
            .iter()
            .zip(cut)
            .try_fold(0_i128, |sum, (&value, inside)| {
                if inside {
                    sum.checked_add(value)
                        .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
                } else {
                    Ok(sum)
                }
            })?;
        flows[index] = balance
            .checked_neg()
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    }
    if basic_divergences(graph, &flows)? != required {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(flows)
}

fn validate_work(work: &WorkingState<'_>) -> Result<(), PolynomialDualSimplexError> {
    validate_tree(work.graph, &work.tree)?;
    if reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)? != work.basic_flows
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let reduced = reduced_costs(work.graph, &work.potentials)?;
    if reduced.iter().any(|&cost| cost < 0)
        || reduced
            .iter()
            .zip(&work.tree)
            .any(|(&cost, &in_tree)| in_tree && cost != 0)
        || work.pseudoflow.len() != work.graph.edges().len()
        || work
            .pseudoflow
            .iter()
            .zip(&work.tree)
            .any(|(&flow, &in_tree)| flow < 0 || !in_tree && flow != 0)
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let excesses = excess_numerators(work)?;
    if excesses.iter().try_fold(0_i128, |sum, &value| {
        sum.checked_add(value)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
    })? != 0
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    if work.phase > 0
        && work.delta > 0
        && work
            .pseudoflow
            .iter()
            .any(|flow| flow.rem_euclid(work.delta) != 0)
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(())
}

fn validate_snapshot(
    graph: &FlowNetwork,
    required: &[i128],
    snapshot: &PolynomialDualSimplexSnapshot,
) -> Result<(), PolynomialDualSimplexError> {
    if snapshot.root != 0
        || snapshot.scale_denominator <= 0
        || snapshot.delta_numerator < 0
        || snapshot.pseudoflow_numerators.len() != graph.edges().len()
        || snapshot.basic_flows.len() != graph.edges().len()
        || snapshot.excess_numerators.len() != graph.nodes().len()
        || snapshot.potentials.len() != graph.nodes().len()
        || snapshot.reduced_costs.len() != graph.edges().len()
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    if matches!(
        snapshot.stage,
        PolynomialDualSimplexStage::Ready | PolynomialDualSimplexStage::InspectInitialArc
    ) {
        if !snapshot.tree_edges.is_empty()
            || snapshot.pseudoflow_numerators.iter().any(|&flow| flow != 0)
            || (snapshot.stage == PolynomialDualSimplexStage::InspectInitialArc)
                != snapshot.inspected_edge.is_some()
            || snapshot
                .inspected_edge
                .as_ref()
                .is_some_and(|id| graph.edges().iter().all(|edge| edge.id() != id))
            || snapshot.active_node.is_some()
            || !snapshot.augment_path.is_empty()
            || snapshot.leaving_edge.is_some()
            || snapshot.entering_edge.is_some()
            || !snapshot.pivot_cut.is_empty()
            || snapshot.pivot_price_delta.is_some()
            || snapshot.certified_flows.is_some()
        {
            return Err(PolynomialDualSimplexError::Invariant);
        }
        return Ok(());
    }
    let tree = edge_mask(graph, &snapshot.tree_edges)?;
    validate_tree(graph, &tree)?;
    if reconstruct_basic_flows(graph, required, &tree)? != snapshot.basic_flows
        || reduced_costs(graph, &snapshot.potentials)? != snapshot.reduced_costs
        || snapshot.reduced_costs.iter().any(|&cost| cost < 0)
        || snapshot
            .reduced_costs
            .iter()
            .zip(&tree)
            .any(|(&cost, &in_tree)| in_tree && cost != 0)
        || snapshot
            .pseudoflow_numerators
            .iter()
            .zip(&tree)
            .any(|(&flow, &in_tree)| flow < 0 || !in_tree && flow != 0)
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let expected_excess = excess_from_parts(
        graph,
        required,
        &snapshot.pseudoflow_numerators,
        snapshot.scale_denominator,
    )?;
    if expected_excess != snapshot.excess_numerators {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let rooted = rooted_tree(graph, &tree, snapshot.root)?;
    let bad = bad_state(graph, &tree, &snapshot.pseudoflow_numerators, &rooted)?;
    let expected_bad_edges = bad
        .edges
        .iter()
        .map(|&index| graph.edges()[index].id().clone())
        .collect::<Vec<_>>();
    let expected_bad_nodes = bool_nodes(&bad.nodes)?;
    if snapshot.bad_edges != expected_bad_edges || snapshot.bad_nodes != expected_bad_nodes {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    validate_snapshot_selection(graph, &tree, snapshot)?;
    if snapshot.stage == PolynomialDualSimplexStage::Optimal {
        if snapshot.certified_flows.is_none() || snapshot.basic_flows.iter().any(|&flow| flow < 0) {
            return Err(PolynomialDualSimplexError::Invariant);
        }
    } else if snapshot.certified_flows.is_some() {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(())
}

fn validate_snapshot_selection(
    graph: &FlowNetwork,
    tree: &[bool],
    snapshot: &PolynomialDualSimplexSnapshot,
) -> Result<(), PolynomialDualSimplexError> {
    let inspects_arc = matches!(
        snapshot.stage,
        PolynomialDualSimplexStage::InspectAugmentationArc
            | PolynomialDualSimplexStage::InspectEnteringArc
    );
    if inspects_arc != snapshot.inspected_edge.is_some()
        || snapshot
            .inspected_edge
            .as_ref()
            .is_some_and(|id| graph.edges().iter().all(|edge| edge.id() != id))
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let stage_has_active = matches!(
        snapshot.stage,
        PolynomialDualSimplexStage::SelectActive | PolynomialDualSimplexStage::AugmentToRoot
    );
    let has_active = snapshot.active_node.is_some();
    if (!inspects_arc && stage_has_active != has_active)
        || has_active == snapshot.augment_path.is_empty()
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let mut cursor = snapshot
        .active_node
        .map_or(snapshot.root, NodeIndex::as_usize);
    let mut seen = vec![false; graph.edges().len()];
    for reference in &snapshot.augment_path {
        let Some(edge) = graph.edges().get(reference.edge_index) else {
            return Err(PolynomialDualSimplexError::Invariant);
        };
        if !tree.get(reference.edge_index).copied().unwrap_or(false) || seen[reference.edge_index] {
            return Err(PolynomialDualSimplexError::Invariant);
        }
        seen[reference.edge_index] = true;
        cursor = if reference.forward && edge.from().as_usize() == cursor {
            edge.to().as_usize()
        } else if !reference.forward && edge.to().as_usize() == cursor {
            edge.from().as_usize()
        } else {
            return Err(PolynomialDualSimplexError::Invariant);
        };
    }
    if has_active && cursor != snapshot.root {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let stage_has_leaving = matches!(
        snapshot.stage,
        PolynomialDualSimplexStage::SelectBadArc
            | PolynomialDualSimplexStage::SelectEntering
            | PolynomialDualSimplexStage::PivotMakeGood
    );
    let stage_has_entering = matches!(
        snapshot.stage,
        PolynomialDualSimplexStage::SelectEntering | PolynomialDualSimplexStage::PivotMakeGood
    );
    let has_leaving = snapshot.leaving_edge.is_some();
    let has_entering = snapshot.entering_edge.is_some();
    let selection_mismatch =
        !inspects_arc && (stage_has_leaving != has_leaving || stage_has_entering != has_entering);
    if selection_mismatch
        || has_entering && !has_leaving
        || has_entering != snapshot.pivot_price_delta.is_some()
        || has_leaving == snapshot.pivot_cut.is_empty()
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    if let Some(delta) = snapshot.pivot_price_delta
        && delta < 0
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    Ok(())
}

fn snapshot(
    work: &WorkingState<'_>,
    stage: PolynomialDualSimplexStage,
    selection: Selection,
) -> Result<PolynomialDualSimplexSnapshot, PolynomialDualSimplexError> {
    let rooted = if work.tree.iter().any(|value| *value) {
        Some(rooted_tree(work.graph, &work.tree, work.root)?)
    } else {
        None
    };
    let bad = rooted
        .as_ref()
        .map(|rooted| bad_state(work.graph, &work.tree, &work.pseudoflow, rooted))
        .transpose()?;
    Ok(PolynomialDualSimplexSnapshot {
        stage,
        root: work.root,
        phase: work.phase,
        delta_numerator: work.delta,
        scale_denominator: work.denominator,
        tree_edges: work
            .graph
            .edges()
            .iter()
            .zip(&work.tree)
            .filter(|&(_, &in_tree)| in_tree)
            .map(|(edge, _)| edge.id().clone())
            .collect(),
        pseudoflow_numerators: work.pseudoflow.clone(),
        basic_flows: work.basic_flows.clone(),
        excess_numerators: excess_numerators(work)?,
        potentials: work.potentials.clone(),
        reduced_costs: reduced_costs(work.graph, &work.potentials)?,
        active_node: selection
            .active
            .map(|node| {
                NodeIndex::try_from_usize(node).ok_or(PolynomialDualSimplexError::Invariant)
            })
            .transpose()?,
        inspected_edge: None,
        augment_path: selection.augment_path,
        bad_edges: bad
            .as_ref()
            .map(|state| {
                state
                    .edges
                    .iter()
                    .map(|&index| work.graph.edges()[index].id().clone())
                    .collect()
            })
            .unwrap_or_default(),
        bad_nodes: bad
            .as_ref()
            .map(|state| bool_nodes(&state.nodes))
            .transpose()?
            .unwrap_or_default(),
        leaving_edge: selection
            .leaving
            .map(|index| work.graph.edges()[index].id().clone()),
        entering_edge: selection
            .entering
            .map(|index| work.graph.edges()[index].id().clone()),
        pivot_cut: bool_nodes(&selection.pivot_cut)?,
        pivot_price_delta: selection.pivot_price_delta,
        certified_flows: selection.certified_flows,
        metrics: work.metrics,
    })
}

fn excess_numerators(work: &WorkingState<'_>) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    excess_from_parts(
        work.graph,
        &work.required_variable,
        &work.pseudoflow,
        work.denominator,
    )
}

fn excess_from_parts(
    graph: &FlowNetwork,
    required: &[i128],
    pseudoflow: &[i128],
    denominator: i128,
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    let divergence = basic_divergences(graph, pseudoflow)?;
    required
        .iter()
        .zip(divergence)
        .map(|(&required, actual)| {
            required
                .checked_mul(denominator)
                .and_then(|value| value.checked_sub(actual))
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
        })
        .collect()
}

fn validate_tree(graph: &FlowNetwork, tree: &[bool]) -> Result<(), PolynomialDualSimplexError> {
    let node_count = graph.nodes().len();
    if tree.len() != graph.edges().len()
        || tree.iter().filter(|value| **value).count() != node_count.saturating_sub(1)
        || graph
            .edges()
            .iter()
            .enumerate()
            .any(|(index, edge)| tree[index] && edge.from() == edge.to())
    {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    if node_count == 1 {
        return Ok(());
    }
    let mut seen = vec![false; node_count];
    let mut queue = VecDeque::from([0_usize]);
    seen[0] = true;
    while let Some(node) = queue.pop_front() {
        for (index, edge) in graph.edges().iter().enumerate() {
            if !tree[index] {
                continue;
            }
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            let next = if from == node {
                Some(to)
            } else if to == node {
                Some(from)
            } else {
                None
            };
            if let Some(next) = next.filter(|&next| !seen[next]) {
                seen[next] = true;
                queue.push_back(next);
            }
        }
    }
    if seen.into_iter().all(|value| value) {
        Ok(())
    } else {
        Err(PolynomialDualSimplexError::Invariant)
    }
}

fn tree_side(
    graph: &FlowNetwork,
    tree: &[bool],
    removed_edge: usize,
    start: usize,
) -> Result<Vec<bool>, PolynomialDualSimplexError> {
    let mut side = vec![false; graph.nodes().len()];
    let mut queue = VecDeque::from([start]);
    side[start] = true;
    while let Some(node) = queue.pop_front() {
        for (index, edge) in graph.edges().iter().enumerate() {
            if index == removed_edge || !tree[index] {
                continue;
            }
            let from = edge.from().as_usize();
            let to = edge.to().as_usize();
            let next = if from == node {
                Some(to)
            } else if to == node {
                Some(from)
            } else {
                None
            };
            if let Some(next) = next.filter(|&next| !side[next]) {
                side[next] = true;
                queue.push_back(next);
            }
        }
    }
    if side.iter().all(|value| *value) {
        Err(PolynomialDualSimplexError::Invariant)
    } else {
        Ok(side)
    }
}

fn head_side_cut(
    graph: &FlowNetwork,
    tree: &[bool],
    leaving: usize,
) -> Result<Vec<bool>, PolynomialDualSimplexError> {
    let edge = graph
        .edges()
        .get(leaving)
        .ok_or(PolynomialDualSimplexError::Invariant)?;
    if !tree.get(leaving).copied().unwrap_or(false) {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    tree_side(graph, tree, leaving, edge.to().as_usize())
}

fn reduced_costs(
    graph: &FlowNetwork,
    potentials: &[i128],
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    if potentials.len() != graph.nodes().len() {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    graph
        .edges()
        .iter()
        .map(|edge| {
            i128::from(edge.cost())
                .checked_add(potentials[edge.from().as_usize()])
                .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)
        })
        .collect()
}

fn basic_divergences(
    graph: &FlowNetwork,
    flows: &[i128],
) -> Result<Vec<i128>, PolynomialDualSimplexError> {
    if flows.len() != graph.edges().len() {
        return Err(PolynomialDualSimplexError::Invariant);
    }
    let mut values = vec![0_i128; graph.nodes().len()];
    for (edge, &flow) in graph.edges().iter().zip(flows) {
        values[edge.from().as_usize()] = values[edge.from().as_usize()]
            .checked_add(flow)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
        values[edge.to().as_usize()] = values[edge.to().as_usize()]
            .checked_sub(flow)
            .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    }
    Ok(values)
}

fn recover_original_flows(
    graph: &FlowNetwork,
    basic_flows: &[i128],
) -> Result<Vec<u64>, PolynomialDualSimplexError> {
    graph
        .edges()
        .iter()
        .zip(basic_flows)
        .map(|(edge, &basic)| {
            let variable =
                u64::try_from(basic).map_err(|_| PolynomialDualSimplexError::Invariant)?;
            let flow = edge
                .lower()
                .checked_add(variable)
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            if flow > edge.capacity() {
                Err(PolynomialDualSimplexError::CapacityBound)
            } else {
                Ok(flow)
            }
        })
        .collect()
}

fn edge_mask(graph: &FlowNetwork, ids: &[EdgeId]) -> Result<Vec<bool>, PolynomialDualSimplexError> {
    let mut mask = vec![false; graph.edges().len()];
    for id in ids {
        let index = graph
            .edges()
            .iter()
            .position(|edge| edge.id() == id)
            .ok_or(PolynomialDualSimplexError::Invariant)?;
        if std::mem::replace(&mut mask[index], true) {
            return Err(PolynomialDualSimplexError::Invariant);
        }
    }
    Ok(mask)
}

fn bool_nodes(values: &[bool]) -> Result<Vec<NodeIndex>, PolynomialDualSimplexError> {
    values
        .iter()
        .enumerate()
        .filter(|&(_, &inside)| inside)
        .map(|(node, _)| {
            NodeIndex::try_from_usize(node).ok_or(PolynomialDualSimplexError::Invariant)
        })
        .collect()
}

fn is_strongly_connected(graph: &FlowNetwork) -> bool {
    if graph.nodes().len() <= 1 {
        return true;
    }
    reachable_count(graph, false) == graph.nodes().len()
        && reachable_count(graph, true) == graph.nodes().len()
}

fn reachable_count(graph: &FlowNetwork, reverse: bool) -> usize {
    let mut seen = vec![false; graph.nodes().len()];
    let mut queue = VecDeque::from([0_usize]);
    seen[0] = true;
    while let Some(node) = queue.pop_front() {
        for edge in graph.edges() {
            if edge.capacity() == edge.lower() {
                continue;
            }
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
    seen.into_iter().filter(|value| *value).count()
}

fn has_negative_forward_cycle(graph: &FlowNetwork) -> Result<bool, PolynomialDualSimplexError> {
    let mut distance = vec![0_i128; graph.nodes().len()];
    for round in 0..graph.nodes().len() {
        let mut changed = false;
        for edge in graph.edges() {
            if edge.capacity() == edge.lower() {
                continue;
            }
            let candidate = distance[edge.from().as_usize()]
                .checked_add(i128::from(edge.cost()))
                .ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
            if candidate < distance[edge.to().as_usize()] {
                distance[edge.to().as_usize()] = candidate;
                changed = true;
                if round + 1 == graph.nodes().len() {
                    return Ok(true);
                }
            }
        }
        if !changed {
            return Ok(false);
        }
    }
    Ok(false)
}

fn enforce_scan_limit(work: &WorkingState<'_>) -> Result<(), PolynomialDualSimplexError> {
    let total =
        total_arc_scans(work.metrics).ok_or(PolynomialDualSimplexError::ArithmeticOverflow)?;
    if total > POLYNOMIAL_DUAL_SIMPLEX_MAX_ARC_SCANS {
        Err(PolynomialDualSimplexError::WorkLimit)
    } else {
        Ok(())
    }
}

fn publish(
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
    after: PolynomialDualSimplexSnapshot,
) {
    if record_trace {
        events.push(PolynomialDualSimplexTraceEvent {
            catalog_id: catalog_id(after.stage),
            before: current.clone(),
            after: after.clone(),
        });
    }
    *current = after;
}

fn publish_arc_scan_checkpoints(
    work: &mut WorkingState<'_>,
    current: &mut PolynomialDualSimplexSnapshot,
    events: &mut Vec<PolynomialDualSimplexTraceEvent>,
    record_trace: bool,
    stage: PolynomialDualSimplexStage,
    checkpoints: Vec<ArcScanCheckpoint>,
) -> Result<(), PolynomialDualSimplexError> {
    if !record_trace || checkpoints.is_empty() {
        return Ok(());
    }
    let final_metrics = work.metrics;
    let result: Result<(), PolynomialDualSimplexError> =
        checkpoints.into_iter().try_for_each(|checkpoint| {
            work.metrics = checkpoint.metrics;
            let mut after = current.clone();
            after.stage = stage;
            after.inspected_edge = Some(work.graph.edges()[checkpoint.edge_index].id().clone());
            after.metrics = checkpoint.metrics;
            validate_snapshot(work.graph, &work.required_variable, &after)?;
            publish(current, events, true, after);
            Ok(())
        });
    work.metrics = final_metrics;
    result
}

const fn catalog_id(stage: PolynomialDualSimplexStage) -> &'static str {
    match stage {
        PolynomialDualSimplexStage::Ready => "polynomial-dual-network-simplex.ready",
        PolynomialDualSimplexStage::InspectInitialArc => {
            "polynomial-dual-network-simplex.inspect-initial-arc"
        }
        PolynomialDualSimplexStage::InitializeTree => {
            "polynomial-dual-network-simplex.initialize-dual-tree"
        }
        PolynomialDualSimplexStage::InitializePseudoflow => {
            "polynomial-dual-network-simplex.initialize-pseudoflow"
        }
        PolynomialDualSimplexStage::BeginScale => {
            "polynomial-dual-network-simplex.begin-delta-scale"
        }
        PolynomialDualSimplexStage::InspectAugmentationArc => {
            "polynomial-dual-network-simplex.inspect-augmentation-arc"
        }
        PolynomialDualSimplexStage::SelectActive => {
            "polynomial-dual-network-simplex.select-active-node"
        }
        PolynomialDualSimplexStage::AugmentToRoot => {
            "polynomial-dual-network-simplex.augment-to-root"
        }
        PolynomialDualSimplexStage::SelectBadArc => {
            "polynomial-dual-network-simplex.select-bad-subtree"
        }
        PolynomialDualSimplexStage::InspectEnteringArc => {
            "polynomial-dual-network-simplex.inspect-entering-arc"
        }
        PolynomialDualSimplexStage::SelectEntering => {
            "polynomial-dual-network-simplex.select-entering-arc"
        }
        PolynomialDualSimplexStage::PivotMakeGood => {
            "polynomial-dual-network-simplex.pivot-make-good"
        }
        PolynomialDualSimplexStage::FinishScale => {
            "polynomial-dual-network-simplex.finish-delta-scale"
        }
        PolynomialDualSimplexStage::Optimal => "polynomial-dual-network-simplex.optimal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph_from(nodes: &[&str], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
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

    fn graph() -> FlowNetwork {
        graph_from(
            &["n0", "n1", "n2", "n3"],
            &[
                ("e0", "n0", "n1", 1, 20, 1),
                ("e1", "n1", "n2", 0, 20, 1),
                ("e2", "n2", "n3", 0, 20, 1),
                ("e3", "n3", "n0", 0, 20, 2),
                ("e4", "n0", "n2", 0, 20, 5),
                ("e5", "n1", "n3", 0, 20, 4),
                ("e6", "n2", "n0", 0, 20, 3),
                ("e7", "n3", "n1", 0, 20, 3),
            ],
        )
    }

    fn pivot_graph() -> FlowNetwork {
        graph_from(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 0, 10, 1),
                ("ac", "a", "c", 0, 10, 5),
                ("ba", "b", "a", 0, 10, 4),
                ("bc", "b", "c", 0, 10, 1),
                ("ca", "c", "a", 0, 10, 4),
                ("cb", "c", "b", 0, 10, 4),
            ],
        )
    }

    fn dense_graph(node_count: usize) -> FlowNetwork {
        let nodes = (0..node_count)
            .map(|index| FlowNode::new(NodeId::parse(&format!("n{index:02}")).expect("node id"), 0))
            .collect();
        let mut edges = Vec::new();
        for from in 0..node_count {
            for to in 0..node_count {
                if from == to {
                    continue;
                }
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("e{from:02}x{to:02}")).expect("edge id"),
                    from: NodeId::parse(&format!("n{from:02}")).expect("tail"),
                    to: NodeId::parse(&format!("n{to:02}")).expect("head"),
                    lower: 0,
                    capacity: 100,
                    cost: 1,
                });
            }
        }
        FlowNetwork::new(nodes, edges).expect("dense graph")
    }

    #[test]
    fn scaling_simplex_certifies_optimum_and_source_invariants() {
        let graph = graph();
        let required = [5, 0, -2, -3];
        let trace = trace_polynomial_dual_network_simplex(&graph, &required).unwrap();
        assert_eq!(trace.result.certificate.total_cost, 13);
        assert!(trace.result.metrics.scaling_phases > 0);
        assert!(trace.result.metrics.augmentations > 0);
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "polynomial-dual-network-simplex.inspect-initial-arc"
                && event.after.inspected_edge.is_some()
                && event.after.metrics.initial_arc_scans > event.before.metrics.initial_arc_scans
        }));
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "polynomial-dual-network-simplex.inspect-augmentation-arc"
                && event.after.inspected_edge.is_some()
                && event.after.metrics.augmentation_arc_scans
                    > event.before.metrics.augmentation_arc_scans
        }));
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.after.stage == PolynomialDualSimplexStage::AugmentToRoot })
        );
        assert!(
            trace
                .final_snapshot
                .basic_flows
                .iter()
                .all(|&flow| flow >= 0)
        );
        check_polynomial_dual_network_simplex_trace(&graph, &required, &trace).unwrap();
    }

    #[test]
    fn every_arc_scan_is_owned_by_a_bounded_inspection_boundary() {
        let graph = dense_graph(20);
        let mut required = vec![0_i128; 20];
        required[0] = 5;
        required[19] = -5;
        let trace = trace_polynomial_dual_network_simplex(&graph, &required).unwrap();
        let mut saw_bounded_block = false;
        for event in &trace.events {
            let before = total_arc_scans(event.before.metrics).unwrap();
            let after = total_arc_scans(event.after.metrics).unwrap();
            if after == before {
                continue;
            }
            assert!(matches!(
                event.after.stage,
                PolynomialDualSimplexStage::InspectInitialArc
                    | PolynomialDualSimplexStage::InspectAugmentationArc
                    | PolynomialDualSimplexStage::InspectEnteringArc
            ));
            assert!(event.after.inspected_edge.is_some());
            assert!(after - before <= POLYNOMIAL_DUAL_SIMPLEX_TRACE_SCAN_BLOCK);
            saw_bounded_block |= after - before > 1;
        }
        assert!(trace.final_snapshot.metrics.initial_arc_scans > 512);
        assert!(saw_bounded_block);
    }

    #[test]
    fn make_good_publishes_bad_cut_entering_and_dual_pivot() {
        let graph = pivot_graph();
        let trace = trace_polynomial_dual_network_simplex(&graph, &[-5, 5, 0]).unwrap();
        assert!(trace.result.metrics.pivots > 0);
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "polynomial-dual-network-simplex.inspect-entering-arc"
                && event.after.inspected_edge.is_some()
                && event.after.metrics.entering_arc_scans > event.before.metrics.entering_arc_scans
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == PolynomialDualSimplexStage::SelectBadArc
                && event.after.leaving_edge.is_some()
                && !event.after.bad_nodes.is_empty()
        }));
        assert!(trace.events.iter().any(|event| {
            event.after.stage == PolynomialDualSimplexStage::PivotMakeGood
                && event.after.entering_edge.is_some()
                && event.after.pivot_price_delta.is_some()
        }));
    }

    #[test]
    fn fast_and_trace_profiles_are_identical() {
        let graph = graph();
        let required = [5, 0, -2, -3];
        let fast = solve_polynomial_dual_network_simplex(&graph, &required).unwrap();
        let traced = trace_polynomial_dual_network_simplex(&graph, &required).unwrap();
        assert_eq!(fast, traced.result);
    }

    #[test]
    fn trace_checker_rejects_excess_drift() {
        let graph = graph();
        let required = [5, 0, -2, -3];
        let mut trace = trace_polynomial_dual_network_simplex(&graph, &required).unwrap();
        trace.events[1].after.excess_numerators[0] += 1;
        assert_eq!(
            check_polynomial_dual_network_simplex_trace(&graph, &required, &trace),
            Err(PolynomialDualSimplexError::TraceVerification)
        );
    }

    #[test]
    fn trace_checker_rejects_discontinuous_active_path() {
        let graph = graph();
        let required = [5, 0, -2, -3];
        let mut trace = trace_polynomial_dual_network_simplex(&graph, &required).unwrap();
        let event = trace
            .events
            .iter_mut()
            .find(|event| event.after.stage == PolynomialDualSimplexStage::SelectActive)
            .expect("select-active boundary");
        let first = event
            .after
            .augment_path
            .first_mut()
            .expect("nonempty root path");
        first.forward = !first.forward;
        assert_eq!(
            check_polynomial_dual_network_simplex_trace(&graph, &required, &trace),
            Err(PolynomialDualSimplexError::TraceVerification)
        );
    }

    #[test]
    fn rejects_binding_capacity_encoding() {
        let graph = graph_from(
            &["a", "b"],
            &[
                ("ab-1", "a", "b", 0, 1, 0),
                ("ab-2", "a", "b", 0, 1, 0),
                ("ba-1", "b", "a", 0, 1, 0),
                ("ba-2", "b", "a", 0, 1, 0),
            ],
        );
        assert_eq!(
            solve_polynomial_dual_network_simplex(&graph, &[2, -2]),
            Err(PolynomialDualSimplexError::CapacityBound)
        );
    }

    #[test]
    fn denominator_exposes_fractional_terminal_scale_exactly() {
        let graph = graph();
        let trace = trace_polynomial_dual_network_simplex(&graph, &[1, 0, 0, -1]).unwrap();
        assert!(trace.final_snapshot.scale_denominator >= 8);
        assert!(trace.final_snapshot.delta_numerator * 8 <= trace.final_snapshot.scale_denominator);
    }
}
