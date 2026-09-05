//! Natural dual network simplex for uncapacitated transshipment.
//!
//! This is the basic pivot from Orlin, Plotkin, and Tardos (1993), Section 2
//! and Figure 1.  The polynomial scaling strategies from later sections are
//! intentionally reserved for the separate polynomial-dual descriptor.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Conservative node limit for explicit cut and tree reconstruction.
pub const DUAL_NETWORK_SIMPLEX_MAX_NODES: usize = 64;
/// Conservative edge limit for explicit pricing scans.
pub const DUAL_NETWORK_SIMPLEX_MAX_EDGES: usize = 512;
/// Deterministic ceiling on natural dual-simplex pivots.
pub const DUAL_NETWORK_SIMPLEX_MAX_PIVOTS: u64 = 100_000;
/// Deterministic ceiling on original-arc scans.
pub const DUAL_NETWORK_SIMPLEX_MAX_ARC_SCANS: u128 = 20_000_000;
/// Preserve every ordinary teaching scan, then exact geometric checkpoints.
const DUAL_NETWORK_SIMPLEX_TRACE_SCAN_PREFIX: u128 = 512;

/// Source-defined semantic boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DualNetworkSimplexStage {
    /// Lower bounds have been shifted but no basis has been selected.
    Ready,
    /// One original arc was inspected while the shortest-path predecessor
    /// forest and tentative prices were being built.
    InspectInitialArc,
    /// A shortest-path tree and its dual-feasible prices were installed.
    InitializeDualTree,
    /// A negative-flow tree arc was selected to leave the basis.
    SelectLeaving,
    /// One original arc was compared against the current head-side cut.
    InspectEnteringArc,
    /// The minimum reduced-cost arc leaving the head-side cut was selected.
    SelectEntering,
    /// The basis and cut-side prices were changed atomically.
    Pivot,
    /// The tree flow is primal feasible and independently certified optimal.
    Optimal,
}

/// Exact counters from the bounded natural pivot kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DualNetworkSimplexMetrics {
    /// Original arcs inspected while building the initial shortest-path tree.
    pub shortest_path_arc_scans: u128,
    /// Full scans for a negative basic tree arc, including the terminal scan.
    pub leaving_searches: u64,
    /// Original arcs inspected while pricing cut-crossing candidates.
    pub entering_arc_scans: u128,
    /// Dual network-simplex basis exchanges.
    pub pivots: u64,
    /// Pivots whose entering reduced cost was zero.
    pub zero_price_pivots: u64,
    /// Explicit tree-flow reconstructions, including initialization.
    pub tree_rebuilds: u64,
    /// Nodes whose price changed across all pivots.
    pub price_updates: u128,
}

/// Complete reversible dual-simplex state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DualNetworkSimplexSnapshot {
    /// Semantic boundary.
    pub stage: DualNetworkSimplexStage,
    /// Original edges currently forming the undirected spanning-tree basis.
    pub tree_edges: Vec<EdgeId>,
    /// Lower-bound-shifted basic flow in canonical original-edge order.
    pub basic_flows: Vec<i128>,
    /// Exact node prices in canonical node order.
    pub potentials: Vec<i128>,
    /// Nodes whose tentative or final price is defined. During initial-tree
    /// inspection this excludes vertices not yet reached from the root.
    pub initialized_nodes: Vec<NodeIndex>,
    /// Exact reduced costs in canonical original-edge order.
    pub reduced_costs: Vec<i128>,
    /// Selected infeasible tree arc.
    pub leaving_edge: Option<EdgeId>,
    /// Selected minimum reduced-cost replacement arc.
    pub entering_edge: Option<EdgeId>,
    /// Original edge examined by the current source-level scan.
    pub inspected_edge: Option<EdgeId>,
    /// Head-side cut of the selected leaving arc.
    pub cut_side: Vec<NodeIndex>,
    /// Entering reduced cost used for the price shift.
    pub pivot_price_delta: Option<i128>,
    /// Independently certified original bounded flow at the terminal boundary.
    pub certified_flows: Option<Vec<u64>>,
    /// Exact work counters.
    pub metrics: DualNetworkSimplexMetrics,
}

/// One reversible source transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DualNetworkSimplexTraceEvent {
    /// Stable event-catalog identity.
    pub catalog_id: &'static str,
    /// State before the transition.
    pub before: DualNetworkSimplexSnapshot,
    /// State after the transition.
    pub after: DualNetworkSimplexSnapshot,
}

/// Certified natural dual-network-simplex result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DualNetworkSimplexResult {
    /// Original bounded flows in canonical edge order.
    pub flows: Vec<u64>,
    /// Independent exact primal/dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact natural-pivot counters.
    pub metrics: DualNetworkSimplexMetrics,
    /// Terminal source state.
    pub final_snapshot: DualNetworkSimplexSnapshot,
}

/// Certified result with every leaving, entering, and pivot boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DualNetworkSimplexTraceResult {
    /// Same result returned by the fast profile.
    pub result: DualNetworkSimplexResult,
    /// Ready boundary.
    pub base_snapshot: DualNetworkSimplexSnapshot,
    /// Reversible transitions.
    pub events: Vec<DualNetworkSimplexTraceEvent>,
    /// Certified terminal boundary.
    pub final_snapshot: DualNetworkSimplexSnapshot,
}

/// Domain, arithmetic, work, basis, or proof failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DualNetworkSimplexError {
    /// Input exceeds the explicit natural-pivot admission band.
    #[error("graph exceeds dual-network-simplex admission limits")]
    AdmissionLimit,
    /// The deterministic pivot or scan ceiling was reached.
    #[error("dual-network-simplex work limit reached")]
    WorkLimit,
    /// The source algorithm requires a strongly connected uncapacitated graph.
    #[error("dual network simplex requires a strongly connected positive-width graph")]
    StrongConnectivity,
    /// Finite project capacities do not encode uncapacitated arcs.
    #[error("dual network simplex requires nonbinding transshipment capacities")]
    CapacityBound,
    /// A negative forward cycle makes the source transshipment unbounded.
    #[error("dual-network-simplex transshipment is unbounded")]
    Unbounded,
    /// Requested balances are infeasible in the original bounded model.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Final independent certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic overflowed.
    #[error("dual-network-simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// The tree, basic flow, cut, or dual-feasibility invariant failed.
    #[error("dual-network-simplex basis invariant failed")]
    BasisInvariant,
    /// The deterministic dual-Bland rule revisited a basis.
    #[error("dual-network-simplex repeated a basis")]
    RepeatedBasis,
    /// A public trace did not match the deterministic source transition grammar.
    #[error("dual-network-simplex trace verification failed")]
    TraceVerification,
}

/// Solves the nonbinding-capacity encoding of uncapacitated transshipment.
///
/// # Errors
///
/// Returns a domain, feasibility, work, arithmetic, basis, or certificate
/// failure.
pub fn solve_dual_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DualNetworkSimplexResult, DualNetworkSimplexError> {
    solve_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves while reporting its feasibility precheck to the enclosing execution
/// context without retaining source trace events.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_dual_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DualNetworkSimplexResult, DualNetworkSimplexError> {
    solve_internal_with_feasibility(graph, required_divergence, false, feasibility)
        .map(|run| run.result)
}

/// Records every natural dual-network-simplex basis transition.
///
/// # Errors
///
/// Returns the same failures as [`solve_dual_network_simplex`] plus trace
/// verification failures.
pub fn trace_dual_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DualNetworkSimplexTraceResult, DualNetworkSimplexError> {
    let run = solve_internal(graph, required_divergence, true)?;
    let trace = DualNetworkSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_dual_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Traces dual network simplex while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_dual_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DualNetworkSimplexTraceResult, DualNetworkSimplexError> {
    let run = solve_internal_with_feasibility(graph, required_divergence, true, feasibility)?;
    let trace = DualNetworkSimplexTraceResult {
        result: run.result,
        base_snapshot: run.base_snapshot,
        events: run.events,
        final_snapshot: run.final_snapshot,
    };
    check_dual_network_simplex_trace(graph, required_divergence, &trace)?;
    Ok(trace)
}

/// Replays a public trace from the deterministic source kernel and validates
/// every published basis snapshot.
///
/// # Errors
///
/// Returns [`DualNetworkSimplexError::TraceVerification`] for any mismatch.
pub fn check_dual_network_simplex_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace: &DualNetworkSimplexTraceResult,
) -> Result<(), DualNetworkSimplexError> {
    validate_public_snapshot(graph, required_divergence, &trace.base_snapshot)?;
    let mut current = &trace.base_snapshot;
    for event in &trace.events {
        if &event.before != current || event.catalog_id != catalog_id(event.after.stage) {
            return Err(DualNetworkSimplexError::TraceVerification);
        }
        validate_public_snapshot(graph, required_divergence, &event.after)?;
        current = &event.after;
    }
    if current != &trace.final_snapshot || trace.result.final_snapshot != trace.final_snapshot {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    let expected = solve_internal(graph, required_divergence, true)?;
    if expected.base_snapshot != trace.base_snapshot
        || expected.events != trace.events
        || expected.final_snapshot != trace.final_snapshot
        || expected.result != trace.result
    {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    Ok(())
}

struct InternalRun {
    result: DualNetworkSimplexResult,
    base_snapshot: DualNetworkSimplexSnapshot,
    events: Vec<DualNetworkSimplexTraceEvent>,
    final_snapshot: DualNetworkSimplexSnapshot,
}

struct WorkingState<'graph> {
    graph: &'graph FlowNetwork,
    required_variable: Vec<i128>,
    tree: Vec<bool>,
    basic_flows: Vec<i128>,
    potentials: Vec<i128>,
    metrics: DualNetworkSimplexMetrics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InitialTreeCheckpoint {
    inspected_edge: usize,
    distances: Vec<Option<i128>>,
    predecessor: Vec<Option<usize>>,
    scans: u128,
}

type InitialDualTree = (Vec<bool>, Vec<i128>, u128, Vec<InitialTreeCheckpoint>);

#[allow(clippy::too_many_lines)]
fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<InternalRun, DualNetworkSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(graph, required_divergence, record_trace, &mut feasibility)
}

#[allow(clippy::too_many_lines)]
fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, DualNetworkSimplexError> {
    let required_variable =
        validate_source_domain_with_feasibility(graph, required_divergence, feasibility)?;
    let mut work = WorkingState {
        graph,
        required_variable,
        tree: vec![false; graph.edges().len()],
        basic_flows: vec![0; graph.edges().len()],
        potentials: vec![0; graph.nodes().len()],
        metrics: DualNetworkSimplexMetrics::default(),
    };
    let base_snapshot = snapshot(
        &work,
        DualNetworkSimplexStage::Ready,
        None,
        None,
        Vec::new(),
        None,
        None,
    )?;
    let mut current = base_snapshot.clone();
    let mut events = Vec::new();

    let initial_checkpoints = install_initial_basis(&mut work, record_trace)?;
    for checkpoint in initial_checkpoints {
        publish(
            &mut current,
            &mut events,
            record_trace,
            initial_tree_checkpoint_snapshot(graph, &checkpoint)?,
        );
    }
    publish(
        &mut current,
        &mut events,
        record_trace,
        snapshot(
            &work,
            DualNetworkSimplexStage::InitializeDualTree,
            None,
            None,
            Vec::new(),
            None,
            None,
        )?,
    );

    let mut seen_bases = BTreeSet::new();
    seen_bases.insert(basis_key(&work.tree));
    loop {
        work.metrics.leaving_searches = work
            .metrics
            .leaving_searches
            .checked_add(1)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
        let Some(leaving) = work
            .tree
            .iter()
            .zip(&work.basic_flows)
            .position(|(&in_tree, &flow)| in_tree && flow < 0)
        else {
            break;
        };
        if work.metrics.pivots >= DUAL_NETWORK_SIMPLEX_MAX_PIVOTS {
            return Err(DualNetworkSimplexError::WorkLimit);
        }
        perform_pivot(
            &mut work,
            &mut current,
            &mut events,
            record_trace,
            leaving,
            &mut seen_bases,
        )?;
    }

    validate_basis(&work)?;
    let flows = recover_original_flows(graph, &work.basic_flows)?;
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    publish(
        &mut current,
        &mut events,
        record_trace,
        snapshot(
            &work,
            DualNetworkSimplexStage::Optimal,
            None,
            None,
            Vec::new(),
            None,
            Some(flows.clone()),
        )?,
    );
    let final_snapshot = current;
    let result = DualNetworkSimplexResult {
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

fn install_initial_basis(
    work: &mut WorkingState<'_>,
    record_trace: bool,
) -> Result<Vec<InitialTreeCheckpoint>, DualNetworkSimplexError> {
    let (tree, potentials, shortest_path_arc_scans, checkpoints) =
        initial_dual_tree(work.graph, record_trace)?;
    work.tree = tree;
    work.potentials = potentials;
    work.metrics.shortest_path_arc_scans = shortest_path_arc_scans;
    work.basic_flows = reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)?;
    work.metrics.tree_rebuilds = 1;
    Ok(checkpoints)
}

fn perform_pivot(
    work: &mut WorkingState<'_>,
    current: &mut DualNetworkSimplexSnapshot,
    events: &mut Vec<DualNetworkSimplexTraceEvent>,
    record_trace: bool,
    leaving: usize,
    seen_bases: &mut BTreeSet<Vec<usize>>,
) -> Result<(), DualNetworkSimplexError> {
    let cut = head_side_cut(work.graph, &work.tree, leaving)?;
    publish(
        current,
        events,
        record_trace,
        snapshot(
            work,
            DualNetworkSimplexStage::SelectLeaving,
            Some(leaving),
            None,
            cut_indices(&cut)?,
            None,
            None,
        )?,
    );

    let reduced_costs = reduced_costs(work.graph, &work.potentials)?;
    let (entering, delta) = select_entering_arc(
        work,
        current,
        events,
        record_trace,
        leaving,
        &cut,
        &reduced_costs,
    )?;
    publish(
        current,
        events,
        record_trace,
        snapshot(
            work,
            DualNetworkSimplexStage::SelectEntering,
            Some(leaving),
            Some(entering),
            cut_indices(&cut)?,
            Some(delta),
            None,
        )?,
    );

    work.tree[leaving] = false;
    work.tree[entering] = true;
    update_cut_prices(work, &cut, delta)?;
    work.basic_flows = reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)?;
    work.metrics.pivots = work
        .metrics
        .pivots
        .checked_add(1)
        .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
    work.metrics.tree_rebuilds = work
        .metrics
        .tree_rebuilds
        .checked_add(1)
        .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
    if delta == 0 {
        work.metrics.zero_price_pivots = work
            .metrics
            .zero_price_pivots
            .checked_add(1)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
    }
    validate_basis(work)?;
    if !seen_bases.insert(basis_key(&work.tree)) {
        return Err(DualNetworkSimplexError::RepeatedBasis);
    }
    publish(
        current,
        events,
        record_trace,
        snapshot(
            work,
            DualNetworkSimplexStage::Pivot,
            Some(leaving),
            Some(entering),
            cut_indices(&cut)?,
            Some(delta),
            None,
        )?,
    );
    Ok(())
}

fn update_cut_prices(
    work: &mut WorkingState<'_>,
    cut: &[bool],
    delta: i128,
) -> Result<(), DualNetworkSimplexError> {
    for (node, &inside) in cut.iter().enumerate() {
        if inside {
            work.potentials[node] = work.potentials[node]
                .checked_sub(delta)
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
            work.metrics.price_updates = work
                .metrics
                .price_updates
                .checked_add(1)
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
        }
    }
    Ok(())
}

fn validate_source_domain(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<Vec<i128>, DualNetworkSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    validate_source_domain_with_feasibility(graph, required_divergence, &mut feasibility)
}

fn validate_source_domain_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<Vec<i128>, DualNetworkSimplexError> {
    if graph.nodes().is_empty()
        || graph.nodes().len() > DUAL_NETWORK_SIMPLEX_MAX_NODES
        || graph.edges().len() > DUAL_NETWORK_SIMPLEX_MAX_EDGES
    {
        return Err(DualNetworkSimplexError::AdmissionLimit);
    }
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let lower_divergence = divergences(graph, &lower)?;
    if lower_divergence.len() != required_divergence.len() {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    let required_variable = required_divergence
        .iter()
        .zip(lower_divergence)
        .map(|(&required, actual)| {
            required
                .checked_sub(actual)
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total_supply = required_variable.iter().try_fold(0_u128, |total, &value| {
        if value > 0 {
            total
                .checked_add(value.unsigned_abs())
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)
        } else {
            Ok(total)
        }
    })?;
    for edge in graph.edges() {
        let width = u128::from(edge.capacity() - edge.lower());
        if width < total_supply {
            return Err(DualNetworkSimplexError::CapacityBound);
        }
    }
    if !is_strongly_connected(graph) {
        return Err(DualNetworkSimplexError::StrongConnectivity);
    }
    if has_negative_forward_cycle(graph)? {
        return Err(DualNetworkSimplexError::Unbounded);
    }
    Ok(required_variable)
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

fn has_negative_forward_cycle(graph: &FlowNetwork) -> Result<bool, DualNetworkSimplexError> {
    let mut distance = vec![0_i128; graph.nodes().len()];
    for round in 0..graph.nodes().len() {
        let mut changed = false;
        for edge in graph.edges() {
            if edge.capacity() == edge.lower() {
                continue;
            }
            let candidate = distance[edge.from().as_usize()]
                .checked_add(i128::from(edge.cost()))
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
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

fn initial_dual_tree(
    graph: &FlowNetwork,
    record_trace: bool,
) -> Result<InitialDualTree, DualNetworkSimplexError> {
    let node_count = graph.nodes().len();
    let mut distances = vec![None; node_count];
    let mut predecessor = vec![None; node_count];
    distances[0] = Some(0_i128);
    let mut scans = 0_u128;
    let mut checkpoints = Vec::new();
    for _ in 1..node_count {
        let mut changed = false;
        for (index, edge) in graph.edges().iter().enumerate() {
            scans = scans
                .checked_add(1)
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
            if scans > DUAL_NETWORK_SIMPLEX_MAX_ARC_SCANS {
                return Err(DualNetworkSimplexError::WorkLimit);
            }
            if edge.capacity() != edge.lower()
                && let Some(from_distance) = distances[edge.from().as_usize()]
            {
                let candidate = from_distance
                    .checked_add(i128::from(edge.cost()))
                    .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
                let to = edge.to().as_usize();
                if distances[to].is_none_or(|current| candidate < current) {
                    distances[to] = Some(candidate);
                    predecessor[to] = Some(index);
                    changed = true;
                }
            }
            if record_trace && should_publish_arc_scan(scans) {
                checkpoints.push(InitialTreeCheckpoint {
                    inspected_edge: index,
                    distances: distances.clone(),
                    predecessor: predecessor.clone(),
                    scans,
                });
            }
        }
        if !changed {
            break;
        }
    }
    let potentials = distances
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(DualNetworkSimplexError::StrongConnectivity)?;
    let mut tree = vec![false; graph.edges().len()];
    for edge in predecessor.into_iter().skip(1) {
        tree[edge.ok_or(DualNetworkSimplexError::BasisInvariant)?] = true;
    }
    validate_tree(graph, &tree)?;
    let reduced = reduced_costs(graph, &potentials)?;
    if reduced.iter().any(|&cost| cost < 0)
        || graph
            .edges()
            .iter()
            .enumerate()
            .any(|(index, _)| tree[index] && reduced[index] != 0)
    {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    Ok((tree, potentials, scans, checkpoints))
}

fn reconstruct_basic_flows(
    graph: &FlowNetwork,
    required: &[i128],
    tree: &[bool],
) -> Result<Vec<i128>, DualNetworkSimplexError> {
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
                        .ok_or(DualNetworkSimplexError::ArithmeticOverflow)
                } else {
                    Ok(sum)
                }
            })?;
        flows[index] = balance
            .checked_neg()
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
    }
    if basic_divergences(graph, &flows)? != required {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    Ok(flows)
}

fn validate_tree(graph: &FlowNetwork, tree: &[bool]) -> Result<(), DualNetworkSimplexError> {
    let node_count = graph.nodes().len();
    if tree.len() != graph.edges().len()
        || tree.iter().filter(|value| **value).count() != node_count.saturating_sub(1)
        || graph
            .edges()
            .iter()
            .enumerate()
            .any(|(index, edge)| tree[index] && edge.from() == edge.to())
    {
        return Err(DualNetworkSimplexError::BasisInvariant);
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
        Err(DualNetworkSimplexError::BasisInvariant)
    }
}

fn tree_side(
    graph: &FlowNetwork,
    tree: &[bool],
    removed_edge: usize,
    start: usize,
) -> Result<Vec<bool>, DualNetworkSimplexError> {
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
        Err(DualNetworkSimplexError::BasisInvariant)
    } else {
        Ok(side)
    }
}

fn head_side_cut(
    graph: &FlowNetwork,
    tree: &[bool],
    leaving: usize,
) -> Result<Vec<bool>, DualNetworkSimplexError> {
    let edge = graph
        .edges()
        .get(leaving)
        .ok_or(DualNetworkSimplexError::BasisInvariant)?;
    if !tree.get(leaving).copied().unwrap_or(false) {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    tree_side(graph, tree, leaving, edge.to().as_usize())
}

fn select_entering_arc(
    work: &mut WorkingState<'_>,
    current: &mut DualNetworkSimplexSnapshot,
    events: &mut Vec<DualNetworkSimplexTraceEvent>,
    record_trace: bool,
    leaving: usize,
    cut: &[bool],
    reduced_costs: &[i128],
) -> Result<(usize, i128), DualNetworkSimplexError> {
    let mut best = None;
    for (index, edge) in work.graph.edges().iter().enumerate() {
        work.metrics.entering_arc_scans = work
            .metrics
            .entering_arc_scans
            .checked_add(1)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
        let total = work
            .metrics
            .shortest_path_arc_scans
            .checked_add(work.metrics.entering_arc_scans)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
        if total > DUAL_NETWORK_SIMPLEX_MAX_ARC_SCANS {
            return Err(DualNetworkSimplexError::WorkLimit);
        }
        if cut[edge.from().as_usize()] && !cut[edge.to().as_usize()] {
            let candidate = (reduced_costs[index], index);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        if record_trace && should_publish_arc_scan(total) {
            let (best_delta, best_index) =
                best.map_or((None, None), |(delta, index)| (Some(delta), Some(index)));
            publish(
                current,
                events,
                record_trace,
                snapshot_with_inspection(
                    work,
                    DualNetworkSimplexStage::InspectEnteringArc,
                    Some(leaving),
                    best_index,
                    Some(index),
                    cut_indices(cut)?,
                    best_delta,
                    None,
                )?,
            );
        }
    }
    let (delta, index) = best.ok_or(DualNetworkSimplexError::StrongConnectivity)?;
    if delta < 0 {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    Ok((index, delta))
}

const fn should_publish_arc_scan(scan: u128) -> bool {
    scan <= DUAL_NETWORK_SIMPLEX_TRACE_SCAN_PREFIX || scan.is_power_of_two()
}

fn validate_basis(work: &WorkingState<'_>) -> Result<(), DualNetworkSimplexError> {
    validate_tree(work.graph, &work.tree)?;
    if reconstruct_basic_flows(work.graph, &work.required_variable, &work.tree)? != work.basic_flows
    {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    let reduced = reduced_costs(work.graph, &work.potentials)?;
    if reduced.iter().any(|&cost| cost < 0)
        || reduced
            .iter()
            .zip(&work.tree)
            .any(|(&cost, &in_tree)| in_tree && cost != 0)
    {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    Ok(())
}

fn reduced_costs(
    graph: &FlowNetwork,
    potentials: &[i128],
) -> Result<Vec<i128>, DualNetworkSimplexError> {
    if potentials.len() != graph.nodes().len() {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    graph
        .edges()
        .iter()
        .map(|edge| {
            i128::from(edge.cost())
                .checked_add(potentials[edge.from().as_usize()])
                .and_then(|value| value.checked_sub(potentials[edge.to().as_usize()]))
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)
        })
        .collect()
}

fn basic_divergences(
    graph: &FlowNetwork,
    flows: &[i128],
) -> Result<Vec<i128>, DualNetworkSimplexError> {
    if flows.len() != graph.edges().len() {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    let mut values = vec![0_i128; graph.nodes().len()];
    for (edge, &flow) in graph.edges().iter().zip(flows) {
        values[edge.from().as_usize()] = values[edge.from().as_usize()]
            .checked_add(flow)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
        values[edge.to().as_usize()] = values[edge.to().as_usize()]
            .checked_sub(flow)
            .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
    }
    Ok(values)
}

fn recover_original_flows(
    graph: &FlowNetwork,
    basic_flows: &[i128],
) -> Result<Vec<u64>, DualNetworkSimplexError> {
    graph
        .edges()
        .iter()
        .zip(basic_flows)
        .map(|(edge, &basic)| {
            let variable =
                u64::try_from(basic).map_err(|_| DualNetworkSimplexError::BasisInvariant)?;
            let flow = edge
                .lower()
                .checked_add(variable)
                .ok_or(DualNetworkSimplexError::ArithmeticOverflow)?;
            if flow > edge.capacity() {
                Err(DualNetworkSimplexError::CapacityBound)
            } else {
                Ok(flow)
            }
        })
        .collect()
}

fn snapshot(
    work: &WorkingState<'_>,
    stage: DualNetworkSimplexStage,
    leaving: Option<usize>,
    entering: Option<usize>,
    cut_side: Vec<NodeIndex>,
    pivot_price_delta: Option<i128>,
    certified_flows: Option<Vec<u64>>,
) -> Result<DualNetworkSimplexSnapshot, DualNetworkSimplexError> {
    snapshot_with_inspection(
        work,
        stage,
        leaving,
        entering,
        None,
        cut_side,
        pivot_price_delta,
        certified_flows,
    )
}

#[allow(clippy::too_many_arguments)]
fn snapshot_with_inspection(
    work: &WorkingState<'_>,
    stage: DualNetworkSimplexStage,
    leaving: Option<usize>,
    entering: Option<usize>,
    inspected: Option<usize>,
    cut_side: Vec<NodeIndex>,
    pivot_price_delta: Option<i128>,
    certified_flows: Option<Vec<u64>>,
) -> Result<DualNetworkSimplexSnapshot, DualNetworkSimplexError> {
    Ok(DualNetworkSimplexSnapshot {
        stage,
        tree_edges: work
            .graph
            .edges()
            .iter()
            .zip(&work.tree)
            .filter(|&(_, &in_tree)| in_tree)
            .map(|(edge, _)| edge.id().clone())
            .collect(),
        basic_flows: work.basic_flows.clone(),
        potentials: work.potentials.clone(),
        initialized_nodes: if stage == DualNetworkSimplexStage::Ready {
            Vec::new()
        } else {
            (0..work.graph.nodes().len())
                .map(|index| {
                    NodeIndex::try_from_usize(index)
                        .ok_or(DualNetworkSimplexError::ArithmeticOverflow)
                })
                .collect::<Result<Vec<_>, _>>()?
        },
        reduced_costs: reduced_costs(work.graph, &work.potentials)?,
        leaving_edge: leaving
            .map(|index| {
                work.graph
                    .edges()
                    .get(index)
                    .map(|edge| edge.id().clone())
                    .ok_or(DualNetworkSimplexError::BasisInvariant)
            })
            .transpose()?,
        entering_edge: entering
            .map(|index| {
                work.graph
                    .edges()
                    .get(index)
                    .map(|edge| edge.id().clone())
                    .ok_or(DualNetworkSimplexError::BasisInvariant)
            })
            .transpose()?,
        inspected_edge: inspected
            .map(|index| {
                work.graph
                    .edges()
                    .get(index)
                    .map(|edge| edge.id().clone())
                    .ok_or(DualNetworkSimplexError::BasisInvariant)
            })
            .transpose()?,
        cut_side,
        pivot_price_delta,
        certified_flows,
        metrics: work.metrics,
    })
}

fn initial_tree_checkpoint_snapshot(
    graph: &FlowNetwork,
    checkpoint: &InitialTreeCheckpoint,
) -> Result<DualNetworkSimplexSnapshot, DualNetworkSimplexError> {
    if checkpoint.distances.len() != graph.nodes().len()
        || checkpoint.predecessor.len() != graph.nodes().len()
    {
        return Err(DualNetworkSimplexError::BasisInvariant);
    }
    let potentials = checkpoint
        .distances
        .iter()
        .map(|distance| distance.unwrap_or(0))
        .collect::<Vec<_>>();
    let initialized_nodes = checkpoint
        .distances
        .iter()
        .enumerate()
        .filter_map(|(index, distance)| distance.is_some().then_some(index))
        .map(|index| {
            NodeIndex::try_from_usize(index).ok_or(DualNetworkSimplexError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tree_indices = checkpoint
        .predecessor
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>();
    let tree_edges = tree_indices
        .iter()
        .map(|&index| {
            graph
                .edges()
                .get(index)
                .map(|edge| edge.id().clone())
                .ok_or(DualNetworkSimplexError::BasisInvariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inspected_edge = graph
        .edges()
        .get(checkpoint.inspected_edge)
        .map(|edge| edge.id().clone())
        .ok_or(DualNetworkSimplexError::BasisInvariant)?;
    Ok(DualNetworkSimplexSnapshot {
        stage: DualNetworkSimplexStage::InspectInitialArc,
        tree_edges,
        basic_flows: vec![0; graph.edges().len()],
        reduced_costs: reduced_costs(graph, &potentials)?,
        potentials,
        initialized_nodes,
        leaving_edge: None,
        entering_edge: None,
        inspected_edge: Some(inspected_edge),
        cut_side: Vec::new(),
        pivot_price_delta: None,
        certified_flows: None,
        metrics: DualNetworkSimplexMetrics {
            shortest_path_arc_scans: checkpoint.scans,
            ..DualNetworkSimplexMetrics::default()
        },
    })
}

fn validate_public_snapshot(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    snapshot: &DualNetworkSimplexSnapshot,
) -> Result<(), DualNetworkSimplexError> {
    let required_variable = validate_source_domain(graph, required_divergence)?;
    if snapshot.basic_flows.len() != graph.edges().len()
        || snapshot.potentials.len() != graph.nodes().len()
        || snapshot.reduced_costs != reduced_costs(graph, &snapshot.potentials)?
    {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    if snapshot.stage == DualNetworkSimplexStage::Ready {
        if !snapshot.tree_edges.is_empty()
            || snapshot.basic_flows.iter().any(|&flow| flow != 0)
            || snapshot.potentials.iter().any(|&potential| potential != 0)
            || !snapshot.initialized_nodes.is_empty()
            || snapshot.inspected_edge.is_some()
        {
            return Err(DualNetworkSimplexError::TraceVerification);
        }
        return Ok(());
    }
    let initialized = snapshot
        .initialized_nodes
        .iter()
        .map(|index| index.as_usize())
        .collect::<BTreeSet<_>>();
    if initialized.len() != snapshot.initialized_nodes.len()
        || initialized
            .iter()
            .any(|&index| index >= graph.nodes().len())
    {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    let tree_ids = snapshot.tree_edges.iter().collect::<BTreeSet<_>>();
    if tree_ids.len() != snapshot.tree_edges.len() {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    let tree = graph
        .edges()
        .iter()
        .map(|edge| tree_ids.contains(&edge.id()))
        .collect::<Vec<_>>();
    if snapshot.stage == DualNetworkSimplexStage::InspectInitialArc {
        if initialized.is_empty()
            || !initialized.contains(&0)
            || snapshot.basic_flows.iter().any(|&flow| flow != 0)
            || snapshot.inspected_edge.is_none()
            || snapshot.leaving_edge.is_some()
            || snapshot.entering_edge.is_some()
            || !snapshot.cut_side.is_empty()
            || snapshot.pivot_price_delta.is_some()
            || snapshot.certified_flows.is_some()
            || tree.iter().filter(|value| **value).count() >= graph.nodes().len()
        {
            return Err(DualNetworkSimplexError::TraceVerification);
        }
        return Ok(());
    }
    if initialized.len() != graph.nodes().len() {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    if reconstruct_basic_flows(graph, &required_variable, &tree)? != snapshot.basic_flows
        || snapshot.reduced_costs.iter().any(|&cost| cost < 0)
        || snapshot
            .reduced_costs
            .iter()
            .zip(&tree)
            .any(|(&cost, &in_tree)| in_tree && cost != 0)
    {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    if snapshot.stage == DualNetworkSimplexStage::Optimal {
        let flows = snapshot
            .certified_flows
            .as_ref()
            .ok_or(DualNetworkSimplexError::TraceVerification)?;
        check_min_cost_flow(graph, required_divergence, flows)
            .map_err(|_| DualNetworkSimplexError::TraceVerification)?;
    }
    if snapshot.stage == DualNetworkSimplexStage::InspectEnteringArc {
        if snapshot.inspected_edge.is_none()
            || snapshot.leaving_edge.is_none()
            || snapshot.cut_side.is_empty()
            || snapshot.certified_flows.is_some()
        {
            return Err(DualNetworkSimplexError::TraceVerification);
        }
    } else if snapshot.inspected_edge.is_some() {
        return Err(DualNetworkSimplexError::TraceVerification);
    }
    Ok(())
}

fn cut_indices(cut: &[bool]) -> Result<Vec<NodeIndex>, DualNetworkSimplexError> {
    cut.iter()
        .enumerate()
        .filter_map(|(index, &inside)| inside.then_some(index))
        .map(|index| {
            NodeIndex::try_from_usize(index).ok_or(DualNetworkSimplexError::ArithmeticOverflow)
        })
        .collect()
}

fn basis_key(tree: &[bool]) -> Vec<usize> {
    tree.iter()
        .enumerate()
        .filter_map(|(index, &in_tree)| in_tree.then_some(index))
        .collect()
}

fn publish(
    current: &mut DualNetworkSimplexSnapshot,
    events: &mut Vec<DualNetworkSimplexTraceEvent>,
    record_trace: bool,
    after: DualNetworkSimplexSnapshot,
) {
    if record_trace {
        events.push(DualNetworkSimplexTraceEvent {
            catalog_id: catalog_id(after.stage),
            before: current.clone(),
            after: after.clone(),
        });
    }
    *current = after;
}

const fn catalog_id(stage: DualNetworkSimplexStage) -> &'static str {
    match stage {
        DualNetworkSimplexStage::Ready => "dual-network-simplex.ready",
        DualNetworkSimplexStage::InspectInitialArc => "dual-network-simplex.inspect-initial-arc",
        DualNetworkSimplexStage::InitializeDualTree => "dual-network-simplex.initialize-dual-tree",
        DualNetworkSimplexStage::SelectLeaving => "dual-network-simplex.select-leaving",
        DualNetworkSimplexStage::InspectEnteringArc => "dual-network-simplex.inspect-entering-arc",
        DualNetworkSimplexStage::SelectEntering => "dual-network-simplex.select-entering",
        DualNetworkSimplexStage::Pivot => "dual-network-simplex.pivot",
        DualNetworkSimplexStage::Optimal => "dual-network-simplex.optimal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

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
    fn pivots_from_dual_feasibility_to_the_independent_optimum() {
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
        let target = [-5, 5, 0];
        let fast = solve_dual_network_simplex(&graph, &target).expect("fast");
        let traced = trace_dual_network_simplex(&graph, &target).expect("trace");
        let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
        assert_eq!(fast, traced.result);
        assert_eq!(fast.certificate.total_cost, oracle.certificate.total_cost);
        assert!(fast.metrics.pivots > 0);
        assert!(traced.events.iter().any(|event| {
            event.after.stage == DualNetworkSimplexStage::Pivot
                && event.after.leaving_edge.is_some()
                && event.after.entering_edge.is_some()
        }));
    }

    #[test]
    fn supports_lower_bounds_and_negative_edges_without_negative_cycles() {
        let graph = graph_with_lower(
            &["a", "b", "c"],
            &[
                ("ab", "a", "b", 1, 12, -2),
                ("ac", "a", "c", 0, 12, 3),
                ("ba", "b", "a", 0, 12, 4),
                ("bc", "b", "c", 0, 12, 1),
                ("ca", "c", "a", 0, 12, 4),
                ("cb", "c", "b", 0, 12, 4),
            ],
        );
        let target = [4, 0, -4];
        let result = solve_dual_network_simplex(&graph, &target).expect("dual simplex");
        let oracle = solve_simple_cycle_canceling(&graph, &target).expect("oracle");
        assert_eq!(result.certificate.total_cost, oracle.certificate.total_cost);
        assert_eq!(
            divergences(&graph, &result.flows).expect("divergences"),
            target
        );
        assert!(result.flows[0] >= 1);
    }

    #[test]
    fn rejects_binding_capacity_non_strong_graph_and_unbounded_cycle() {
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
            solve_dual_network_simplex(&narrow, &[2, -2]),
            Err(DualNetworkSimplexError::CapacityBound)
        );
        let one_way = graph(&["a", "b"], &[("ab", "a", "b", 10, 0)]);
        assert_eq!(
            solve_dual_network_simplex(&one_way, &[1, -1]),
            Err(DualNetworkSimplexError::StrongConnectivity)
        );
        let negative_cycle = graph(
            &["a", "b"],
            &[("ab", "a", "b", 10, -2), ("ba", "b", "a", 10, 1)],
        );
        assert_eq!(
            solve_dual_network_simplex(&negative_cycle, &[0, 0]),
            Err(DualNetworkSimplexError::Unbounded)
        );
    }

    #[test]
    fn deterministic_small_complete_graphs_match_cycle_canceling() {
        for first_cost in 0_i64..=2 {
            for second_cost in 0_i64..=2 {
                let graph = graph(
                    &["a", "b", "c"],
                    &[
                        ("ab", "a", "b", 12, first_cost),
                        ("ac", "a", "c", 12, second_cost + 1),
                        ("ba", "b", "a", 12, second_cost + 2),
                        ("bc", "b", "c", 12, first_cost + 1),
                        ("ca", "c", "a", 12, second_cost + 2),
                        ("cb", "c", "b", 12, first_cost + 2),
                    ],
                );
                for target in [[4, -1, -3], [-3, 4, -1], [-1, -3, 4]] {
                    let actual = solve_dual_network_simplex(&graph, &target).expect("dual");
                    let oracle =
                        solve_simple_cycle_canceling(&graph, &target).expect("cycle oracle");
                    assert_eq!(
                        actual.certificate.total_cost, oracle.certificate.total_cost,
                        "costs {first_cost}/{second_cost}, target {target:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn trace_checker_rejects_forged_basis_price() {
        let graph = graph(
            &["a", "b"],
            &[("ab", "a", "b", 10, 1), ("ba", "b", "a", 10, 2)],
        );
        let mut trace = trace_dual_network_simplex(&graph, &[-3, 3]).expect("trace");
        trace.events[0].after.potentials[0] = 1;
        assert_eq!(
            check_dual_network_simplex_trace(&graph, &[-3, 3], &trace),
            Err(DualNetworkSimplexError::TraceVerification)
        );
    }

    #[test]
    fn admission_accepts_exact_limits_and_rejects_the_next_value() {
        let node_ids = (0..DUAL_NETWORK_SIMPLEX_MAX_NODES)
            .map(|index| format!("n{index:02}"))
            .collect::<Vec<_>>();
        let node_refs = node_ids.iter().map(String::as_str).collect::<Vec<_>>();
        let ring_edges = (0..DUAL_NETWORK_SIMPLEX_MAX_NODES)
            .map(|index| {
                (
                    format!("e{index:02}"),
                    node_ids[index].clone(),
                    node_ids[(index + 1) % node_ids.len()].clone(),
                )
            })
            .collect::<Vec<_>>();
        let ring_edge_refs = ring_edges
            .iter()
            .map(|(id, from, to)| (id.as_str(), from.as_str(), to.as_str(), 64, 0))
            .collect::<Vec<_>>();
        let graph_at_node_limit = graph(&node_refs, &ring_edge_refs);
        let mut target = vec![0_i128; DUAL_NETWORK_SIMPLEX_MAX_NODES];
        target[0] = 1;
        target[1] = -1;
        solve_dual_network_simplex(&graph_at_node_limit, &target).expect("node limit is inclusive");

        let mut too_many_node_ids = node_ids;
        too_many_node_ids.push("overflow".to_owned());
        let too_many_node_refs = too_many_node_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let too_many_nodes = graph(&too_many_node_refs, &[("loop", "n00", "n00", 1, 0)]);
        assert_eq!(
            solve_dual_network_simplex(&too_many_nodes, &vec![0; too_many_node_refs.len()]),
            Err(DualNetworkSimplexError::AdmissionLimit)
        );

        let edge_storage = (0..=DUAL_NETWORK_SIMPLEX_MAX_EDGES)
            .map(|index| {
                let (from, to) = if index % 2 == 0 {
                    ("a", "b")
                } else {
                    ("b", "a")
                };
                (format!("p{index:03}"), from, to)
            })
            .collect::<Vec<_>>();
        let at_edge_limit = edge_storage[..DUAL_NETWORK_SIMPLEX_MAX_EDGES]
            .iter()
            .map(|(id, from, to)| (id.as_str(), *from, *to, 1, 0))
            .collect::<Vec<_>>();
        solve_dual_network_simplex(&graph(&["a", "b"], &at_edge_limit), &[1, -1])
            .expect("edge limit is inclusive");
        let over_edge_limit = edge_storage
            .iter()
            .map(|(id, from, to)| (id.as_str(), *from, *to, 1, 0))
            .collect::<Vec<_>>();
        assert_eq!(
            solve_dual_network_simplex(&graph(&["a", "b"], &over_edge_limit), &[1, -1]),
            Err(DualNetworkSimplexError::AdmissionLimit)
        );
    }
}
