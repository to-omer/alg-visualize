//! Solver-independent feasibility and optimality certificate checkers.

use std::collections::{BTreeSet, VecDeque};

use thiserror::Error;

use crate::assignment::{AssignmentGraph, AssignmentObjectiveV1};
use crate::bipartite::BipartiteMatchingGraph;
use crate::model::{EdgeId, FlowNetwork, FlowNode, NodeId, NodeIndex};
use crate::residual::{ResidualError, ResidualState};

/// Verified maximum-flow value and original-graph minimum cut.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaxFlowCertificate {
    /// Net original flow leaving the source.
    pub value: i128,
    /// Lower-aware capacity of the verified original cut.
    pub cut_bound: i128,
    /// Canonically ordered node identities reachable from the source.
    pub source_side: Vec<NodeId>,
}

/// Verified minimum-cost-flow objective and dual potentials.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinCostFlowCertificate {
    /// Exact integral objective value.
    pub total_cost: i128,
    /// One potential per canonical node; every residual reduced cost is nonnegative.
    pub potentials: Vec<i128>,
}

/// Verified lexicographic minimum-cost maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinCostMaxFlowCertificate {
    /// Maximum-flow value and an original-graph minimum cut.
    pub max_flow: MaxFlowCertificate,
    /// Minimum cost and feasible dual potentials at that maximum-flow value.
    pub min_cost: MinCostFlowCertificate,
}

/// One independently verified matched compatibility edge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BipartiteMatchingPair {
    /// Stable compatibility-edge identity.
    pub edge: EdgeId,
    /// Matched left vertex.
    pub left: NodeId,
    /// Matched right vertex.
    pub right: NodeId,
}

/// Maximum-cardinality matching plus its Kőnig minimum-vertex-cover witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BipartiteMatchingCertificate {
    /// Number of matched pairs.
    pub cardinality: u64,
    /// Matched compatibility edges in canonical edge-ID order.
    pub pairs: Vec<BipartiteMatchingPair>,
    /// Left vertices in the minimum cover, in canonical node-ID order.
    pub cover_left: Vec<NodeId>,
    /// Right vertices in the minimum cover, in canonical node-ID order.
    pub cover_right: Vec<NodeId>,
}

/// One selected edge in an independently verified complete assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentPair {
    /// Stable allowed-edge identity.
    pub edge: EdgeId,
    /// Assigned agent identity.
    pub agent: NodeId,
    /// Assigned task identity.
    pub task: NodeId,
    /// Original user-declared edge cost.
    pub cost: i64,
}

/// Complete assignment plus a feasible tight primal/dual certificate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentCertificate {
    /// User-declared objective direction.
    pub objective: AssignmentObjectiveV1,
    /// Exact objective in the original cost orientation.
    pub total_cost: i128,
    /// One selected edge per agent, in canonical edge-ID order.
    pub pairs: Vec<AssignmentPair>,
    /// One dual label per canonical agent.
    pub agent_labels: Vec<i128>,
    /// One dual label per canonical task.
    pub task_labels: Vec<i128>,
}

/// Hall-deficient agent set proving that complete assignment is impossible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignmentHallWitness {
    /// Nonempty canonical subset of agents.
    pub agents: Vec<NodeId>,
    /// Exact canonical neighborhood of `agents` through allowed edges.
    pub neighbor_tasks: Vec<NodeId>,
    /// Exact positive value `|agents| - |neighbor_tasks|`.
    pub deficiency: u64,
}

/// Certificate rejection reason reconstructed only from graph and candidate result.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CertificateError {
    /// Candidate flow count differs from original edge count.
    #[error("candidate flow vector length does not match original edge count")]
    FlowVectorLength,
    /// A candidate original-edge flow violates its lower or upper bound.
    #[error("candidate flow violates bounds on edge {0}")]
    EdgeBounds(String),
    /// Max-flow terminals are missing, equal, or otherwise invalid.
    #[error("invalid maximum-flow terminals")]
    InvalidTerminals,
    /// Max-flow inputs must not combine the terminal model with node supplies.
    #[error("maximum-flow certificate requires zero node supplies")]
    UnexpectedSupply,
    /// A node's reconstructed divergence differs from the required value.
    #[error("flow conservation failed at node {node}: expected {expected}, got {actual}")]
    Conservation {
        /// Stable node identity.
        node: String,
        /// Required outgoing-minus-incoming flow.
        expected: i128,
        /// Reconstructed outgoing-minus-incoming flow.
        actual: i128,
    },
    /// Source and sink values do not cancel.
    #[error("source and sink flow values do not agree")]
    TerminalValueMismatch,
    /// The sink remains reachable through a positive-capacity residual arc.
    #[error("candidate maximum flow still has an augmenting path")]
    SinkReachable,
    /// Reconstructed original cut capacity does not equal the flow value.
    #[error("lower-aware original cut does not equal candidate flow value")]
    CutMismatch,
    /// A residual negative cycle exists in at least one component.
    #[error("candidate residual graph contains a negative-cost cycle")]
    NegativeCycle,
    /// An internal dual check failed after shortest-distance reconstruction.
    #[error("reconstructed potential has a negative residual reduced cost")]
    DualInfeasible,
    /// Exact certificate arithmetic exceeded the declared numeric domain.
    #[error("certificate arithmetic overflow")]
    ArithmeticOverflow,
    /// Candidate unit flows do not describe a matching.
    #[error("candidate edges do not form a bipartite matching")]
    InvalidMatching,
    /// Optional `s-L-R-t` adapter flows disagree with the native matching.
    #[error("candidate flow adapter disagrees with the native matching")]
    MatchingAdapterMismatch,
    /// The alternating graph still contains an augmenting path.
    #[error("candidate matching still has an augmenting path")]
    MatchingAugmentingPath,
    /// Matching cardinality and reconstructed minimum cover disagree.
    #[error("candidate matching and minimum vertex cover cardinalities disagree")]
    MatchingCoverMismatch,
    /// Selected edges do not assign every agent to a distinct allowed task.
    #[error("candidate edges do not form a complete assignment")]
    InvalidAssignment,
    /// Assignment dual labels have the wrong dimensions or sign domain.
    #[error("assignment dual label domain is invalid")]
    AssignmentDualDomain,
    /// An allowed assignment edge violates dual feasibility.
    #[error("assignment dual labels are infeasible")]
    AssignmentDualInfeasible,
    /// A selected assignment edge is not dual-tight.
    #[error("selected assignment edge is not dual-tight")]
    AssignmentMatchedEdgeNotTight,
    /// Assignment primal and dual objective values differ.
    #[error("assignment primal and dual objectives differ")]
    AssignmentObjectiveMismatch,
    /// No exact dual can be tight on every selected assignment edge.
    #[error("candidate assignment is not optimal")]
    AssignmentNotOptimal,
    /// A Hall witness is empty, noncanonical, or references the wrong partition.
    #[error("assignment Hall witness identities are invalid")]
    AssignmentHallIdentity,
    /// The declared Hall neighborhood is not exact.
    #[error("assignment Hall witness neighborhood is not exact")]
    AssignmentHallNeighborhood,
    /// The declared Hall set is not deficient.
    #[error("assignment Hall witness is not deficient")]
    AssignmentHallDeficiency,
    /// A native assignment model was paired with a different canonical graph.
    #[error("assignment model does not match the canonical graph")]
    AssignmentModelMismatch,
}

/// Verifies a complete rectangular assignment and its oriented LP dual.
///
/// For minimization, task labels must be nonpositive and every allowed edge
/// satisfies `agent_label + task_label <= cost`. For maximization the signs
/// and inequality reverse. Every selected edge must be tight, and the dual
/// label sum must equal the reconstructed original-cost objective.
///
/// # Errors
///
/// Rejects invalid unit flows, repeated/missing endpoints, infeasible or
/// nontight labels, arithmetic overflow, or unequal primal/dual objectives.
pub fn check_assignment(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    flows: &[u64],
    agent_labels: &[i128],
    task_labels: &[i128],
) -> Result<AssignmentCertificate, CertificateError> {
    model
        .validate_against(graph)
        .map_err(|_| CertificateError::AssignmentModelMismatch)?;
    checked_state(graph, flows)?;
    if agent_labels.len() != model.agents.len() || task_labels.len() != model.tasks.len() {
        return Err(CertificateError::AssignmentDualDomain);
    }
    let mut pair_by_agent = vec![None; model.agents.len()];
    let mut used_tasks = vec![false; model.tasks.len()];
    let mut pairs = Vec::with_capacity(model.agents.len());
    let mut total_cost = 0_i128;
    for (ordinal, assignment) in model.edges.iter().enumerate() {
        let flow = flows
            .get(assignment.edge.as_usize())
            .copied()
            .ok_or(CertificateError::FlowVectorLength)?;
        if flow == 0 {
            continue;
        }
        if flow != 1
            || pair_by_agent[assignment.agent].replace(ordinal).is_some()
            || std::mem::replace(&mut used_tasks[assignment.task], true)
        {
            return Err(CertificateError::InvalidAssignment);
        }
        total_cost = total_cost
            .checked_add(i128::from(assignment.cost))
            .ok_or(CertificateError::ArithmeticOverflow)?;
        let edge = graph
            .edge(assignment.edge)
            .ok_or(CertificateError::InvalidAssignment)?;
        let agent = graph
            .node(model.agents[assignment.agent])
            .ok_or(CertificateError::InvalidAssignment)?;
        let task = graph
            .node(model.tasks[assignment.task])
            .ok_or(CertificateError::InvalidAssignment)?;
        pairs.push(AssignmentPair {
            edge: edge.id().clone(),
            agent: agent.id().clone(),
            task: task.id().clone(),
            cost: assignment.cost,
        });
    }
    if pair_by_agent.iter().any(Option::is_none) {
        return Err(CertificateError::InvalidAssignment);
    }
    verify_assignment_dual(model, &pair_by_agent, agent_labels, task_labels, total_cost)?;
    Ok(AssignmentCertificate {
        objective: model.objective,
        total_cost,
        pairs,
        agent_labels: agent_labels.to_vec(),
        task_labels: task_labels.to_vec(),
    })
}

/// Reconstructs an exact rectangular-assignment dual from candidate unit flows.
///
/// The reconstruction is independent of any solver labels. It substitutes the
/// tight matched-edge equalities into the dual and solves the remaining task
/// difference constraints. A negative cycle therefore proves that the
/// candidate assignment is not optimal. Otherwise [`check_assignment`] checks
/// the reconstructed primal/dual certificate from scratch.
///
/// # Errors
///
/// Rejects incomplete/noninjective candidate flows, a nonoptimal assignment,
/// or checked arithmetic/certificate failure.
pub fn certify_assignment_optimality(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    flows: &[u64],
) -> Result<AssignmentCertificate, CertificateError> {
    model
        .validate_against(graph)
        .map_err(|_| CertificateError::AssignmentModelMismatch)?;
    checked_state(graph, flows)?;
    let (matched_edge_by_agent, used_tasks) = assignment_matching(model, flows)?;
    let (agent_labels, task_labels) =
        normalized_assignment_dual(model, &matched_edge_by_agent, &used_tasks)?;
    let (agent_labels, task_labels) =
        orient_assignment_dual(model.objective, agent_labels, task_labels)?;
    check_assignment(graph, model, flows, &agent_labels, &task_labels)
}

fn assignment_matching(
    model: &AssignmentGraph,
    flows: &[u64],
) -> Result<(Vec<usize>, Vec<bool>), CertificateError> {
    let mut matched_edge_by_agent = vec![None; model.agents.len()];
    let mut used_tasks = vec![false; model.tasks.len()];
    for (ordinal, assignment) in model.edges.iter().enumerate() {
        let flow = flows
            .get(assignment.edge.as_usize())
            .copied()
            .ok_or(CertificateError::FlowVectorLength)?;
        if flow == 0 {
            continue;
        }
        if flow != 1
            || matched_edge_by_agent[assignment.agent]
                .replace(ordinal)
                .is_some()
            || std::mem::replace(&mut used_tasks[assignment.task], true)
        {
            return Err(CertificateError::InvalidAssignment);
        }
    }
    let matched_edge_by_agent = matched_edge_by_agent
        .into_iter()
        .map(|edge| edge.ok_or(CertificateError::InvalidAssignment))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((matched_edge_by_agent, used_tasks))
}

fn normalized_assignment_dual(
    model: &AssignmentGraph,
    matched_edge_by_agent: &[usize],
    used_tasks: &[bool],
) -> Result<(Vec<i128>, Vec<i128>), CertificateError> {
    // In normalized minimization orientation the rectangular dual is
    // U_i + V_j <= d_ij with V_j <= 0. Tightness on matched task m(i)
    // gives V_j <= V_m(i) + d_ij - d_i,m(i). A synthetic zero vertex
    // expresses V_j <= 0; unmatched-task complementary slackness adds the
    // reverse constraint and therefore fixes its label to zero.
    let synthetic = model.tasks.len();
    let mut constraints = Vec::with_capacity(model.edges.len() + model.tasks.len() * 2);
    for (task, &matched) in used_tasks.iter().enumerate() {
        constraints.push((synthetic, task, 0_i128));
        if !matched {
            constraints.push((task, synthetic, 0_i128));
        }
    }
    for edge in &model.edges {
        let matched = model
            .edges
            .get(matched_edge_by_agent[edge.agent])
            .ok_or(CertificateError::InvalidAssignment)?;
        let difference = model
            .normalized_cost(edge)
            .checked_sub(model.normalized_cost(matched))
            .ok_or(CertificateError::ArithmeticOverflow)?;
        constraints.push((matched.task, edge.task, difference));
    }
    let vertex_count = model
        .tasks
        .len()
        .checked_add(1)
        .ok_or(CertificateError::ArithmeticOverflow)?;
    let mut distances = vec![0_i128; vertex_count];
    for pass in 0..vertex_count {
        let mut changed = false;
        for &(from, to, difference) in &constraints {
            let candidate = distances[from]
                .checked_add(difference)
                .ok_or(CertificateError::ArithmeticOverflow)?;
            if candidate < distances[to] {
                distances[to] = candidate;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        if pass + 1 == vertex_count {
            return Err(CertificateError::AssignmentNotOptimal);
        }
    }
    let task_labels = distances[..model.tasks.len()]
        .iter()
        .map(|&distance| {
            distance
                .checked_sub(distances[synthetic])
                .ok_or(CertificateError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let agent_labels = matched_edge_by_agent
        .iter()
        .map(|&ordinal| {
            let edge = model
                .edges
                .get(ordinal)
                .ok_or(CertificateError::InvalidAssignment)?;
            model
                .normalized_cost(edge)
                .checked_sub(task_labels[edge.task])
                .ok_or(CertificateError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((agent_labels, task_labels))
}

fn orient_assignment_dual(
    objective: AssignmentObjectiveV1,
    agent_labels: Vec<i128>,
    task_labels: Vec<i128>,
) -> Result<(Vec<i128>, Vec<i128>), CertificateError> {
    Ok(match objective {
        AssignmentObjectiveV1::Minimize => (agent_labels, task_labels),
        AssignmentObjectiveV1::Maximize => (
            agent_labels
                .into_iter()
                .map(|label| {
                    label
                        .checked_neg()
                        .ok_or(CertificateError::ArithmeticOverflow)
                })
                .collect::<Result<Vec<_>, _>>()?,
            task_labels
                .into_iter()
                .map(|label| {
                    label
                        .checked_neg()
                        .ok_or(CertificateError::ArithmeticOverflow)
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    })
}

fn verify_assignment_dual(
    model: &AssignmentGraph,
    pair_by_agent: &[Option<usize>],
    agent_labels: &[i128],
    task_labels: &[i128],
    total_cost: i128,
) -> Result<(), CertificateError> {
    match model.objective {
        AssignmentObjectiveV1::Minimize if task_labels.iter().any(|&label| label > 0) => {
            return Err(CertificateError::AssignmentDualDomain);
        }
        AssignmentObjectiveV1::Maximize if task_labels.iter().any(|&label| label < 0) => {
            return Err(CertificateError::AssignmentDualDomain);
        }
        AssignmentObjectiveV1::Minimize | AssignmentObjectiveV1::Maximize => {}
    }
    for assignment in &model.edges {
        let label_sum = agent_labels[assignment.agent]
            .checked_add(task_labels[assignment.task])
            .ok_or(CertificateError::ArithmeticOverflow)?;
        let cost = i128::from(assignment.cost);
        let feasible = match model.objective {
            AssignmentObjectiveV1::Minimize => label_sum <= cost,
            AssignmentObjectiveV1::Maximize => label_sum >= cost,
        };
        if !feasible {
            return Err(CertificateError::AssignmentDualInfeasible);
        }
    }
    for (agent, pair) in pair_by_agent.iter().copied().enumerate() {
        let assignment = model
            .edges
            .get(pair.ok_or(CertificateError::InvalidAssignment)?)
            .ok_or(CertificateError::InvalidAssignment)?;
        let label_sum = agent_labels[agent]
            .checked_add(task_labels[assignment.task])
            .ok_or(CertificateError::ArithmeticOverflow)?;
        if label_sum != i128::from(assignment.cost) {
            return Err(CertificateError::AssignmentMatchedEdgeNotTight);
        }
    }
    let dual_cost = agent_labels
        .iter()
        .chain(task_labels)
        .try_fold(0_i128, |sum, &label| sum.checked_add(label))
        .ok_or(CertificateError::ArithmeticOverflow)?;
    if dual_cost != total_cost {
        return Err(CertificateError::AssignmentObjectiveMismatch);
    }
    Ok(())
}

/// Independently verifies an exact Hall-deficiency witness.
///
/// # Errors
///
/// Rejects noncanonical identities, an inexact allowed-edge neighborhood, or
/// a nonpositive/misstated deficiency.
pub fn check_assignment_infeasibility(
    graph: &FlowNetwork,
    model: &AssignmentGraph,
    witness: &AssignmentHallWitness,
) -> Result<(), CertificateError> {
    model
        .validate_against(graph)
        .map_err(|_| CertificateError::AssignmentModelMismatch)?;
    if witness.agents.is_empty()
        || !strictly_increasing(&witness.agents)
        || !strictly_increasing(&witness.neighbor_tasks)
    {
        return Err(CertificateError::AssignmentHallIdentity);
    }
    let agent_positions = model
        .agents
        .iter()
        .enumerate()
        .filter_map(|(position, &node)| graph.node(node).map(|value| (value.id(), position)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let task_ids = model
        .tasks
        .iter()
        .filter_map(|&node| graph.node(node).map(FlowNode::id))
        .collect::<BTreeSet<_>>();
    if witness
        .agents
        .iter()
        .any(|agent| !agent_positions.contains_key(agent))
        || witness
            .neighbor_tasks
            .iter()
            .any(|task| !task_ids.contains(task))
    {
        return Err(CertificateError::AssignmentHallIdentity);
    }
    let mut exact_neighbors = BTreeSet::new();
    for agent in &witness.agents {
        let position = *agent_positions
            .get(agent)
            .ok_or(CertificateError::AssignmentHallIdentity)?;
        for &ordinal in &model.adjacency[position] {
            let assignment = model
                .edges
                .get(ordinal)
                .ok_or(CertificateError::AssignmentHallNeighborhood)?;
            let task = graph
                .node(model.tasks[assignment.task])
                .ok_or(CertificateError::AssignmentHallNeighborhood)?;
            exact_neighbors.insert(task.id().clone());
        }
    }
    if exact_neighbors.iter().ne(witness.neighbor_tasks.iter()) {
        return Err(CertificateError::AssignmentHallNeighborhood);
    }
    let deficiency = witness
        .agents
        .len()
        .checked_sub(witness.neighbor_tasks.len())
        .and_then(|value| u64::try_from(value).ok())
        .filter(|&value| value > 0)
        .ok_or(CertificateError::AssignmentHallDeficiency)?;
    if witness.deficiency != deficiency {
        return Err(CertificateError::AssignmentHallDeficiency);
    }
    Ok(())
}

fn strictly_increasing<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Independently validates a native bipartite matching and reconstructs a
/// minimum vertex cover from its final alternating-reachability graph.
///
/// The checker does not trust solver-maintained BFS levels or pair arrays. It
/// derives matching incidence only from the candidate original-edge flows,
/// rejects any remaining augmenting path, and verifies Kőnig equality between
/// the matching cardinality and `(L \\ Z_L) ∪ Z_R`.
///
/// # Errors
///
/// Rejects invalid flow bounds, repeated endpoints, inconsistent adapter flow,
/// a remaining augmenting path, or a matching/cover cardinality mismatch.
pub fn check_bipartite_matching(
    graph: &FlowNetwork,
    model: &BipartiteMatchingGraph,
    flows: &[u64],
) -> Result<BipartiteMatchingCertificate, CertificateError> {
    checked_state(graph, flows)?;
    let matching = reconstruct_matching(graph, model, flows)?;
    verify_matching_adapter(model, flows, &matching)?;
    let (reachable_left, reachable_right) = alternating_reachability(model, &matching)?;
    build_matching_certificate(
        graph,
        model,
        matching.pairs,
        &reachable_left,
        &reachable_right,
    )
}

struct ReconstructedMatching {
    pair_by_left: Vec<Option<usize>>,
    pair_by_right: Vec<Option<usize>>,
    pairs: Vec<BipartiteMatchingPair>,
}

fn reconstruct_matching(
    graph: &FlowNetwork,
    model: &BipartiteMatchingGraph,
    flows: &[u64],
) -> Result<ReconstructedMatching, CertificateError> {
    let mut pair_by_left = vec![None; model.left.len()];
    let mut pair_by_right = vec![None; model.right.len()];
    let mut pairs = Vec::new();
    for (ordinal, compatibility) in model.compatibility_edges.iter().enumerate() {
        let flow = flows
            .get(compatibility.edge.as_usize())
            .copied()
            .ok_or(CertificateError::FlowVectorLength)?;
        if flow == 0 {
            continue;
        }
        if flow != 1
            || pair_by_left[compatibility.left].replace(ordinal).is_some()
            || pair_by_right[compatibility.right]
                .replace(ordinal)
                .is_some()
        {
            return Err(CertificateError::InvalidMatching);
        }
        let edge = graph
            .edge(compatibility.edge)
            .ok_or(CertificateError::InvalidMatching)?;
        let left = graph
            .node(model.left[compatibility.left])
            .ok_or(CertificateError::InvalidMatching)?;
        let right = graph
            .node(model.right[compatibility.right])
            .ok_or(CertificateError::InvalidMatching)?;
        pairs.push(BipartiteMatchingPair {
            edge: edge.id().clone(),
            left: left.id().clone(),
            right: right.id().clone(),
        });
    }
    Ok(ReconstructedMatching {
        pair_by_left,
        pair_by_right,
        pairs,
    })
}

fn verify_matching_adapter(
    model: &BipartiteMatchingGraph,
    flows: &[u64],
    matching: &ReconstructedMatching,
) -> Result<(), CertificateError> {
    if model.source.is_none() {
        return Ok(());
    }
    for (left, edge) in model.source_edges.iter().copied().enumerate() {
        let expected = u64::from(matching.pair_by_left[left].is_some());
        if flows.get(edge.as_usize()).copied() != Some(expected) {
            return Err(CertificateError::MatchingAdapterMismatch);
        }
    }
    for (right, edge) in model.sink_edges.iter().copied().enumerate() {
        let expected = u64::from(matching.pair_by_right[right].is_some());
        if flows.get(edge.as_usize()).copied() != Some(expected) {
            return Err(CertificateError::MatchingAdapterMismatch);
        }
    }
    Ok(())
}

fn alternating_reachability(
    model: &BipartiteMatchingGraph,
    matching: &ReconstructedMatching,
) -> Result<(Vec<bool>, Vec<bool>), CertificateError> {
    let mut reachable_left = vec![false; model.left.len()];
    let mut reachable_right = vec![false; model.right.len()];
    let mut queue = VecDeque::new();
    for (left, pair) in matching.pair_by_left.iter().enumerate() {
        if pair.is_none() {
            reachable_left[left] = true;
            queue.push_back(left);
        }
    }
    while let Some(left) = queue.pop_front() {
        for &ordinal in &model.adjacency[left] {
            if matching.pair_by_left[left] == Some(ordinal) {
                continue;
            }
            let compatibility = model
                .compatibility_edges
                .get(ordinal)
                .ok_or(CertificateError::InvalidMatching)?;
            let right = compatibility.right;
            if reachable_right[right] {
                continue;
            }
            reachable_right[right] = true;
            let Some(matched) = matching.pair_by_right[right] else {
                return Err(CertificateError::MatchingAugmentingPath);
            };
            let matched_edge = model
                .compatibility_edges
                .get(matched)
                .ok_or(CertificateError::InvalidMatching)?;
            if !reachable_left[matched_edge.left] {
                reachable_left[matched_edge.left] = true;
                queue.push_back(matched_edge.left);
            }
        }
    }
    Ok((reachable_left, reachable_right))
}

fn build_matching_certificate(
    graph: &FlowNetwork,
    model: &BipartiteMatchingGraph,
    pairs: Vec<BipartiteMatchingPair>,
    reachable_left: &[bool],
    reachable_right: &[bool],
) -> Result<BipartiteMatchingCertificate, CertificateError> {
    let cover_left_indices = reachable_left
        .iter()
        .enumerate()
        .filter_map(|(index, &reachable)| (!reachable).then_some(index))
        .collect::<BTreeSet<_>>();
    let cover_right_indices = reachable_right
        .iter()
        .enumerate()
        .filter_map(|(index, &reachable)| reachable.then_some(index))
        .collect::<BTreeSet<_>>();
    for compatibility in &model.compatibility_edges {
        if !cover_left_indices.contains(&compatibility.left)
            && !cover_right_indices.contains(&compatibility.right)
        {
            return Err(CertificateError::MatchingCoverMismatch);
        }
    }
    let cover_size = cover_left_indices
        .len()
        .checked_add(cover_right_indices.len())
        .ok_or(CertificateError::ArithmeticOverflow)?;
    if cover_size != pairs.len() {
        return Err(CertificateError::MatchingCoverMismatch);
    }
    let cardinality =
        u64::try_from(pairs.len()).map_err(|_| CertificateError::ArithmeticOverflow)?;
    let cover_left = cover_left_indices
        .into_iter()
        .map(|index| {
            graph
                .node(model.left[index])
                .map(|node| node.id().clone())
                .ok_or(CertificateError::InvalidMatching)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cover_right = cover_right_indices
        .into_iter()
        .map(|index| {
            graph
                .node(model.right[index])
                .map(|node| node.id().clone())
                .ok_or(CertificateError::InvalidMatching)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BipartiteMatchingCertificate {
        cardinality,
        pairs,
        cover_left,
        cover_right,
    })
}

/// Builds required node divergences from supplies plus an exact terminal flow.
///
/// Positive values mean outgoing flow minus incoming flow. This helper permits
/// fixed-flow transshipment instances where ordinary supplies coexist with the
/// requested source-to-sink amount.
///
/// # Errors
///
/// Rejects invalid terminals or checked arithmetic overflow.
pub fn fixed_flow_divergences(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    required_flow: u64,
) -> Result<Vec<i128>, CertificateError> {
    validate_terminals(graph, source, sink)?;
    let mut target = graph
        .nodes()
        .iter()
        .map(|node| i128::from(node.supply()))
        .collect::<Vec<_>>();
    target[source.as_usize()] = target[source.as_usize()]
        .checked_add(i128::from(required_flow))
        .ok_or(CertificateError::ArithmeticOverflow)?;
    target[sink.as_usize()] = target[sink.as_usize()]
        .checked_sub(i128::from(required_flow))
        .ok_or(CertificateError::ArithmeticOverflow)?;
    validate_target_sum(&target)?;
    Ok(target)
}

/// Returns the canonical supply/demand divergence vector.
///
/// # Errors
///
/// Rejects supplies that do not sum to zero.
pub fn supply_divergences(graph: &FlowNetwork) -> Result<Vec<i128>, CertificateError> {
    let target = graph
        .nodes()
        .iter()
        .map(|node| i128::from(node.supply()))
        .collect::<Vec<_>>();
    validate_target_sum(&target)?;
    Ok(target)
}

/// Independently validates a maximum-flow candidate and reconstructs its cut.
///
/// The cut bound is computed only from original edges as
/// `sum upper(S, !S) - sum lower(!S, S)`; auxiliary feasibility edges cannot
/// enter the certificate.
///
/// # Errors
///
/// Rejects bound, conservation, residual-reachability, or cut-equality failure.
pub fn check_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    flows: &[u64],
) -> Result<MaxFlowCertificate, CertificateError> {
    validate_terminals(graph, source, sink)?;
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(CertificateError::UnexpectedSupply);
    }
    let state = checked_state(graph, flows)?;
    let divergence = divergences(graph, flows)?;
    for node in graph.node_indices() {
        if node != source && node != sink && divergence[node.as_usize()] != 0 {
            return Err(conservation_error(
                graph,
                node,
                0,
                divergence[node.as_usize()],
            ));
        }
    }
    let value = divergence[source.as_usize()];
    if divergence[sink.as_usize()]
        != value
            .checked_neg()
            .ok_or(CertificateError::ArithmeticOverflow)?
    {
        return Err(CertificateError::TerminalValueMismatch);
    }

    let reachable = residual_reachable(&state, source);
    if reachable[sink.as_usize()] {
        return Err(CertificateError::SinkReachable);
    }
    let mut cut_bound = 0_i128;
    for edge in graph.edges() {
        let from_source_side = reachable[edge.from().as_usize()];
        let to_source_side = reachable[edge.to().as_usize()];
        if from_source_side && !to_source_side {
            cut_bound = cut_bound
                .checked_add(i128::from(edge.capacity()))
                .ok_or(CertificateError::ArithmeticOverflow)?;
        } else if !from_source_side && to_source_side {
            cut_bound = cut_bound
                .checked_sub(i128::from(edge.lower()))
                .ok_or(CertificateError::ArithmeticOverflow)?;
        }
    }
    if cut_bound != value {
        return Err(CertificateError::CutMismatch);
    }
    let source_side = graph
        .node_indices()
        .filter(|node| reachable[node.as_usize()])
        .filter_map(|node| graph.node(node).map(|item| item.id().clone()))
        .collect();
    Ok(MaxFlowCertificate {
        value,
        cut_bound,
        source_side,
    })
}

/// Independently validates both halves of a minimum-cost maximum-flow result.
///
/// The candidate must first pass the lower-aware maximum-flow/minimum-cut
/// checker. Its certified value is then converted into an exact source/sink
/// divergence, and the same candidate must pass the minimum-cost primal/dual
/// checker for that value.
///
/// # Errors
///
/// Rejects every failure reported by either independent checker, as well as a
/// certified flow value outside the public unsigned terminal-flow domain.
pub fn check_min_cost_max_flow(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    flows: &[u64],
) -> Result<MinCostMaxFlowCertificate, CertificateError> {
    let max_flow = check_max_flow(graph, source, sink, flows)?;
    let value = u64::try_from(max_flow.value).map_err(|_| CertificateError::ArithmeticOverflow)?;
    let target = fixed_flow_divergences(graph, source, sink, value)?;
    let min_cost = check_min_cost_flow(graph, &target, flows)?;
    Ok(MinCostMaxFlowCertificate { max_flow, min_cost })
}

/// Independently validates exact balances, objective, and min-cost optimality.
///
/// Potentials are reconstructed by Bellman–Ford from an implicit super-source
/// with zero-cost arcs to every node. Consequently disconnected negative cycles
/// are rejected too; no solver-maintained potential is trusted.
///
/// # Errors
///
/// Rejects invalid target size/sum, bound or conservation failure, arithmetic
/// overflow, or a negative residual cycle in any component.
pub fn check_min_cost_flow(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    flows: &[u64],
) -> Result<MinCostFlowCertificate, CertificateError> {
    if required_divergence.len() != graph.nodes().len() {
        return Err(CertificateError::FlowVectorLength);
    }
    validate_target_sum(required_divergence)?;
    let state = checked_state(graph, flows)?;
    let actual = divergences(graph, flows)?;
    for node in graph.node_indices() {
        let expected = required_divergence[node.as_usize()];
        if actual[node.as_usize()] != expected {
            return Err(conservation_error(
                graph,
                node,
                expected,
                actual[node.as_usize()],
            ));
        }
    }
    let total_cost = graph
        .edges()
        .iter()
        .zip(flows)
        .try_fold(0_i128, |sum, (edge, &flow)| {
            i128::from(flow)
                .checked_mul(i128::from(edge.cost()))
                .and_then(|term| sum.checked_add(term))
                .ok_or(CertificateError::ArithmeticOverflow)
        })?;
    let potentials = residual_potentials(&state)?;
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            let reduced = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(CertificateError::ArithmeticOverflow)?;
            if reduced < 0 {
                return Err(CertificateError::DualInfeasible);
            }
        }
    }
    Ok(MinCostFlowCertificate {
        total_cost,
        potentials,
    })
}

/// Reconstructs dual potentials if the candidate residual graph has no negative cycle.
///
/// This compatibility check intentionally ignores node-balance feasibility so an
/// SSP implementation can validate its lower-bound pseudoflow before routing
/// remaining imbalance. All residual components are inspected.
///
/// # Errors
///
/// Rejects wrong flow vectors, bound violations, arithmetic overflow, or a
/// negative residual cycle anywhere in the graph.
pub fn check_residual_min_cost_optimality(
    graph: &FlowNetwork,
    flows: &[u64],
) -> Result<Vec<i128>, CertificateError> {
    let state = checked_state(graph, flows)?;
    let potentials = residual_potentials(&state)?;
    for node in graph.node_indices() {
        for arc in state.outgoing_arcs(node) {
            let reduced = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(CertificateError::ArithmeticOverflow)?;
            if reduced < 0 {
                return Err(CertificateError::DualInfeasible);
            }
        }
    }
    Ok(potentials)
}

/// Reconstructs outgoing-minus-incoming flow for every canonical node.
///
/// # Errors
///
/// Rejects wrong vector length, bound violations, or checked overflow.
pub fn divergences(graph: &FlowNetwork, flows: &[u64]) -> Result<Vec<i128>, CertificateError> {
    checked_state(graph, flows)?;
    let mut values = vec![0_i128; graph.nodes().len()];
    for (edge, &flow) in graph.edges().iter().zip(flows) {
        values[edge.from().as_usize()] = values[edge.from().as_usize()]
            .checked_add(i128::from(flow))
            .ok_or(CertificateError::ArithmeticOverflow)?;
        values[edge.to().as_usize()] = values[edge.to().as_usize()]
            .checked_sub(i128::from(flow))
            .ok_or(CertificateError::ArithmeticOverflow)?;
    }
    Ok(values)
}

fn checked_state<'graph>(
    graph: &'graph FlowNetwork,
    flows: &[u64],
) -> Result<ResidualState<'graph>, CertificateError> {
    ResidualState::from_flows(graph, flows).map_err(|error| match error {
        ResidualError::FlowVectorLength => CertificateError::FlowVectorLength,
        ResidualError::FlowBounds => {
            let edge = graph
                .edges()
                .iter()
                .zip(flows)
                .find(|(edge, flow)| **flow < edge.lower() || **flow > edge.capacity())
                .map_or("unknown", |(edge, _)| edge.id().as_str());
            CertificateError::EdgeBounds(edge.to_owned())
        }
        _ => CertificateError::ArithmeticOverflow,
    })
}

fn validate_terminals(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
) -> Result<(), CertificateError> {
    if source == sink || graph.node(source).is_none() || graph.node(sink).is_none() {
        return Err(CertificateError::InvalidTerminals);
    }
    Ok(())
}

fn validate_target_sum(target: &[i128]) -> Result<(), CertificateError> {
    let sum = target.iter().try_fold(0_i128, |sum, value| {
        sum.checked_add(*value)
            .ok_or(CertificateError::ArithmeticOverflow)
    })?;
    if sum != 0 {
        return Err(CertificateError::TerminalValueMismatch);
    }
    Ok(())
}

fn conservation_error(
    graph: &FlowNetwork,
    node: NodeIndex,
    expected: i128,
    actual: i128,
) -> CertificateError {
    CertificateError::Conservation {
        node: graph
            .node(node)
            .map_or("unknown", |item| item.id().as_str())
            .to_owned(),
        expected,
        actual,
    }
}

fn residual_reachable(state: &ResidualState<'_>, source: NodeIndex) -> Vec<bool> {
    let mut reachable = vec![false; state.graph().nodes().len()];
    let mut queue = VecDeque::new();
    reachable[source.as_usize()] = true;
    queue.push_back(source);
    while let Some(node) = queue.pop_front() {
        for arc in state.outgoing_arcs(node) {
            if !reachable[arc.to.as_usize()] {
                reachable[arc.to.as_usize()] = true;
                queue.push_back(arc.to);
            }
        }
    }
    reachable
}

fn residual_potentials(state: &ResidualState<'_>) -> Result<Vec<i128>, CertificateError> {
    let node_count = state.graph().nodes().len();
    let mut distance = vec![0_i128; node_count];
    for round in 0..node_count {
        let mut changed = false;
        for node in state.graph().node_indices() {
            for arc in state.outgoing_arcs(node) {
                let candidate = distance[arc.from.as_usize()]
                    .checked_add(arc.cost)
                    .ok_or(CertificateError::ArithmeticOverflow)?;
                if candidate < distance[arc.to.as_usize()] {
                    distance[arc.to.as_usize()] = candidate;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(distance);
        }
        if round + 1 == node_count {
            return Err(CertificateError::NegativeCycle);
        }
    }
    Ok(distance)
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, UnresolvedFlowEdge};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| {
                    FlowNode::new(NodeId::parse(id).expect("valid node id"), supply)
                })
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("valid edge id"),
                        from: NodeId::parse(from).expect("valid tail"),
                        to: NodeId::parse(to).expect("valid head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("valid graph")
    }

    fn terminals(graph: &FlowNetwork) -> (NodeIndex, NodeIndex) {
        (
            graph
                .node_index(&NodeId::parse("s").expect("valid source"))
                .expect("source exists"),
            graph
                .node_index(&NodeId::parse("t").expect("valid sink"))
                .expect("sink exists"),
        )
    }

    #[test]
    fn max_flow_cut_uses_reverse_lower_bound_and_ignores_self_loop() {
        let graph = network(
            &[("s", 0), ("t", 0)],
            &[
                ("forward", "s", "t", 2, 5, 0),
                ("loop", "s", "s", 0, 9, 0),
                ("reverse", "t", "s", 1, 1, 0),
            ],
        );
        let (source, sink) = terminals(&graph);
        let certificate = check_max_flow(&graph, source, sink, &[5, 0, 1]).expect("optimal");

        assert_eq!(certificate.value, 4);
        assert_eq!(certificate.cut_bound, 4);
        assert_eq!(
            certificate.source_side,
            vec![NodeId::parse("s").expect("id")]
        );
    }

    #[test]
    fn augmenting_path_and_corrupt_conservation_are_rejected() {
        let graph = network(
            &[("a", 0), ("s", 0), ("t", 0)],
            &[("sa", "s", "a", 0, 2, 0), ("at", "a", "t", 0, 2, 0)],
        );
        let (source, sink) = terminals(&graph);

        assert_eq!(
            check_max_flow(&graph, source, sink, &[0, 0]),
            Err(CertificateError::SinkReachable)
        );
        assert!(matches!(
            check_max_flow(&graph, source, sink, &[1, 0]),
            Err(CertificateError::Conservation { .. })
        ));
    }

    #[test]
    fn negative_edge_without_cycle_gets_valid_dual_potential() {
        let graph = network(&[("s", 0), ("t", 0)], &[("edge", "s", "t", 0, 3, -7)]);
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 2).expect("target");
        let certificate = check_min_cost_flow(&graph, &target, &[2]).expect("optimal");

        assert_eq!(certificate.total_cost, -14);
        assert_eq!(certificate.potentials.len(), 2);
    }

    #[test]
    fn disconnected_finite_negative_cycle_is_rejected() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0)],
            &[
                ("st", "s", "t", 0, 1, 1),
                ("negative-loop", "x", "x", 0, 1, -1),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");

        assert_eq!(
            check_min_cost_flow(&graph, &target, &[0, 1]),
            Err(CertificateError::NegativeCycle)
        );
    }

    #[test]
    fn assignment_dual_is_reconstructed_independently_and_rejects_suboptimal_pairs() {
        let graph = network(
            &[("a0", 0), ("a1", 0), ("t0", 0), ("t1", 0), ("t2", 0)],
            &[
                ("e00", "a0", "t0", 0, 1, 1),
                ("e01", "a0", "t1", 0, 1, 10),
                ("e02", "a0", "t2", 0, 1, 5),
                ("e10", "a1", "t0", 0, 1, 10),
                ("e11", "a1", "t1", 0, 1, 1),
                ("e12", "a1", "t2", 0, 1, 5),
            ],
        );
        let model = AssignmentGraph::new(
            &graph,
            &["a0".to_owned(), "a1".to_owned()],
            &["t0".to_owned(), "t1".to_owned(), "t2".to_owned()],
            AssignmentObjectiveV1::Minimize,
        )
        .expect("assignment model");
        let certificate = certify_assignment_optimality(&graph, &model, &[1, 0, 0, 0, 1, 0])
            .expect("exact dual reconstructs");
        assert_eq!(certificate.total_cost, 2);
        assert_eq!(certificate.task_labels[2], 0);
        assert_eq!(
            certify_assignment_optimality(&graph, &model, &[0, 1, 0, 1, 0, 0]),
            Err(CertificateError::AssignmentNotOptimal)
        );
    }
}
