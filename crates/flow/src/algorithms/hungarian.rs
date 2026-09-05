//! Dense primal-dual Hungarian method for native rectangular assignment.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::assignment::{
    ASSIGNMENT_MAX_EDGES, ASSIGNMENT_MAX_NODES, AssignmentGraph, AssignmentModelError,
    AssignmentObjectiveV1,
};
use crate::certificate::{
    AssignmentCertificate, AssignmentHallWitness, CertificateError, check_assignment,
    check_assignment_infeasibility,
};
use crate::model::{FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative interactive node limit for native assignment.
pub const HUNGARIAN_MAX_NODES: usize = ASSIGNMENT_MAX_NODES;
/// Conservative interactive allowed-edge limit.
pub const HUNGARIAN_MAX_EDGES: usize = ASSIGNMENT_MAX_EDGES;
/// Explicit dense matrix-cell scan ceiling.
pub const HUNGARIAN_MAX_CELL_SCANS: u128 = 20_000_000;
/// Explicit eager-trace transition ceiling.
pub const HUNGARIAN_MAX_STATE_TRANSITIONS: u64 = 100_000;

/// Exact counters from the stable rectangular primal-dual kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HungarianMetrics {
    /// Agent-rooted augmenting searches started.
    pub agent_searches: u64,
    /// Dense task cells inspected, including forbidden pairs.
    pub cell_scans: u128,
    /// Dual-label update steps.
    pub dual_updates: u64,
    /// Strict predecessor/slack improvements.
    pub predecessor_updates: u64,
    /// Successful alternating-path augmentations.
    pub augmentations: u64,
}

/// Certified terminal result of a native assignment solve.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HungarianOutcome {
    /// Complete optimum with primal/dual equality.
    Optimal {
        /// Unit-flow projection in canonical original-edge order.
        flows: Vec<u64>,
        /// Independently checked complete assignment and dual labels.
        certificate: AssignmentCertificate,
    },
    /// Hall-deficient allowed-edge neighborhood.
    Infeasible {
        /// Partial matching retained only for the final trace boundary.
        partial_flows: Vec<u64>,
        /// Independently checked Hall witness.
        witness: AssignmentHallWitness,
    },
}

/// Certified result and deterministic work counters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HungarianResult {
    /// Optimal assignment or verified Hall infeasibility.
    pub outcome: HungarianOutcome,
    /// Exact bounded-work counters.
    pub metrics: HungarianMetrics,
}

/// Certified result with reversible search/dual/augmentation events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HungarianTraceResult {
    /// Same terminal result produced by the fast profile.
    pub result: HungarianResult,
    /// Replay boundary before the first agent search.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible trace events.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after terminal certificate verification.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Native assignment construction, execution, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HungarianError {
    /// Input exceeds the practical interactive admission band.
    #[error("graph exceeds Hungarian admission limits")]
    AdmissionLimit,
    /// Exact scan or trace-transition ceiling was exceeded.
    #[error("Hungarian work limit exceeded")]
    WorkLimit,
    /// Native assignment model validation failed.
    #[error(transparent)]
    Model(#[from] AssignmentModelError),
    /// Alternating-path flow mutation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent optimality or infeasibility certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Exact dual or counter arithmetic overflowed.
    #[error("Hungarian arithmetic overflow")]
    ArithmeticOverflow,
    /// Pair/predecessor arrays contradicted the primal-dual invariant.
    #[error("Hungarian assignment invariant failed")]
    Invariant,
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves a native rectangular assignment with stable task-ID tie-breaking.
///
/// # Errors
///
/// Rejects malformed/out-of-band models, bounded-work exhaustion, arithmetic
/// overflow, invariant failure, or independently rejected terminal evidence.
pub fn solve_hungarian(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
) -> Result<HungarianResult, HungarianError> {
    solve_internal(graph, agents, tasks, objective, false).map(|run| run.result)
}

/// Solves while recording agent search, dual update, atomic augmentation, and
/// terminal certificate events.
///
/// # Errors
///
/// Returns the same failures as [`solve_hungarian`], plus trace projection
/// failures.
pub fn trace_hungarian(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
) -> Result<HungarianTraceResult, HungarianError> {
    let run = solve_internal(graph, agents, tasks, objective, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(HungarianError::Invariant)?;
    Ok(HungarianTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: HungarianResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_internal(
    graph: &FlowNetwork,
    agents: &[String],
    tasks: &[String],
    objective: AssignmentObjectiveV1,
    with_trace: bool,
) -> Result<InternalRun, HungarianError> {
    if graph.nodes().len() > HUNGARIAN_MAX_NODES || graph.edges().len() > HUNGARIAN_MAX_EDGES {
        return Err(HungarianError::AdmissionLimit);
    }
    let model = AssignmentGraph::new(graph, agents, tasks, objective)?;
    let mut kernel = HungarianKernel::new(graph, &model, with_trace)?;
    let outcome = kernel.solve(graph, &model)?;
    Ok(kernel.finish(outcome))
}

struct HungarianKernel<'graph> {
    agent_labels: Vec<i128>,
    task_labels: Vec<i128>,
    task_owner: Vec<Option<usize>>,
    state: ResidualState<'graph>,
    metrics: HungarianMetrics,
    transitions: u64,
    recorder: Option<FlowTraceRecorder<'graph>>,
}

impl<'graph> HungarianKernel<'graph> {
    fn new(
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        with_trace: bool,
    ) -> Result<Self, HungarianError> {
        let state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
        let metrics = HungarianMetrics::default();
        let recorder = start_trace(graph, &state, model, metrics, with_trace)?;
        Ok(Self {
            agent_labels: vec![0; model.agents.len()],
            task_labels: vec![0; model.tasks.len()],
            task_owner: vec![None; model.tasks.len()],
            state,
            metrics,
            transitions: 0,
            recorder,
        })
    }

    fn solve(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<HungarianOutcome, HungarianError> {
        if model.agents.len() > model.tasks.len() {
            return self.infeasible_from_agents(graph, model, 0..model.agents.len());
        }
        for root in 0..model.agents.len() {
            if let Some(outcome) = self.insert_agent(graph, model, root)? {
                return Ok(outcome);
            }
        }
        self.optimal_outcome(graph, model)
    }

    fn insert_agent(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        root: usize,
    ) -> Result<Option<HungarianOutcome>, HungarianError> {
        self.metrics.agent_searches = checked_add_u64(self.metrics.agent_searches, 1)?;
        let mut search = HungarianSearch::new(model.tasks.len());
        self.record(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "hungarian.start-agent",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "hungarian:start-unassigned-agent-search",
            },
            &search,
            root,
            Vec::new(),
            vec![node_focus(graph, model.agents[root])],
            Some(i128::try_from(root + 1).map_err(|_| HungarianError::ArithmeticOverflow)?),
        )?;
        let mut current_task: Option<usize> = None;
        loop {
            let current_agent = current_task
                .map(|task| self.task_owner[task].ok_or(HungarianError::Invariant))
                .transpose()?
                .unwrap_or(root);
            if let Some(task) = current_task {
                if std::mem::replace(&mut search.used_tasks[task], true) {
                    return Err(HungarianError::Invariant);
                }
                search.used_order.push(task);
            }
            let next =
                self.scan_frontier(graph, model, root, current_agent, current_task, &mut search)?;
            let Some((next_task, delta)) = next else {
                let reachable = reachable_agents(root, &search.used_order, &self.task_owner)?;
                return self
                    .infeasible_from_agents(graph, model, reachable.into_iter())
                    .map(Some);
            };
            let active =
                predecessor_edge(model, &self.task_owner, root, &search, next_task, graph)?;
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "hungarian.select-minimum-slack",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "hungarian:select-minimum-slack-task",
                },
                &search,
                root,
                vec![active.clone()],
                vec![FlowTraceEntityRef::ResidualArc(active.clone())],
                Some(delta),
            )?;
            self.apply_dual_update(root, delta, &mut search)?;
            self.metrics.dual_updates = checked_add_u64(self.metrics.dual_updates, 1)?;
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "hungarian.dual-update",
                    minimum_granularity: TraceGranularityV1::Operation,
                    pseudocode_line: "hungarian:raise-tree-labels-by-minimum-slack",
                },
                &search,
                root,
                vec![active.clone()],
                vec![FlowTraceEntityRef::ResidualArc(active)],
                Some(delta),
            )?;
            current_task = Some(next_task);
            if self.task_owner[next_task].is_none() {
                let path =
                    augmenting_path(graph, model, &self.task_owner, root, &search, next_task)?;
                self.state.augment(&path, 1)?;
                apply_augmentation(&mut self.task_owner, root, &search, next_task)?;
                self.metrics.augmentations = checked_add_u64(self.metrics.augmentations, 1)?;
                self.verify_state(graph, model)?;
                self.record(
                    graph,
                    model,
                    FlowTraceEventMetadata {
                        catalog_id: "hungarian.augment",
                        minimum_granularity: TraceGranularityV1::Operation,
                        pseudocode_line: "hungarian:flip-alternating-assignment-path",
                    },
                    &search,
                    root,
                    path.clone(),
                    path.into_iter()
                        .map(FlowTraceEntityRef::ResidualArc)
                        .collect(),
                    Some(i128::from(self.metrics.augmentations)),
                )?;
                return Ok(None);
            }
        }
    }

    fn scan_frontier(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        root: usize,
        agent: usize,
        predecessor_task: Option<usize>,
        search: &mut HungarianSearch,
    ) -> Result<Option<(usize, i128)>, HungarianError> {
        for task in 0..model.tasks.len() {
            increment_cell_scans(&mut self.metrics)?;
            search.inspected_cell = Some((agent, task));
            let mut active_path = Vec::new();
            if !search.used_tasks[task]
                && let Some(ordinal) = model.edge_by_pair[agent][task]
            {
                let edge = model.edges.get(ordinal).ok_or(HungarianError::Invariant)?;
                let reduced = model
                    .normalized_cost(edge)
                    .checked_sub(self.agent_labels[agent])
                    .and_then(|value| value.checked_sub(self.task_labels[task]))
                    .ok_or(HungarianError::ArithmeticOverflow)?;
                if search.minimum_slack[task].is_none_or(|value| reduced < value) {
                    search.minimum_slack[task] = Some(reduced);
                    search.predecessor_task[task] = TaskPredecessor::discovered(predecessor_task);
                    self.metrics.predecessor_updates =
                        checked_add_u64(self.metrics.predecessor_updates, 1)?;
                }
                active_path.push(residual_id(graph, edge.edge, ResidualDirection::Forward)?);
            }
            let focus = active_path.first().map_or_else(
                || {
                    vec![
                        node_focus(graph, model.agents[agent]),
                        node_focus(graph, model.tasks[task]),
                    ]
                },
                |arc| vec![FlowTraceEntityRef::ResidualArc(arc.clone())],
            );
            self.record(
                graph,
                model,
                FlowTraceEventMetadata {
                    catalog_id: "hungarian.inspect-cell",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "hungarian:inspect-one-agent-task-cell",
                },
                search,
                root,
                active_path,
                focus,
                Some(
                    i128::try_from(self.metrics.cell_scans)
                        .map_err(|_| HungarianError::ArithmeticOverflow)?,
                ),
            )?;
        }
        let result = search
            .minimum_slack
            .iter()
            .enumerate()
            .filter(|(task, _)| !search.used_tasks[*task])
            .filter_map(|(task, &slack)| slack.map(|value| (task, value)))
            .min_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
        search.inspected_cell = None;
        Ok(result)
    }

    fn apply_dual_update(
        &mut self,
        root: usize,
        delta: i128,
        search: &mut HungarianSearch,
    ) -> Result<(), HungarianError> {
        self.agent_labels[root] = self.agent_labels[root]
            .checked_add(delta)
            .ok_or(HungarianError::ArithmeticOverflow)?;
        for &task in &search.used_order {
            let agent = self.task_owner[task].ok_or(HungarianError::Invariant)?;
            self.agent_labels[agent] = self.agent_labels[agent]
                .checked_add(delta)
                .ok_or(HungarianError::ArithmeticOverflow)?;
            self.task_labels[task] = self.task_labels[task]
                .checked_sub(delta)
                .ok_or(HungarianError::ArithmeticOverflow)?;
        }
        for task in 0..search.minimum_slack.len() {
            if search.used_tasks[task] {
                continue;
            }
            if let Some(value) = search.minimum_slack[task] {
                let updated = value
                    .checked_sub(delta)
                    .ok_or(HungarianError::ArithmeticOverflow)?;
                search.minimum_slack[task] = Some(updated);
            }
        }
        Ok(())
    }

    fn optimal_outcome(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<HungarianOutcome, HungarianError> {
        let task_by_agent = task_by_agent(model, &self.task_owner)?;
        let flows = model.flows_from_tasks(graph, &task_by_agent)?;
        if flows != self.state.flows() {
            return Err(HungarianError::Invariant);
        }
        let (agent_labels, task_labels) =
            oriented_labels(model, &self.agent_labels, &self.task_labels)?;
        let certificate = check_assignment(graph, model, &flows, &agent_labels, &task_labels)?;
        let view = HungarianSearch::terminal(model.tasks.len());
        self.record(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "hungarian.optimal",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "hungarian:return-tight-primal-dual-assignment",
            },
            &view,
            0,
            Vec::new(),
            Vec::new(),
            Some(certificate.total_cost),
        )?;
        Ok(HungarianOutcome::Optimal { flows, certificate })
    }

    fn infeasible_from_agents(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        agents: impl Iterator<Item = usize>,
    ) -> Result<HungarianOutcome, HungarianError> {
        let witness = build_hall_witness(graph, model, agents)?;
        check_assignment_infeasibility(graph, model, &witness)?;
        let partial_flows = self.state.flows().to_vec();
        let view = HungarianSearch::from_witness(graph, model, &witness)?;
        let focus = view
            .witness_order
            .as_deref()
            .unwrap_or_default()
            .iter()
            .copied()
            .map(|node| node_focus(graph, node))
            .collect();
        self.record(
            graph,
            model,
            FlowTraceEventMetadata {
                catalog_id: "hungarian.hall-witness",
                minimum_granularity: TraceGranularityV1::Phase,
                pseudocode_line: "hungarian:return-hall-deficient-neighborhood",
            },
            &view,
            0,
            Vec::new(),
            focus,
            Some(i128::from(witness.deficiency)),
        )?;
        Ok(HungarianOutcome::Infeasible {
            partial_flows,
            witness,
        })
    }

    fn verify_state(
        &self,
        graph: &FlowNetwork,
        model: &AssignmentGraph,
    ) -> Result<(), HungarianError> {
        let mut expected = vec![0; graph.edges().len()];
        let mut used_agents = vec![false; model.agents.len()];
        for (task, owner) in self.task_owner.iter().copied().enumerate() {
            let Some(agent) = owner else { continue };
            if std::mem::replace(&mut used_agents[agent], true) {
                return Err(HungarianError::Invariant);
            }
            let ordinal = model.edge_by_pair[agent][task].ok_or(HungarianError::Invariant)?;
            expected[model.edges[ordinal].edge.as_usize()] = 1;
        }
        (expected == self.state.flows())
            .then_some(())
            .ok_or(HungarianError::Invariant)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "trace projection keeps kernel state explicit"
    )]
    fn record(
        &mut self,
        graph: &'graph FlowNetwork,
        model: &AssignmentGraph,
        metadata: FlowTraceEventMetadata,
        search: &HungarianSearch,
        root: usize,
        path: Vec<ResidualArcId>,
        focus: Vec<FlowTraceEntityRef>,
        detail: Option<i128>,
    ) -> Result<(), HungarianError> {
        let Some(recorder) = self.recorder.as_mut() else {
            return Ok(());
        };
        self.transitions = checked_add_u64(self.transitions, 1)?;
        if self.transitions > HUNGARIAN_MAX_STATE_TRANSITIONS {
            return Err(HungarianError::WorkLimit);
        }
        let snapshot = assignment_snapshot(
            graph,
            model,
            &self.state,
            &self.agent_labels,
            &self.task_labels,
            &self.task_owner,
            search,
            root,
            path,
            self.metrics,
        )?;
        let label = match metadata.catalog_id {
            "hungarian.start-agent" => "agent-ordinal",
            "hungarian.inspect-cell" => "cell-scans",
            "hungarian.select-minimum-slack" => "minimum-slack",
            "hungarian.dual-update" => "delta",
            "hungarian.augment" => "assigned-agents",
            "hungarian.hall-witness" => "deficiency",
            _ => "objective",
        };
        recorder.record_transition_with_detail_and_focus(
            metadata,
            &snapshot,
            detail.map(|value| (label, value)),
            focus,
        )?;
        Ok(())
    }

    fn finish(self, outcome: HungarianOutcome) -> InternalRun {
        InternalRun {
            result: HungarianResult {
                outcome,
                metrics: self.metrics,
            },
            trace: self.recorder.map(FlowTraceRecorder::finish),
        }
    }
}

#[derive(Clone, Copy)]
enum TaskPredecessor {
    Undiscovered,
    Root,
    Task(usize),
}

impl TaskPredecessor {
    fn discovered(predecessor: Option<usize>) -> Self {
        predecessor.map_or(Self::Root, Self::Task)
    }

    fn value(self) -> Result<Option<usize>, HungarianError> {
        match self {
            Self::Undiscovered => Err(HungarianError::Invariant),
            Self::Root => Ok(None),
            Self::Task(task) => Ok(Some(task)),
        }
    }
}

/// Mutable alternating-tree state for one root agent.
struct HungarianSearch {
    minimum_slack: Vec<Option<i128>>,
    predecessor_task: Vec<TaskPredecessor>,
    used_tasks: Vec<bool>,
    used_order: Vec<usize>,
    inspected_cell: Option<(usize, usize)>,
    witness_order: Option<Vec<NodeIndex>>,
}

impl HungarianSearch {
    fn new(task_count: usize) -> Self {
        Self {
            minimum_slack: vec![None; task_count],
            predecessor_task: vec![TaskPredecessor::Undiscovered; task_count],
            used_tasks: vec![false; task_count],
            used_order: Vec::new(),
            inspected_cell: None,
            witness_order: None,
        }
    }

    fn terminal(task_count: usize) -> Self {
        Self::new(task_count)
    }

    fn from_witness(
        graph: &FlowNetwork,
        model: &AssignmentGraph,
        witness: &AssignmentHallWitness,
    ) -> Result<Self, HungarianError> {
        let mut result = Self::new(model.tasks.len());
        let mut order = Vec::with_capacity(witness.agents.len() + witness.neighbor_tasks.len());
        for id in witness.agents.iter().chain(&witness.neighbor_tasks) {
            order.push(graph.node_index(id).ok_or(HungarianError::Invariant)?);
        }
        result.witness_order = Some(order);
        Ok(result)
    }
}

fn checked_add_u64(value: u64, delta: u64) -> Result<u64, HungarianError> {
    value
        .checked_add(delta)
        .ok_or(HungarianError::ArithmeticOverflow)
}

fn increment_cell_scans(metrics: &mut HungarianMetrics) -> Result<(), HungarianError> {
    metrics.cell_scans = metrics
        .cell_scans
        .checked_add(1)
        .ok_or(HungarianError::ArithmeticOverflow)?;
    if metrics.cell_scans > HUNGARIAN_MAX_CELL_SCANS {
        return Err(HungarianError::WorkLimit);
    }
    Ok(())
}

fn reachable_agents(
    root: usize,
    used_tasks: &[usize],
    task_owner: &[Option<usize>],
) -> Result<BTreeSet<usize>, HungarianError> {
    let mut agents = BTreeSet::from([root]);
    for &task in used_tasks {
        agents.insert(task_owner[task].ok_or(HungarianError::Invariant)?);
    }
    Ok(agents)
}

fn build_hall_witness(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    agents: impl Iterator<Item = usize>,
) -> Result<AssignmentHallWitness, HungarianError> {
    let positions = agents.collect::<BTreeSet<_>>();
    let mut agent_ids = Vec::with_capacity(positions.len());
    let mut neighbor_positions = BTreeSet::new();
    for &agent in &positions {
        let node = model.agents.get(agent).ok_or(HungarianError::Invariant)?;
        agent_ids.push(
            graph
                .node(*node)
                .ok_or(HungarianError::Invariant)?
                .id()
                .clone(),
        );
        for &ordinal in &model.adjacency[agent] {
            neighbor_positions.insert(
                model
                    .edges
                    .get(ordinal)
                    .ok_or(HungarianError::Invariant)?
                    .task,
            );
        }
    }
    let neighbor_tasks = neighbor_positions
        .into_iter()
        .map(|task| {
            graph
                .node(model.tasks[task])
                .map(|node| node.id().clone())
                .ok_or(HungarianError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let deficiency = agent_ids
        .len()
        .checked_sub(neighbor_tasks.len())
        .and_then(|value| u64::try_from(value).ok())
        .filter(|&value| value > 0)
        .ok_or(HungarianError::Invariant)?;
    Ok(AssignmentHallWitness {
        agents: agent_ids,
        neighbor_tasks,
        deficiency,
    })
}

fn predecessor_agent(
    task_owner: &[Option<usize>],
    root: usize,
    predecessor: Option<usize>,
) -> Result<usize, HungarianError> {
    predecessor.map_or(Ok(root), |task| {
        task_owner[task].ok_or(HungarianError::Invariant)
    })
}

fn predecessor_edge(
    model: &AssignmentGraph,
    task_owner: &[Option<usize>],
    root: usize,
    search: &HungarianSearch,
    task: usize,
    graph: &FlowNetwork,
) -> Result<ResidualArcId, HungarianError> {
    let predecessor = search.predecessor_task[task].value()?;
    let agent = predecessor_agent(task_owner, root, predecessor)?;
    residual_id(
        graph,
        model.edges[model.edge_by_pair[agent][task].ok_or(HungarianError::Invariant)?].edge,
        ResidualDirection::Forward,
    )
}

fn augmenting_path(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    task_owner: &[Option<usize>],
    root: usize,
    search: &HungarianSearch,
    free_task: usize,
) -> Result<Vec<ResidualArcId>, HungarianError> {
    let mut segments = Vec::new();
    let mut task = free_task;
    loop {
        let predecessor = search.predecessor_task[task].value()?;
        let agent = predecessor_agent(task_owner, root, predecessor)?;
        segments.push((predecessor, agent, task));
        let Some(previous_task) = predecessor else {
            break;
        };
        task = previous_task;
    }
    segments.reverse();
    let mut path = Vec::with_capacity(segments.len().saturating_mul(2).saturating_sub(1));
    for (predecessor, agent, task) in segments {
        if let Some(previous_task) = predecessor {
            let matched = model.edge_by_pair[agent][previous_task]
                .and_then(|ordinal| model.edges.get(ordinal))
                .ok_or(HungarianError::Invariant)?;
            path.push(residual_id(
                graph,
                matched.edge,
                ResidualDirection::Reverse,
            )?);
        }
        let unmatched = model.edge_by_pair[agent][task]
            .and_then(|ordinal| model.edges.get(ordinal))
            .ok_or(HungarianError::Invariant)?;
        path.push(residual_id(
            graph,
            unmatched.edge,
            ResidualDirection::Forward,
        )?);
    }
    Ok(path)
}

fn apply_augmentation(
    task_owner: &mut [Option<usize>],
    root: usize,
    search: &HungarianSearch,
    free_task: usize,
) -> Result<(), HungarianError> {
    let mut task = free_task;
    loop {
        let predecessor = search.predecessor_task[task].value()?;
        let agent = predecessor_agent(task_owner, root, predecessor)?;
        task_owner[task] = Some(agent);
        let Some(previous_task) = predecessor else {
            break;
        };
        task = previous_task;
    }
    Ok(())
}

fn task_by_agent(
    model: &AssignmentGraph,
    task_owner: &[Option<usize>],
) -> Result<Vec<usize>, HungarianError> {
    let mut result = vec![None; model.agents.len()];
    for (task, owner) in task_owner.iter().copied().enumerate() {
        let Some(agent) = owner else { continue };
        if result[agent].replace(task).is_some() {
            return Err(HungarianError::Invariant);
        }
    }
    result
        .into_iter()
        .map(|task| task.ok_or(HungarianError::Invariant))
        .collect()
}

fn oriented_labels(
    model: &AssignmentGraph,
    agent_labels: &[i128],
    task_labels: &[i128],
) -> Result<(Vec<i128>, Vec<i128>), HungarianError> {
    match model.objective {
        AssignmentObjectiveV1::Minimize => Ok((agent_labels.to_vec(), task_labels.to_vec())),
        AssignmentObjectiveV1::Maximize => Ok((
            agent_labels
                .iter()
                .map(|value| {
                    value
                        .checked_neg()
                        .ok_or(HungarianError::ArithmeticOverflow)
                })
                .collect::<Result<_, _>>()?,
            task_labels
                .iter()
                .map(|value| {
                    value
                        .checked_neg()
                        .ok_or(HungarianError::ArithmeticOverflow)
                })
                .collect::<Result<_, _>>()?,
        )),
    }
}

fn residual_id(
    graph: &FlowNetwork,
    edge: crate::model::EdgeIndex,
    direction: ResidualDirection,
) -> Result<ResidualArcId, HungarianError> {
    graph
        .edge(edge)
        .map(|value| ResidualArcId::new(value.id().clone(), direction))
        .ok_or(HungarianError::Invariant)
}

fn node_focus(graph: &FlowNetwork, node: NodeIndex) -> FlowTraceEntityRef {
    FlowTraceEntityRef::Node(graph.nodes()[node.as_usize()].id().clone())
}

fn start_trace<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    model: &AssignmentGraph,
    metrics: HungarianMetrics,
    enabled: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !enabled {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        assignment_labels(
            graph,
            model,
            &vec![0; model.agents.len()],
            &vec![0; model.tasks.len()],
        ),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot projection is an explicit contract boundary"
)]
fn assignment_snapshot(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    state: &ResidualState<'_>,
    agent_labels: &[i128],
    task_labels: &[i128],
    task_owner: &[Option<usize>],
    search: &HungarianSearch,
    root: usize,
    path: Vec<ResidualArcId>,
    metrics: HungarianMetrics,
) -> Result<FlowTraceSnapshot, HungarianError> {
    let (agents, tasks) = oriented_labels(model, agent_labels, task_labels)?;
    let search_order = if let Some(order) = &search.witness_order {
        order.clone()
    } else if model.agents.is_empty() {
        Vec::new()
    } else {
        let mut order = vec![model.agents[root.min(model.agents.len() - 1)]];
        for &task in &search.used_order {
            order.push(model.tasks[task]);
            order.push(model.agents[task_owner[task].ok_or(HungarianError::Invariant)?]);
        }
        if let Some((agent, task)) = search.inspected_cell {
            order.push(model.agents[agent]);
            order.push(model.tasks[task]);
        }
        order
    };
    Ok(FlowTraceSnapshot::capture(
        graph,
        state,
        assignment_labels(graph, model, &agents, &tasks),
        search_order,
        path,
        Vec::new(),
        trace_metrics(metrics),
    ))
}

fn assignment_labels(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    agent_labels: &[i128],
    task_labels: &[i128],
) -> Vec<Option<i128>> {
    let mut labels = vec![None; graph.nodes().len()];
    for (&node, &label) in model.agents.iter().zip(agent_labels) {
        labels[node.as_usize()] = Some(label);
    }
    for (&node, &label) in model.tasks.iter().zip(task_labels) {
        labels[node.as_usize()] = Some(label);
    }
    labels
}

const fn trace_metrics(metrics: HungarianMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.agent_searches as u128,
        relaxation_passes: metrics.dual_updates as u128,
        residual_arc_scans: metrics.cell_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.predecessor_updates as u128,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
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

    fn visit_assignments(
        agent: usize,
        used: &mut [bool],
        costs: &[Vec<Option<i128>>],
        objective: AssignmentObjectiveV1,
        total: i128,
        best: &mut Option<i128>,
    ) {
        if agent == costs.len() {
            *best = Some(best.map_or(total, |current| match objective {
                AssignmentObjectiveV1::Minimize => current.min(total),
                AssignmentObjectiveV1::Maximize => current.max(total),
            }));
            return;
        }
        for task in 0..used.len() {
            let Some(cost) = costs[agent][task] else {
                continue;
            };
            if used[task] {
                continue;
            }
            used[task] = true;
            visit_assignments(agent + 1, used, costs, objective, total + cost, best);
            used[task] = false;
        }
    }

    fn brute_force(
        agent_count: usize,
        task_count: usize,
        edges: &[(usize, usize, i64)],
        objective: AssignmentObjectiveV1,
    ) -> Option<i128> {
        let mut costs = vec![vec![None; task_count]; agent_count];
        for &(agent, task, cost) in edges {
            costs[agent][task] = Some(i128::from(cost));
        }
        let mut best = None;
        visit_assignments(
            0,
            &mut vec![false; task_count],
            &costs,
            objective,
            0,
            &mut best,
        );
        best
    }

    #[test]
    fn solves_rectangular_minimize_and_maximize_with_negative_extremes() {
        let edges = vec![
            (0, 0, i64::MIN),
            (0, 1, 4),
            (0, 2, 8),
            (1, 0, -3),
            (1, 1, 9),
            (1, 2, 2),
        ];
        let (graph, agents, tasks) = graph(2, 3, &edges);
        for objective in [
            AssignmentObjectiveV1::Minimize,
            AssignmentObjectiveV1::Maximize,
        ] {
            let result = solve_hungarian(&graph, &agents, &tasks, objective).expect("solve");
            let HungarianOutcome::Optimal { certificate, .. } = result.outcome else {
                panic!("expected optimum");
            };
            assert_eq!(
                Some(certificate.total_cost),
                brute_force(2, 3, &edges, objective)
            );
        }
    }

    #[test]
    fn exhausts_every_two_by_three_allowed_edge_mask_against_permutations() {
        for mask in 0_u64..(1 << 6) {
            let edges = (0..2)
                .flat_map(|agent| (0..3).map(move |task| (agent, task)))
                .enumerate()
                .filter_map(|(bit, (agent, task))| {
                    let agent_cost = i64::try_from(agent).expect("small test agent") * 5;
                    let task_cost = i64::try_from(task).expect("small test task") * 3;
                    ((mask >> bit) & 1 == 1).then_some((agent, task, agent_cost - task_cost))
                })
                .collect::<Vec<_>>();
            let (graph, agents, tasks) = graph(2, 3, &edges);
            let result = solve_hungarian(&graph, &agents, &tasks, AssignmentObjectiveV1::Minimize)
                .expect("solve");
            match (
                brute_force(2, 3, &edges, AssignmentObjectiveV1::Minimize),
                result.outcome,
            ) {
                (Some(expected), HungarianOutcome::Optimal { certificate, .. }) => {
                    assert_eq!(certificate.total_cost, expected, "mask {mask}");
                }
                (None, HungarianOutcome::Infeasible { witness, .. }) => {
                    let model = AssignmentGraph::new(
                        &graph,
                        &agents,
                        &tasks,
                        AssignmentObjectiveV1::Minimize,
                    )
                    .expect("model");
                    check_assignment_infeasibility(&graph, &model, &witness).expect("Hall witness");
                }
                _ => panic!("terminal mismatch for mask {mask}"),
            }
        }
    }

    #[test]
    fn trace_replays_forward_and_reverse_across_alternating_flip() {
        let edges = vec![(0, 0, 0), (0, 1, 2), (1, 0, 0), (1, 1, 5)];
        let (graph, agents, tasks) = graph(2, 2, &edges);
        let run = trace_hungarian(&graph, &agents, &tasks, AssignmentObjectiveV1::Minimize)
            .expect("trace");
        let (selection, dual_update) = run
            .events
            .windows(2)
            .find_map(|events| {
                (events[0].catalog_id == "hungarian.select-minimum-slack"
                    && events[1].catalog_id == "hungarian.dual-update")
                    .then_some((&events[0], &events[1]))
            })
            .expect("minimum-slack selection precedes its dual update");
        let selection_detail = selection.detail.as_ref().expect("selection detail");
        let dual_detail = dual_update.detail.as_ref().expect("dual update detail");
        assert_eq!(selection_detail.label, "minimum-slack");
        assert_eq!(dual_detail.label, "delta");
        assert_eq!(selection_detail.value, dual_detail.value);
        assert_eq!(selection.minimum_granularity, TraceGranularityV1::Micro);
        assert_eq!(
            dual_update.minimum_granularity,
            TraceGranularityV1::Operation
        );
        assert_eq!(selection.entity_refs.len(), 1);
        assert!(matches!(
            selection.entity_refs[0],
            crate::trace::FlowTraceEntityRef::ResidualArc(_)
        ));
        assert_eq!(dual_update.entity_refs, selection.entity_refs);
        assert!(
            run.events
                .iter()
                .filter(|event| {
                    event.catalog_id == "hungarian.inspect-cell"
                        && event.entity_refs.len() == 1
                        && matches!(
                            event.entity_refs[0],
                            crate::trace::FlowTraceEntityRef::ResidualArc(_)
                        )
                })
                .count()
                >= 2
        );
        assert!(run.events.iter().any(|event| {
            event.catalog_id == "hungarian.augment"
                && event
                    .entity_refs
                    .iter()
                    .filter(|entity| {
                        matches!(entity, crate::trace::FlowTraceEntityRef::ResidualArc(_))
                    })
                    .count()
                    >= 2
        }));
        let mut snapshot = run.base_snapshot.clone();
        for event in &run.events {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Forward)
                .expect("forward");
        }
        assert_eq!(snapshot, run.final_snapshot);
        for event in run.events.iter().rev() {
            apply_trace_event(&graph, &mut snapshot, event, FlowTraceDirection::Reverse)
                .expect("reverse");
        }
        assert_eq!(snapshot, run.base_snapshot);
    }

    #[test]
    fn trace_identifies_a_forbidden_cell_by_its_exact_row_and_column() {
        let edges = vec![(0, 0, 0), (1, 1, 0)];
        let (graph, agents, tasks) = graph(2, 2, &edges);
        let run = trace_hungarian(&graph, &agents, &tasks, AssignmentObjectiveV1::Minimize)
            .expect("trace");
        let event = run
            .events
            .iter()
            .find(|event| {
                event.catalog_id == "hungarian.inspect-cell"
                    && matches!(
                        event.entity_refs.as_slice(),
                        [
                            FlowTraceEntityRef::Node(agent),
                            FlowTraceEntityRef::Node(task)
                        ] if agent.as_str() == "a00" && task.as_str() == "t01"
                    )
            })
            .expect("the forbidden matrix cell keeps its exact row and column identity");

        assert_eq!(event.minimum_granularity, TraceGranularityV1::Micro);
    }
}
