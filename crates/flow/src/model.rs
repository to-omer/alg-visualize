//! Validated canonical graph model shared by flow algorithms.

use std::collections::BTreeMap;

use thiserror::Error;

/// Absolute plugin-wide node limit.
pub const MAX_FLOW_NODES: usize = 10_000;
/// Absolute plugin-wide edge limit.
pub const MAX_FLOW_EDGES: usize = 100_000;
/// Maximum number of Unicode scalar values in a user-visible identity.
pub const MAX_FLOW_ID_SCALARS: usize = 64;

/// Stable user-provided node identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(String);

impl NodeId {
    /// Parses and validates a node identity.
    ///
    /// # Errors
    ///
    /// Rejects empty, overlong, or control-character-containing identities.
    pub fn parse(value: &str) -> Result<Self, FlowModelError> {
        validate_identity(value, "node id")?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the original identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable user-provided edge identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EdgeId(String);

impl EdgeId {
    /// Parses and validates an edge identity.
    ///
    /// # Errors
    ///
    /// Rejects empty, overlong, or control-character-containing identities.
    pub fn parse(value: &str) -> Result<Self, FlowModelError> {
        validate_identity(value, "edge id")?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the original identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Dense immutable node index used by algorithm kernels.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeIndex(u32);

impl NodeIndex {
    pub(crate) fn try_from_usize(value: usize) -> Option<Self> {
        u32::try_from(value).ok().map(Self)
    }

    /// Returns the zero-based index.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Dense immutable original-edge index used by algorithm kernels.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EdgeIndex(u32);

impl EdgeIndex {
    /// Returns the zero-based index.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Canonical node with an optional supply or demand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowNode {
    id: NodeId,
    supply: i64,
}

impl FlowNode {
    /// Creates a canonical node.
    #[must_use]
    pub const fn new(id: NodeId, supply: i64) -> Self {
        Self { id, supply }
    }

    /// Returns the stable identity.
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns positive supply or negative demand.
    #[must_use]
    pub const fn supply(&self) -> i64 {
        self.supply
    }
}

/// Canonical directed edge with lower/upper capacity and integral unit cost.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowEdge {
    id: EdgeId,
    from: NodeIndex,
    to: NodeIndex,
    lower: u64,
    capacity: u64,
    cost: i64,
}

impl FlowEdge {
    /// Creates an edge after its endpoint identities have been resolved.
    ///
    /// # Errors
    ///
    /// Rejects a lower bound larger than capacity.
    pub fn new(
        id: EdgeId,
        from: NodeIndex,
        to: NodeIndex,
        lower: u64,
        capacity: u64,
        cost: i64,
    ) -> Result<Self, FlowModelError> {
        if lower > capacity {
            return Err(FlowModelError::LowerExceedsCapacity);
        }
        Ok(Self {
            id,
            from,
            to,
            lower,
            capacity,
            cost,
        })
    }

    /// Returns the stable identity.
    #[must_use]
    pub const fn id(&self) -> &EdgeId {
        &self.id
    }

    /// Returns the tail node.
    #[must_use]
    pub const fn from(&self) -> NodeIndex {
        self.from
    }

    /// Returns the head node.
    #[must_use]
    pub const fn to(&self) -> NodeIndex {
        self.to
    }

    /// Returns the lower flow bound.
    #[must_use]
    pub const fn lower(&self) -> u64 {
        self.lower
    }

    /// Returns the upper capacity bound.
    #[must_use]
    pub const fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Returns the integral unit cost.
    #[must_use]
    pub const fn cost(&self) -> i64 {
        self.cost
    }
}

/// Edge declaration before endpoint identity resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedFlowEdge {
    /// Stable edge identity.
    pub id: EdgeId,
    /// Stable tail-node identity.
    pub from: NodeId,
    /// Stable head-node identity.
    pub to: NodeId,
    /// Lower flow bound.
    pub lower: u64,
    /// Upper capacity bound.
    pub capacity: u64,
    /// Integral unit cost.
    pub cost: i64,
}

/// Validated immutable directed network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowNetwork {
    nodes: Box<[FlowNode]>,
    edges: Box<[FlowEdge]>,
    node_by_id: BTreeMap<NodeId, NodeIndex>,
    edge_by_id: BTreeMap<EdgeId, EdgeIndex>,
    outgoing: Box<[Box<[EdgeIndex]>]>,
    incoming: Box<[Box<[EdgeIndex]>]>,
    capacity_sum: u128,
    max_total_cost_magnitude: i128,
}

impl FlowNetwork {
    /// Resolves declarations into a bounded canonical graph.
    ///
    /// Parallel edges, opposite edges, and self-loops are preserved. Nodes and
    /// edges are sorted by canonical UTF-8 identity before dense indices are
    /// assigned, so declaration order cannot affect algorithm tie-breaking.
    ///
    /// # Errors
    ///
    /// Rejects resource-limit violations, duplicate identities, dangling
    /// endpoints, invalid lower bounds, and aggregate numeric overflow.
    pub fn new(
        mut nodes: Vec<FlowNode>,
        mut unresolved_edges: Vec<UnresolvedFlowEdge>,
    ) -> Result<Self, FlowModelError> {
        if nodes.len() > MAX_FLOW_NODES {
            return Err(FlowModelError::NodeLimit);
        }
        if unresolved_edges.len() > MAX_FLOW_EDGES {
            return Err(FlowModelError::EdgeLimit);
        }
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        unresolved_edges.sort_by(|left, right| left.id.cmp(&right.id));
        let mut node_by_id = BTreeMap::new();
        for (position, node) in nodes.iter().enumerate() {
            let index = NodeIndex(u32::try_from(position).map_err(|_| FlowModelError::NodeLimit)?);
            if node_by_id.insert(node.id.clone(), index).is_some() {
                return Err(FlowModelError::DuplicateNode);
            }
        }

        let mut edge_by_id = BTreeMap::new();
        let mut edges = Vec::with_capacity(unresolved_edges.len());
        let mut capacity_sum = 0_u128;
        let mut worst_cost_magnitude = 0_u128;
        for (position, edge) in unresolved_edges.into_iter().enumerate() {
            let index = EdgeIndex(u32::try_from(position).map_err(|_| FlowModelError::EdgeLimit)?);
            if edge_by_id.insert(edge.id.clone(), index).is_some() {
                return Err(FlowModelError::DuplicateEdge);
            }
            let from = *node_by_id
                .get(&edge.from)
                .ok_or(FlowModelError::DanglingEndpoint)?;
            let to = *node_by_id
                .get(&edge.to)
                .ok_or(FlowModelError::DanglingEndpoint)?;
            capacity_sum = capacity_sum
                .checked_add(u128::from(edge.capacity))
                .ok_or(FlowModelError::AggregateOverflow)?;
            worst_cost_magnitude = worst_cost_magnitude
                .checked_add(
                    u128::from(edge.capacity)
                        .checked_mul(u128::from(edge.cost.unsigned_abs()))
                        .ok_or(FlowModelError::AggregateOverflow)?,
                )
                .ok_or(FlowModelError::AggregateOverflow)?;
            edges.push(FlowEdge::new(
                edge.id,
                from,
                to,
                edge.lower,
                edge.capacity,
                edge.cost,
            )?);
        }

        let max_total_cost_magnitude =
            i128::try_from(worst_cost_magnitude).map_err(|_| FlowModelError::AggregateOverflow)?;
        let mut outgoing = vec![Vec::new(); nodes.len()];
        let mut incoming = vec![Vec::new(); nodes.len()];
        for (position, edge) in edges.iter().enumerate() {
            let index = EdgeIndex(u32::try_from(position).map_err(|_| FlowModelError::EdgeLimit)?);
            outgoing[edge.from.as_usize()].push(index);
            incoming[edge.to.as_usize()].push(index);
        }

        Ok(Self {
            nodes: nodes.into_boxed_slice(),
            edges: edges.into_boxed_slice(),
            node_by_id,
            edge_by_id,
            outgoing: outgoing
                .into_iter()
                .map(Vec::into_boxed_slice)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            incoming: incoming
                .into_iter()
                .map(Vec::into_boxed_slice)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            capacity_sum,
            max_total_cost_magnitude,
        })
    }

    /// Returns all nodes in canonical node-ID order.
    #[must_use]
    pub fn nodes(&self) -> &[FlowNode] {
        &self.nodes
    }

    /// Returns one node by dense canonical index.
    #[must_use]
    pub fn node(&self, index: NodeIndex) -> Option<&FlowNode> {
        self.nodes.get(index.as_usize())
    }

    /// Iterates dense node indices in canonical node-ID order.
    pub fn node_indices(&self) -> impl Iterator<Item = NodeIndex> + '_ {
        (0..self.nodes.len()).filter_map(|position| u32::try_from(position).ok().map(NodeIndex))
    }

    /// Returns all original edges in stable input order.
    #[must_use]
    pub fn edges(&self) -> &[FlowEdge] {
        &self.edges
    }

    /// Iterates dense original-edge indices in canonical edge-ID order.
    pub fn edge_indices(&self) -> impl Iterator<Item = EdgeIndex> + '_ {
        (0..self.edges.len()).filter_map(|position| u32::try_from(position).ok().map(EdgeIndex))
    }

    /// Resolves a node identity.
    #[must_use]
    pub fn node_index(&self, id: &NodeId) -> Option<NodeIndex> {
        self.node_by_id.get(id).copied()
    }

    /// Resolves an edge identity.
    #[must_use]
    pub fn edge_index(&self, id: &EdgeId) -> Option<EdgeIndex> {
        self.edge_by_id.get(id).copied()
    }

    /// Returns one edge by dense canonical index.
    #[must_use]
    pub fn edge(&self, index: EdgeIndex) -> Option<&FlowEdge> {
        self.edges.get(index.as_usize())
    }

    /// Returns original edges leaving a node in canonical edge-ID order.
    #[must_use]
    pub fn outgoing_edges(&self, node: NodeIndex) -> &[EdgeIndex] {
        self.outgoing
            .get(node.as_usize())
            .map_or(&[], AsRef::as_ref)
    }

    /// Returns original edges entering a node in canonical edge-ID order.
    #[must_use]
    pub fn incoming_edges(&self, node: NodeIndex) -> &[EdgeIndex] {
        self.incoming
            .get(node.as_usize())
            .map_or(&[], AsRef::as_ref)
    }

    /// Returns the checked sum of all original edge capacities.
    #[must_use]
    pub const fn capacity_sum(&self) -> u128 {
        self.capacity_sum
    }

    /// Returns a conservative checked magnitude bound for total linear cost.
    #[must_use]
    pub const fn max_total_cost_magnitude(&self) -> i128 {
        self.max_total_cost_magnitude
    }

    /// Checks that supplies and demands sum to zero without overflowing.
    ///
    /// # Errors
    ///
    /// Rejects an unbalanced network.
    pub fn validate_balanced_supplies(&self) -> Result<(), FlowModelError> {
        let balance = self
            .nodes
            .iter()
            .try_fold(0_i128, |sum, node| sum.checked_add(i128::from(node.supply)))
            .ok_or(FlowModelError::AggregateOverflow)?;
        if balance != 0 {
            return Err(FlowModelError::UnbalancedSupply);
        }
        Ok(())
    }

    /// Returns whether every edge has zero lower bound and cost.
    #[must_use]
    pub fn is_plain_max_flow_network(&self) -> bool {
        self.edges
            .iter()
            .all(|edge| edge.lower == 0 && edge.cost == 0)
    }
}

fn validate_identity(value: &str, field: &'static str) -> Result<(), FlowModelError> {
    let mut count = 0_usize;
    for scalar in value.chars() {
        count += 1;
        if scalar.is_control() {
            return Err(FlowModelError::InvalidIdentity(field));
        }
    }
    if count == 0 || count > MAX_FLOW_ID_SCALARS {
        return Err(FlowModelError::InvalidIdentity(field));
    }
    Ok(())
}

/// Canonical graph validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum FlowModelError {
    /// Too many nodes were declared.
    #[error("flow node limit exceeded")]
    NodeLimit,
    /// Too many edges were declared.
    #[error("flow edge limit exceeded")]
    EdgeLimit,
    /// A user-visible identity violates its bounded Unicode contract.
    #[error("invalid {0}")]
    InvalidIdentity(&'static str),
    /// Two nodes use the same identity.
    #[error("duplicate flow node id")]
    DuplicateNode,
    /// Two edges use the same identity.
    #[error("duplicate flow edge id")]
    DuplicateEdge,
    /// An edge names a node that is not present.
    #[error("flow edge has a dangling endpoint")]
    DanglingEndpoint,
    /// An edge lower bound is larger than its capacity.
    #[error("flow edge lower bound exceeds capacity")]
    LowerExceedsCapacity,
    /// A checked aggregate cannot be represented by the engine contract.
    #[error("flow aggregate numeric bound exceeded")]
    AggregateOverflow,
    /// Node supplies and demands do not sum to zero.
    #[error("flow supplies and demands are unbalanced")]
    UnbalancedSupply,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, supply: i64) -> FlowNode {
        FlowNode::new(NodeId::parse(id).expect("fixture node id is valid"), supply)
    }

    fn edge(id: &str, from: &str, to: &str, lower: u64, capacity: u64) -> UnresolvedFlowEdge {
        UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("fixture edge id is valid"),
            from: NodeId::parse(from).expect("fixture tail id is valid"),
            to: NodeId::parse(to).expect("fixture head id is valid"),
            lower,
            capacity,
            cost: 0,
        }
    }

    #[test]
    fn parallel_opposite_and_self_loop_edges_keep_distinct_stable_indices() {
        let graph = FlowNetwork::new(
            vec![node("s", 0), node("t", 0)],
            vec![
                edge("a", "s", "t", 0, 4),
                edge("b", "s", "t", 0, 5),
                edge("c", "t", "s", 0, 6),
                edge("d", "s", "s", 0, 7),
            ],
        )
        .expect("fixture graph is valid");

        assert_eq!(graph.edges().len(), 4);
        assert_eq!(
            graph.edge_index(&EdgeId::parse("d").expect("fixture edge id is valid")),
            Some(EdgeIndex(3))
        );
    }

    #[test]
    fn failed_resolution_does_not_return_a_partial_graph() {
        let result = FlowNetwork::new(
            vec![node("s", 0)],
            vec![edge("dangling", "s", "missing", 0, 1)],
        );

        assert_eq!(result, Err(FlowModelError::DanglingEndpoint));
    }

    #[test]
    fn lower_bound_and_balance_contracts_are_exact() {
        assert_eq!(
            FlowEdge::new(
                EdgeId::parse("e").expect("fixture edge id is valid"),
                NodeIndex(0),
                NodeIndex(1),
                2,
                1,
                -3,
            ),
            Err(FlowModelError::LowerExceedsCapacity)
        );
        let balanced = FlowNetwork::new(vec![node("a", 4), node("b", -4)], vec![])
            .expect("fixture graph is valid");
        assert_eq!(balanced.validate_balanced_supplies(), Ok(()));
        let unbalanced =
            FlowNetwork::new(vec![node("a", 4)], vec![]).expect("fixture graph is valid");
        assert_eq!(
            unbalanced.validate_balanced_supplies(),
            Err(FlowModelError::UnbalancedSupply)
        );
    }

    #[test]
    fn identity_rejects_empty_overlong_and_control_input() {
        assert_eq!(
            NodeId::parse(""),
            Err(FlowModelError::InvalidIdentity("node id"))
        );
        assert_eq!(
            EdgeId::parse("bad\nedge"),
            Err(FlowModelError::InvalidIdentity("edge id"))
        );
        assert_eq!(
            NodeId::parse(&"x".repeat(MAX_FLOW_ID_SCALARS + 1)),
            Err(FlowModelError::InvalidIdentity("node id"))
        );
    }

    #[test]
    fn declaration_order_does_not_change_canonical_indices() {
        let first = FlowNetwork::new(
            vec![node("z", 0), node("a", 0)],
            vec![
                edge("z-edge", "z", "a", 0, 1),
                edge("a-edge", "a", "z", 0, 2),
            ],
        )
        .expect("fixture graph is valid");
        let second = FlowNetwork::new(
            vec![node("a", 0), node("z", 0)],
            vec![
                edge("a-edge", "a", "z", 0, 2),
                edge("z-edge", "z", "a", 0, 1),
            ],
        )
        .expect("fixture graph is valid");

        assert_eq!(first, second);
        assert_eq!(first.nodes()[0].id().as_str(), "a");
        assert_eq!(first.edges()[0].id().as_str(), "a-edge");
    }

    #[test]
    fn capacity_aggregate_uses_full_u128_domain() {
        let graph = FlowNetwork::new(
            vec![node("s", 0), node("t", 0)],
            vec![
                edge("a", "s", "t", 0, u64::MAX),
                edge("b", "s", "t", 0, u64::MAX),
            ],
        )
        .expect("u128 capacity aggregate is supported");

        assert_eq!(graph.capacity_sum(), u128::from(u64::MAX) * 2);
    }
}
