//! Validated native bipartite-matching model and optional unit-flow adapter.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::model::{EdgeIndex, FlowNetwork, NodeId, NodeIndex};

/// One canonical compatibility edge from the left partition to the right partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BipartiteCompatibilityEdge {
    /// Original edge in canonical edge-ID order.
    pub edge: EdgeIndex,
    /// Dense position in [`BipartiteMatchingGraph::left`].
    pub left: usize,
    /// Dense position in [`BipartiteMatchingGraph::right`].
    pub right: usize,
}

/// Strict bipartite model reconstructed from a canonical flow graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BipartiteMatchingGraph {
    /// Left vertices in canonical node-ID order.
    pub left: Vec<NodeIndex>,
    /// Right vertices in canonical node-ID order.
    pub right: Vec<NodeIndex>,
    /// Compatibility edges in canonical edge-ID order.
    pub compatibility_edges: Vec<BipartiteCompatibilityEdge>,
    /// Compatibility-edge ordinals leaving each left vertex.
    pub adjacency: Vec<Vec<usize>>,
    /// Optional explicit source used by the equivalent unit-flow network.
    pub source: Option<NodeIndex>,
    /// Optional explicit sink used by the equivalent unit-flow network.
    pub sink: Option<NodeIndex>,
    /// Source-to-left unit edge for every left vertex when an adapter is present.
    pub source_edges: Vec<EdgeIndex>,
    /// Right-to-sink unit edge for every right vertex when an adapter is present.
    pub sink_edges: Vec<EdgeIndex>,
}

impl BipartiteMatchingGraph {
    /// Validates explicit partitions and an optional `s-L-R-t` unit-flow adapter.
    ///
    /// Compatibility edges must be simple, directed left-to-right, unit-capacity,
    /// zero-lower-bound, and zero-cost. With an adapter, every left vertex has
    /// exactly one source edge and every right vertex exactly one sink edge; no
    /// undeclared node or edge is admitted.
    ///
    /// # Errors
    ///
    /// Rejects missing, duplicate, overlapping, noncanonical partitions, malformed
    /// compatibility arcs, or an incomplete/ambiguous flow adapter.
    pub fn new(
        graph: &FlowNetwork,
        left_ids: &[String],
        right_ids: &[String],
        adapter: Option<(&str, &str)>,
    ) -> Result<Self, BipartiteModelError> {
        let nodes = resolve_model_nodes(graph, left_ids, right_ids, adapter)?;
        let edges = resolve_model_edges(graph, &nodes)?;
        Ok(Self {
            left: nodes.left,
            right: nodes.right,
            compatibility_edges: edges.compatibility,
            adjacency: edges.adjacency,
            source: nodes.source,
            sink: nodes.sink,
            source_edges: edges.source,
            sink_edges: edges.sink,
        })
    }

    /// Materializes a canonical unit-flow vector for a matching.
    ///
    /// # Errors
    ///
    /// Rejects inconsistent left/right pairing arrays or a pair without its
    /// declared compatibility edge.
    pub fn flows_from_pairs(
        &self,
        graph: &FlowNetwork,
        pair_by_left: &[Option<usize>],
        pair_by_right: &[Option<usize>],
    ) -> Result<Vec<u64>, BipartiteModelError> {
        if pair_by_left.len() != self.left.len() || pair_by_right.len() != self.right.len() {
            return Err(BipartiteModelError::InvalidMatching);
        }
        let mut flows = vec![0; graph.edges().len()];
        for (left, pair) in pair_by_left.iter().copied().enumerate() {
            let Some(ordinal) = pair else { continue };
            let compatibility = self
                .compatibility_edges
                .get(ordinal)
                .ok_or(BipartiteModelError::InvalidMatching)?;
            if compatibility.left != left || pair_by_right[compatibility.right] != Some(ordinal) {
                return Err(BipartiteModelError::InvalidMatching);
            }
            flows[compatibility.edge.as_usize()] = 1;
            if let Some(source_edge) = self.source_edges.get(left) {
                flows[source_edge.as_usize()] = 1;
            }
            if let Some(sink_edge) = self.sink_edges.get(compatibility.right) {
                flows[sink_edge.as_usize()] = 1;
            }
        }
        for (right, pair) in pair_by_right.iter().copied().enumerate() {
            if let Some(ordinal) = pair {
                let compatibility = self
                    .compatibility_edges
                    .get(ordinal)
                    .ok_or(BipartiteModelError::InvalidMatching)?;
                if compatibility.right != right || pair_by_left[compatibility.left] != Some(ordinal)
                {
                    return Err(BipartiteModelError::InvalidMatching);
                }
            }
        }
        Ok(flows)
    }
}

struct ResolvedModelNodes {
    left: Vec<NodeIndex>,
    right: Vec<NodeIndex>,
    source: Option<NodeIndex>,
    sink: Option<NodeIndex>,
}

struct ResolvedModelEdges {
    compatibility: Vec<BipartiteCompatibilityEdge>,
    adjacency: Vec<Vec<usize>>,
    source: Vec<EdgeIndex>,
    sink: Vec<EdgeIndex>,
}

fn resolve_model_nodes(
    graph: &FlowNetwork,
    left_ids: &[String],
    right_ids: &[String],
    adapter: Option<(&str, &str)>,
) -> Result<ResolvedModelNodes, BipartiteModelError> {
    if left_ids.is_empty() || right_ids.is_empty() {
        return Err(BipartiteModelError::EmptyPartition);
    }
    validate_canonical_ids(left_ids)?;
    validate_canonical_ids(right_ids)?;
    let left = resolve_partition(graph, left_ids)?;
    let right = resolve_partition(graph, right_ids)?;
    let left_set = left.iter().copied().collect::<BTreeSet<_>>();
    let right_set = right.iter().copied().collect::<BTreeSet<_>>();
    if !left_set.is_disjoint(&right_set) {
        return Err(BipartiteModelError::OverlappingPartitions);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0) {
        return Err(BipartiteModelError::NonzeroSupply);
    }
    let (source, sink) = resolve_adapter(graph, adapter, &left_set, &right_set)?;
    let expected_nodes = left
        .len()
        .checked_add(right.len())
        .and_then(|count| count.checked_add(usize::from(source.is_some()) * 2))
        .ok_or(BipartiteModelError::ModelOverflow)?;
    if graph.nodes().len() != expected_nodes
        || graph.node_indices().any(|node| {
            !left_set.contains(&node)
                && !right_set.contains(&node)
                && Some(node) != source
                && Some(node) != sink
        })
    {
        return Err(BipartiteModelError::UnexpectedNode);
    }
    Ok(ResolvedModelNodes {
        left,
        right,
        source,
        sink,
    })
}

fn resolve_adapter(
    graph: &FlowNetwork,
    adapter: Option<(&str, &str)>,
    left: &BTreeSet<NodeIndex>,
    right: &BTreeSet<NodeIndex>,
) -> Result<(Option<NodeIndex>, Option<NodeIndex>), BipartiteModelError> {
    let Some((source, sink)) = adapter else {
        return Ok((None, None));
    };
    let source = resolve_node(graph, source)?;
    let sink = resolve_node(graph, sink)?;
    if source == sink
        || left.contains(&source)
        || left.contains(&sink)
        || right.contains(&source)
        || right.contains(&sink)
    {
        return Err(BipartiteModelError::InvalidAdapterTerminals);
    }
    Ok((Some(source), Some(sink)))
}

fn resolve_model_edges(
    graph: &FlowNetwork,
    nodes: &ResolvedModelNodes,
) -> Result<ResolvedModelEdges, BipartiteModelError> {
    let left_position = partition_positions(graph.nodes().len(), &nodes.left);
    let right_position = partition_positions(graph.nodes().len(), &nodes.right);
    let mut compatibility = Vec::new();
    let mut adjacency = vec![Vec::new(); nodes.left.len()];
    let mut source_edges = vec![None; nodes.left.len()];
    let mut sink_edges = vec![None; nodes.right.len()];
    let mut pairs = BTreeSet::new();
    for edge_index in graph.edge_indices() {
        let edge = graph
            .edge(edge_index)
            .ok_or(BipartiteModelError::ModelOverflow)?;
        if edge.lower() != 0 || edge.capacity() != 1 || edge.cost() != 0 {
            return Err(BipartiteModelError::NonunitEdge);
        }
        let from_left = left_position[edge.from().as_usize()];
        let to_right = right_position[edge.to().as_usize()];
        if let (Some(left), Some(right)) = (from_left, to_right) {
            if !pairs.insert((left, right)) {
                return Err(BipartiteModelError::DuplicateCompatibility);
            }
            adjacency[left].push(compatibility.len());
            compatibility.push(BipartiteCompatibilityEdge {
                edge: edge_index,
                left,
                right,
            });
        } else if Some(edge.from()) == nodes.source {
            insert_adapter_edge(
                &mut source_edges,
                left_position[edge.to().as_usize()],
                edge_index,
            )?;
        } else if Some(edge.to()) == nodes.sink {
            insert_adapter_edge(
                &mut sink_edges,
                right_position[edge.from().as_usize()],
                edge_index,
            )?;
        } else {
            return Err(BipartiteModelError::UnexpectedEdge);
        }
    }
    let (source, sink) = complete_adapter_edges(nodes.source.is_some(), source_edges, sink_edges)?;
    Ok(ResolvedModelEdges {
        compatibility,
        adjacency,
        source,
        sink,
    })
}

fn partition_positions(node_count: usize, partition: &[NodeIndex]) -> Vec<Option<usize>> {
    let mut result = vec![None; node_count];
    for (position, &node) in partition.iter().enumerate() {
        result[node.as_usize()] = Some(position);
    }
    result
}

fn insert_adapter_edge(
    edges: &mut [Option<EdgeIndex>],
    position: Option<usize>,
    edge: EdgeIndex,
) -> Result<(), BipartiteModelError> {
    let position = position.ok_or(BipartiteModelError::UnexpectedEdge)?;
    if edges[position].replace(edge).is_some() {
        return Err(BipartiteModelError::DuplicateAdapterEdge);
    }
    Ok(())
}

fn complete_adapter_edges(
    has_adapter: bool,
    source: Vec<Option<EdgeIndex>>,
    sink: Vec<Option<EdgeIndex>>,
) -> Result<(Vec<EdgeIndex>, Vec<EdgeIndex>), BipartiteModelError> {
    if !has_adapter {
        if source.iter().any(Option::is_some) || sink.iter().any(Option::is_some) {
            return Err(BipartiteModelError::UnexpectedEdge);
        }
        return Ok((Vec::new(), Vec::new()));
    }
    Ok((
        source
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(BipartiteModelError::MissingAdapterEdge)?,
        sink.into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(BipartiteModelError::MissingAdapterEdge)?,
    ))
}

fn validate_canonical_ids(ids: &[String]) -> Result<(), BipartiteModelError> {
    let mut previous: Option<NodeId> = None;
    for id in ids {
        let parsed = NodeId::parse(id).map_err(|_| BipartiteModelError::InvalidNodeId)?;
        if previous
            .as_ref()
            .is_some_and(|previous| previous >= &parsed)
        {
            return Err(BipartiteModelError::NoncanonicalPartition);
        }
        previous = Some(parsed);
    }
    Ok(())
}

fn resolve_partition(
    graph: &FlowNetwork,
    ids: &[String],
) -> Result<Vec<NodeIndex>, BipartiteModelError> {
    ids.iter().map(|id| resolve_node(graph, id)).collect()
}

fn resolve_node(graph: &FlowNetwork, id: &str) -> Result<NodeIndex, BipartiteModelError> {
    let id = NodeId::parse(id).map_err(|_| BipartiteModelError::InvalidNodeId)?;
    graph
        .node_index(&id)
        .ok_or(BipartiteModelError::MissingNode)
}

/// Strict native matching-model validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BipartiteModelError {
    /// Either partition is empty.
    #[error("bipartite matching partitions must both be non-empty")]
    EmptyPartition,
    /// A partition ID violates the bounded node-ID contract.
    #[error("bipartite matching contains an invalid node id")]
    InvalidNodeId,
    /// Partition IDs are not strictly increasing in canonical node-ID order.
    #[error("bipartite matching partitions must use canonical unique node-id order")]
    NoncanonicalPartition,
    /// A declared partition vertex does not exist.
    #[error("bipartite matching partition references a missing node")]
    MissingNode,
    /// One node belongs to both partitions.
    #[error("bipartite matching partitions overlap")]
    OverlappingPartitions,
    /// Matching vertices carry flow supply/demand semantics.
    #[error("bipartite matching nodes must have zero supply")]
    NonzeroSupply,
    /// Adapter terminals overlap or coincide with matching vertices.
    #[error("bipartite matching flow-adapter terminals are invalid")]
    InvalidAdapterTerminals,
    /// A graph node is outside the declared model.
    #[error("bipartite matching graph contains an undeclared node")]
    UnexpectedNode,
    /// An edge is not a compatibility or declared adapter edge.
    #[error("bipartite matching graph contains an unexpected edge")]
    UnexpectedEdge,
    /// A model edge is not zero-lower, unit-capacity, and zero-cost.
    #[error("bipartite matching edges must have lower 0, capacity 1, and cost 0")]
    NonunitEdge,
    /// Two original edges represent the same left/right compatibility pair.
    #[error("bipartite matching graph contains duplicate compatibility edges")]
    DuplicateCompatibility,
    /// A terminal has more than one adapter edge for one partition vertex.
    #[error("bipartite matching graph contains duplicate flow-adapter edges")]
    DuplicateAdapterEdge,
    /// A declared adapter omits a required source or sink edge.
    #[error("bipartite matching flow adapter is incomplete")]
    MissingAdapterEdge,
    /// Pair arrays do not describe one consistent matching.
    #[error("bipartite matching pair state is inconsistent")]
    InvalidMatching,
    /// A checked model index or cardinality overflowed.
    #[error("bipartite matching model bound overflow")]
    ModelOverflow,
}

#[cfg(test)]
mod tests {
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn edge(
        id: &str,
        from: &str,
        to: &str,
        lower: u64,
        capacity: u64,
        cost: i64,
    ) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge id"),
            from: NodeId::parse(from).expect("tail"),
            to: NodeId::parse(to).expect("head"),
            lower,
            capacity,
            cost,
        }
    }

    fn network(nodes: &[(&str, i64)], edges: Vec<UnresolvedFlowEdge>) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node id"), supply))
                .collect(),
            edges,
        )
        .expect("test network")
    }

    fn partitions() -> (Vec<String>, Vec<String>) {
        (
            vec!["l0".to_owned(), "l1".to_owned()],
            vec!["r0".to_owned(), "r1".to_owned()],
        )
    }

    #[test]
    fn native_contract_accepts_isolated_vertices_and_canonical_edges() {
        let graph = network(
            &[("l0", 0), ("l1", 0), ("r0", 0), ("r1", 0)],
            vec![edge("e0", "l0", "r1", 0, 1, 0)],
        );
        let (left, right) = partitions();
        let model = BipartiteMatchingGraph::new(&graph, &left, &right, None).expect("native model");
        assert_eq!(model.compatibility_edges.len(), 1);
        assert!(model.source.is_none());
        assert!(model.source_edges.is_empty());
    }

    #[test]
    fn partitions_are_explicit_disjoint_and_canonical() {
        let graph = network(
            &[("l0", 0), ("l1", 0), ("r0", 0), ("r1", 0)],
            vec![edge("e0", "l0", "r0", 0, 1, 0)],
        );
        let (_, right) = partitions();
        assert_eq!(
            BipartiteMatchingGraph::new(&graph, &["l1".to_owned(), "l0".to_owned()], &right, None,),
            Err(BipartiteModelError::NoncanonicalPartition)
        );
        assert_eq!(
            BipartiteMatchingGraph::new(&graph, &["l0".to_owned(), "r0".to_owned()], &right, None,),
            Err(BipartiteModelError::OverlappingPartitions)
        );
        assert_eq!(
            BipartiteMatchingGraph::new(
                &graph,
                &["l0".to_owned(), "missing".to_owned()],
                &right,
                None,
            ),
            Err(BipartiteModelError::MissingNode)
        );
    }

    #[test]
    fn compatibility_edges_are_simple_left_to_right_units() {
        let (left, right) = partitions();
        for malformed in [
            edge("e0", "r0", "l0", 0, 1, 0),
            edge("e0", "l0", "r0", 1, 1, 0),
            edge("e0", "l0", "r0", 0, 2, 0),
            edge("e0", "l0", "r0", 0, 1, -1),
        ] {
            let graph = network(
                &[("l0", 0), ("l1", 0), ("r0", 0), ("r1", 0)],
                vec![malformed.clone()],
            );
            assert!(matches!(
                BipartiteMatchingGraph::new(&graph, &left, &right, None),
                Err(BipartiteModelError::UnexpectedEdge | BipartiteModelError::NonunitEdge)
            ));
        }
        let parallel = network(
            &[("l0", 0), ("l1", 0), ("r0", 0), ("r1", 0)],
            vec![
                edge("e0", "l0", "r0", 0, 1, 0),
                edge("e1", "l0", "r0", 0, 1, 0),
            ],
        );
        assert_eq!(
            BipartiteMatchingGraph::new(&parallel, &left, &right, None),
            Err(BipartiteModelError::DuplicateCompatibility)
        );
    }

    #[test]
    fn adapter_requires_exactly_one_unit_terminal_edge_per_vertex() {
        let (left, right) = partitions();
        let complete_edges = vec![
            edge("a0", "s", "l0", 0, 1, 0),
            edge("a1", "s", "l1", 0, 1, 0),
            edge("b0", "l0", "r0", 0, 1, 0),
            edge("c0", "r0", "t", 0, 1, 0),
            edge("c1", "r1", "t", 0, 1, 0),
        ];
        let graph = network(
            &[
                ("l0", 0),
                ("l1", 0),
                ("r0", 0),
                ("r1", 0),
                ("s", 0),
                ("t", 0),
            ],
            complete_edges.clone(),
        );
        BipartiteMatchingGraph::new(&graph, &left, &right, Some(("s", "t")))
            .expect("complete adapter");

        let missing = network(
            &[
                ("l0", 0),
                ("l1", 0),
                ("r0", 0),
                ("r1", 0),
                ("s", 0),
                ("t", 0),
            ],
            complete_edges
                .into_iter()
                .filter(|edge| edge.id.as_str() != "c1")
                .collect(),
        );
        assert_eq!(
            BipartiteMatchingGraph::new(&missing, &left, &right, Some(("s", "t"))),
            Err(BipartiteModelError::MissingAdapterEdge)
        );
    }

    #[test]
    fn undeclared_nodes_and_nonzero_supply_are_rejected() {
        let (left, right) = partitions();
        let extra = network(
            &[("extra", 0), ("l0", 0), ("l1", 0), ("r0", 0), ("r1", 0)],
            vec![edge("e0", "l0", "r0", 0, 1, 0)],
        );
        assert_eq!(
            BipartiteMatchingGraph::new(&extra, &left, &right, None),
            Err(BipartiteModelError::UnexpectedNode)
        );
        let supplied = network(
            &[("l0", 1), ("l1", 0), ("r0", 0), ("r1", 0)],
            vec![edge("e0", "l0", "r0", 0, 1, 0)],
        );
        assert_eq!(
            BipartiteMatchingGraph::new(&supplied, &left, &right, None),
            Err(BipartiteModelError::NonzeroSupply)
        );
    }
}
