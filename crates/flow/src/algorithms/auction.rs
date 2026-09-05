//! Sequential epsilon-scaling auction algorithm for native assignment.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::assignment::{
    ASSIGNMENT_MAX_EDGES, ASSIGNMENT_MAX_NODES, AssignmentEdge, AssignmentGraph,
    AssignmentModelError, AssignmentObjectiveV1,
};
use crate::certificate::{
    AssignmentCertificate, AssignmentHallWitness, CertificateError, certify_assignment_optimality,
    check_assignment_infeasibility,
};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for the sequential auction.
pub const AUCTION_MAX_NODES: usize = ASSIGNMENT_MAX_NODES;
/// Conservative interactive allowed-edge limit.
pub const AUCTION_MAX_EDGES: usize = ASSIGNMENT_MAX_EDGES;
/// Feasibility, bidding, and epsilon-CS edge-scan ceiling.
pub const AUCTION_MAX_EDGE_SCANS: u128 = 20_000_000;
/// Bid ceiling, chosen so eager operation traces remain bounded.
pub const AUCTION_MAX_BIDS: u64 = 40_000;
/// Explicit eager-trace transition ceiling.
pub const AUCTION_MAX_STATE_TRANSITIONS: u64 = 100_000;
/// Aggregate full-snapshot cells admitted by the eager trace projector.
///
/// A boundary touches every node, original edge, and both residual directions.
/// Fast mode is unaffected by this trace-only ceiling.
pub const AUCTION_MAX_TRACE_PROJECTION_CELLS: u128 = 4_000_000;

/// Exact counters from feasibility precheck and sequential bidding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuctionMetrics {
    /// Alternating feasibility searches.
    pub feasibility_searches: u64,
    /// Feasibility augmentations before bidding.
    pub feasibility_augmentations: u64,
    /// Allowed assignment edges inspected by all bounded phases.
    pub edge_scans: u128,
    /// Epsilon scales entered.
    pub scaling_phases: u64,
    /// Unassigned-agent bids.
    pub bids: u64,
    /// Object price increases.
    pub price_raises: u64,
    /// Object awards, including reassignment.
    pub awards: u64,
    /// Previously assigned agents displaced by an award.
    pub evictions: u64,
}

/// Certified terminal result of the auction solver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuctionOutcome {
    /// Exact optimum, independently certified after the final epsilon scale.
    Optimal {
        /// Unit-flow projection in canonical original-edge order.
        flows: Vec<u64>,
        /// Exact primal/dual certificate reconstructed independently of prices.
        certificate: AssignmentCertificate,
    },
    /// Hall-deficient allowed-edge neighborhood found before price iteration.
    Infeasible {
        /// Maximum-cardinality partial matching from the feasibility precheck.
        partial_flows: Vec<u64>,
        /// Independently checked Hall witness.
        witness: AssignmentHallWitness,
    },
}

/// Certified auction result and deterministic work counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionResult {
    /// Exact optimum or verified infeasibility.
    pub outcome: AuctionOutcome,
    /// Exact bounded-work counters.
    pub metrics: AuctionMetrics,
}

/// Certified auction result with reversible scale, bid, and award events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuctionTraceResult {
    /// Same terminal result produced by the fast profile.
    pub result: AuctionResult,
    /// Replay boundary before feasibility checking and bidding.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible trace events.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after terminal certificate verification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Native auction construction, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AuctionError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds Auction admission limits")]
    AdmissionLimit,
    /// Exact scan, bid, or trace-transition ceiling was exceeded.
    #[error("Auction work limit exceeded")]
    WorkLimit,
    /// Native assignment model validation failed.
    #[error(transparent)]
    Model(#[from] AssignmentModelError),
    /// Atomic award flow mutation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent optimality or infeasibility evidence failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact value, price, epsilon, or counter arithmetic overflowed.
    #[error("Auction arithmetic overflow")]
    ArithmeticOverflow,
    /// Matching, ownership, or epsilon-CS state contradicted the algorithm.
    #[error("Auction assignment invariant failed")]
    Invariant,
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves a native rectangular assignment by sequential epsilon-scaling auction.
///
/// Costs are converted to maximization benefits and multiplied by `a + 1`,
/// where `a` is the agent count. The final epsilon is one, so epsilon-CS gives
/// a scaled gap below `a + 1` and therefore an exact integral optimum.
///
/// # Errors
///
/// Rejects malformed/out-of-band models, bounded-work exhaustion, arithmetic
/// overflow, invariant failure, or independently rejected terminal evidence.
pub fn solve_auction(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
) -> Result<AuctionResult, AuctionError> {
    solve_internal(graph, agents, tasks, objective, false).map(|run| run.result)
}

/// Solves while recording epsilon scales, bids, atomic awards, and evidence.
///
/// # Errors
///
/// Returns the same failures as [`solve_auction`], plus trace projection
/// failures.
pub fn trace_auction(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
) -> Result<AuctionTraceResult, AuctionError> {
    let run = solve_internal(graph, agents, tasks, objective, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(AuctionError::Invariant)?;
    Ok(AuctionTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: AuctionResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
    with_trace: bool,
) -> Result<InternalRun, AuctionError> {
    if graph.nodes().len() > AUCTION_MAX_NODES || graph.edges().len() > AUCTION_MAX_EDGES {
        return Err(AuctionError::AdmissionLimit);
    }
    let model = AssignmentGraph::new(graph, agents, tasks, objective)?;
    let mut kernel = AuctionKernel::new(graph, &model, with_trace)?;
    let outcome = kernel.solve(graph, &model)?;
    Ok(kernel.finish(outcome))
}

struct AuctionKernel<'graph> {
    scale_factor: i128,
    task_prices: Vec<i128>,
    task_owner: Vec<Option<usize>>,
    agent_task: Vec<Option<usize>>,
    state: ResidualState<'graph>,
    metrics: AuctionMetrics,
    transitions: u64,
    prices_initialized: bool,
    recorder: Option<FlowTraceRecorder<'graph>>,
}

impl<'graph> AuctionKernel<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        with_trace: bool,
    ) -> Result<Self, AuctionError> {
        let scale_factor = i128::try_from(model.agents.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(AuctionError::ArithmeticOverflow)?;
        let state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
        let metrics = AuctionMetrics::default();
        let task_prices = vec![0; model.tasks.len()];
        let agent_task = vec![None; model.agents.len()];
        let recorder = start_trace(
            graph,
            &state,
            model,
            &task_prices,
            &agent_task,
            scale_factor,
            metrics,
            with_trace,
        )?;
        Ok(Self {
            scale_factor,
            task_prices,
            task_owner: vec![None; model.tasks.len()],
            agent_task,
            state,
            metrics,
            transitions: 0,
            prices_initialized: false,
            recorder,
        })
    }

    fn solve(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<AuctionOutcome, AuctionError> {
        if let Some((partial_flows, witness)) = self.feasibility_precheck(graph, model)? {
            self.state = ResidualState::from_flows(graph, &partial_flows)?;
            let view = AuctionView::witness(graph, model, &witness)?;
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "auction.hall-witness",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "auction:reject-hall-deficient-assignment",
                },
                &view,
                Vec::new(),
                Some(("deficiency", i128::from(witness.deficiency))),
            )?;
            return Ok(AuctionOutcome::Infeasible {
                partial_flows,
                witness,
            });
        }

        let mut epsilon = initial_epsilon(model, self.scale_factor)?;
        loop {
            self.begin_scale(graph, model, epsilon)?;
            self.run_scale(graph, model, epsilon)?;
            self.verify_epsilon_cs(graph, model, epsilon)?;
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "auction.scale-complete",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "auction:complete-epsilon-cs-assignment",
                },
                &AuctionView::idle(),
                Vec::new(),
                Some(("epsilon", epsilon)),
            )?;
            if epsilon == 1 {
                break;
            }
            epsilon = epsilon
                .checked_div(4)
                .ok_or(AuctionError::ArithmeticOverflow)?;
            if epsilon == 0 {
                return Err(AuctionError::Invariant);
            }
        }
        self.optimal_outcome(graph, model)
    }

    fn feasibility_precheck(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<Option<(Vec<u64>, AssignmentHallWitness)>, AuctionError> {
        let mut task_owner = vec![None; model.tasks.len()];
        let mut agent_task = vec![None; model.agents.len()];
        loop {
            let mut progressed = false;
            for root in 0..model.agents.len() {
                if agent_task[root].is_some() {
                    continue;
                }
                self.metrics.feasibility_searches =
                    checked_add_u64(self.metrics.feasibility_searches, 1)?;
                if self.augment_feasibility(graph, model, root, &mut task_owner, &mut agent_task)? {
                    self.metrics.feasibility_augmentations =
                        checked_add_u64(self.metrics.feasibility_augmentations, 1)?;
                    progressed = true;
                }
            }
            if !progressed || agent_task.iter().all(Option::is_some) {
                break;
            }
        }
        if agent_task.iter().all(Option::is_some) {
            return Ok(None);
        }
        let witness = self.hall_witness(graph, model, &task_owner, &agent_task)?;
        check_assignment_infeasibility(graph, model, &witness)?;
        let partial_flows = partial_flows(graph, model, &task_owner)?;
        Ok(Some((partial_flows, witness)))
    }

    fn augment_feasibility(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        root: usize,
        task_owner: &mut [Option<usize>],
        agent_task: &mut [Option<usize>],
    ) -> Result<bool, AuctionError> {
        let mut queue = VecDeque::from([root]);
        let mut seen_agents = vec![false; model.agents.len()];
        let mut seen_tasks = vec![false; model.tasks.len()];
        let mut predecessor_agent = vec![None; model.tasks.len()];
        seen_agents[root] = true;
        while let Some(agent) = queue.pop_front() {
            for &ordinal in &model.adjacency[agent] {
                self.inspect_assignment_edge(graph, model, agent, ordinal)?;
                let edge = model.edges.get(ordinal).ok_or(AuctionError::Invariant)?;
                if std::mem::replace(&mut seen_tasks[edge.task], true) {
                    continue;
                }
                predecessor_agent[edge.task] = Some(agent);
                if task_owner[edge.task].is_none() {
                    apply_feasibility_augmentation(
                        edge.task,
                        &predecessor_agent,
                        task_owner,
                        agent_task,
                    )?;
                    return Ok(true);
                }
                let owner = task_owner[edge.task].ok_or(AuctionError::Invariant)?;
                if !std::mem::replace(&mut seen_agents[owner], true) {
                    queue.push_back(owner);
                }
            }
        }
        Ok(false)
    }

    fn hall_witness(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        task_owner: &[Option<usize>],
        agent_task: &[Option<usize>],
    ) -> Result<AssignmentHallWitness, AuctionError> {
        let mut queue = VecDeque::new();
        let mut reachable_agents = vec![false; model.agents.len()];
        let mut reachable_tasks = vec![false; model.tasks.len()];
        for (agent, task) in agent_task.iter().enumerate() {
            if task.is_none() {
                reachable_agents[agent] = true;
                queue.push_back(agent);
            }
        }
        while let Some(agent) = queue.pop_front() {
            for &ordinal in &model.adjacency[agent] {
                self.inspect_assignment_edge(graph, model, agent, ordinal)?;
                let edge = model.edges.get(ordinal).ok_or(AuctionError::Invariant)?;
                if agent_task[agent] == Some(edge.task) || reachable_tasks[edge.task] {
                    continue;
                }
                reachable_tasks[edge.task] = true;
                let owner = task_owner[edge.task].ok_or(AuctionError::Invariant)?;
                if !std::mem::replace(&mut reachable_agents[owner], true) {
                    queue.push_back(owner);
                }
            }
        }
        let agents = reachable_agents
            .iter()
            .enumerate()
            .filter(|(_, reachable)| **reachable)
            .map(|(agent, _)| {
                graph
                    .node(model.agents[agent])
                    .map(|node| node.id().clone())
                    .ok_or(AuctionError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let neighbor_tasks = reachable_tasks
            .iter()
            .enumerate()
            .filter(|(_, reachable)| **reachable)
            .map(|(task, _)| {
                graph
                    .node(model.tasks[task])
                    .map(|node| node.id().clone())
                    .ok_or(AuctionError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let deficiency = agents
            .len()
            .checked_sub(neighbor_tasks.len())
            .and_then(|value| u64::try_from(value).ok())
            .filter(|&value| value > 0)
            .ok_or(AuctionError::Invariant)?;
        Ok(AssignmentHallWitness {
            agents,
            neighbor_tasks,
            deficiency,
        })
    }

    fn begin_scale(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        epsilon: i128,
    ) -> Result<(), AuctionError> {
        self.metrics.scaling_phases = checked_add_u64(self.metrics.scaling_phases, 1)?;
        // Forward auction on a rectangular graph also needs every unassigned
        // task price to be no larger than every assigned task price. Equal
        // initial prices preserve that condition because a task that receives
        // a bid remains assigned for the rest of the phase. Symmetric phases
        // have no unassigned task and can retain their warm-start prices.
        if model.agents.len() < model.tasks.len() {
            self.task_prices.fill(0);
        }
        self.task_owner.fill(None);
        self.agent_task.fill(None);
        self.state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
        self.prices_initialized = true;
        self.record(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "auction.scale-start",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "auction:reset-assignment-retain-prices",
            },
            &AuctionView::idle(),
            Vec::new(),
            Some(("epsilon", epsilon)),
        )
    }

    fn run_scale(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        epsilon: i128,
    ) -> Result<(), AuctionError> {
        let mut unassigned = (0..model.agents.len()).collect::<VecDeque<_>>();
        while let Some(agent) = unassigned.pop_front() {
            if self.agent_task[agent].is_some() {
                return Err(AuctionError::Invariant);
            }
            self.metrics.bids = checked_add_u64(self.metrics.bids, 1)?;
            if self.metrics.bids > AUCTION_MAX_BIDS {
                return Err(AuctionError::WorkLimit);
            }
            let bid = self.select_bid(graph, model, agent, epsilon)?;
            let edge = model
                .edges
                .get(bid.edge_ordinal)
                .ok_or(AuctionError::Invariant)?;
            let forward = residual_id(graph, edge, ResidualDirection::Forward)?;
            self.record_edge_primitive(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "auction.bid",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "auction:bid-best-net-value-plus-epsilon",
                },
                &AuctionView::bid(agent, edge.task, bid.best_net),
                vec![forward.clone()],
                Some(("bid-increment", bid.increment)),
            )?;

            let previous_owner = self.task_owner[edge.task];
            let mut award_path = vec![forward];
            if let Some(previous_agent) = previous_owner {
                let previous_task = self.agent_task[previous_agent]
                    .take()
                    .ok_or(AuctionError::Invariant)?;
                if previous_task != edge.task {
                    return Err(AuctionError::Invariant);
                }
                let previous_ordinal =
                    model.edge_by_pair[previous_agent][edge.task].ok_or(AuctionError::Invariant)?;
                let previous_edge = model
                    .edges
                    .get(previous_ordinal)
                    .ok_or(AuctionError::Invariant)?;
                award_path.push(residual_id(
                    graph,
                    previous_edge,
                    ResidualDirection::Reverse,
                )?);
                self.metrics.evictions = checked_add_u64(self.metrics.evictions, 1)?;
                unassigned.push_back(previous_agent);
            }
            self.state.augment(&award_path, 1)?;
            self.task_prices[edge.task] = self.task_prices[edge.task]
                .checked_add(bid.increment)
                .ok_or(AuctionError::ArithmeticOverflow)?;
            self.task_owner[edge.task] = Some(agent);
            self.agent_task[agent] = Some(edge.task);
            self.metrics.price_raises = checked_add_u64(self.metrics.price_raises, 1)?;
            self.metrics.awards = checked_add_u64(self.metrics.awards, 1)?;
            self.verify_assignment_state(graph, model)?;
            let assigned_net = benefit(edge, model.objective, self.scale_factor)?
                .checked_sub(self.task_prices[edge.task])
                .ok_or(AuctionError::ArithmeticOverflow)?;
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "auction.award",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "auction:raise-price-award-and-evict",
                },
                &AuctionView::award(agent, edge.task, previous_owner, assigned_net),
                award_path,
                Some(("price", self.task_prices[edge.task])),
            )?;
        }
        Ok(())
    }

    fn select_bid(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        agent: usize,
        epsilon: i128,
    ) -> Result<Bid, AuctionError> {
        let mut best: Option<(usize, i128)> = None;
        let mut second: Option<(usize, i128)> = None;
        for &ordinal in &model.adjacency[agent] {
            self.inspect_assignment_edge(graph, model, agent, ordinal)?;
            let edge = model.edges.get(ordinal).ok_or(AuctionError::Invariant)?;
            let net = benefit(edge, model.objective, self.scale_factor)?
                .checked_sub(self.task_prices[edge.task])
                .ok_or(AuctionError::ArithmeticOverflow)?;
            insert_ranked_candidate((ordinal, net), model, &mut best, &mut second)?;
        }
        let (edge_ordinal, best_net) = best.ok_or(AuctionError::Invariant)?;
        let increment = second.map_or(Ok(epsilon), |(_, second_net)| {
            best_net
                .checked_sub(second_net)
                .and_then(|gap| gap.checked_add(epsilon))
                .ok_or(AuctionError::ArithmeticOverflow)
        })?;
        if increment < epsilon {
            return Err(AuctionError::Invariant);
        }
        Ok(Bid {
            edge_ordinal,
            best_net,
            increment,
        })
    }

    fn verify_epsilon_cs(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        epsilon: i128,
    ) -> Result<(), AuctionError> {
        if self.agent_task.iter().any(Option::is_none) {
            return Err(AuctionError::Invariant);
        }
        for agent in 0..model.agents.len() {
            let assigned_task = self.agent_task[agent].ok_or(AuctionError::Invariant)?;
            let assigned_ordinal =
                model.edge_by_pair[agent][assigned_task].ok_or(AuctionError::Invariant)?;
            let assigned_edge = model
                .edges
                .get(assigned_ordinal)
                .ok_or(AuctionError::Invariant)?;
            let assigned_net = benefit(assigned_edge, model.objective, self.scale_factor)?
                .checked_sub(self.task_prices[assigned_task])
                .ok_or(AuctionError::ArithmeticOverflow)?;
            for &ordinal in &model.adjacency[agent] {
                self.inspect_assignment_edge(graph, model, agent, ordinal)?;
                let edge = model.edges.get(ordinal).ok_or(AuctionError::Invariant)?;
                let net = benefit(edge, model.objective, self.scale_factor)?
                    .checked_sub(self.task_prices[edge.task])
                    .ok_or(AuctionError::ArithmeticOverflow)?;
                if assigned_net
                    .checked_add(epsilon)
                    .ok_or(AuctionError::ArithmeticOverflow)?
                    < net
                {
                    return Err(AuctionError::Invariant);
                }
            }
        }
        self.verify_assignment_state(graph, model)
    }

    fn optimal_outcome(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<AuctionOutcome, AuctionError> {
        let flows = self.state.flows().to_vec();
        let certificate = certify_assignment_optimality(graph, model, &flows)?;
        self.record(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "auction.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "auction:verify-exact-integral-optimum",
            },
            &AuctionView::idle(),
            Vec::new(),
            Some(("objective", certificate.total_cost)),
        )?;
        Ok(AuctionOutcome::Optimal { flows, certificate })
    }

    fn verify_assignment_state(
        &self,
        graph: &FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<(), AuctionError> {
        let mut expected = vec![0; graph.edges().len()];
        let mut seen_agents = vec![false; model.agents.len()];
        for (task, owner) in self.task_owner.iter().copied().enumerate() {
            let Some(agent) = owner else { continue };
            if std::mem::replace(&mut seen_agents[agent], true)
                || self.agent_task[agent] != Some(task)
            {
                return Err(AuctionError::Invariant);
            }
            let ordinal = model.edge_by_pair[agent][task].ok_or(AuctionError::Invariant)?;
            expected[model.edges[ordinal].edge.as_usize()] = 1;
        }
        if self
            .agent_task
            .iter()
            .enumerate()
            .any(|(agent, task)| task.is_some() != seen_agents[agent])
            || expected != self.state.flows()
        {
            return Err(AuctionError::Invariant);
        }
        Ok(())
    }

    fn increment_edge_scans(&mut self) -> Result<(), AuctionError> {
        self.metrics.edge_scans = self
            .metrics
            .edge_scans
            .checked_add(1)
            .ok_or(AuctionError::ArithmeticOverflow)?;
        if self.metrics.edge_scans > AUCTION_MAX_EDGE_SCANS {
            return Err(AuctionError::WorkLimit);
        }
        Ok(())
    }

    fn inspect_assignment_edge(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        agent: usize,
        ordinal: usize,
    ) -> Result<(), AuctionError> {
        self.increment_edge_scans()?;
        let edge = model.edges.get(ordinal).ok_or(AuctionError::Invariant)?;
        let forward = residual_id(graph, edge, ResidualDirection::Forward)?;
        self.record_edge_primitive(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "auction.inspect-assignment-edge",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "auction:inspect-one-allowed-assignment-edge",
            },
            &AuctionView::inspect(agent, edge.task),
            vec![forward],
            Some((
                "scan",
                i128::try_from(self.metrics.edge_scans)
                    .map_err(|_| AuctionError::ArithmeticOverflow)?,
            )),
        )
    }

    fn record(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        metadata: FlowTraceEventMetadata,
        view: &AuctionView,
        path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), AuctionError> {
        self.record_with_focus(graph, model, metadata, view, path, detail, None)
    }

    fn record_edge_primitive(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        metadata: FlowTraceEventMetadata,
        view: &AuctionView,
        path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
    ) -> Result<(), AuctionError> {
        let focus = path
            .iter()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .collect();
        self.record_with_focus(graph, model, metadata, view, path, detail, Some(focus))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the source trace boundary keeps reversible state and its local focus explicit"
    )]
    fn record_with_focus(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        metadata: FlowTraceEventMetadata,
        view: &AuctionView,
        path: Vec<ResidualArcId>,
        detail: Option<(&'static str, i128)>,
        focus: Option<Vec<FlowTraceEntityRef>>,
    ) -> Result<(), AuctionError> {
        let Some(recorder) = self.recorder.as_mut() else {
            return Ok(());
        };
        self.transitions = checked_add_u64(self.transitions, 1)?;
        if self.transitions > AUCTION_MAX_STATE_TRANSITIONS {
            return Err(AuctionError::WorkLimit);
        }
        check_trace_projection_budget(graph, self.transitions)?;
        let snapshot = auction_snapshot(
            graph,
            model,
            &self.state,
            &self.task_prices,
            &self.agent_task,
            self.scale_factor,
            self.prices_initialized,
            view,
            path,
            self.metrics,
        )?;
        if let Some(focus) = focus {
            recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)?;
        } else {
            recorder.record_transition_with_detail(metadata, &snapshot, detail)?;
        }
        Ok(())
    }

    fn finish(self, outcome: AuctionOutcome) -> InternalRun {
        InternalRun {
            result: AuctionResult {
                outcome,
                metrics: self.metrics,
            },
            trace: self.recorder.map(FlowTraceRecorder::finish),
        }
    }
}

fn check_trace_projection_budget(
    graph: &FlowNetwork,
    transitions: u64,
) -> Result<(), AuctionError> {
    let snapshot_cells = u128::try_from(graph.nodes().len())
        .ok()
        .and_then(|nodes| {
            u128::try_from(graph.edges().len())
                .ok()
                .and_then(|edges| edges.checked_mul(3))
                .and_then(|edge_cells| nodes.checked_add(edge_cells))
        })
        .ok_or(AuctionError::ArithmeticOverflow)?;
    let boundaries = u128::from(transitions)
        .checked_add(1)
        .ok_or(AuctionError::ArithmeticOverflow)?;
    let projected_cells = snapshot_cells
        .checked_mul(boundaries)
        .ok_or(AuctionError::ArithmeticOverflow)?;
    if projected_cells > AUCTION_MAX_TRACE_PROJECTION_CELLS {
        return Err(AuctionError::WorkLimit);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Bid {
    edge_ordinal: usize,
    best_net: i128,
    increment: i128,
}

struct AuctionView {
    active_agent: Option<usize>,
    active_task: Option<usize>,
    displaced_agent: Option<usize>,
    bidder_net: Option<i128>,
    witness_order: Option<Vec<NodeIndex>>,
}

impl AuctionView {
    const fn idle() -> Self {
        Self {
            active_agent: None,
            active_task: None,
            displaced_agent: None,
            bidder_net: None,
            witness_order: None,
        }
    }

    const fn bid(agent: usize, task: usize, bidder_net: i128) -> Self {
        Self {
            active_agent: Some(agent),
            active_task: Some(task),
            displaced_agent: None,
            bidder_net: Some(bidder_net),
            witness_order: None,
        }
    }

    const fn inspect(agent: usize, task: usize) -> Self {
        Self {
            active_agent: Some(agent),
            active_task: Some(task),
            displaced_agent: None,
            bidder_net: None,
            witness_order: None,
        }
    }

    const fn award(
        agent: usize,
        task: usize,
        displaced_agent: Option<usize>,
        bidder_net: i128,
    ) -> Self {
        Self {
            active_agent: Some(agent),
            active_task: Some(task),
            displaced_agent,
            bidder_net: Some(bidder_net),
            witness_order: None,
        }
    }

    fn witness(
        graph: &FlowNetwork,
        model: &AssignmentGraph,
        witness: &AssignmentHallWitness,
    ) -> Result<Self, AuctionError> {
        let witness_order = witness
            .agents
            .iter()
            .chain(&witness.neighbor_tasks)
            .map(|id| graph.node_index(id).ok_or(AuctionError::Invariant))
            .collect::<Result<Vec<_>, _>>()?;
        if witness_order
            .iter()
            .any(|node| !model.agents.contains(node) && !model.tasks.contains(node))
        {
            return Err(AuctionError::Invariant);
        }
        Ok(Self {
            witness_order: Some(witness_order),
            ..Self::idle()
        })
    }
}

fn checked_add_u64(value: u64, delta: u64) -> Result<u64, AuctionError> {
    value
        .checked_add(delta)
        .ok_or(AuctionError::ArithmeticOverflow)
}

fn benefit(
    edge: &AssignmentEdge,
    objective: AssignmentObjectiveV1,
    scale_factor: i128,
) -> Result<i128, AuctionError> {
    let value = match objective {
        AssignmentObjectiveV1::Minimize => i128::from(edge.cost)
            .checked_neg()
            .ok_or(AuctionError::ArithmeticOverflow)?,
        AssignmentObjectiveV1::Maximize => i128::from(edge.cost),
    };
    value
        .checked_mul(scale_factor)
        .ok_or(AuctionError::ArithmeticOverflow)
}

fn initial_epsilon(model: &AssignmentGraph, scale_factor: i128) -> Result<i128, AuctionError> {
    let mut minimum = None;
    let mut maximum = None;
    for edge in &model.edges {
        let value = benefit(edge, model.objective, scale_factor)?;
        minimum = Some(minimum.map_or(value, |current: i128| current.min(value)));
        maximum = Some(maximum.map_or(value, |current: i128| current.max(value)));
    }
    let range = maximum
        .zip(minimum)
        .and_then(|(high, low)| high.checked_sub(low))
        .ok_or(AuctionError::Invariant)?;
    let mut epsilon = 1_i128;
    while epsilon < range {
        epsilon = epsilon
            .checked_mul(4)
            .ok_or(AuctionError::ArithmeticOverflow)?;
    }
    Ok(epsilon)
}

fn insert_ranked_candidate(
    candidate: (usize, i128),
    model: &AssignmentGraph,
    best: &mut Option<(usize, i128)>,
    second: &mut Option<(usize, i128)>,
) -> Result<(), AuctionError> {
    let better = |left: (usize, i128), right: (usize, i128)| -> Result<bool, AuctionError> {
        let left_task = model
            .edges
            .get(left.0)
            .map(|edge| edge.task)
            .ok_or(AuctionError::Invariant)?;
        let right_task = model
            .edges
            .get(right.0)
            .map(|edge| edge.task)
            .ok_or(AuctionError::Invariant)?;
        Ok(left.1 > right.1 || (left.1 == right.1 && left_task < right_task))
    };
    match *best {
        None => *best = Some(candidate),
        Some(current) if better(candidate, current)? => {
            *second = Some(current);
            *best = Some(candidate);
        }
        Some(_) => match *second {
            None => *second = Some(candidate),
            Some(current) if better(candidate, current)? => *second = Some(candidate),
            Some(_) => {}
        },
    }
    Ok(())
}

fn apply_feasibility_augmentation(
    free_task: usize,
    predecessor_agent: &[Option<usize>],
    task_owner: &mut [Option<usize>],
    agent_task: &mut [Option<usize>],
) -> Result<(), AuctionError> {
    let mut task = free_task;
    loop {
        let agent = predecessor_agent[task].ok_or(AuctionError::Invariant)?;
        let previous_task = agent_task[agent];
        task_owner[task] = Some(agent);
        agent_task[agent] = Some(task);
        let Some(previous_task) = previous_task else {
            break;
        };
        task = previous_task;
    }
    Ok(())
}

fn partial_flows(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    task_owner: &[Option<usize>],
) -> Result<Vec<u64>, AuctionError> {
    let mut flows = vec![0; graph.edges().len()];
    let mut seen_agents = BTreeSet::new();
    for (task, owner) in task_owner.iter().copied().enumerate() {
        let Some(agent) = owner else { continue };
        if !seen_agents.insert(agent) {
            return Err(AuctionError::Invariant);
        }
        let ordinal = model.edge_by_pair[agent][task].ok_or(AuctionError::Invariant)?;
        flows[model.edges[ordinal].edge.as_usize()] = 1;
    }
    Ok(flows)
}

fn residual_id(
    graph: &FlowNetwork,
    edge: &AssignmentEdge,
    direction: ResidualDirection,
) -> Result<ResidualArcId, AuctionError> {
    graph
        .edge(edge.edge)
        .map(|value| ResidualArcId::new(value.id().clone(), direction))
        .ok_or(AuctionError::Invariant)
}

#[expect(
    clippy::too_many_arguments,
    reason = "trace base projection keeps scaled auction state explicit"
)]
fn start_trace<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    _model: &AssignmentGraph,
    _task_prices: &[i128],
    _agent_task: &[Option<usize>],
    _scale_factor: i128,
    _metrics: AuctionMetrics,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, AuctionError> {
    if !enabled {
        return Ok(None);
    }
    // The published timeline already owns the zero-flow Ready state. Auction
    // labels, prices, assignments, and counters belong to the source
    // algorithm and must first appear in a recorded transition; placing them
    // in the recorder base would replace Ready with untraced work.
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    FlowTraceRecorder::new(graph, snapshot)
        .map(Some)
        .map_err(AuctionError::Trace)
}

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot projection is an explicit contract boundary"
)]
fn auction_snapshot(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    state: &ResidualState<'_>,
    task_prices: &[i128],
    agent_task: &[Option<usize>],
    scale_factor: i128,
    prices_initialized: bool,
    view: &AuctionView,
    path: Vec<ResidualArcId>,
    metrics: AuctionMetrics,
) -> Result<FlowTraceSnapshot, AuctionError> {
    let mut labels = vec![None; graph.nodes().len()];
    if prices_initialized {
        for (&node, &price) in model.tasks.iter().zip(task_prices) {
            labels[node.as_usize()] = Some(price);
        }
        for (agent, task) in agent_task.iter().copied().enumerate() {
            let Some(task) = task else { continue };
            let ordinal = model.edge_by_pair[agent][task].ok_or(AuctionError::Invariant)?;
            let net = benefit(&model.edges[ordinal], model.objective, scale_factor)?
                .checked_sub(task_prices[task])
                .ok_or(AuctionError::ArithmeticOverflow)?;
            labels[model.agents[agent].as_usize()] = Some(net);
        }
    }
    if let (Some(agent), Some(net)) = (view.active_agent, view.bidder_net) {
        labels[model.agents[agent].as_usize()] = Some(net);
    }
    let search_order = if let Some(order) = &view.witness_order {
        order.clone()
    } else {
        [
            view.active_agent.map(|agent| model.agents[agent]),
            view.active_task.map(|task| model.tasks[task]),
            view.displaced_agent.map(|agent| model.agents[agent]),
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    Ok(FlowTraceSnapshot::capture(
        graph,
        state,
        labels,
        search_order,
        path,
        Vec::new(),
        trace_metrics(metrics),
    ))
}

const fn trace_metrics(metrics: AuctionMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.feasibility_searches as u128,
        relaxation_passes: metrics.price_raises as u128,
        residual_arc_scans: metrics.edge_scans,
        augmentations: metrics.awards as u128,
        path_searches: metrics.feasibility_augmentations as u128,
        scaling_phases: metrics.scaling_phases as u128,
        blocking_flow_phases: metrics.evictions as u128,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.bids as u128,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    fn graph(
        agent_count: usize,
        task_count: usize,
        edges: &[(usize, usize, i64)],
    ) -> (FlowNetwork, Vec<String>, Vec<String>) {
        let agents = (0..agent_count)
            .map(|index| format!("a{index:02}"))
            .collect::<Vec<_>>();
        let tasks = (0..task_count)
            .map(|index| format!("t{index:02}"))
            .collect::<Vec<_>>();
        let nodes = agents
            .iter()
            .chain(&tasks)
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edges = edges
            .iter()
            .enumerate()
            .map(|(ordinal, &(agent, task, cost))| UnresolvedFlowEdge {
                id: EdgeId::parse(&format!("e{ordinal:03}")).expect("edge"),
                from: NodeId::parse(&agents[agent]).expect("agent"),
                to: NodeId::parse(&tasks[task]).expect("task"),
                lower: 0,
                capacity: 1,
                cost,
            })
            .collect();
        (
            FlowNetwork::new(nodes, edges).expect("graph"),
            agents,
            tasks,
        )
    }

    #[test]
    fn solves_rectangular_minimize_and_maximize_with_extreme_costs() {
        let edges = [
            (0, 0, i64::MIN),
            (0, 1, 7),
            (0, 2, 8),
            (1, 0, 5),
            (1, 1, -9),
            (1, 2, i64::MAX),
        ];
        let (graph, agents, tasks) = graph(2, 3, &edges);
        let minimum = solve_auction(&graph, &agents, &tasks, AssignmentObjectiveV1::Minimize)
            .expect("minimum");
        let maximum = solve_auction(&graph, &agents, &tasks, AssignmentObjectiveV1::Maximize)
            .expect("maximum");
        match minimum.outcome {
            AuctionOutcome::Optimal { certificate, .. } => {
                assert_eq!(certificate.total_cost, i128::from(i64::MIN) - 9);
            }
            AuctionOutcome::Infeasible { .. } => panic!("feasible minimum"),
        }
        match maximum.outcome {
            AuctionOutcome::Optimal { certificate, .. } => {
                assert_eq!(certificate.total_cost, i128::from(i64::MAX) + 7);
            }
            AuctionOutcome::Infeasible { .. } => panic!("feasible maximum"),
        }
    }

    #[test]
    fn exhaustive_allowed_masks_match_brute_force_optima() {
        for mask in 0_u64..64 {
            let edges = (0..2)
                .flat_map(|agent| (0..3).map(move |task| (agent, task)))
                .enumerate()
                .filter(|(bit, _)| mask & (1_u64 << bit) != 0)
                .map(|(_, (agent, task))| {
                    let cost = [[7_i64, -4, 11], [3, 9, -8]][agent][task];
                    (agent, task, cost)
                })
                .collect::<Vec<_>>();
            let (graph, agents, tasks) = graph(2, 3, &edges);
            for objective in [
                AssignmentObjectiveV1::Minimize,
                AssignmentObjectiveV1::Maximize,
            ] {
                let result = solve_auction(&graph, &agents, &tasks, objective).expect("solve");
                let mut expected: Option<i128> = None;
                for first in 0..3 {
                    for second in 0..3 {
                        if first == second {
                            continue;
                        }
                        let left = edges
                            .iter()
                            .find(|&&(agent, task, _)| agent == 0 && task == first);
                        let right = edges
                            .iter()
                            .find(|&&(agent, task, _)| agent == 1 && task == second);
                        let (Some(left), Some(right)) = (left, right) else {
                            continue;
                        };
                        let value = i128::from(left.2) + i128::from(right.2);
                        expected = Some(match (expected, objective) {
                            (None, _) => value,
                            (Some(current), AssignmentObjectiveV1::Minimize) => current.min(value),
                            (Some(current), AssignmentObjectiveV1::Maximize) => current.max(value),
                        });
                    }
                }
                match (result.outcome, expected) {
                    (AuctionOutcome::Optimal { certificate, .. }, Some(expected)) => {
                        assert_eq!(certificate.total_cost, expected, "mask {mask}");
                    }
                    (AuctionOutcome::Infeasible { witness, .. }, None) => {
                        assert!(witness.deficiency > 0);
                    }
                    _ => panic!("mask {mask} feasibility mismatch"),
                }
            }
        }
    }

    #[test]
    fn price_war_uses_multiple_scales_and_displaces_agents() {
        let (graph, agents, tasks) = graph(
            3,
            3,
            &[
                (0, 0, 20),
                (0, 1, 20),
                (0, 2, 0),
                (1, 0, 20),
                (1, 1, 20),
                (1, 2, 0),
                (2, 0, 20),
                (2, 1, 20),
                (2, 2, 0),
            ],
        );
        let result = solve_auction(&graph, &agents, &tasks, AssignmentObjectiveV1::Maximize)
            .expect("auction");
        assert!(result.metrics.scaling_phases > 1);
        assert!(result.metrics.evictions > 0);
        assert!(result.metrics.bids > agents.len() as u64);

        let trace = trace_auction(&graph, &agents, &tasks, AssignmentObjectiveV1::Maximize)
            .expect("price-war trace");
        let mut replay = trace.base_snapshot.clone();
        let mut checked_atomic_eviction = false;
        for event in &trace.events {
            let before = replay.clone();
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward price-war event");
            if event.catalog_id != "auction.award" || replay.active_path.len() != 2 {
                continue;
            }
            let forward = &replay.active_path[0];
            let reverse = &replay.active_path[1];
            assert_eq!(forward.direction(), ResidualDirection::Forward);
            assert_eq!(reverse.direction(), ResidualDirection::Reverse);
            let new_edge = graph
                .edge_index(forward.original_edge())
                .expect("new award edge");
            let evicted_edge = graph
                .edge_index(reverse.original_edge())
                .expect("evicted award edge");
            assert_eq!(before.flows[new_edge.as_usize()], 0);
            assert_eq!(before.flows[evicted_edge.as_usize()], 1);
            assert_eq!(replay.flows[new_edge.as_usize()], 1);
            assert_eq!(replay.flows[evicted_edge.as_usize()], 0);
            let after = replay.clone();
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("atomic eviction reverses");
            assert_eq!(replay, before);
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("atomic eviction reapplies");
            assert_eq!(replay, after);
            checked_atomic_eviction = true;
        }
        assert!(
            checked_atomic_eviction,
            "fixture must contain an atomic eviction"
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one contract test covers feasible replay and its paired Hall-witness replay"
    )]
    fn trace_replays_scale_bid_award_and_hall_events_both_directions() {
        let (network, agents, tasks) = graph(2, 3, &[(0, 0, 4), (0, 1, 4), (1, 0, 4), (1, 2, 1)]);
        let run = trace_auction(&network, &agents, &tasks, AssignmentObjectiveV1::Maximize)
            .expect("trace");
        let ids = run
            .events
            .iter()
            .map(|event| event.catalog_id.as_str())
            .collect::<BTreeSet<_>>();
        for required in [
            "auction.scale-start",
            "auction.inspect-assignment-edge",
            "auction.bid",
            "auction.award",
            "auction.scale-complete",
            "auction.optimal",
        ] {
            assert!(ids.contains(required));
        }
        let scale_start = run
            .events
            .iter()
            .position(|event| event.catalog_id == "auction.scale-start")
            .expect("scale start");
        for event in &run.events[..scale_start] {
            if event.catalog_id == "auction.inspect-assignment-edge" {
                assert!(
                    event.patches.iter().all(|patch| !matches!(
                        patch,
                        crate::trace::FlowTracePatch::NodeLabel { .. }
                    ))
                );
            }
        }
        for event in &run.events {
            if matches!(
                event.catalog_id.as_str(),
                "auction.inspect-assignment-edge" | "auction.bid"
            ) {
                assert!(matches!(
                    event.entity_refs.as_slice(),
                    [FlowTraceEntityRef::ResidualArc(_)]
                ));
            }
        }
        let mut replay = run.base_snapshot.clone();
        for event in &run.events {
            apply_trace_event(&network, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward");
        }
        assert_eq!(replay, run.final_snapshot);
        for event in run.events.iter().rev() {
            apply_trace_event(&network, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse");
        }
        assert_eq!(replay, run.base_snapshot);

        let (bad_graph, bad_agents, bad_tasks) = graph(
            3,
            4,
            &[(0, 0, 1), (1, 0, 2), (2, 1, 0), (2, 2, 0), (2, 3, 0)],
        );
        let bad = trace_auction(
            &bad_graph,
            &bad_agents,
            &bad_tasks,
            AssignmentObjectiveV1::Minimize,
        )
        .expect("Hall witness");
        assert!(bad.events.len() > 1);
        assert!(
            bad.events[..bad.events.len() - 1]
                .iter()
                .all(|event| event.catalog_id == "auction.inspect-assignment-edge")
        );
        assert_eq!(
            bad.events.last().map(|event| event.catalog_id.as_str()),
            Some("auction.hall-witness")
        );
        let mut bad_replay = bad.base_snapshot.clone();
        for event in &bad.events {
            apply_trace_event(
                &bad_graph,
                &mut bad_replay,
                event,
                FlowTraceDirection::Forward,
            )
            .expect("Hall trace forward");
        }
        assert_eq!(bad_replay, bad.final_snapshot);
        for event in bad.events.iter().rev() {
            apply_trace_event(
                &bad_graph,
                &mut bad_replay,
                event,
                FlowTraceDirection::Reverse,
            )
            .expect("Hall trace reverse");
        }
        assert_eq!(bad_replay, bad.base_snapshot);
        assert_eq!(bad.result.metrics.bids, 0);
        assert_eq!(bad.result.metrics.price_raises, 0);
        match bad.result.outcome {
            AuctionOutcome::Infeasible { witness, .. } => {
                assert_eq!(witness.deficiency, 1);
                assert_eq!(witness.agents.len(), 2);
                assert_eq!(witness.neighbor_tasks.len(), 1);
            }
            AuctionOutcome::Optimal { .. } => panic!("Hall-deficient graph"),
        }
    }

    #[test]
    fn trace_projection_budget_counts_every_full_snapshot_boundary() {
        let (network, _agents, _tasks) = graph(2, 3, &[(0, 0, 1), (0, 1, 2), (1, 1, 3)]);
        let cells = u128::try_from(network.nodes().len() + network.edges().len() * 3)
            .expect("small graph cell count");
        let admitted_boundaries = AUCTION_MAX_TRACE_PROJECTION_CELLS / cells;
        let admitted_transitions =
            u64::try_from(admitted_boundaries - 1).expect("admitted transitions fit u64");
        check_trace_projection_budget(&network, admitted_transitions)
            .expect("exact budget is admitted");
        assert_eq!(
            check_trace_projection_budget(&network, admitted_transitions + 1),
            Err(AuctionError::WorkLimit)
        );
    }
}
