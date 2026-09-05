//! Validated native rectangular assignment model.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::model::{EdgeIndex, FlowEdge, FlowNetwork, FlowNode, NodeId, NodeIndex};

/// Shared practical node limit for native assignment algorithms.
pub const ASSIGNMENT_MAX_NODES: usize = 2_000;
/// Shared practical allowed-edge limit for native assignment algorithms.
pub const ASSIGNMENT_MAX_EDGES: usize = 20_000;
/// Maximum dense agent-by-task index allocated during model construction.
pub const ASSIGNMENT_MAX_DENSE_CELLS: usize = 1_000_000;

/// Whether the declared edge costs are minimized or maximized.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentObjectiveV1 {
    /// Minimize the sum of selected edge costs.
    Minimize,
    /// Maximize the sum of selected edge costs.
    Maximize,
}

/// One allowed agent-to-task assignment edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssignmentEdge {
    /// Original edge in canonical edge-ID order.
    pub(crate) edge: EdgeIndex,
    /// Dense position in [`AssignmentGraph::agents`].
    pub(crate) agent: usize,
    /// Dense position in [`AssignmentGraph::tasks`].
    pub(crate) task: usize,
    /// Cost in the user-declared objective orientation.
    pub(crate) cost: i64,
}

/// Strict native assignment reconstructed from a canonical flow graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentGraph {
    /// Exact canonical source graph snapshot used to reject cross-graph reuse.
    source_nodes: Box<[FlowNode]>,
    /// Exact canonical source edges used to reject identity or cost drift.
    source_edges: Box<[FlowEdge]>,
    /// Agents that must each receive exactly one task.
    pub(crate) agents: Vec<NodeIndex>,
    /// Tasks that may each be used at most once.
    pub(crate) tasks: Vec<NodeIndex>,
    /// Allowed assignment edges in canonical edge-ID order.
    pub(crate) edges: Vec<AssignmentEdge>,
    /// Allowed-edge ordinals leaving each agent.
    pub(crate) adjacency: Vec<Vec<usize>>,
    /// Allowed-edge ordinal for each dense `(agent, task)` pair.
    pub(crate) edge_by_pair: Vec<Vec<Option<usize>>>,
    /// User-declared objective direction.
    pub(crate) objective: AssignmentObjectiveV1,
}

impl AssignmentGraph {
    /// Validates the complete assignment declaration without allocating the
    /// dense agent-by-task lookup used by the executable algorithms.
    ///
    /// This validator intentionally has no solver admission limit. It is used
    /// at the Scenario trust boundary so an oversized malformed declaration
    /// cannot be mistaken for a valid resource-limit result.
    ///
    /// # Errors
    ///
    /// Rejects the same semantic model violations as [`Self::new`].
    pub fn validate_declaration(
        graph: &FlowNetwork,
        agent_ids: &[String],
        task_ids: &[String],
    ) -> Result<(), AssignmentModelError> {
        validate_assignment_declaration(graph, agent_ids, task_ids).map(|_| ())
    }

    /// Validates a native rectangular assignment declaration.
    ///
    /// Every graph node must belong to exactly one declared partition. Every
    /// edge must be a simple agent-to-task arc with lower bound zero and unit
    /// capacity. Missing pairs are forbidden assignments; costs may be any
    /// signed 64-bit integer. More agents than tasks is valid but infeasible.
    ///
    /// # Errors
    ///
    /// Rejects malformed partitions, undeclared nodes, duplicate pairs,
    /// non-assignment edges, nonzero supplies, or nonunit bounds.
    pub fn new(
        graph: &FlowNetwork,
        agent_ids: &[String],
        task_ids: &[String],
        objective: AssignmentObjectiveV1,
    ) -> Result<Self, AssignmentModelError> {
        let dense_cells = agent_ids
            .len()
            .checked_mul(task_ids.len())
            .ok_or(AssignmentModelError::AdmissionLimit)?;
        if graph.nodes().len() > ASSIGNMENT_MAX_NODES
            || graph.edges().len() > ASSIGNMENT_MAX_EDGES
            || dense_cells > ASSIGNMENT_MAX_DENSE_CELLS
        {
            return Err(AssignmentModelError::AdmissionLimit);
        }
        let declaration = validate_assignment_declaration(graph, agent_ids, task_ids)?;
        let mut edge_by_pair = vec![vec![None; declaration.tasks.len()]; declaration.agents.len()];
        for (ordinal, edge) in declaration.edges.iter().enumerate() {
            edge_by_pair[edge.agent][edge.task] = Some(ordinal);
        }
        Ok(Self {
            source_nodes: graph.nodes().to_vec().into_boxed_slice(),
            source_edges: graph.edges().to_vec().into_boxed_slice(),
            agents: declaration.agents,
            tasks: declaration.tasks,
            edges: declaration.edges,
            adjacency: declaration.adjacency,
            edge_by_pair,
            objective,
        })
    }

    /// Returns agents in canonical node-ID order.
    #[must_use]
    pub fn agents(&self) -> &[NodeIndex] {
        &self.agents
    }

    /// Returns tasks in canonical node-ID order.
    #[must_use]
    pub fn tasks(&self) -> &[NodeIndex] {
        &self.tasks
    }

    /// Returns allowed assignment edges in canonical original-edge order.
    #[must_use]
    pub fn edges(&self) -> &[AssignmentEdge] {
        &self.edges
    }

    /// Returns the declared objective direction.
    #[must_use]
    pub const fn objective(&self) -> AssignmentObjectiveV1 {
        self.objective
    }

    /// Verifies that this native model still belongs to `graph` and that all
    /// dense indices agree with a fresh canonical reconstruction.
    ///
    /// # Errors
    ///
    /// Rejects a different graph even when its node/edge counts happen to
    /// match, as well as any internal partition or adjacency drift.
    pub fn validate_against(&self, graph: &FlowNetwork) -> Result<(), AssignmentModelError> {
        if self.source_nodes.as_ref() != graph.nodes()
            || self.source_edges.as_ref() != graph.edges()
        {
            return Err(AssignmentModelError::GraphMismatch);
        }
        let agent_ids = self
            .agents
            .iter()
            .map(|&node| {
                graph
                    .node(node)
                    .map(|value| value.id().as_str().to_owned())
                    .ok_or(AssignmentModelError::ModelInvariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let task_ids = self
            .tasks
            .iter()
            .map(|&node| {
                graph
                    .node(node)
                    .map(|value| value.id().as_str().to_owned())
                    .ok_or(AssignmentModelError::ModelInvariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let reconstructed = Self::new(graph, &agent_ids, &task_ids, self.objective)?;
        if &reconstructed != self {
            return Err(AssignmentModelError::ModelInvariant);
        }
        Ok(())
    }

    /// Converts a task choice per agent into a canonical unit-flow vector.
    ///
    /// # Errors
    ///
    /// Rejects repeated tasks, missing agents, or a forbidden pair.
    pub fn flows_from_tasks(
        &self,
        graph: &FlowNetwork,
        task_by_agent: &[usize],
    ) -> Result<Vec<u64>, AssignmentModelError> {
        self.validate_against(graph)?;
        if task_by_agent.len() != self.agents.len() {
            return Err(AssignmentModelError::InvalidAssignment);
        }
        let mut used = vec![false; self.tasks.len()];
        let mut flows = vec![0; graph.edges().len()];
        for (agent, &task) in task_by_agent.iter().enumerate() {
            let used_task = used
                .get_mut(task)
                .ok_or(AssignmentModelError::InvalidAssignment)?;
            if std::mem::replace(used_task, true) {
                return Err(AssignmentModelError::InvalidAssignment);
            }
            let ordinal =
                self.edge_by_pair[agent][task].ok_or(AssignmentModelError::InvalidAssignment)?;
            let edge = self
                .edges
                .get(ordinal)
                .ok_or(AssignmentModelError::ModelInvariant)?;
            *flows
                .get_mut(edge.edge.as_usize())
                .ok_or(AssignmentModelError::GraphMismatch)? = 1;
        }
        Ok(flows)
    }

    /// Returns a cost in the normalized minimization orientation.
    #[must_use]
    pub fn normalized_cost(&self, edge: &AssignmentEdge) -> i128 {
        match self.objective {
            AssignmentObjectiveV1::Minimize => i128::from(edge.cost),
            AssignmentObjectiveV1::Maximize => -i128::from(edge.cost),
        }
    }
}

struct ValidatedAssignmentDeclaration {
    agents: Vec<NodeIndex>,
    tasks: Vec<NodeIndex>,
    edges: Vec<AssignmentEdge>,
    adjacency: Vec<Vec<usize>>,
}

fn validate_assignment_declaration(
    graph: &FlowNetwork,
    agent_ids: &[String],
    task_ids: &[String],
) -> Result<ValidatedAssignmentDeclaration, AssignmentModelError> {
    let (agents, tasks) = resolve_partitions(graph, agent_ids, task_ids)?;
    let agent_positions = dense_positions(graph.nodes().len(), &agents);
    let task_positions = dense_positions(graph.nodes().len(), &tasks);
    let mut pairs = BTreeSet::new();
    let mut edges = Vec::with_capacity(graph.edges().len());
    let mut adjacency = vec![Vec::new(); agents.len()];
    for (edge_position, edge) in graph.edges().iter().enumerate() {
        if edge.lower() != 0 || edge.capacity() != 1 {
            return Err(AssignmentModelError::NonunitEdge);
        }
        let agent = agent_positions
            .get(edge.from().as_usize())
            .copied()
            .flatten()
            .ok_or(AssignmentModelError::UnexpectedEdge)?;
        let task = task_positions
            .get(edge.to().as_usize())
            .copied()
            .flatten()
            .ok_or(AssignmentModelError::UnexpectedEdge)?;
        if !pairs.insert((agent, task)) {
            return Err(AssignmentModelError::DuplicatePair);
        }
        let edge_index = graph
            .edge_index(edge.id())
            .ok_or(AssignmentModelError::ModelInvariant)?;
        if edge_index.as_usize() != edge_position {
            return Err(AssignmentModelError::ModelInvariant);
        }
        let ordinal = edges.len();
        edges.push(AssignmentEdge {
            edge: edge_index,
            agent,
            task,
            cost: edge.cost(),
        });
        adjacency[agent].push(ordinal);
    }
    Ok(ValidatedAssignmentDeclaration {
        agents,
        tasks,
        edges,
        adjacency,
    })
}

impl AssignmentEdge {
    /// Returns the corresponding canonical original edge.
    #[must_use]
    pub const fn edge(&self) -> EdgeIndex {
        self.edge
    }

    /// Returns the dense canonical agent position.
    #[must_use]
    pub const fn agent(&self) -> usize {
        self.agent
    }

    /// Returns the dense canonical task position.
    #[must_use]
    pub const fn task(&self) -> usize {
        self.task
    }

    /// Returns the original signed edge cost.
    #[must_use]
    pub const fn cost(&self) -> i64 {
        self.cost
    }
}

fn resolve_partitions(
    graph: &FlowNetwork,
    agent_ids: &[String],
    task_ids: &[String],
) -> Result<(Vec<NodeIndex>, Vec<NodeIndex>), AssignmentModelError> {
    if agent_ids.is_empty() || task_ids.is_empty() {
        return Err(AssignmentModelError::EmptyPartition);
    }
    validate_canonical_ids(agent_ids)?;
    validate_canonical_ids(task_ids)?;
    let agents = resolve_partition(graph, agent_ids)?;
    let tasks = resolve_partition(graph, task_ids)?;
    let agent_set = agents.iter().copied().collect::<BTreeSet<_>>();
    let task_set = tasks.iter().copied().collect::<BTreeSet<_>>();
    if !agent_set.is_disjoint(&task_set) {
        return Err(AssignmentModelError::OverlappingPartitions);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(AssignmentModelError::NonzeroSupply);
    }
    if agent_set.len().checked_add(task_set.len()) != Some(graph.nodes().len()) {
        return Err(AssignmentModelError::UnexpectedNode);
    }
    Ok((agents, tasks))
}

fn validate_canonical_ids(ids: &[String]) -> Result<(), AssignmentModelError> {
    let mut previous: Option<NodeId> = None;
    for id in ids {
        let parsed = NodeId::parse(id).map_err(|_| AssignmentModelError::InvalidNodeId)?;
        if previous.as_ref().is_some_and(|value| value >= &parsed) {
            return Err(AssignmentModelError::NoncanonicalPartition);
        }
        previous = Some(parsed);
    }
    Ok(())
}

fn resolve_partition(
    graph: &FlowNetwork,
    ids: &[String],
) -> Result<Vec<NodeIndex>, AssignmentModelError> {
    ids.iter()
        .map(|id| {
            let id = NodeId::parse(id).map_err(|_| AssignmentModelError::InvalidNodeId)?;
            graph
                .node_index(&id)
                .ok_or(AssignmentModelError::MissingNode)
        })
        .collect()
}

fn dense_positions(node_count: usize, nodes: &[NodeIndex]) -> Vec<Option<usize>> {
    let mut positions = vec![None; node_count];
    for (position, &node) in nodes.iter().enumerate() {
        positions[node.as_usize()] = Some(position);
    }
    positions
}

/// Native assignment validation or projection failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AssignmentModelError {
    /// Input exceeds the practical assignment admission band.
    #[error("assignment graph exceeds admission limits")]
    AdmissionLimit,
    /// Both partitions must contain at least one node.
    #[error("assignment partitions must be nonempty")]
    EmptyPartition,
    /// Partition identities must be strictly increasing.
    #[error("assignment partitions must use canonical node-ID order")]
    NoncanonicalPartition,
    /// A partition entry is not a valid stable node identity.
    #[error("assignment partition contains an invalid node ID")]
    InvalidNodeId,
    /// A declared partition node is absent from the graph.
    #[error("assignment partition references a missing node")]
    MissingNode,
    /// An agent is also declared as a task.
    #[error("assignment partitions overlap")]
    OverlappingPartitions,
    /// A graph node is outside both assignment partitions.
    #[error("assignment graph contains an undeclared node")]
    UnexpectedNode,
    /// Native assignment nodes must have zero supply.
    #[error("assignment graph requires zero node supplies")]
    NonzeroSupply,
    /// An edge is not directed from an agent to a task.
    #[error("assignment graph contains a non agent-to-task edge")]
    UnexpectedEdge,
    /// Assignment edges must have lower zero and capacity one.
    #[error("assignment edges must have lower zero and capacity one")]
    NonunitEdge,
    /// The same agent/task pair was declared more than once.
    #[error("assignment graph contains duplicate allowed pairs")]
    DuplicatePair,
    /// A task vector is not a complete injective allowed assignment.
    #[error("candidate is not a complete allowed assignment")]
    InvalidAssignment,
    /// A model was reused with a graph other than the one it validated.
    #[error("assignment model does not belong to this graph")]
    GraphMismatch,
    /// Canonical graph indices contradicted their public ordering contract.
    #[error("assignment graph canonical index invariant failed")]
    ModelInvariant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::{
        AssignmentHallWitness, CertificateError, certify_assignment_optimality, check_assignment,
        check_assignment_infeasibility,
    };
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    fn graph(edges: &[(&str, &str, &str, i64)]) -> FlowNetwork {
        let nodes = ["a0", "a1", "t0", "t1", "t2"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("id"), 0))
            .collect();
        let edges = edges
            .iter()
            .map(|&(id, from, to, cost)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).expect("edge id"),
                from: NodeId::parse(from).expect("from"),
                to: NodeId::parse(to).expect("to"),
                lower: 0,
                capacity: 1,
                cost,
            })
            .collect();
        FlowNetwork::new(nodes, edges).expect("graph")
    }

    #[test]
    fn validates_rectangular_sparse_assignment_and_projects_flow() {
        let graph = graph(&[("e00", "a0", "t0", -2), ("e12", "a1", "t2", 7)]);
        let model = AssignmentGraph::new(
            &graph,
            &["a0".to_owned(), "a1".to_owned()],
            &["t0".to_owned(), "t1".to_owned(), "t2".to_owned()],
            AssignmentObjectiveV1::Minimize,
        )
        .expect("model");
        assert_eq!(model.flows_from_tasks(&graph, &[0, 2]), Ok(vec![1, 1]));
    }

    #[test]
    fn rejects_noncanonical_duplicate_and_wrong_direction_models() {
        let base = graph(&[("e00", "a0", "t0", 0)]);
        assert_eq!(
            AssignmentGraph::new(
                &base,
                &["a1".to_owned(), "a0".to_owned()],
                &["t0".to_owned(), "t1".to_owned(), "t2".to_owned()],
                AssignmentObjectiveV1::Minimize,
            ),
            Err(AssignmentModelError::NoncanonicalPartition)
        );
        let reverse = graph(&[("e00", "t0", "a0", 0)]);
        assert_eq!(
            AssignmentGraph::new(
                &reverse,
                &["a0".to_owned(), "a1".to_owned()],
                &["t0".to_owned(), "t1".to_owned(), "t2".to_owned()],
                AssignmentObjectiveV1::Minimize,
            ),
            Err(AssignmentModelError::UnexpectedEdge)
        );
    }

    #[test]
    fn rejects_cross_graph_model_reuse_at_every_public_projection_boundary() {
        let source = graph(&[("e00", "a0", "t0", 1), ("e12", "a1", "t2", 2)]);
        let changed_cost = graph(&[("e00", "a0", "t0", 1), ("e12", "a1", "t2", 3)]);
        let model = AssignmentGraph::new(
            &source,
            &["a0".to_owned(), "a1".to_owned()],
            &["t0".to_owned(), "t1".to_owned(), "t2".to_owned()],
            AssignmentObjectiveV1::Minimize,
        )
        .expect("source model");

        assert_eq!(
            model.flows_from_tasks(&changed_cost, &[0, 2]),
            Err(AssignmentModelError::GraphMismatch)
        );
        assert_eq!(
            check_assignment(&changed_cost, &model, &[1, 1], &[1, 2], &[0, 0, 0]),
            Err(CertificateError::AssignmentModelMismatch)
        );
        assert_eq!(
            certify_assignment_optimality(&changed_cost, &model, &[1, 1]),
            Err(CertificateError::AssignmentModelMismatch)
        );
        let witness = AssignmentHallWitness {
            agents: vec![NodeId::parse("a0").expect("agent")],
            neighbor_tasks: Vec::new(),
            deficiency: 1,
        };
        assert_eq!(
            check_assignment_infeasibility(&changed_cost, &model, &witness),
            Err(CertificateError::AssignmentModelMismatch)
        );

        let smaller = graph(&[("e00", "a0", "t0", 1)]);
        assert_eq!(
            model.flows_from_tasks(&smaller, &[0, 2]),
            Err(AssignmentModelError::GraphMismatch)
        );
    }

    #[test]
    fn rejects_dense_assignment_before_allocating_pair_table() {
        let base = graph(&[]);
        let agents = (0..1_001)
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>();
        let tasks = (0..1_000)
            .map(|index| format!("t{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            AssignmentGraph::new(&base, &agents, &tasks, AssignmentObjectiveV1::Minimize,),
            Err(AssignmentModelError::AdmissionLimit)
        );
    }
}
