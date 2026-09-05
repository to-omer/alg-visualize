//! Balanced transportation simplex with Bland Rule I and MODI trace preset.

use std::collections::VecDeque;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow, divergences,
};
use crate::feasibility::{
    FeasibilityError, FeasibilityExecution, FeasibilityUse, InfeasibilityWitness,
};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};
use crate::transportation::{
    TRANSPORTATION_MAX_EDGES, TRANSPORTATION_MAX_NODES, TransportationGraph,
    TransportationModelError, TransportationRoute,
};

/// Complete pricing-scan ceiling for the educational table kernel.
pub const TRANSPORTATION_MAX_PRICING_SCANS: u128 = 8_000_000;
/// Deterministic simplex-pivot ceiling, including degenerate exchanges.
pub const TRANSPORTATION_MAX_PIVOTS: u64 = 100_000;
/// Support-cycle route-scan and basis-path scan ceiling.
pub const TRANSPORTATION_MAX_STRUCTURE_SCANS: u128 = 8_000_000;
/// Eager trace transition ceiling.
pub const TRANSPORTATION_MAX_STATE_TRANSITIONS: u64 = 200_000;
/// Aggregate full-snapshot cells admitted by trace projection.
pub const TRANSPORTATION_MAX_TRACE_PROJECTION_CELLS: u128 = 4_000_000;

/// User-facing trace vocabulary over the same exact simplex kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportationPreset {
    /// Basis-forest simplex terminology.
    TransportationSimplex,
    /// u-v potentials, opportunity cost, and closed-loop terminology.
    Modi,
}

/// Exact deterministic transportation-kernel counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransportationMetrics {
    /// Feasible-flow constructions attempted.
    pub feasibility_searches: u64,
    /// Positive-support cycles eliminated before basis completion.
    pub support_cycle_cancellations: u64,
    /// Zero-shipment routes added to complete the basis forest.
    pub basis_extensions: u64,
    /// Full u-v potential reconstructions.
    pub potential_recomputations: u64,
    /// Complete Bland pricing searches, including the final scan.
    pub pricing_searches: u64,
    /// Nonbasic table cells inspected during pricing.
    pub pricing_scans: u128,
    /// Fundamental-cycle pivots.
    pub pivots: u64,
    /// Pivots with positive shipment change.
    pub nondegenerate_pivots: u64,
    /// Zero-theta basis exchanges.
    pub degenerate_pivots: u64,
    /// Entering/leaving route exchanges.
    pub basis_exchanges: u64,
    /// Basis routes inspected while finding support cycles and pivot loops.
    pub structure_scans: u128,
}

/// Certified transportation optimum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportationResult {
    /// Canonical original-edge shipment vector.
    pub flows: Vec<u64>,
    /// Independent generic min-cost primal/dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Exact bounded-work counters.
    pub metrics: TransportationMetrics,
    /// Trace/display preset used to run the shared kernel.
    pub preset: TransportationPreset,
}

/// Certified result with reversible basis, pricing, and closed-loop events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportationTraceResult {
    /// Same terminal result as fast mode.
    pub result: TransportationResult,
    /// Replay boundary before feasible-shipment initialization.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible trace events.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after independent certification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Transportation construction, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransportationError {
    /// Model exceeds the practical interactive band.
    #[error("transportation graph exceeds admission limits")]
    AdmissionLimit,
    /// A pricing, pivot, structure, or trace ceiling was reached.
    #[error("transportation simplex work limit reached")]
    WorkLimit,
    /// Native table-model validation failed.
    #[error(transparent)]
    Model(#[from] TransportationModelError),
    /// No allowed-route shipment satisfies all supplies and demands.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual shipment mutation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent final certificate rejected the candidate.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic overflowed.
    #[error("transportation simplex arithmetic overflow")]
    ArithmeticOverflow,
    /// Support forest, u-v labels, or pivot path contradicted the model.
    #[error("transportation simplex basis invariant failed")]
    BasisInvariant,
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves a balanced transportation table with Bland Rule I.
///
/// # Errors
///
/// Rejects malformed/unbalanced tables, infeasibility, bounded-work
/// exhaustion, invariant failure, arithmetic overflow, or a failed
/// independent min-cost certificate.
pub fn solve_transportation_simplex(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
) -> Result<TransportationResult, TransportationError> {
    solve_internal(
        graph,
        origins,
        destinations,
        TransportationPreset::TransportationSimplex,
        false,
    )
    .map(|run| run.result)
}

/// Solves the same exact kernel while selecting MODI terminology.
///
/// # Errors
///
/// Returns the same failures as [`solve_transportation_simplex`].
pub fn solve_modi(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
) -> Result<TransportationResult, TransportationError> {
    solve_internal(
        graph,
        origins,
        destinations,
        TransportationPreset::Modi,
        false,
    )
    .map(|run| run.result)
}

/// Solves one transportation preset while reporting its initial feasible
/// shipment construction to the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_transportation_preset_with_feasibility(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    preset: TransportationPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<TransportationResult, TransportationError> {
    solve_internal_with_feasibility(graph, origins, destinations, preset, false, feasibility)
        .map(|run| run.result)
}

/// Records basis-forest simplex events.
///
/// # Errors
///
/// Returns the same failures as [`solve_transportation_simplex`], plus trace
/// projection failures.
pub fn trace_transportation_simplex(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
) -> Result<TransportationTraceResult, TransportationError> {
    trace_preset(
        graph,
        origins,
        destinations,
        TransportationPreset::TransportationSimplex,
    )
}

/// Records MODI u-v, opportunity-cost, and closed-loop events.
///
/// # Errors
///
/// Returns the same failures as [`solve_transportation_simplex`], plus trace
/// projection failures.
pub fn trace_modi(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
) -> Result<TransportationTraceResult, TransportationError> {
    trace_preset(graph, origins, destinations, TransportationPreset::Modi)
}

fn trace_preset(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    preset: TransportationPreset,
) -> Result<TransportationTraceResult, TransportationError> {
    let run = solve_internal(graph, origins, destinations, preset, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(TransportationError::BasisInvariant)?;
    Ok(TransportationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces one transportation preset while explicitly publishing its initial
/// feasible-shipment construction to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_transportation_preset_with_feasibility(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    preset: TransportationPreset,
    feasibility: &mut FeasibilityExecution,
) -> Result<TransportationTraceResult, TransportationError> {
    let run =
        solve_internal_with_feasibility(graph, origins, destinations, preset, true, feasibility)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(TransportationError::BasisInvariant)?;
    Ok(TransportationTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: TransportationResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    preset: TransportationPreset,
    trace_enabled: bool,
) -> Result<InternalRun, TransportationError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_internal_with_feasibility(
        graph,
        origins,
        destinations,
        preset,
        trace_enabled,
        &mut feasibility,
    )
}

fn solve_internal_with_feasibility(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    preset: TransportationPreset,
    trace_enabled: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<InternalRun, TransportationError> {
    if graph.nodes().len() > TRANSPORTATION_MAX_NODES
        || graph.edges().len() > TRANSPORTATION_MAX_EDGES
    {
        return Err(TransportationError::AdmissionLimit);
    }
    let model = TransportationGraph::new(graph, origins, destinations)?;
    let required = model.required_divergence(graph);
    let feasible = feasibility.find_feasible_flow(graph, &required, FeasibilityUse::InitialFlow)?;
    let mut recorder = start_recorder(graph, &required, &feasible.flows, trace_enabled)?;
    let mut work = WorkingState::new(graph, &model, required, &feasible.flows, preset)?;
    work.metrics.feasibility_searches = 1;
    record(
        recorder.as_mut(),
        &mut work,
        EventKind::Initialize.metadata(preset),
        TraceView::idle(),
        Some(("shipment", u128_to_i128(model.total_shipment())?)),
    )?;
    work.eliminate_support_cycles(&mut recorder)?;
    work.complete_basis()?;
    work.recompute_potentials()?;
    work.validate_basis()?;
    let basis_route_count = work.basis.iter().filter(|&&basic| basic).count();
    record(
        recorder.as_mut(),
        &mut work,
        EventKind::CompleteBasis.metadata(preset),
        TraceView::basis(),
        Some(("basis-routes", usize_to_i128(basis_route_count)?)),
    )?;

    run_transportation_pivots(&mut work, &mut recorder, preset)?;

    work.validate_basis()?;
    let flows = work.state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, &work.required, &flows)?;
    record(
        recorder.as_mut(),
        &mut work,
        EventKind::Optimal.metadata(preset),
        TraceView::basis(),
        Some(("total-cost", certificate.total_cost)),
    )?;
    Ok(InternalRun {
        result: TransportationResult {
            flows,
            certificate,
            metrics: work.metrics,
            preset,
        },
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn run_transportation_pivots(
    work: &mut WorkingState<'_, '_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    preset: TransportationPreset,
) -> Result<(), TransportationError> {
    loop {
        let pricing = work.find_entering(recorder)?;
        let pricing_view = match pricing.entering {
            Some((route, _)) => {
                TraceView::cycle(vec![work.residual_id(route, ResidualDirection::Forward)?])
            }
            None => TraceView::idle(),
        };
        record(
            recorder.as_mut(),
            work,
            EventKind::Price.metadata(preset),
            pricing_view,
            pricing
                .entering
                .map(|(_, reduced)| ("opportunity-cost", reduced)),
        )?;
        let Some((entering, _)) = pricing.entering else {
            break;
        };
        if work.metrics.pivots >= TRANSPORTATION_MAX_PIVOTS {
            return Err(TransportationError::WorkLimit);
        }
        let cycle = work.fundamental_cycle(entering)?;
        record(
            recorder.as_mut(),
            work,
            EventKind::FormCycle.metadata(preset),
            TraceView::cycle(cycle.path.clone()),
            Some(("theta", i128::from(cycle.theta))),
        )?;
        work.apply_pivot(&cycle)?;
        record(
            recorder.as_mut(),
            work,
            if cycle.theta == 0 {
                EventKind::DegeneratePivot.metadata(preset)
            } else {
                EventKind::AdjustCycle.metadata(preset)
            },
            TraceView::cycle(cycle.path.clone()),
            Some(("theta", i128::from(cycle.theta))),
        )?;
        work.exchange_basis(cycle.entering, cycle.leaving)?;
        work.recompute_potentials()?;
        work.validate_basis()?;
        record(
            recorder.as_mut(),
            work,
            EventKind::ExchangeBasis.metadata(preset),
            TraceView::edge(
                work.graph
                    .edges()
                    .get(cycle.leaving)
                    .ok_or(TransportationError::BasisInvariant)?
                    .id()
                    .clone(),
            ),
            Some(("leaving-edge", usize_to_i128(cycle.leaving)?)),
        )?;
    }
    Ok(())
}

struct WorkingState<'graph, 'model> {
    graph: &'graph FlowNetwork,
    model: &'model TransportationGraph,
    required: Vec<i128>,
    state: ResidualState<'graph>,
    basis: Vec<bool>,
    potentials: Vec<Option<i128>>,
    metrics: TransportationMetrics,
    preset: TransportationPreset,
    trace_transitions: u64,
}

impl<'graph, 'model> WorkingState<'graph, 'model> {
    fn new(
        graph: &'graph FlowNetwork,
        model: &'model TransportationGraph,
        required: Vec<i128>,
        flows: &[u64],
        preset: TransportationPreset,
    ) -> Result<Self, TransportationError> {
        Ok(Self {
            graph,
            model,
            required,
            state: ResidualState::from_flows(graph, flows)?,
            basis: vec![false; model.routes.len()],
            potentials: vec![None; graph.nodes().len()],
            metrics: TransportationMetrics::default(),
            preset,
            trace_transitions: 0,
        })
    }

    fn eliminate_support_cycles(
        &mut self,
        recorder: &mut Option<FlowTraceRecorder<'graph>>,
    ) -> Result<(), TransportationError> {
        while let Some(path) = self.positive_support_cycle()? {
            let theta = path
                .iter()
                .filter(|arc| arc.direction() == ResidualDirection::Reverse)
                .map(|arc| {
                    self.state
                        .arc(arc)
                        .map(|value| value.capacity)
                        .ok_or(TransportationError::BasisInvariant)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .min()
                .ok_or(TransportationError::BasisInvariant)?;
            if theta == 0 {
                return Err(TransportationError::BasisInvariant);
            }
            self.state.augment(&path, theta)?;
            self.metrics.support_cycle_cancellations = self
                .metrics
                .support_cycle_cancellations
                .checked_add(1)
                .ok_or(TransportationError::ArithmeticOverflow)?;
            record(
                recorder.as_mut(),
                self,
                EventKind::RemoveSupportCycle.metadata(self.preset),
                TraceView::cycle(path),
                Some(("theta", i128::from(theta))),
            )?;
        }
        Ok(())
    }

    fn positive_support_cycle(
        &mut self,
    ) -> Result<Option<Vec<ResidualArcId>>, TransportationError> {
        let mut dsu = DisjointSet::new(self.graph.nodes().len());
        let mut forest = vec![Vec::<(NodeIndex, usize)>::new(); self.graph.nodes().len()];
        for (route, data) in self.model.routes.iter().enumerate() {
            self.bump_structure_scan()?;
            if self.state.flows()[data.edge.as_usize()] == 0 {
                continue;
            }
            let (origin, destination) = self.route_nodes(data)?;
            if dsu.union(origin.as_usize(), destination.as_usize()) {
                forest[origin.as_usize()].push((destination, route));
                forest[destination.as_usize()].push((origin, route));
                continue;
            }
            let mut cycle = vec![self.residual_id(route, ResidualDirection::Reverse)?];
            cycle.extend(self.forest_path(&forest, origin, destination)?);
            return Ok(Some(cycle));
        }
        Ok(None)
    }

    fn complete_basis(&mut self) -> Result<(), TransportationError> {
        let mut dsu = DisjointSet::new(self.graph.nodes().len());
        for (route, data) in self.model.routes.iter().enumerate() {
            self.bump_structure_scan()?;
            if self.state.flows()[data.edge.as_usize()] == 0 {
                continue;
            }
            let (origin, destination) = self.route_nodes(data)?;
            if !dsu.union(origin.as_usize(), destination.as_usize()) {
                return Err(TransportationError::BasisInvariant);
            }
            self.basis[route] = true;
        }
        for (route, data) in self.model.routes.iter().enumerate() {
            self.bump_structure_scan()?;
            if self.basis[route] {
                continue;
            }
            let (origin, destination) = self.route_nodes(data)?;
            if dsu.union(origin.as_usize(), destination.as_usize()) {
                self.basis[route] = true;
                self.metrics.basis_extensions = self
                    .metrics
                    .basis_extensions
                    .checked_add(1)
                    .ok_or(TransportationError::ArithmeticOverflow)?;
            }
        }
        Ok(())
    }

    fn recompute_potentials(&mut self) -> Result<(), TransportationError> {
        self.potentials.fill(None);
        let adjacency = self.basis_adjacency()?;
        for root in self.graph.node_indices() {
            if self.potentials[root.as_usize()].is_some() {
                continue;
            }
            self.potentials[root.as_usize()] = Some(0);
            let mut queue = VecDeque::from([root]);
            while let Some(node) = queue.pop_front() {
                let current =
                    self.potentials[node.as_usize()].ok_or(TransportationError::BasisInvariant)?;
                for &(next, route) in &adjacency[node.as_usize()] {
                    self.bump_structure_scan()?;
                    let candidate = i128::from(self.model.routes[route].cost)
                        .checked_sub(current)
                        .ok_or(TransportationError::ArithmeticOverflow)?;
                    match self.potentials[next.as_usize()] {
                        Some(existing) if existing != candidate => {
                            return Err(TransportationError::BasisInvariant);
                        }
                        Some(_) => {}
                        None => {
                            self.potentials[next.as_usize()] = Some(candidate);
                            queue.push_back(next);
                        }
                    }
                }
            }
        }
        self.metrics.potential_recomputations = self
            .metrics
            .potential_recomputations
            .checked_add(1)
            .ok_or(TransportationError::ArithmeticOverflow)?;
        Ok(())
    }

    fn find_entering(
        &mut self,
        recorder: &mut Option<FlowTraceRecorder<'_>>,
    ) -> Result<PricingResult, TransportationError> {
        self.metrics.pricing_searches = self
            .metrics
            .pricing_searches
            .checked_add(1)
            .ok_or(TransportationError::ArithmeticOverflow)?;
        for (route, data) in self.model.routes.iter().enumerate() {
            if self.basis[route] {
                continue;
            }
            self.bump_pricing_scan()?;
            let (origin, destination) = self.route_nodes(data)?;
            let reduced = self.reduced_cost(route)?;
            if self.metrics.pricing_scans.is_power_of_two() {
                let inspected = self.residual_id(route, ResidualDirection::Forward)?;
                record(
                    recorder.as_mut(),
                    self,
                    EventKind::InspectPrice.metadata(self.preset),
                    TraceView {
                        search_order: vec![origin, destination],
                        active_path: vec![inspected.clone()],
                        focus: vec![FlowTraceEntityRef::ResidualArc(inspected)],
                    },
                    Some(("reduced-cost", reduced)),
                )?;
            }
            if reduced < 0 {
                return Ok(PricingResult {
                    entering: Some((route, reduced)),
                });
            }
        }
        Ok(PricingResult { entering: None })
    }

    fn fundamental_cycle(&mut self, entering: usize) -> Result<PivotCycle, TransportationError> {
        if self.basis.get(entering).copied() != Some(false) {
            return Err(TransportationError::BasisInvariant);
        }
        let data = self
            .model
            .routes
            .get(entering)
            .ok_or(TransportationError::BasisInvariant)?;
        let (origin, destination) = self.route_nodes(data)?;
        let adjacency = self.basis_adjacency()?;
        let mut path = vec![self.residual_id(entering, ResidualDirection::Forward)?];
        path.extend(self.forest_path(&adjacency, destination, origin)?);
        let mut theta = u64::MAX;
        let mut leaving = None;
        for arc in path.iter().skip(1) {
            self.bump_structure_scan()?;
            if arc.direction() != ResidualDirection::Reverse {
                continue;
            }
            let route = self.route_ordinal(arc)?;
            let flow = self.state.flows()[self.model.routes[route].edge.as_usize()];
            if flow < theta || (flow == theta && leaving.is_none_or(|current| route < current)) {
                theta = flow;
                leaving = Some(route);
            }
        }
        let leaving = leaving.ok_or(TransportationError::BasisInvariant)?;
        if theta == u64::MAX {
            return Err(TransportationError::BasisInvariant);
        }
        Ok(PivotCycle {
            entering,
            leaving,
            theta,
            path,
        })
    }

    fn apply_pivot(&mut self, cycle: &PivotCycle) -> Result<(), TransportationError> {
        if cycle.theta > 0 {
            self.state.augment(&cycle.path, cycle.theta)?;
            self.metrics.nondegenerate_pivots = self
                .metrics
                .nondegenerate_pivots
                .checked_add(1)
                .ok_or(TransportationError::ArithmeticOverflow)?;
        } else {
            self.metrics.degenerate_pivots = self
                .metrics
                .degenerate_pivots
                .checked_add(1)
                .ok_or(TransportationError::ArithmeticOverflow)?;
        }
        self.metrics.pivots = self
            .metrics
            .pivots
            .checked_add(1)
            .ok_or(TransportationError::ArithmeticOverflow)?;
        Ok(())
    }

    fn exchange_basis(
        &mut self,
        entering: usize,
        leaving: usize,
    ) -> Result<(), TransportationError> {
        if entering == leaving
            || self.basis.get(entering).copied() != Some(false)
            || self.basis.get(leaving).copied() != Some(true)
        {
            return Err(TransportationError::BasisInvariant);
        }
        self.basis[entering] = true;
        self.basis[leaving] = false;
        self.metrics.basis_exchanges = self
            .metrics
            .basis_exchanges
            .checked_add(1)
            .ok_or(TransportationError::ArithmeticOverflow)?;
        Ok(())
    }

    fn validate_basis(&mut self) -> Result<(), TransportationError> {
        if self.basis.len() != self.model.routes.len()
            || self.potentials.len() != self.graph.nodes().len()
        {
            return Err(TransportationError::BasisInvariant);
        }
        let mut dsu = DisjointSet::new(self.graph.nodes().len());
        for (route, data) in self.model.routes.iter().enumerate() {
            self.bump_structure_scan()?;
            let flow = self.state.flows()[data.edge.as_usize()];
            if flow > 0 && !self.basis[route] {
                return Err(TransportationError::BasisInvariant);
            }
            if !self.basis[route] {
                continue;
            }
            let (origin, destination) = self.route_nodes(data)?;
            if !dsu.union(origin.as_usize(), destination.as_usize())
                || self.reduced_cost(route)? != 0
            {
                return Err(TransportationError::BasisInvariant);
            }
        }
        for data in &self.model.routes {
            self.bump_structure_scan()?;
            let (origin, destination) = self.route_nodes(data)?;
            if dsu.find(origin.as_usize()) != dsu.find(destination.as_usize()) {
                return Err(TransportationError::BasisInvariant);
            }
        }
        self.bump_structure_scans(self.graph.nodes().len() + self.graph.edges().len())?;
        if divergences(self.graph, self.state.flows())? != self.required {
            return Err(TransportationError::BasisInvariant);
        }
        Ok(())
    }

    fn basis_adjacency(&mut self) -> Result<Vec<Vec<(NodeIndex, usize)>>, TransportationError> {
        let mut adjacency = vec![Vec::new(); self.graph.nodes().len()];
        for (route, data) in self.model.routes.iter().enumerate() {
            self.bump_structure_scan()?;
            if !self.basis[route] {
                continue;
            }
            let (origin, destination) = self.route_nodes(data)?;
            adjacency[origin.as_usize()].push((destination, route));
            adjacency[destination.as_usize()].push((origin, route));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable_by_key(|&(node, route)| (node, route));
        }
        Ok(adjacency)
    }

    fn forest_path(
        &mut self,
        adjacency: &[Vec<(NodeIndex, usize)>],
        start: NodeIndex,
        goal: NodeIndex,
    ) -> Result<Vec<ResidualArcId>, TransportationError> {
        let mut parent = vec![None::<(NodeIndex, usize)>; self.graph.nodes().len()];
        let mut queue = VecDeque::from([start]);
        parent[start.as_usize()] = Some((start, usize::MAX));
        while let Some(node) = queue.pop_front() {
            if node == goal {
                break;
            }
            for &(next, route) in adjacency
                .get(node.as_usize())
                .ok_or(TransportationError::BasisInvariant)?
            {
                self.bump_structure_scan()?;
                if parent[next.as_usize()].is_some() {
                    continue;
                }
                parent[next.as_usize()] = Some((node, route));
                queue.push_back(next);
            }
        }
        if parent[goal.as_usize()].is_none() {
            return Err(TransportationError::BasisInvariant);
        }
        let mut reversed = Vec::new();
        let mut node = goal;
        while node != start {
            self.bump_structure_scan()?;
            let (previous, route) =
                parent[node.as_usize()].ok_or(TransportationError::BasisInvariant)?;
            reversed.push((route, previous, node));
            node = previous;
        }
        reversed.reverse();
        reversed
            .into_iter()
            .map(|(route, from, to)| {
                let data = self
                    .model
                    .routes
                    .get(route)
                    .ok_or(TransportationError::BasisInvariant)?;
                let (origin, destination) = self.route_nodes(data)?;
                let direction = if from == origin && to == destination {
                    ResidualDirection::Forward
                } else if from == destination && to == origin {
                    ResidualDirection::Reverse
                } else {
                    return Err(TransportationError::BasisInvariant);
                };
                self.residual_id(route, direction)
            })
            .collect()
    }

    fn route_nodes(
        &self,
        route: &TransportationRoute,
    ) -> Result<(NodeIndex, NodeIndex), TransportationError> {
        Ok((
            self.model
                .node_for_origin(route.origin)
                .ok_or(TransportationError::BasisInvariant)?,
            self.model
                .node_for_destination(route.destination)
                .ok_or(TransportationError::BasisInvariant)?,
        ))
    }

    fn residual_id(
        &self,
        route: usize,
        direction: ResidualDirection,
    ) -> Result<ResidualArcId, TransportationError> {
        let edge = self
            .graph
            .edge(
                self.model
                    .routes
                    .get(route)
                    .ok_or(TransportationError::BasisInvariant)?
                    .edge,
            )
            .ok_or(TransportationError::BasisInvariant)?;
        Ok(ResidualArcId::new(edge.id().clone(), direction))
    }

    fn route_ordinal(&self, arc: &ResidualArcId) -> Result<usize, TransportationError> {
        let edge = self
            .graph
            .edge_index(arc.original_edge())
            .ok_or(TransportationError::BasisInvariant)?;
        let ordinal = edge.as_usize();
        self.model
            .routes
            .get(ordinal)
            .filter(|route| route.edge == edge)
            .map(|_| ordinal)
            .ok_or(TransportationError::BasisInvariant)
    }

    fn reduced_cost(&self, route: usize) -> Result<i128, TransportationError> {
        let data = self
            .model
            .routes
            .get(route)
            .ok_or(TransportationError::BasisInvariant)?;
        let (origin, destination) = self.route_nodes(data)?;
        let origin_potential =
            self.potentials[origin.as_usize()].ok_or(TransportationError::BasisInvariant)?;
        let destination_potential =
            self.potentials[destination.as_usize()].ok_or(TransportationError::BasisInvariant)?;
        i128::from(data.cost)
            .checked_sub(origin_potential)
            .and_then(|value| value.checked_sub(destination_potential))
            .ok_or(TransportationError::ArithmeticOverflow)
    }

    fn bump_pricing_scan(&mut self) -> Result<(), TransportationError> {
        self.metrics.pricing_scans = self
            .metrics
            .pricing_scans
            .checked_add(1)
            .ok_or(TransportationError::ArithmeticOverflow)?;
        if self.metrics.pricing_scans > TRANSPORTATION_MAX_PRICING_SCANS {
            return Err(TransportationError::WorkLimit);
        }
        Ok(())
    }

    fn bump_structure_scan(&mut self) -> Result<(), TransportationError> {
        self.bump_structure_scans(1)
    }

    fn bump_structure_scans(&mut self, count: usize) -> Result<(), TransportationError> {
        self.metrics.structure_scans = self
            .metrics
            .structure_scans
            .checked_add(
                u128::try_from(count).map_err(|_| TransportationError::ArithmeticOverflow)?,
            )
            .ok_or(TransportationError::ArithmeticOverflow)?;
        if self.metrics.structure_scans > TRANSPORTATION_MAX_STRUCTURE_SCANS {
            return Err(TransportationError::WorkLimit);
        }
        Ok(())
    }

    fn remaining_divergence(&self) -> Result<Vec<i128>, TransportationError> {
        let actual = divergences(self.graph, self.state.flows())?;
        self.required
            .iter()
            .zip(actual)
            .map(|(&required, actual)| {
                required
                    .checked_sub(actual)
                    .ok_or(TransportationError::ArithmeticOverflow)
            })
            .collect()
    }

    fn forest_overlay(&self) -> Result<Vec<ResidualArcId>, TransportationError> {
        let mut adjacency = vec![Vec::<(NodeIndex, usize)>::new(); self.graph.nodes().len()];
        for (route, &basic) in self.basis.iter().enumerate() {
            if !basic {
                continue;
            }
            let data = self
                .model
                .routes
                .get(route)
                .ok_or(TransportationError::BasisInvariant)?;
            let (origin, destination) = self.route_nodes(data)?;
            adjacency[origin.as_usize()].push((destination, route));
            adjacency[destination.as_usize()].push((origin, route));
        }
        for neighbors in &mut adjacency {
            neighbors.sort_unstable_by_key(|&(node, route)| (node, route));
        }

        let mut seen = vec![false; self.graph.nodes().len()];
        let mut overlay = Vec::new();
        for root in self.graph.node_indices() {
            if seen[root.as_usize()] {
                continue;
            }
            seen[root.as_usize()] = true;
            let mut queue = VecDeque::from([root]);
            while let Some(parent) = queue.pop_front() {
                for &(child, route) in &adjacency[parent.as_usize()] {
                    if seen[child.as_usize()] {
                        continue;
                    }
                    seen[child.as_usize()] = true;
                    queue.push_back(child);
                    let data = self
                        .model
                        .routes
                        .get(route)
                        .ok_or(TransportationError::BasisInvariant)?;
                    let (origin, destination) = self.route_nodes(data)?;
                    let direction = if parent == origin && child == destination {
                        ResidualDirection::Forward
                    } else if parent == destination && child == origin {
                        ResidualDirection::Reverse
                    } else {
                        return Err(TransportationError::BasisInvariant);
                    };
                    overlay.push(self.residual_id(route, direction)?);
                }
            }
        }
        Ok(overlay)
    }
}

struct PricingResult {
    entering: Option<(usize, i128)>,
}

struct PivotCycle {
    entering: usize,
    leaving: usize,
    theta: u64,
    path: Vec<ResidualArcId>,
}

#[derive(Clone, Copy)]
enum EventKind {
    Initialize,
    RemoveSupportCycle,
    CompleteBasis,
    InspectPrice,
    Price,
    FormCycle,
    AdjustCycle,
    DegeneratePivot,
    ExchangeBasis,
    Optimal,
}

impl EventKind {
    #[allow(clippy::too_many_lines)]
    const fn metadata(self, preset: TransportationPreset) -> FlowTraceEventMetadata {
        match (preset, self) {
            (TransportationPreset::TransportationSimplex, Self::Initialize) => metadata(
                "transportation-simplex.initialize-feasible",
                TraceGranularityV1::Phase,
                "transportation-simplex:construct-feasible-shipment",
            ),
            (TransportationPreset::TransportationSimplex, Self::RemoveSupportCycle) => metadata(
                "transportation-simplex.remove-support-cycle",
                TraceGranularityV1::Operation,
                "transportation-simplex:make-positive-support-acyclic",
            ),
            (TransportationPreset::TransportationSimplex, Self::CompleteBasis) => metadata(
                "transportation-simplex.complete-basis-forest",
                TraceGranularityV1::Phase,
                "transportation-simplex:add-zero-basic-routes",
            ),
            (TransportationPreset::TransportationSimplex, Self::InspectPrice) => metadata(
                "transportation-simplex.inspect-pricing-route",
                TraceGranularityV1::Micro,
                "transportation-simplex:inspect-one-nonbasic-route",
            ),
            (TransportationPreset::TransportationSimplex, Self::Price) => metadata(
                "transportation-simplex.bland-price",
                TraceGranularityV1::Phase,
                "transportation-simplex:least-index-negative-reduced-cost",
            ),
            (TransportationPreset::TransportationSimplex, Self::FormCycle) => metadata(
                "transportation-simplex.form-fundamental-cycle",
                TraceGranularityV1::Operation,
                "transportation-simplex:add-entering-route-and-form-cycle",
            ),
            (TransportationPreset::TransportationSimplex, Self::AdjustCycle) => metadata(
                "transportation-simplex.augment-cycle",
                TraceGranularityV1::Operation,
                "transportation-simplex:alternate-plus-minus-by-theta",
            ),
            (TransportationPreset::TransportationSimplex, Self::DegeneratePivot) => metadata(
                "transportation-simplex.degenerate-pivot",
                TraceGranularityV1::Operation,
                "transportation-simplex:exchange-basis-at-zero-theta",
            ),
            (TransportationPreset::TransportationSimplex, Self::ExchangeBasis) => metadata(
                "transportation-simplex.exchange-basis",
                TraceGranularityV1::Operation,
                "transportation-simplex:bland-least-index-ratio-tie",
            ),
            (TransportationPreset::TransportationSimplex, Self::Optimal) => metadata(
                "transportation-simplex.optimal",
                TraceGranularityV1::Phase,
                "transportation-simplex:all-reduced-costs-nonnegative",
            ),
            (TransportationPreset::Modi, Self::Initialize) => metadata(
                "modi.initialize-feasible",
                TraceGranularityV1::Phase,
                "modi:construct-feasible-shipment",
            ),
            (TransportationPreset::Modi, Self::RemoveSupportCycle) => metadata(
                "modi.remove-support-cycle",
                TraceGranularityV1::Operation,
                "modi:remove-dependent-positive-cell",
            ),
            (TransportationPreset::Modi, Self::CompleteBasis) => metadata(
                "modi.complete-basic-cells",
                TraceGranularityV1::Phase,
                "modi:add-zero-basic-cells",
            ),
            (TransportationPreset::Modi, Self::InspectPrice) => metadata(
                "modi.inspect-pricing-route",
                TraceGranularityV1::Micro,
                "modi:inspect-one-opportunity-cost",
            ),
            (TransportationPreset::Modi, Self::Price) => metadata(
                "modi.compute-uv-opportunity-cost",
                TraceGranularityV1::Phase,
                "modi:compute-u-v-and-c-minus-u-minus-v",
            ),
            (TransportationPreset::Modi, Self::FormCycle) => metadata(
                "modi.form-closed-loop",
                TraceGranularityV1::Operation,
                "modi:alternate-plus-minus-on-closed-loop",
            ),
            (TransportationPreset::Modi, Self::AdjustCycle) => metadata(
                "modi.adjust-closed-loop",
                TraceGranularityV1::Operation,
                "modi:add-subtract-theta",
            ),
            (TransportationPreset::Modi, Self::DegeneratePivot) => metadata(
                "modi.degenerate-loop-adjustment",
                TraceGranularityV1::Operation,
                "modi:exchange-zero-basic-cell",
            ),
            (TransportationPreset::Modi, Self::ExchangeBasis) => metadata(
                "modi.update-basic-cells",
                TraceGranularityV1::Operation,
                "modi:replace-leaving-cell-and-recompute-u-v",
            ),
            (TransportationPreset::Modi, Self::Optimal) => metadata(
                "modi.optimal",
                TraceGranularityV1::Phase,
                "modi:all-opportunity-costs-nonnegative",
            ),
        }
    }
}

const fn metadata(
    catalog_id: &'static str,
    minimum_granularity: TraceGranularityV1,
    pseudocode_line: &'static str,
) -> FlowTraceEventMetadata {
    FlowTraceEventMetadata {
        catalog_id,
        minimum_granularity,
        pseudocode_line,
    }
}

struct TraceView {
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    focus: Vec<FlowTraceEntityRef>,
}

impl TraceView {
    const fn idle() -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
            focus: Vec::new(),
        }
    }

    const fn basis() -> Self {
        Self::idle()
    }

    fn cycle(active_path: Vec<ResidualArcId>) -> Self {
        let focus = active_path
            .first()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .into_iter()
            .collect();
        Self {
            search_order: Vec::new(),
            active_path,
            focus,
        }
    }

    fn edge(edge: crate::model::EdgeId) -> Self {
        Self {
            search_order: Vec::new(),
            active_path: Vec::new(),
            focus: vec![FlowTraceEntityRef::Edge(edge)],
        }
    }
}

fn start_recorder<'graph>(
    graph: &'graph FlowNetwork,
    required: &[i128],
    initial_flows: &[u64],
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, TransportationError> {
    if !enabled {
        return Ok(None);
    }
    let state = ResidualState::from_flows(graph, initial_flows)?;
    let base = FlowTraceSnapshot::capture(
        graph,
        &state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        required.to_vec(),
        FlowTraceMetrics::default(),
    );
    FlowTraceRecorder::new(graph, base)
        .map(Some)
        .map_err(TransportationError::Trace)
}

fn record(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    work: &mut WorkingState<'_, '_>,
    metadata: FlowTraceEventMetadata,
    view: TraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), TransportationError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    work.trace_transitions = work
        .trace_transitions
        .checked_add(1)
        .ok_or(TransportationError::ArithmeticOverflow)?;
    if work.trace_transitions > TRANSPORTATION_MAX_STATE_TRANSITIONS {
        return Err(TransportationError::WorkLimit);
    }
    let cells = u128::try_from(work.graph.nodes().len())
        .ok()
        .and_then(|nodes| {
            u128::try_from(work.graph.edges().len())
                .ok()
                .and_then(|edges| edges.checked_mul(3))
                .and_then(|edge_cells| nodes.checked_add(edge_cells))
        })
        .ok_or(TransportationError::ArithmeticOverflow)?;
    let boundaries = u128::from(work.trace_transitions)
        .checked_add(1)
        .ok_or(TransportationError::ArithmeticOverflow)?;
    if cells
        .checked_mul(boundaries)
        .ok_or(TransportationError::ArithmeticOverflow)?
        > TRANSPORTATION_MAX_TRACE_PROJECTION_CELLS
    {
        return Err(TransportationError::WorkLimit);
    }
    let focus = view.focus.clone();
    let snapshot = trace_snapshot(work, view)?;
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
    Ok(())
}

fn trace_snapshot(
    work: &WorkingState<'_, '_>,
    view: TraceView,
) -> Result<FlowTraceSnapshot, TransportationError> {
    let labels = work.potentials.clone();
    Ok(FlowTraceSnapshot::capture(
        work.graph,
        &work.state,
        labels,
        view.search_order,
        view.active_path,
        work.remaining_divergence()?,
        trace_metrics(work.metrics),
    )
    .with_forest_overlay(work.graph, work.forest_overlay()?, Vec::new()))
}

const fn trace_metrics(metrics: TransportationMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.feasibility_searches as u128,
        relaxation_passes: metrics.support_cycle_cancellations as u128,
        residual_arc_scans: metrics.pricing_scans,
        augmentations: metrics.pivots as u128,
        path_searches: metrics.pricing_searches as u128,
        scaling_phases: metrics.basis_extensions as u128,
        blocking_flow_phases: metrics.basis_exchanges as u128,
        relabels: metrics.potential_recomputations as u128,
        retreats: metrics.degenerate_pivots as u128,
        reverse_bfs_runs: metrics.structure_scans,
        gap_terminations: metrics.nondegenerate_pivots as u128,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
    }
}

#[derive(Clone, Debug)]
struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            rank: vec![0; count],
        }
    }

    fn find(&mut self, node: usize) -> usize {
        let parent = self.parent[node];
        if parent != node {
            self.parent[node] = self.find(parent);
        }
        self.parent[node]
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return false;
        }
        if self.rank[left_root] < self.rank[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parent[right_root] = left_root;
        if self.rank[left_root] == self.rank[right_root] {
            self.rank[left_root] = self.rank[left_root].saturating_add(1);
        }
        true
    }
}

fn usize_to_i128(value: usize) -> Result<i128, TransportationError> {
    i128::try_from(value).map_err(|_| TransportationError::ArithmeticOverflow)
}

fn u128_to_i128(value: u128) -> Result<i128, TransportationError> {
    i128::try_from(value).map_err(|_| TransportationError::ArithmeticOverflow)
}

/// Independently checks a transportation infeasibility witness.
///
/// This public helper keeps WASM integration from trusting the feasibility
/// search that produced the cut.
///
/// # Errors
///
/// Rejects a malformed transportation model or an inexact balance cut.
pub fn check_transportation_infeasibility(
    graph: &FlowNetwork,
    origins: &[String],
    destinations: &[String],
    witness: &InfeasibilityWitness,
) -> Result<(), TransportationError> {
    let model = TransportationGraph::new(graph, origins, destinations)?;
    crate::feasibility::check_balance_infeasibility(
        graph,
        &model.required_divergence(graph),
        witness,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::solve_primal_network_simplex;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, FlowTracePatch, apply_trace_event};

    fn graph(
        supplies: &[u64],
        demands: &[u64],
        routes: &[(usize, usize, i64)],
    ) -> (FlowNetwork, Vec<String>, Vec<String>) {
        let origins = (0..supplies.len())
            .map(|index| format!("o{index:02}"))
            .collect::<Vec<_>>();
        let destinations = (0..demands.len())
            .map(|index| format!("d{index:02}"))
            .collect::<Vec<_>>();
        let nodes = origins
            .iter()
            .zip(supplies)
            .map(|(id, &supply)| {
                FlowNode::new(
                    NodeId::parse(id).expect("origin"),
                    i64::try_from(supply).expect("supply"),
                )
            })
            .chain(destinations.iter().zip(demands).map(|(id, &demand)| {
                FlowNode::new(
                    NodeId::parse(id).expect("destination"),
                    i64::try_from(demand)
                        .expect("demand")
                        .checked_neg()
                        .expect("negative"),
                )
            }))
            .collect();
        let edges = routes
            .iter()
            .enumerate()
            .map(
                |(ordinal, &(origin, destination, cost))| UnresolvedFlowEdge {
                    id: EdgeId::parse(&format!("e{ordinal:03}")).expect("edge"),
                    from: NodeId::parse(&origins[origin]).expect("from"),
                    to: NodeId::parse(&destinations[destination]).expect("to"),
                    lower: 0,
                    capacity: supplies[origin].min(demands[destination]),
                    cost,
                },
            )
            .collect();
        (
            FlowNetwork::new(nodes, edges).expect("transportation network"),
            origins,
            destinations,
        )
    }

    #[test]
    fn simplex_and_modi_match_independent_network_simplex_and_replay() {
        let routes = [
            (0, 0, 8),
            (0, 1, 6),
            (0, 2, 10),
            (1, 0, 9),
            (1, 1, 7),
            (1, 2, 4),
        ];
        let (graph, origins, destinations) = graph(&[4, 3], &[2, 3, 2], &routes);
        let target = graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect::<Vec<_>>();
        let oracle = solve_primal_network_simplex(&graph, &target).expect("network simplex oracle");
        let simplex = solve_transportation_simplex(&graph, &origins, &destinations)
            .expect("transportation simplex");
        let modi = solve_modi(&graph, &origins, &destinations).expect("MODI");
        assert_eq!(
            simplex.certificate.total_cost,
            oracle.certificate.total_cost
        );
        assert_eq!(modi.certificate.total_cost, oracle.certificate.total_cost);
        assert_eq!(simplex.flows, modi.flows);

        for run in [
            trace_transportation_simplex(&graph, &origins, &destinations).expect("trace"),
            trace_modi(&graph, &origins, &destinations).expect("MODI trace"),
        ] {
            let mut replay = run.base_snapshot.clone();
            let mut price_events = 0;
            for event in &run.events {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                    .expect("forward replay");
                if matches!(
                    event.catalog_id.as_str(),
                    "transportation-simplex.bland-price" | "modi.compute-uv-opportunity-cost"
                ) {
                    price_events += 1;
                    assert!(
                        replay.search_order.is_empty(),
                        "aggregate pricing must not publish every inspected endpoint"
                    );
                    assert!(
                        replay.active_path.len() <= 1,
                        "aggregate pricing may focus only the chosen entering route"
                    );
                }
            }
            assert!(price_events > 0, "trace must publish a pricing boundary");
            assert_eq!(replay, run.final_snapshot);
            for event in run.events.iter().rev() {
                apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                    .expect("reverse replay");
            }
            assert_eq!(replay, run.base_snapshot);
        }
    }

    #[test]
    fn sparse_hall_cut_is_reported_and_independently_checked() {
        let (graph, origins, destinations) =
            graph(&[3, 3], &[2, 4], &[(0, 0, 1), (1, 0, 2), (1, 1, 3)]);
        let error = solve_transportation_simplex(&graph, &origins, &destinations)
            .expect_err("route cut is infeasible");
        let TransportationError::Feasibility(FeasibilityError::Infeasible(witness)) = error else {
            panic!("expected cut witness");
        };
        check_transportation_infeasibility(&graph, &origins, &destinations, &witness)
            .expect("independent cut checker");
    }

    #[test]
    fn zero_basic_route_forces_a_traced_degenerate_bland_pivot() {
        let routes = [
            (0, 0, 0),
            (1, 1, 0),
            (2, 2, 0),
            (0, 1, 0),
            (1, 2, 0),
            (0, 2, -1),
        ];
        let (graph, origins, destinations) = graph(&[1, 1, 1], &[1, 1, 1], &routes);
        let traced = trace_transportation_simplex(&graph, &origins, &destinations)
            .expect("degenerate transportation trace");

        assert!(traced.result.metrics.degenerate_pivots > 0);
        assert!(traced.result.metrics.structure_scans > 0);
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "transportation-simplex.degenerate-pivot"
                && event
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.value == 0)
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "transportation-simplex.form-fundamental-cycle"
                && event
                    .patches
                    .iter()
                    .any(|patch| matches!(patch, FlowTracePatch::ActivePath { after, .. } if !after.is_empty()))
        }));
    }
}
