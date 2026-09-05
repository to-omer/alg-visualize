//! Primal network simplex with artificial-root initialization and block pricing.

use num_bigint::BigInt;
use num_traits::ToPrimitive;
use thiserror::Error;

use super::data_structures::link_cut::{
    DynamicTreeEdge, DynamicTreeVertex, LinkCutError, LinkCutForest,
};
use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot,
};

/// Conservative interactive node limit for the natural primal network simplex.
pub const NETWORK_SIMPLEX_MAX_NODES: usize = 256;
/// Conservative interactive edge limit for the natural primal network simplex.
pub const NETWORK_SIMPLEX_MAX_EDGES: usize = 2_048;
/// Deterministic ceiling on simplex pivots, including degenerate pivots.
pub const NETWORK_SIMPLEX_MAX_PIVOTS: u64 = 250_000;
/// Deterministic ceiling on priced original arcs.
pub const NETWORK_SIMPLEX_MAX_PRICING_ARC_SCANS: u128 = 8_000_000;

/// Exact deterministic counters from the natural primal network simplex.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NetworkSimplexMetrics {
    /// Complete block-pricing searches, including the final optimality scan.
    pub pricing_searches: u64,
    /// Original non-tree arcs inspected by block pricing.
    pub pricing_arc_scans: u128,
    /// Basic cycles selected and pivoted.
    pub pivots: u64,
    /// Pivots that changed at least one flow value.
    pub nondegenerate_pivots: u64,
    /// Zero-augmentation pivots that changed only the basis.
    pub degenerate_pivots: u64,
    /// Pivots that exchanged an entering and leaving tree arc.
    pub basis_exchanges: u64,
    /// Pivots whose entering arc crossed directly from one bound to the other.
    pub bound_flips: u64,
    /// Tree arcs inspected while finding a basic-cycle bottleneck.
    pub cycle_arc_scans: u128,
    /// Rooted-tree reconstructions after basis exchanges, including initialization.
    pub tree_rebuilds: u64,
    /// Full tree-potential reconstructions, including initialization.
    pub potential_recomputations: u64,
}

/// Exact counters for the bounded link-cut-tree network-simplex variant.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DynamicTreeNetworkSimplexMetrics {
    /// Counters shared with the natural primal network-simplex pivot rule.
    pub simplex: NetworkSimplexMetrics,
    /// Link-cut path-minimum queries used to obtain cycle bottlenecks.
    pub path_minimum_queries: u64,
    /// Lazy link-cut path updates applied to directional residual capacities.
    pub path_updates: u64,
    /// Leaving basis arcs cut from both directional forests.
    pub tree_cuts: u64,
    /// Entering basis arcs linked into both directional forests.
    pub tree_links: u64,
    /// Full directional-forest constructions, including initialization.
    ///
    /// This correctness-first visual kernel intentionally retains the natural
    /// implementation's explicit rooted-tree and potential reconstruction.
    pub directional_forest_rebuilds: u64,
    /// Full comparisons of directional link-cut values with explicit flows.
    pub directional_value_validations: u64,
}

/// Certified natural primal-network-simplex result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSimplexResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: NetworkSimplexMetrics,
    /// Strict artificial-arc cost used by the initial star basis.
    pub artificial_cost: i128,
    /// Number of original arcs examined in one pricing block.
    pub pricing_block_size: usize,
}

/// Certified result with reversible pricing, cycle, and basis-exchange events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkSimplexTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: NetworkSimplexResult,
    /// Replay boundary at the artificial-root star basis.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after exact optimality certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Certified minimum-cost flow from the bounded dynamic-tree pivot kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeNetworkSimplexResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed primal/dual optimum certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact natural-pivot and link-cut operation counters.
    pub metrics: DynamicTreeNetworkSimplexMetrics,
    /// Strict artificial-arc cost used by the initial star basis.
    pub artificial_cost: i128,
    /// Number of original arcs examined in one pricing block.
    pub pricing_block_size: usize,
}

/// Certified dynamic-tree result with a reversible semantic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicTreeNetworkSimplexTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: DynamicTreeNetworkSimplexResult,
    /// Replay boundary at the artificial-root star basis.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible dynamic-tree pivot sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after exact optimality certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Network-simplex construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum NetworkSimplexError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds primal network-simplex admission limits")]
    AdmissionLimit,
    /// A deterministic pivot or pricing-scan ceiling was reached.
    #[error("primal network-simplex work limit reached")]
    WorkLimit,
    /// No original flow satisfies the requested balances and bounds.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// A trace snapshot could not reconstruct the original residual network.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Final independent primal/dual certification failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded the declared domain.
    #[error("primal network-simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// The tree partition, rooted representation, or tree potentials disagreed.
    #[error("primal network-simplex basis invariant failed")]
    BasisInvariant,
    /// The leaving-arc tie rule failed to preserve a strongly feasible tree.
    #[error("primal network-simplex strong-feasibility invariant failed")]
    StrongFeasibility,
    /// An artificial arc retained positive flow despite the feasibility precheck.
    #[error("primal network-simplex terminated with artificial flow")]
    ArtificialFlow,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced minimum-cost flow by the natural primal network simplex.
///
/// The kernel starts from a strongly feasible artificial-root star basis, uses
/// cyclic square-root block pricing, and applies the asymmetric leaving-arc tie
/// rule used by practical network-simplex implementations. Tree labels and
/// potentials are rebuilt in linear time after each basis exchange; this keeps
/// the educational implementation explicit rather than hiding the pivot in a
/// threaded-tree data structure.
///
/// # Errors
///
/// Rejects admission, infeasibility, arithmetic, work-limit, basis-invariant,
/// artificial-flow, trace, or independent certificate failures.
pub fn solve_primal_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<NetworkSimplexResult, NetworkSimplexError> {
    solve_internal(graph, required_divergence, false, SimplexMode::Explicit).map(|run| run.result)
}

/// Solves primal network simplex while reporting its feasibility precheck to
/// the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_primal_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<NetworkSimplexResult, NetworkSimplexError> {
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        SimplexMode::Explicit,
        feasibility,
    )
    .map(|run| run.result)
}

/// Records every pricing, basic-cycle, augmentation, and basis-update boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_primal_network_simplex`] plus trace
/// transaction failures.
pub fn trace_primal_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<NetworkSimplexTraceResult, NetworkSimplexError> {
    let run = solve_internal(graph, required_divergence, true, SimplexMode::Explicit)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(NetworkSimplexError::BasisInvariant)?;
    Ok(NetworkSimplexTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Solves balanced minimum-cost flow using directional link-cut cycle pivots.
///
/// The kernel uses dynamic trees for cycle minima, lazy directional residual
/// updates, and basis cut/link operations. It deliberately rebuilds the rooted
/// tree and potentials explicitly after an exchange, so it does not claim the
/// complete `O(log n)`-per-pivot implementation described for more elaborate
/// dual data structures.
///
/// # Errors
///
/// Returns the same bounded construction and certification failures as
/// [`solve_primal_network_simplex`].
pub fn solve_dynamic_tree_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DynamicTreeNetworkSimplexResult, NetworkSimplexError> {
    let run = solve_internal(graph, required_divergence, false, SimplexMode::DynamicTree)?;
    Ok(dynamic_result(run.result, run.dynamic_metrics))
}

/// Solves the dynamic-tree variant while reporting its feasibility precheck to
/// the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_dynamic_tree_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreeNetworkSimplexResult, NetworkSimplexError> {
    let run = solve_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        SimplexMode::DynamicTree,
        feasibility,
    )?;
    Ok(dynamic_result(run.result, run.dynamic_metrics))
}

/// Records every dynamic-tree pricing, cycle, update, and exchange boundary.
///
/// # Errors
///
/// Returns the same failures as [`solve_dynamic_tree_network_simplex`], plus
/// reversible-trace transaction failures.
pub fn trace_dynamic_tree_network_simplex(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<DynamicTreeNetworkSimplexTraceResult, NetworkSimplexError> {
    let run = solve_internal(graph, required_divergence, true, SimplexMode::DynamicTree)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(NetworkSimplexError::BasisInvariant)?;
    Ok(DynamicTreeNetworkSimplexTraceResult {
        result: dynamic_result(run.result, run.dynamic_metrics),
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces primal network simplex while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_primal_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<NetworkSimplexTraceResult, NetworkSimplexError> {
    let run = solve_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        SimplexMode::Explicit,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(NetworkSimplexError::BasisInvariant)?;
    Ok(NetworkSimplexTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces dynamic-tree network simplex while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_dynamic_tree_network_simplex_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<DynamicTreeNetworkSimplexTraceResult, NetworkSimplexError> {
    let run = solve_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        SimplexMode::DynamicTree,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(NetworkSimplexError::BasisInvariant)?;
    Ok(DynamicTreeNetworkSimplexTraceResult {
        result: dynamic_result(run.result, run.dynamic_metrics),
        base_snapshot,
        events,
        final_snapshot,
    })
}

fn dynamic_result(
    result: NetworkSimplexResult,
    metrics: DynamicTreeNetworkSimplexMetrics,
) -> DynamicTreeNetworkSimplexResult {
    DynamicTreeNetworkSimplexResult {
        flows: result.flows,
        certificate: result.certificate,
        metrics,
        artificial_cost: result.artificial_cost,
        pricing_block_size: result.pricing_block_size,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimplexMode {
    Explicit,
    DynamicTree,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArcState {
    Lower,
    Tree,
    Upper,
}

impl ArcState {
    const fn residual_sign(self) -> i128 {
        match self {
            Self::Lower => 1,
            Self::Tree => 0,
            Self::Upper => -1,
        }
    }
}

#[derive(Clone, Debug)]
struct ArcData {
    source: usize,
    target: usize,
    capacity: i128,
    cost: i128,
    flow: i128,
    state: ArcState,
    original: Option<usize>,
}

#[derive(Clone, Debug)]
struct RootedTree {
    root: usize,
    parent: Vec<Option<usize>>,
    parent_arc: Vec<Option<usize>>,
    depth: Vec<usize>,
    potentials: Vec<i128>,
}

struct DirectionalDynamicBasis {
    upward: LinkCutForest,
    downward: LinkCutForest,
    vertices: Vec<DynamicTreeVertex>,
    edges: Vec<DynamicTreeEdge>,
}

impl DirectionalDynamicBasis {
    fn build(
        arcs: &[ArcData],
        tree: &RootedTree,
        node_count: usize,
    ) -> Result<Self, NetworkSimplexError> {
        let mut upward = LinkCutForest::new(node_count, arcs.len());
        let mut downward = LinkCutForest::new(node_count, arcs.len());
        let vertices = (0..node_count)
            .map(|index| dynamic_tree(upward.vertex(index)))
            .collect::<Result<Vec<_>, _>>()?;
        let edges = (0..arcs.len())
            .map(|index| dynamic_tree(upward.edge(index)))
            .collect::<Result<Vec<_>, _>>()?;
        for child in 0..node_count {
            let (Some(parent), Some(arc_index)) = (tree.parent[child], tree.parent_arc[child])
            else {
                continue;
            };
            let up = directed_residual(&arcs[arc_index], child, parent)?;
            let down = directed_residual(&arcs[arc_index], parent, child)?;
            dynamic_tree(upward.link(
                edges[arc_index],
                vertices[child],
                vertices[parent],
                BigInt::from(up),
            ))?;
            dynamic_tree(downward.link(
                edges[arc_index],
                vertices[child],
                vertices[parent],
                BigInt::from(down),
            ))?;
        }
        Ok(Self {
            upward,
            downward,
            vertices,
            edges,
        })
    }

    fn upward_minimum(
        &mut self,
        node: usize,
        ancestor: usize,
    ) -> Result<Option<i128>, NetworkSimplexError> {
        self.path_minimum(true, node, ancestor)
    }

    fn downward_minimum(
        &mut self,
        node: usize,
        ancestor: usize,
    ) -> Result<Option<i128>, NetworkSimplexError> {
        self.path_minimum(false, node, ancestor)
    }

    fn path_minimum(
        &mut self,
        upward: bool,
        left: usize,
        right: usize,
    ) -> Result<Option<i128>, NetworkSimplexError> {
        if left == right {
            return Ok(None);
        }
        let forest = if upward {
            &mut self.upward
        } else {
            &mut self.downward
        };
        let minimum = dynamic_tree(forest.path_minimum(self.vertices[left], self.vertices[right]))?
            .ok_or(NetworkSimplexError::BasisInvariant)?;
        minimum
            .value
            .to_i128()
            .map(Some)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)
    }

    fn update_first_branch(
        &mut self,
        node: usize,
        ancestor: usize,
        delta: i128,
    ) -> Result<(), NetworkSimplexError> {
        if node == ancestor {
            return Ok(());
        }
        dynamic_tree(self.downward.path_add(
            self.vertices[node],
            self.vertices[ancestor],
            &BigInt::from(-delta),
        ))?;
        dynamic_tree(self.upward.path_add(
            self.vertices[node],
            self.vertices[ancestor],
            &BigInt::from(delta),
        ))
    }

    fn update_second_branch(
        &mut self,
        node: usize,
        ancestor: usize,
        delta: i128,
    ) -> Result<(), NetworkSimplexError> {
        if node == ancestor {
            return Ok(());
        }
        dynamic_tree(self.upward.path_add(
            self.vertices[node],
            self.vertices[ancestor],
            &BigInt::from(-delta),
        ))?;
        dynamic_tree(self.downward.path_add(
            self.vertices[node],
            self.vertices[ancestor],
            &BigInt::from(delta),
        ))
    }

    fn cut(&mut self, arc_index: usize) -> Result<(), NetworkSimplexError> {
        dynamic_tree(self.upward.cut(self.edges[arc_index]))?;
        dynamic_tree(self.downward.cut(self.edges[arc_index]))?;
        Ok(())
    }

    fn link(
        &mut self,
        arc_index: usize,
        source: usize,
        target: usize,
        forward_residual: i128,
        reverse_residual: i128,
    ) -> Result<(), NetworkSimplexError> {
        dynamic_tree(self.upward.link(
            self.edges[arc_index],
            self.vertices[source],
            self.vertices[target],
            BigInt::from(forward_residual),
        ))?;
        dynamic_tree(self.downward.link(
            self.edges[arc_index],
            self.vertices[source],
            self.vertices[target],
            BigInt::from(reverse_residual),
        ))?;
        Ok(())
    }

    fn validate(&mut self, arcs: &[ArcData], tree: &RootedTree) -> Result<(), NetworkSimplexError> {
        for child in 0..tree.parent.len() {
            let (Some(parent), Some(arc_index)) = (tree.parent[child], tree.parent_arc[child])
            else {
                continue;
            };
            let stored_up = dynamic_tree(self.upward.edge_value(self.edges[arc_index]))?
                .to_i128()
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
            let stored_down = dynamic_tree(self.downward.edge_value(self.edges[arc_index]))?
                .to_i128()
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
            if stored_up != directed_residual(&arcs[arc_index], child, parent)?
                || stored_down != directed_residual(&arcs[arc_index], parent, child)?
            {
                return Err(NetworkSimplexError::BasisInvariant);
            }
        }
        Ok(())
    }
}

struct WorkingState<'graph> {
    graph: &'graph FlowNetwork,
    required_divergence: Vec<i128>,
    arcs: Vec<ArcData>,
    original_arc_count: usize,
    tree: RootedTree,
    pricing_cursor: usize,
    pricing_block_size: usize,
    artificial_cost: i128,
    metrics: NetworkSimplexMetrics,
    mode: SimplexMode,
    dynamic_basis: Option<DirectionalDynamicBasis>,
    dynamic_metrics: DynamicTreeNetworkSimplexMetrics,
}

struct InternalRun {
    result: NetworkSimplexResult,
    dynamic_metrics: DynamicTreeNetworkSimplexMetrics,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    mode: SimplexMode,
) -> Result<InternalRun, NetworkSimplexError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(
        graph,
        required_divergence,
        trace_enabled,
        mode,
        &mut feasibility,
    )
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_enabled: bool,
    mode: SimplexMode,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, NetworkSimplexError> {
    validate_admission(graph)?;
    // This precheck provides the existing cut-based infeasibility witness and
    // lets the simplex kernel treat positive artificial flow as an invariant
    // failure rather than as its public infeasibility protocol.
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let mut work = WorkingState::initialize(graph, required_divergence, mode)?;
    let mut recorder = start_trace_recorder(graph, &work, trace_enabled)?;

    record_trace(
        recorder.as_mut(),
        &work,
        EventKind::Initialize.metadata(work.mode),
        TraceView::tree(&work),
        Some(("artificial-flow", work.total_artificial_flow()?)),
    )?;

    execute_pivots(&mut work, &mut recorder)?;

    if work.total_artificial_flow()? != 0 {
        return Err(NetworkSimplexError::ArtificialFlow);
    }
    work.validate_basis()?;
    let flows = work.original_flows()?;
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_trace(
        recorder.as_mut(),
        &work,
        EventKind::Optimal.metadata(work.mode),
        TraceView::tree(&work),
        Some(("total-cost", certificate.total_cost)),
    )?;
    let result = NetworkSimplexResult {
        flows,
        certificate,
        metrics: work.metrics,
        artificial_cost: work.artificial_cost,
        pricing_block_size: work.pricing_block_size,
    };
    Ok(InternalRun {
        result,
        dynamic_metrics: DynamicTreeNetworkSimplexMetrics {
            simplex: work.metrics,
            ..work.dynamic_metrics
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn execute_pivots(
    work: &mut WorkingState<'_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), NetworkSimplexError> {
    loop {
        let pricing = work.find_entering_arc(recorder)?;
        let pricing_detail = pricing
            .entering
            .map(|arc| {
                work.entering_residual_reduced_cost(arc)
                    .map(|cost| ("reduced-cost", cost))
            })
            .transpose()?;
        record_trace(
            recorder.as_mut(),
            work,
            EventKind::Price.metadata(work.mode),
            TraceView::priced(work, pricing.scanned_nodes),
            pricing_detail,
        )?;

        let Some(entering) = pricing.entering else {
            return Ok(());
        };
        if work.metrics.pivots >= NETWORK_SIMPLEX_MAX_PIVOTS {
            return Err(NetworkSimplexError::WorkLimit);
        }

        let cycle = work.basic_cycle(entering)?;
        record_trace(
            recorder.as_mut(),
            work,
            EventKind::FormCycle.metadata(work.mode),
            TraceView::cycle(work, &cycle),
            Some(("cycle-cost", cycle.residual_reduced_cost)),
        )?;

        let pivot = work.pivot(entering, &cycle)?;

        if pivot.basis_changed {
            record_trace(
                recorder.as_mut(),
                work,
                EventKind::ExchangeBasis.metadata(work.mode),
                TraceView::tree(work),
                Some(("delta", pivot.delta)),
            )?;
        } else {
            record_trace(
                recorder.as_mut(),
                work,
                EventKind::FlipBound.metadata(work.mode),
                TraceView::tree(work),
                Some(("delta", pivot.delta)),
            )?;
        }
    }
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), NetworkSimplexError> {
    if graph.nodes().len() > NETWORK_SIMPLEX_MAX_NODES
        || graph.edges().len() > NETWORK_SIMPLEX_MAX_EDGES
    {
        return Err(NetworkSimplexError::AdmissionLimit);
    }
    Ok(())
}

impl<'graph> WorkingState<'graph> {
    fn initialize(
        graph: &'graph FlowNetwork,
        required_divergence: &[i128],
        mode: SimplexMode,
    ) -> Result<Self, NetworkSimplexError> {
        let node_count = graph.nodes().len();
        let original_arc_count = graph.edges().len();
        let root = node_count;
        let lower_flows = graph
            .edges()
            .iter()
            .map(crate::model::FlowEdge::lower)
            .collect::<Vec<_>>();
        let lower_divergence = divergences(graph, &lower_flows)?;
        let transformed_balance = required_divergence
            .iter()
            .zip(lower_divergence)
            .map(|(&required, lower)| {
                required
                    .checked_sub(lower)
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let max_abs_cost = graph.edges().iter().try_fold(0_i128, |maximum, edge| {
            let cost = i128::from(edge.cost()).abs();
            Ok::<_, NetworkSimplexError>(maximum.max(cost))
        })?;
        let artificial_cost = i128::try_from(node_count)
            .map_err(|_| NetworkSimplexError::ArithmeticOverflow)?
            .checked_add(1)
            .and_then(|factor| max_abs_cost.checked_add(1)?.checked_mul(factor))
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        let total_balance = transformed_balance.iter().try_fold(0_i128, |sum, value| {
            sum.checked_add(
                value
                    .checked_abs()
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)?,
            )
            .ok_or(NetworkSimplexError::ArithmeticOverflow)
        })?;
        let total_capacity = graph.edges().iter().try_fold(0_i128, |sum, edge| {
            sum.checked_add(i128::from(edge.capacity() - edge.lower()))
                .ok_or(NetworkSimplexError::ArithmeticOverflow)
        })?;
        let artificial_capacity = total_balance
            .checked_add(total_capacity)
            .and_then(|value| value.checked_add(1))
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;

        let mut arcs = original_arcs(graph);
        arcs.reserve(node_count);
        for (node, &balance) in transformed_balance.iter().enumerate() {
            let (source, target, flow) = if balance >= 0 {
                (node, root, balance)
            } else {
                (
                    root,
                    node,
                    balance
                        .checked_neg()
                        .ok_or(NetworkSimplexError::ArithmeticOverflow)?,
                )
            };
            arcs.push(ArcData {
                source,
                target,
                capacity: artificial_capacity,
                cost: artificial_cost,
                flow,
                state: ArcState::Tree,
                original: None,
            });
        }

        let pricing_block_size = integer_ceil_sqrt(original_arc_count);
        let mut work = Self {
            graph,
            required_divergence: required_divergence.to_vec(),
            arcs,
            original_arc_count,
            tree: RootedTree {
                root,
                parent: Vec::new(),
                parent_arc: Vec::new(),
                depth: Vec::new(),
                potentials: Vec::new(),
            },
            pricing_cursor: 0,
            pricing_block_size,
            artificial_cost,
            metrics: NetworkSimplexMetrics::default(),
            mode,
            dynamic_basis: None,
            dynamic_metrics: DynamicTreeNetworkSimplexMetrics::default(),
        };
        work.rebuild_tree_and_potentials()?;
        work.rebuild_directional_dynamic_basis()?;
        work.validate_basis()?;
        Ok(work)
    }

    fn rebuild_directional_dynamic_basis(&mut self) -> Result<(), NetworkSimplexError> {
        if self.mode == SimplexMode::Explicit {
            self.dynamic_basis = None;
            return Ok(());
        }
        self.dynamic_basis = Some(DirectionalDynamicBasis::build(
            &self.arcs,
            &self.tree,
            self.graph.nodes().len() + 1,
        )?);
        self.dynamic_metrics.directional_forest_rebuilds = self
            .dynamic_metrics
            .directional_forest_rebuilds
            .checked_add(1)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        self.validate_directional_dynamic_basis()
    }

    fn dynamic_cycle_bottleneck(
        &mut self,
        cycle: &BasicCycle,
        entering_residual: i128,
    ) -> Result<Option<i128>, NetworkSimplexError> {
        let Some(basis) = self.dynamic_basis.as_mut() else {
            return Ok(None);
        };
        let mut bottleneck = entering_residual;
        if let Some(value) = basis.downward_minimum(cycle.first, cycle.join)? {
            bottleneck = bottleneck.min(value);
            self.dynamic_metrics.path_minimum_queries = self
                .dynamic_metrics
                .path_minimum_queries
                .checked_add(1)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        if let Some(value) = basis.upward_minimum(cycle.second, cycle.join)? {
            bottleneck = bottleneck.min(value);
            self.dynamic_metrics.path_minimum_queries = self
                .dynamic_metrics
                .path_minimum_queries
                .checked_add(1)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        Ok(Some(bottleneck))
    }

    fn update_dynamic_cycle_paths(
        &mut self,
        cycle: &BasicCycle,
        delta: i128,
    ) -> Result<(), NetworkSimplexError> {
        let Some(basis) = self.dynamic_basis.as_mut() else {
            return Ok(());
        };
        if cycle.first != cycle.join {
            basis.update_first_branch(cycle.first, cycle.join, delta)?;
            self.dynamic_metrics.path_updates = self
                .dynamic_metrics
                .path_updates
                .checked_add(1)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        if cycle.second != cycle.join {
            basis.update_second_branch(cycle.second, cycle.join, delta)?;
            self.dynamic_metrics.path_updates = self
                .dynamic_metrics
                .path_updates
                .checked_add(1)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        Ok(())
    }

    fn exchange_dynamic_basis(
        &mut self,
        leaving: usize,
        entering: usize,
    ) -> Result<(), NetworkSimplexError> {
        let Some(basis) = self.dynamic_basis.as_mut() else {
            return Ok(());
        };
        basis.cut(leaving)?;
        self.dynamic_metrics.tree_cuts = self
            .dynamic_metrics
            .tree_cuts
            .checked_add(1)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        let arc = &self.arcs[entering];
        let forward = directed_residual(arc, arc.source, arc.target)?;
        let reverse = directed_residual(arc, arc.target, arc.source)?;
        basis.link(entering, arc.source, arc.target, forward, reverse)?;
        self.dynamic_metrics.tree_links = self
            .dynamic_metrics
            .tree_links
            .checked_add(1)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        Ok(())
    }

    fn validate_directional_dynamic_basis(&mut self) -> Result<(), NetworkSimplexError> {
        let Some(basis) = self.dynamic_basis.as_mut() else {
            return Ok(());
        };
        basis.validate(&self.arcs, &self.tree)?;
        self.dynamic_metrics.directional_value_validations = self
            .dynamic_metrics
            .directional_value_validations
            .checked_add(1)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        Ok(())
    }

    fn find_entering_arc(
        &mut self,
        recorder: &mut Option<FlowTraceRecorder<'_>>,
    ) -> Result<PricingResult, NetworkSimplexError> {
        self.metrics.pricing_searches = self.metrics.pricing_searches.saturating_add(1);
        if self.original_arc_count == 0 {
            return Ok(PricingResult {
                entering: None,
                scanned_nodes: Vec::new(),
            });
        }
        let mut inspected = 0_usize;
        let mut scanned_nodes = Vec::new();
        while inspected < self.original_arc_count {
            let block_len = self
                .pricing_block_size
                .min(self.original_arc_count - inspected);
            let mut best: Option<(usize, i128)> = None;
            for offset in 0..block_len {
                let arc_index = (self.pricing_cursor + offset) % self.original_arc_count;
                self.bump_pricing_scan()?;
                let arc = &self.arcs[arc_index];
                scanned_nodes.push(arc.source);
                scanned_nodes.push(arc.target);
                let residual_reduced = self.entering_residual_reduced_cost(arc_index)?;
                if self.metrics.pricing_arc_scans.is_power_of_two() {
                    let arc = &self.arcs[arc_index];
                    let endpoints = match arc.state {
                        ArcState::Lower => Some((arc.source, arc.target)),
                        ArcState::Upper => Some((arc.target, arc.source)),
                        ArcState::Tree if arc.flow < arc.capacity => Some((arc.source, arc.target)),
                        ArcState::Tree if arc.flow > 0 => Some((arc.target, arc.source)),
                        ArcState::Tree => None,
                    };
                    let active = endpoints
                        .map(|(from, to)| self.original_residual_id(arc_index, from, to))
                        .transpose()?;
                    record_trace(
                        recorder.as_mut(),
                        self,
                        EventKind::InspectPrice.metadata(self.mode),
                        TraceView::inspect_price(self, arc_index, active),
                        Some(("reduced-cost", residual_reduced)),
                    )?;
                }
                if residual_reduced < 0
                    && best.is_none_or(|(best_index, best_cost)| {
                        residual_reduced < best_cost
                            || (residual_reduced == best_cost && arc_index < best_index)
                    })
                {
                    best = Some((arc_index, residual_reduced));
                }
            }
            inspected += block_len;
            self.pricing_cursor = (self.pricing_cursor + block_len) % self.original_arc_count;
            if let Some((entering, _)) = best {
                return Ok(PricingResult {
                    entering: Some(entering),
                    scanned_nodes,
                });
            }
        }
        Ok(PricingResult {
            entering: None,
            scanned_nodes,
        })
    }

    fn bump_pricing_scan(&mut self) -> Result<(), NetworkSimplexError> {
        if self.metrics.pricing_arc_scans >= NETWORK_SIMPLEX_MAX_PRICING_ARC_SCANS {
            return Err(NetworkSimplexError::WorkLimit);
        }
        self.metrics.pricing_arc_scans += 1;
        Ok(())
    }

    fn entering_residual_reduced_cost(
        &self,
        arc_index: usize,
    ) -> Result<i128, NetworkSimplexError> {
        let arc = self
            .arcs
            .get(arc_index)
            .ok_or(NetworkSimplexError::BasisInvariant)?;
        let reduced = arc
            .cost
            .checked_add(self.tree.potentials[arc.source])
            .and_then(|value| value.checked_sub(self.tree.potentials[arc.target]))
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        reduced
            .checked_mul(arc.state.residual_sign())
            .ok_or(NetworkSimplexError::ArithmeticOverflow)
    }

    fn basic_cycle(&mut self, entering: usize) -> Result<BasicCycle, NetworkSimplexError> {
        let arc = self
            .arcs
            .get(entering)
            .ok_or(NetworkSimplexError::BasisInvariant)?;
        if arc.state == ArcState::Tree || arc.original.is_none() {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        let (first, second, entering_forward) = match arc.state {
            ArcState::Lower => (arc.source, arc.target, true),
            ArcState::Upper => (arc.target, arc.source, false),
            ArcState::Tree => return Err(NetworkSimplexError::BasisInvariant),
        };
        let join = self.lowest_common_ancestor(first, second)?;
        let mut first_branch = Vec::new();
        let mut node = first;
        while node != join {
            let tree_arc = self.tree.parent_arc[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            first_branch.push((node, tree_arc));
            node = self.tree.parent[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            self.metrics.cycle_arc_scans += 1;
        }
        let mut second_branch = Vec::new();
        node = second;
        while node != join {
            let tree_arc = self.tree.parent_arc[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            second_branch.push((node, tree_arc));
            node = self.tree.parent[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            self.metrics.cycle_arc_scans += 1;
        }
        let residual_reduced_cost = self.entering_residual_reduced_cost(entering)?;
        if residual_reduced_cost >= 0 {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        let mut original_path = Vec::new();
        original_path.push(self.original_residual_id(entering, first, second)?);
        for &(child, arc_index) in &second_branch {
            let parent = self.tree.parent[child].ok_or(NetworkSimplexError::BasisInvariant)?;
            if let Some(id) = self.original_residual_id_optional(arc_index, child, parent)? {
                original_path.push(id);
            }
        }
        for &(child, arc_index) in first_branch.iter().rev() {
            let parent = self.tree.parent[child].ok_or(NetworkSimplexError::BasisInvariant)?;
            if let Some(id) = self.original_residual_id_optional(arc_index, parent, child)? {
                original_path.push(id);
            }
        }
        Ok(BasicCycle {
            entering,
            entering_forward,
            first,
            second,
            join,
            first_branch,
            second_branch,
            residual_reduced_cost,
            original_path,
        })
    }

    fn original_residual_id(
        &self,
        arc_index: usize,
        from: usize,
        to: usize,
    ) -> Result<ResidualArcId, NetworkSimplexError> {
        self.original_residual_id_optional(arc_index, from, to)?
            .ok_or(NetworkSimplexError::BasisInvariant)
    }

    fn original_residual_id_optional(
        &self,
        arc_index: usize,
        from: usize,
        to: usize,
    ) -> Result<Option<ResidualArcId>, NetworkSimplexError> {
        let arc = self
            .arcs
            .get(arc_index)
            .ok_or(NetworkSimplexError::BasisInvariant)?;
        let direction = if arc.source == from && arc.target == to {
            ResidualDirection::Forward
        } else if arc.source == to && arc.target == from {
            ResidualDirection::Reverse
        } else {
            return Err(NetworkSimplexError::BasisInvariant);
        };
        Ok(arc.original.map(|original| {
            ResidualArcId::new(self.graph.edges()[original].id().clone(), direction)
        }))
    }

    fn lowest_common_ancestor(
        &self,
        mut left: usize,
        mut right: usize,
    ) -> Result<usize, NetworkSimplexError> {
        while self.tree.depth[left] > self.tree.depth[right] {
            left = self.tree.parent[left].ok_or(NetworkSimplexError::BasisInvariant)?;
        }
        while self.tree.depth[right] > self.tree.depth[left] {
            right = self.tree.parent[right].ok_or(NetworkSimplexError::BasisInvariant)?;
        }
        while left != right {
            left = self.tree.parent[left].ok_or(NetworkSimplexError::BasisInvariant)?;
            right = self.tree.parent[right].ok_or(NetworkSimplexError::BasisInvariant)?;
        }
        Ok(left)
    }

    fn pivot(
        &mut self,
        entering: usize,
        cycle: &BasicCycle,
    ) -> Result<PivotResult, NetworkSimplexError> {
        if cycle.entering != entering {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        let entering_residual = self.entering_residual_capacity(entering)?;
        let dynamic_bottleneck = self.dynamic_cycle_bottleneck(cycle, entering_residual)?;
        let mut delta = entering_residual;
        let mut leaving = None;

        // The strict comparison on the first branch and non-strict comparison
        // on the second branch are the strong-feasibility tie rule.
        for &(child, arc_index) in &cycle.first_branch {
            let residual = self.tree_path_residual(child, false)?;
            if residual < delta {
                delta = residual;
                leaving = Some(arc_index);
            }
        }
        for &(child, arc_index) in &cycle.second_branch {
            let residual = self.tree_path_residual(child, true)?;
            if residual <= delta {
                delta = residual;
                leaving = Some(arc_index);
            }
        }
        if delta < 0 {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        if dynamic_bottleneck.is_some_and(|minimum| minimum != delta) {
            return Err(NetworkSimplexError::BasisInvariant);
        }

        self.update_dynamic_cycle_paths(cycle, delta)?;
        self.augment_arc(entering, cycle.entering_forward, delta)?;
        for &(child, arc_index) in &cycle.second_branch {
            let parent = self.tree.parent[child].ok_or(NetworkSimplexError::BasisInvariant)?;
            self.augment_directed(arc_index, child, parent, delta)?;
        }
        for &(child, arc_index) in cycle.first_branch.iter().rev() {
            let parent = self.tree.parent[child].ok_or(NetworkSimplexError::BasisInvariant)?;
            self.augment_directed(arc_index, parent, child, delta)?;
        }

        self.metrics.pivots = self.metrics.pivots.saturating_add(1);
        if delta == 0 {
            self.metrics.degenerate_pivots = self.metrics.degenerate_pivots.saturating_add(1);
        } else {
            self.metrics.nondegenerate_pivots = self.metrics.nondegenerate_pivots.saturating_add(1);
        }

        let basis_changed = if let Some(leaving_arc) = leaving {
            self.exchange_dynamic_basis(leaving_arc, entering)?;
            self.arcs[entering].state = ArcState::Tree;
            self.arcs[leaving_arc].state = if self.arcs[leaving_arc].flow == 0 {
                ArcState::Lower
            } else if self.arcs[leaving_arc].flow == self.arcs[leaving_arc].capacity {
                ArcState::Upper
            } else {
                return Err(NetworkSimplexError::BasisInvariant);
            };
            self.metrics.basis_exchanges = self.metrics.basis_exchanges.saturating_add(1);
            self.rebuild_tree_and_potentials()?;
            self.rebuild_directional_dynamic_basis()?;
            true
        } else {
            let state = self.arcs[entering].state;
            self.arcs[entering].state = match state {
                ArcState::Lower => ArcState::Upper,
                ArcState::Upper => ArcState::Lower,
                ArcState::Tree => return Err(NetworkSimplexError::BasisInvariant),
            };
            self.metrics.bound_flips = self.metrics.bound_flips.saturating_add(1);
            self.validate_directional_dynamic_basis()?;
            false
        };
        self.validate_basis()?;
        Ok(PivotResult {
            delta,
            basis_changed,
        })
    }

    fn entering_residual_capacity(&self, arc_index: usize) -> Result<i128, NetworkSimplexError> {
        let arc = &self.arcs[arc_index];
        match arc.state {
            ArcState::Lower => arc
                .capacity
                .checked_sub(arc.flow)
                .ok_or(NetworkSimplexError::ArithmeticOverflow),
            ArcState::Upper => Ok(arc.flow),
            ArcState::Tree => Err(NetworkSimplexError::BasisInvariant),
        }
    }

    fn tree_path_residual(
        &self,
        child: usize,
        child_to_parent: bool,
    ) -> Result<i128, NetworkSimplexError> {
        let parent = self.tree.parent[child].ok_or(NetworkSimplexError::BasisInvariant)?;
        let arc_index = self.tree.parent_arc[child].ok_or(NetworkSimplexError::BasisInvariant)?;
        let (from, to) = if child_to_parent {
            (child, parent)
        } else {
            (parent, child)
        };
        self.directed_residual(arc_index, from, to)
    }

    fn directed_residual(
        &self,
        arc_index: usize,
        from: usize,
        to: usize,
    ) -> Result<i128, NetworkSimplexError> {
        directed_residual(&self.arcs[arc_index], from, to)
    }

    fn augment_directed(
        &mut self,
        arc_index: usize,
        from: usize,
        to: usize,
        delta: i128,
    ) -> Result<(), NetworkSimplexError> {
        let forward = {
            let arc = &self.arcs[arc_index];
            if arc.source == from && arc.target == to {
                true
            } else if arc.source == to && arc.target == from {
                false
            } else {
                return Err(NetworkSimplexError::BasisInvariant);
            }
        };
        self.augment_arc(arc_index, forward, delta)
    }

    fn augment_arc(
        &mut self,
        arc_index: usize,
        forward: bool,
        delta: i128,
    ) -> Result<(), NetworkSimplexError> {
        let arc = self
            .arcs
            .get_mut(arc_index)
            .ok_or(NetworkSimplexError::BasisInvariant)?;
        arc.flow = if forward {
            arc.flow.checked_add(delta)
        } else {
            arc.flow.checked_sub(delta)
        }
        .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        if arc.flow < 0 || arc.flow > arc.capacity {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        Ok(())
    }

    fn rebuild_tree_and_potentials(&mut self) -> Result<(), NetworkSimplexError> {
        let node_count = self.graph.nodes().len() + 1;
        let root = self.tree.root;
        let tree_arc_count = self
            .arcs
            .iter()
            .filter(|arc| arc.state == ArcState::Tree)
            .count();
        if tree_arc_count + 1 != node_count {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        let mut adjacency = vec![Vec::<(usize, usize)>::new(); node_count];
        for (arc_index, arc) in self.arcs.iter().enumerate() {
            if arc.state != ArcState::Tree {
                continue;
            }
            if arc.source == arc.target {
                return Err(NetworkSimplexError::BasisInvariant);
            }
            adjacency[arc.source].push((arc.target, arc_index));
            adjacency[arc.target].push((arc.source, arc_index));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable_by_key(|&(node, arc)| (node, arc));
        }
        let mut parent = vec![None; node_count];
        let mut parent_arc = vec![None; node_count];
        let mut depth = vec![0_usize; node_count];
        let mut order = vec![root];
        parent[root] = Some(root);
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
                depth[next] = depth[node]
                    .checked_add(1)
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
                order.push(next);
            }
        }
        if order.len() != node_count {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        parent[root] = None;
        let mut potentials = vec![0_i128; node_count];
        for &node in order.iter().skip(1) {
            let parent_node = parent[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            let arc_index = parent_arc[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            let arc = &self.arcs[arc_index];
            potentials[node] = if arc.source == parent_node && arc.target == node {
                potentials[parent_node].checked_add(arc.cost)
            } else if arc.source == node && arc.target == parent_node {
                potentials[parent_node].checked_sub(arc.cost)
            } else {
                return Err(NetworkSimplexError::BasisInvariant);
            }
            .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        self.tree.parent = parent;
        self.tree.parent_arc = parent_arc;
        self.tree.depth = depth;
        self.tree.potentials = potentials;
        self.metrics.tree_rebuilds = self.metrics.tree_rebuilds.saturating_add(1);
        self.metrics.potential_recomputations =
            self.metrics.potential_recomputations.saturating_add(1);
        Ok(())
    }

    fn validate_basis(&self) -> Result<(), NetworkSimplexError> {
        let node_count = self.graph.nodes().len() + 1;
        if self.tree.parent.len() != node_count
            || self.tree.parent_arc.len() != node_count
            || self.tree.depth.len() != node_count
            || self.tree.potentials.len() != node_count
        {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        let mut extended_divergence = vec![0_i128; node_count];
        for arc in &self.arcs {
            if arc.flow < 0 || arc.flow > arc.capacity {
                return Err(NetworkSimplexError::BasisInvariant);
            }
            match arc.state {
                ArcState::Lower if arc.flow != 0 => {
                    return Err(NetworkSimplexError::BasisInvariant);
                }
                ArcState::Upper if arc.flow != arc.capacity => {
                    return Err(NetworkSimplexError::BasisInvariant);
                }
                ArcState::Tree => {
                    let reduced = arc
                        .cost
                        .checked_add(self.tree.potentials[arc.source])
                        .and_then(|value| value.checked_sub(self.tree.potentials[arc.target]))
                        .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
                    if reduced != 0 {
                        return Err(NetworkSimplexError::BasisInvariant);
                    }
                }
                ArcState::Lower | ArcState::Upper => {}
            }
            extended_divergence[arc.source] = extended_divergence[arc.source]
                .checked_add(arc.flow)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
            extended_divergence[arc.target] = extended_divergence[arc.target]
                .checked_sub(arc.flow)
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
        }
        let lower_flows = self
            .graph
            .edges()
            .iter()
            .map(crate::model::FlowEdge::lower)
            .collect::<Vec<_>>();
        let lower_divergence = divergences(self.graph, &lower_flows)?;
        for node in 0..self.graph.nodes().len() {
            let expected = self.required_divergence[node]
                .checked_sub(lower_divergence[node])
                .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
            if extended_divergence[node] != expected {
                return Err(NetworkSimplexError::BasisInvariant);
            }
            let parent = self.tree.parent[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            let tree_arc = self.tree.parent_arc[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            if self.directed_residual(tree_arc, node, parent)? <= 0 {
                return Err(NetworkSimplexError::StrongFeasibility);
            }
        }
        if extended_divergence[self.tree.root] != 0 {
            return Err(NetworkSimplexError::BasisInvariant);
        }
        Ok(())
    }

    fn original_flows(&self) -> Result<Vec<u64>, NetworkSimplexError> {
        self.arcs[..self.original_arc_count]
            .iter()
            .zip(self.graph.edges())
            .map(|(arc, edge)| {
                let flow = i128::from(edge.lower())
                    .checked_add(arc.flow)
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)?;
                u64::try_from(flow).map_err(|_| NetworkSimplexError::ArithmeticOverflow)
            })
            .collect()
    }

    fn total_artificial_flow(&self) -> Result<i128, NetworkSimplexError> {
        self.arcs[self.original_arc_count..]
            .iter()
            .try_fold(0_i128, |sum, arc| {
                sum.checked_add(arc.flow)
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)
            })
    }

    fn remaining_divergence(&self) -> Result<Vec<i128>, NetworkSimplexError> {
        let flows = self.original_flows()?;
        let actual = divergences(self.graph, &flows)?;
        self.required_divergence
            .iter()
            .zip(actual)
            .map(|(&required, actual)| {
                required
                    .checked_sub(actual)
                    .ok_or(NetworkSimplexError::ArithmeticOverflow)
            })
            .collect()
    }

    fn forest_overlay(&self) -> Result<Vec<ResidualArcId>, NetworkSimplexError> {
        let mut forest = Vec::new();
        for node in 0..self.graph.nodes().len() {
            let Some(arc_index) = self.tree.parent_arc[node] else {
                continue;
            };
            let arc = &self.arcs[arc_index];
            let Some(original) = arc.original else {
                continue;
            };
            let parent = self.tree.parent[node].ok_or(NetworkSimplexError::BasisInvariant)?;
            let direction = if arc.source == parent && arc.target == node {
                ResidualDirection::Forward
            } else if arc.source == node && arc.target == parent {
                ResidualDirection::Reverse
            } else {
                return Err(NetworkSimplexError::BasisInvariant);
            };
            forest.push(ResidualArcId::new(
                self.graph.edges()[original].id().clone(),
                direction,
            ));
        }
        Ok(forest)
    }

    fn original_node(&self, index: usize) -> Option<NodeIndex> {
        self.graph.node_indices().nth(index)
    }
}

struct PricingResult {
    entering: Option<usize>,
    scanned_nodes: Vec<usize>,
}

struct BasicCycle {
    entering: usize,
    entering_forward: bool,
    first: usize,
    second: usize,
    join: usize,
    first_branch: Vec<(usize, usize)>,
    second_branch: Vec<(usize, usize)>,
    residual_reduced_cost: i128,
    original_path: Vec<ResidualArcId>,
}

struct PivotResult {
    delta: i128,
    basis_changed: bool,
}

fn original_arcs(graph: &FlowNetwork) -> Vec<ArcData> {
    graph
        .edges()
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| ArcData {
            source: edge.from().as_usize(),
            target: edge.to().as_usize(),
            capacity: i128::from(edge.capacity() - edge.lower()),
            cost: i128::from(edge.cost()),
            flow: 0,
            state: ArcState::Lower,
            original: Some(edge_index),
        })
        .collect()
}

fn directed_residual(arc: &ArcData, from: usize, to: usize) -> Result<i128, NetworkSimplexError> {
    if arc.source == from && arc.target == to {
        arc.capacity
            .checked_sub(arc.flow)
            .ok_or(NetworkSimplexError::ArithmeticOverflow)
    } else if arc.source == to && arc.target == from {
        Ok(arc.flow)
    } else {
        Err(NetworkSimplexError::BasisInvariant)
    }
}

fn dynamic_tree<T>(result: Result<T, LinkCutError>) -> Result<T, NetworkSimplexError> {
    result.map_err(|_| NetworkSimplexError::BasisInvariant)
}

const fn integer_ceil_sqrt(value: usize) -> usize {
    if value == 0 {
        return 0;
    }
    let mut root = 1_usize;
    while root.saturating_mul(root) < value {
        root += 1;
    }
    root
}

#[derive(Clone, Copy)]
enum EventKind {
    Initialize,
    InspectPrice,
    Price,
    FormCycle,
    ExchangeBasis,
    FlipBound,
    Optimal,
}

impl EventKind {
    const fn metadata(self, mode: SimplexMode) -> FlowTraceEventMetadata {
        match mode {
            SimplexMode::Explicit => self.explicit_metadata(),
            SimplexMode::DynamicTree => self.dynamic_tree_metadata(),
        }
    }

    const fn explicit_metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::Initialize => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.initialize-artificial-basis",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "primal-network-simplex:build-strongly-feasible-artificial-star",
            },
            Self::InspectPrice => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.inspect-pricing-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "primal-network-simplex:inspect-one-nontree-arc",
            },
            Self::Price => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.price-block",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "primal-network-simplex:price-cyclic-sqrt-blocks",
            },
            Self::FormCycle => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.form-basic-cycle",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "primal-network-simplex:add-entering-arc-and-form-cycle",
            },
            Self::ExchangeBasis => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.exchange-basis",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "primal-network-simplex:replace-leaving-tree-arc-and-reprice",
            },
            Self::FlipBound => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.flip-entering-bound",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "primal-network-simplex:entering-arc-hits-opposite-bound",
            },
            Self::Optimal => FlowTraceEventMetadata {
                catalog_id: "primal-network-simplex.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "primal-network-simplex:return-complementary-slack-flow",
            },
        }
    }

    const fn dynamic_tree_metadata(self) -> FlowTraceEventMetadata {
        match self {
            Self::Initialize => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.initialize-directional-forests",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "dynamic-tree-network-simplex:build-artificial-basis-and-two-residual-forests",
            },
            Self::InspectPrice => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.inspect-pricing-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "dynamic-tree-network-simplex:inspect-one-nontree-arc",
            },
            Self::Price => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.price-block",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "dynamic-tree-network-simplex:price-cyclic-sqrt-blocks",
            },
            Self::FormCycle => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.query-cycle-minimum",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "dynamic-tree-network-simplex:query-upward-and-downward-path-minima",
            },
            Self::ExchangeBasis => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.cut-link-basis",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "dynamic-tree-network-simplex:cut-leaving-link-entering-and-reprice",
            },
            Self::FlipBound => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.flip-entering-bound",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "dynamic-tree-network-simplex:entering-arc-hits-opposite-bound",
            },
            Self::Optimal => FlowTraceEventMetadata {
                catalog_id: "dynamic-tree-network-simplex.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "dynamic-tree-network-simplex:return-certified-complementary-slack-flow",
            },
        }
    }
}

struct TraceView {
    search_order: Vec<usize>,
    active_path: Vec<ResidualArcId>,
}

impl TraceView {
    fn tree(_work: &WorkingState<'_>) -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
        }
    }

    fn priced(work: &WorkingState<'_>, nodes: Vec<usize>) -> Self {
        Self {
            search_order: nodes
                .into_iter()
                .filter(|&node| node < work.graph.nodes().len())
                .collect(),
            active_path: Vec::new(),
        }
    }

    fn inspect_price(
        work: &WorkingState<'_>,
        arc_index: usize,
        active: Option<ResidualArcId>,
    ) -> Self {
        let arc = &work.arcs[arc_index];
        Self {
            search_order: [arc.source, arc.target]
                .into_iter()
                .filter(|&node| node < work.graph.nodes().len())
                .collect(),
            active_path: active.into_iter().collect(),
        }
    }

    fn cycle(work: &WorkingState<'_>, cycle: &BasicCycle) -> Self {
        let search_order = [cycle.first, cycle.second, cycle.join]
            .into_iter()
            .filter(|&node| node < work.graph.nodes().len())
            .collect();
        Self {
            search_order,
            active_path: cycle.original_path.clone(),
        }
    }
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    work: &WorkingState<'_>,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, NetworkSimplexError> {
    if !enabled {
        return Ok(None);
    }
    let snapshot = trace_snapshot(work, TraceView::tree(work))?;
    FlowTraceRecorder::new(graph, snapshot)
        .map(Some)
        .map_err(NetworkSimplexError::Trace)
}

fn record_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    work: &WorkingState<'_>,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), NetworkSimplexError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let snapshot = trace_snapshot(work, view)?;
    recorder
        .record_transition_with_detail(metadata, &snapshot, detail)
        .map_err(NetworkSimplexError::Trace)
}

fn trace_snapshot(
    work: &WorkingState<'_>,
    view: TraceView,
) -> Result<FlowTraceSnapshot, NetworkSimplexError> {
    let flows = work.original_flows()?;
    let residual = ResidualState::from_flows(work.graph, &flows)?;
    let labels = work.tree.potentials[..work.graph.nodes().len()]
        .iter()
        .copied()
        .map(Some)
        .collect();
    let search_order = view
        .search_order
        .into_iter()
        .filter_map(|node| work.original_node(node))
        .collect();
    let snapshot = FlowTraceSnapshot::capture(
        work.graph,
        &residual,
        labels,
        search_order,
        view.active_path,
        work.remaining_divergence()?,
        trace_metrics(work.mode, work.metrics, work.dynamic_metrics),
    )
    .with_forest_overlay(work.graph, work.forest_overlay()?, Vec::new());
    Ok(snapshot)
}

const fn trace_metrics(
    mode: SimplexMode,
    metrics: NetworkSimplexMetrics,
    dynamic: DynamicTreeNetworkSimplexMetrics,
) -> FlowTraceMetrics {
    match mode {
        SimplexMode::Explicit => FlowTraceMetrics {
            bfs_runs: 0,
            relaxation_passes: 0,
            residual_arc_scans: metrics.pricing_arc_scans,
            augmentations: metrics.pivots as u128,
            path_searches: metrics.pricing_searches as u128,
            scaling_phases: 0,
            blocking_flow_phases: 0,
            relabels: metrics.potential_recomputations as u128,
            retreats: metrics.cycle_arc_scans,
            reverse_bfs_runs: 0,
            gap_terminations: metrics.bound_flips as u128,
            pushes: metrics.basis_exchanges as u128,
            saturating_pushes: metrics.nondegenerate_pivots as u128,
            nonsaturating_pushes: metrics.degenerate_pivots as u128,
            discharges: 0,
            active_vertex_selections: 0,
        },
        SimplexMode::DynamicTree => FlowTraceMetrics {
            bfs_runs: dynamic.directional_forest_rebuilds as u128,
            relaxation_passes: dynamic.path_minimum_queries as u128,
            residual_arc_scans: metrics.pricing_arc_scans,
            augmentations: metrics.pivots as u128,
            path_searches: metrics.pricing_searches as u128,
            scaling_phases: dynamic.path_updates as u128,
            blocking_flow_phases: dynamic.directional_value_validations as u128,
            relabels: metrics.potential_recomputations as u128,
            retreats: metrics.cycle_arc_scans,
            reverse_bfs_runs: dynamic.tree_links as u128,
            gap_terminations: metrics.bound_flips as u128,
            pushes: metrics.basis_exchanges as u128,
            saturating_pushes: metrics.nondegenerate_pivots as u128,
            nonsaturating_pushes: metrics.degenerate_pivots as u128,
            discharges: dynamic.tree_cuts as u128,
            active_vertex_selections: metrics.tree_rebuilds as u128,
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_simple_cycle_canceling;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("valid node"), *supply))
                .collect(),
            edges
                .iter()
                .map(|(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("valid edge"),
                    from: NodeId::parse(from).expect("valid tail"),
                    to: NodeId::parse(to).expect("valid head"),
                    lower: *lower,
                    capacity: *capacity,
                    cost: *cost,
                })
                .collect(),
        )
        .expect("valid graph")
    }

    #[test]
    fn solves_lower_bounds_and_disconnected_negative_cycle() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[
                ("st", "s", "t", 1, 3, 5),
                ("xy", "x", "y", 0, 2, -4),
                ("yx", "y", "x", 0, 2, 1),
            ],
        );
        let target = [2, -2, 0, 0];

        let result = solve_primal_network_simplex(&graph, &target).expect("simplex succeeds");

        assert_eq!(result.flows, vec![2, 2, 2]);
        assert_eq!(result.certificate.total_cost, 4);
        assert!(result.metrics.pivots > 0);
        assert!(result.metrics.basis_exchanges > 0);
        assert!(result.metrics.degenerate_pivots > 0);
        check_min_cost_flow(&graph, &target, &result.flows).expect("independent certificate");
    }

    #[test]
    fn self_loop_can_flip_directly_between_bounds() {
        let graph = network(&[("v", 0)], &[("negative-loop", "v", "v", 1, 4, -3)]);

        let result = solve_primal_network_simplex(&graph, &[0]).expect("simplex succeeds");
        let dynamic =
            solve_dynamic_tree_network_simplex(&graph, &[0]).expect("dynamic simplex succeeds");

        assert_eq!(result.flows, vec![4]);
        assert_eq!(result.certificate.total_cost, -12);
        assert_eq!(result.metrics.bound_flips, 1);
        assert_eq!(dynamic.flows, result.flows);
        assert_eq!(dynamic.metrics.simplex.bound_flips, 1);
        assert_eq!(dynamic.metrics.directional_forest_rebuilds, 1);
    }

    #[test]
    fn dynamic_tree_pivots_query_update_cut_and_link_directional_forests() {
        let graph = network(
            &[("s", 0), ("a", 0), ("t", 0), ("x", 0)],
            &[
                ("sa", "s", "a", 1, 5, 4),
                ("at", "a", "t", 0, 5, -2),
                ("st", "s", "t", 0, 4, 7),
                ("as", "a", "s", 0, 2, 1),
                ("xx", "x", "x", 0, 3, -1),
            ],
        );
        let target = [3, 0, -3, 0];
        let explicit = solve_primal_network_simplex(&graph, &target).expect("explicit simplex");
        let dynamic = solve_dynamic_tree_network_simplex(&graph, &target).expect("dynamic simplex");

        assert_eq!(dynamic.flows, explicit.flows);
        assert_eq!(dynamic.certificate, explicit.certificate);
        assert!(dynamic.metrics.path_minimum_queries > 0);
        assert!(dynamic.metrics.path_updates > 0);
        assert!(dynamic.metrics.tree_cuts > 0);
        assert_eq!(dynamic.metrics.tree_links, dynamic.metrics.tree_cuts);
        assert!(dynamic.metrics.directional_forest_rebuilds > 1);
        assert!(dynamic.metrics.directional_value_validations > 1);
    }

    #[test]
    fn trace_replays_in_both_directions_and_exposes_basis() {
        let graph = network(
            &[("s", 0), ("a", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 3, 2),
                ("st", "s", "t", 0, 3, 7),
                ("at", "a", "t", 0, 3, -1),
            ],
        );
        let target = [2, 0, -2];
        let fast = solve_primal_network_simplex(&graph, &target).expect("fast succeeds");
        let trace = trace_primal_network_simplex(&graph, &target).expect("trace succeeds");

        assert_eq!(fast, trace.result);
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "primal-network-simplex.form-basic-cycle"
                && !event.entity_refs.is_empty()
        }));
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.catalog_id == "primal-network-simplex.exchange-basis" })
        );

        let mut replay = trace.base_snapshot.clone();
        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, trace.base_snapshot);
    }

    #[test]
    fn dynamic_tree_trace_replays_cycle_minimum_and_cut_link_boundaries() {
        let graph = network(
            &[("s", 0), ("a", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 3, 2),
                ("st", "s", "t", 0, 3, 7),
                ("at", "a", "t", 0, 3, -1),
            ],
        );
        let target = [2, 0, -2];
        let fast = solve_dynamic_tree_network_simplex(&graph, &target).expect("fast succeeds");
        let trace = trace_dynamic_tree_network_simplex(&graph, &target).expect("trace succeeds");

        assert_eq!(fast, trace.result);
        assert!(trace.events.iter().any(|event| {
            event.catalog_id == "dynamic-tree-network-simplex.query-cycle-minimum"
                && !event.entity_refs.is_empty()
        }));
        assert!(
            trace
                .events
                .iter()
                .any(|event| { event.catalog_id == "dynamic-tree-network-simplex.cut-link-basis" })
        );

        let mut replay = trace.base_snapshot.clone();
        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward replay");
        }
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse replay");
        }
        assert_eq!(replay, trace.base_snapshot);
    }

    #[test]
    fn deterministic_differential_cases_match_cycle_canceling() {
        let mut seed = 0x7a91_c33d_5e21_11f0_u64;
        for case in 0..48 {
            let node_count = 2 + (next(&mut seed) % 5) as usize;
            let mut nodes = (0..node_count)
                .map(|index| {
                    FlowNode::new(NodeId::parse(&format!("v{index}")).expect("valid node"), 0)
                })
                .collect::<Vec<_>>();
            let amount = 1 + i64::try_from(next(&mut seed) % 4).expect("small amount");
            nodes[0] = FlowNode::new(NodeId::parse("v0").expect("valid node"), amount);
            nodes[node_count - 1] = FlowNode::new(
                NodeId::parse(&format!("v{}", node_count - 1)).expect("valid node"),
                -amount,
            );
            let mut edges = Vec::new();
            for index in 0..node_count - 1 {
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("backbone-{index}")).expect("valid edge"),
                    from: NodeId::parse(&format!("v{index}")).expect("valid tail"),
                    to: NodeId::parse(&format!("v{}", index + 1)).expect("valid head"),
                    lower: 0,
                    capacity: u64::try_from(amount).expect("positive amount") + 3,
                    cost: i64::try_from(next(&mut seed) % 11).expect("small cost") - 5,
                });
            }
            let extras = 2 + (next(&mut seed) % 8) as usize;
            for edge_index in 0..extras {
                let node_count_u64 = u64::try_from(node_count).expect("small node count");
                let from =
                    usize::try_from(next(&mut seed) % node_count_u64).expect("node index fits");
                let to =
                    usize::try_from(next(&mut seed) % node_count_u64).expect("node index fits");
                edges.push(UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("extra-{edge_index}")).expect("valid edge"),
                    from: NodeId::parse(&format!("v{from}")).expect("valid tail"),
                    to: NodeId::parse(&format!("v{to}")).expect("valid head"),
                    lower: 0,
                    capacity: 1 + next(&mut seed) % 6,
                    cost: i64::try_from(next(&mut seed) % 15).expect("small cost") - 7,
                });
            }
            let graph = FlowNetwork::new(nodes, edges).expect("valid random graph");
            let target = graph
                .nodes()
                .iter()
                .map(|node| i128::from(node.supply()))
                .collect::<Vec<_>>();
            let simplex = solve_primal_network_simplex(&graph, &target)
                .unwrap_or_else(|error| panic!("case {case}: simplex failed: {error}"));
            let dynamic = solve_dynamic_tree_network_simplex(&graph, &target)
                .unwrap_or_else(|error| panic!("case {case}: dynamic simplex failed: {error}"));
            let reference = solve_simple_cycle_canceling(&graph, &target)
                .unwrap_or_else(|error| panic!("case {case}: reference failed: {error}"));
            assert_eq!(
                simplex.certificate.total_cost, reference.certificate.total_cost,
                "case {case}"
            );
            assert_eq!(
                dynamic.certificate.total_cost, reference.certificate.total_cost,
                "dynamic case {case}"
            );
        }
    }

    fn next(seed: &mut u64) -> u64 {
        *seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *seed
    }
}
