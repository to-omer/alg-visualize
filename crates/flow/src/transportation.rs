//! Strict native balanced transportation model.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::model::{EdgeIndex, FlowNetwork, NodeId, NodeIndex};

/// Conservative interactive node limit for transportation-table methods.
pub const TRANSPORTATION_MAX_NODES: usize = 256;
/// Conservative interactive allowed-route limit.
pub const TRANSPORTATION_MAX_EDGES: usize = 2_048;

/// One allowed origin-to-destination transportation cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportationRoute {
    pub(crate) edge: EdgeIndex,
    pub(crate) origin: usize,
    pub(crate) destination: usize,
    pub(crate) cost: i64,
}

impl TransportationRoute {
    /// Returns the canonical original edge represented by this table cell.
    #[must_use]
    pub const fn edge(&self) -> EdgeIndex {
        self.edge
    }

    /// Returns the dense canonical origin position.
    #[must_use]
    pub const fn origin(&self) -> usize {
        self.origin
    }

    /// Returns the dense canonical destination position.
    #[must_use]
    pub const fn destination(&self) -> usize {
        self.destination
    }

    /// Returns the signed unit transportation cost.
    #[must_use]
    pub const fn cost(&self) -> i64 {
        self.cost
    }
}

/// Validated balanced transportation table reconstructed from a flow graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportationGraph {
    pub(crate) origins: Vec<NodeIndex>,
    pub(crate) destinations: Vec<NodeIndex>,
    pub(crate) routes: Vec<TransportationRoute>,
    pub(crate) total_shipment: u128,
}

impl TransportationGraph {
    /// Validates the complete transportation declaration without applying the
    /// interactive table-method admission limit.
    ///
    /// The Scenario trust boundary uses this linear-space validator before it
    /// may publish an oversized input as a resource-limit result.
    ///
    /// # Errors
    ///
    /// Rejects the same semantic model violations as [`Self::new`].
    pub fn validate_declaration(
        graph: &FlowNetwork,
        origin_ids: &[String],
        destination_ids: &[String],
    ) -> Result<(), TransportationModelError> {
        validate_transportation_declaration(graph, origin_ids, destination_ids).map(|_| ())
    }

    /// Validates an explicit balanced equality transportation model.
    ///
    /// Every graph node belongs to exactly one canonical partition. Origins
    /// have positive supply, destinations have negative supply, and the
    /// two totals are equal. Edges are unique origin-to-destination routes with
    /// lower bound zero. Their capacities must be nonbinding for the table
    /// model: at least `min(origin supply, destination demand)`. Missing pairs
    /// are forbidden routes. No dummy balancing row or column is inserted.
    ///
    /// # Errors
    ///
    /// Rejects malformed partitions, unbalanced totals, route drift, binding
    /// capacities, duplicate cells, or arithmetic overflow.
    pub fn new(
        graph: &FlowNetwork,
        origin_ids: &[String],
        destination_ids: &[String],
    ) -> Result<Self, TransportationModelError> {
        if graph.nodes().len() > TRANSPORTATION_MAX_NODES
            || graph.edges().len() > TRANSPORTATION_MAX_EDGES
        {
            return Err(TransportationModelError::AdmissionLimit);
        }
        let declaration = validate_transportation_declaration(graph, origin_ids, destination_ids)?;
        Ok(Self {
            origins: declaration.origins,
            destinations: declaration.destinations,
            routes: declaration.routes,
            total_shipment: declaration.total_shipment,
        })
    }

    /// Returns origins in canonical node-ID order.
    #[must_use]
    pub fn origins(&self) -> &[NodeIndex] {
        &self.origins
    }

    /// Returns destinations in canonical node-ID order.
    #[must_use]
    pub fn destinations(&self) -> &[NodeIndex] {
        &self.destinations
    }

    /// Returns allowed routes in canonical original-edge order.
    #[must_use]
    pub fn routes(&self) -> &[TransportationRoute] {
        &self.routes
    }

    /// Returns the balanced amount shipped by every feasible solution.
    #[must_use]
    pub const fn total_shipment(&self) -> u128 {
        self.total_shipment
    }

    /// Returns the canonical graph supply vector used as exact divergence.
    #[must_use]
    pub fn required_divergence(&self, graph: &FlowNetwork) -> Vec<i128> {
        graph
            .nodes()
            .iter()
            .map(|node| i128::from(node.supply()))
            .collect()
    }

    pub(crate) fn node_for_origin(&self, origin: usize) -> Option<NodeIndex> {
        self.origins.get(origin).copied()
    }

    pub(crate) fn node_for_destination(&self, destination: usize) -> Option<NodeIndex> {
        self.destinations.get(destination).copied()
    }
}

struct ValidatedTransportationDeclaration {
    origins: Vec<NodeIndex>,
    destinations: Vec<NodeIndex>,
    routes: Vec<TransportationRoute>,
    total_shipment: u128,
}

fn validate_transportation_declaration(
    graph: &FlowNetwork,
    origin_ids: &[String],
    destination_ids: &[String],
) -> Result<ValidatedTransportationDeclaration, TransportationModelError> {
    let (origins, destinations) = resolve_partitions(graph, origin_ids, destination_ids)?;
    let origin_positions = dense_positions(graph.nodes().len(), &origins);
    let destination_positions = dense_positions(graph.nodes().len(), &destinations);
    let origin_supplies = origins
        .iter()
        .map(|&node| {
            let supply = graph
                .node(node)
                .ok_or(TransportationModelError::ModelInvariant)?
                .supply();
            u64::try_from(supply)
                .ok()
                .filter(|&value| value > 0)
                .ok_or(TransportationModelError::OriginSupply)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let destination_demands = destinations
        .iter()
        .map(|&node| {
            let supply = graph
                .node(node)
                .ok_or(TransportationModelError::ModelInvariant)?
                .supply();
            i128::from(supply)
                .checked_neg()
                .and_then(|demand| u64::try_from(demand).ok())
                .filter(|&value| value > 0)
                .ok_or(TransportationModelError::DestinationDemand)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total_supply = origin_supplies.iter().try_fold(0_u128, |sum, &supply| {
        sum.checked_add(u128::from(supply))
            .ok_or(TransportationModelError::ArithmeticOverflow)
    })?;
    let total_demand = destination_demands
        .iter()
        .try_fold(0_u128, |sum, &demand| {
            sum.checked_add(u128::from(demand))
                .ok_or(TransportationModelError::ArithmeticOverflow)
        })?;
    if total_supply != total_demand {
        return Err(TransportationModelError::Unbalanced);
    }

    let mut routes = Vec::with_capacity(graph.edges().len());
    let mut route_pairs = BTreeSet::new();
    for (position, edge) in graph.edges().iter().enumerate() {
        if edge.lower() != 0 {
            return Err(TransportationModelError::NonzeroLowerBound);
        }
        let origin = origin_positions
            .get(edge.from().as_usize())
            .copied()
            .flatten()
            .ok_or(TransportationModelError::UnexpectedRoute)?;
        let destination = destination_positions
            .get(edge.to().as_usize())
            .copied()
            .flatten()
            .ok_or(TransportationModelError::UnexpectedRoute)?;
        if !route_pairs.insert((origin, destination)) {
            return Err(TransportationModelError::DuplicateRoute);
        }
        let nonbinding_capacity = origin_supplies[origin].min(destination_demands[destination]);
        if edge.capacity() < nonbinding_capacity {
            return Err(TransportationModelError::BindingCapacity);
        }
        let edge_index = graph
            .edge_index(edge.id())
            .ok_or(TransportationModelError::ModelInvariant)?;
        if edge_index.as_usize() != position {
            return Err(TransportationModelError::ModelInvariant);
        }
        routes.push(TransportationRoute {
            edge: edge_index,
            origin,
            destination,
            cost: edge.cost(),
        });
    }
    Ok(ValidatedTransportationDeclaration {
        origins,
        destinations,
        routes,
        total_shipment: total_supply,
    })
}

fn resolve_partitions(
    graph: &FlowNetwork,
    origin_ids: &[String],
    destination_ids: &[String],
) -> Result<(Vec<NodeIndex>, Vec<NodeIndex>), TransportationModelError> {
    if origin_ids.is_empty() || destination_ids.is_empty() {
        return Err(TransportationModelError::EmptyPartition);
    }
    validate_canonical_ids(origin_ids)?;
    validate_canonical_ids(destination_ids)?;
    let origins = resolve_partition(graph, origin_ids)?;
    let destinations = resolve_partition(graph, destination_ids)?;
    let origin_set = origins.iter().copied().collect::<BTreeSet<_>>();
    let destination_set = destinations.iter().copied().collect::<BTreeSet<_>>();
    if !origin_set.is_disjoint(&destination_set) {
        return Err(TransportationModelError::OverlappingPartitions);
    }
    if origin_set.len().checked_add(destination_set.len()) != Some(graph.nodes().len()) {
        return Err(TransportationModelError::UnexpectedNode);
    }
    Ok((origins, destinations))
}

fn validate_canonical_ids(ids: &[String]) -> Result<(), TransportationModelError> {
    let mut previous: Option<NodeId> = None;
    for id in ids {
        let parsed = NodeId::parse(id).map_err(|_| TransportationModelError::InvalidNodeId)?;
        if previous.as_ref().is_some_and(|value| value >= &parsed) {
            return Err(TransportationModelError::NoncanonicalPartition);
        }
        previous = Some(parsed);
    }
    Ok(())
}

fn resolve_partition(
    graph: &FlowNetwork,
    ids: &[String],
) -> Result<Vec<NodeIndex>, TransportationModelError> {
    ids.iter()
        .map(|id| {
            let id = NodeId::parse(id).map_err(|_| TransportationModelError::InvalidNodeId)?;
            graph
                .node_index(&id)
                .ok_or(TransportationModelError::MissingNode)
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

/// Native transportation-table validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransportationModelError {
    /// Input exceeds the practical table-method admission band.
    #[error("transportation graph exceeds admission limits")]
    AdmissionLimit,
    /// Both partitions must contain at least one node.
    #[error("transportation partitions must be nonempty")]
    EmptyPartition,
    /// Partition identities must use strictly increasing canonical order.
    #[error("transportation partitions must use canonical node-ID order")]
    NoncanonicalPartition,
    /// A partition entry is not a valid node identity.
    #[error("transportation partition contains an invalid node ID")]
    InvalidNodeId,
    /// A declared partition node is absent from the graph.
    #[error("transportation partition references a missing node")]
    MissingNode,
    /// An origin is also declared as a destination.
    #[error("transportation partitions overlap")]
    OverlappingPartitions,
    /// A graph node is outside both partitions.
    #[error("transportation graph contains an undeclared node")]
    UnexpectedNode,
    /// An origin has nonpositive supply.
    #[error("transportation origins require positive supply")]
    OriginSupply,
    /// A destination has nonnegative supply.
    #[error("transportation destinations require negative supply")]
    DestinationDemand,
    /// Explicit supply and demand totals differ; no dummy node is inserted.
    #[error("transportation supply and demand must balance explicitly")]
    Unbalanced,
    /// A route is not directed from an origin to a destination.
    #[error("transportation graph contains a non origin-to-destination route")]
    UnexpectedRoute,
    /// Transportation-table routes require lower bound zero.
    #[error("transportation routes require lower bound zero")]
    NonzeroLowerBound,
    /// A route capacity would bind a transportation cell.
    #[error("transportation route capacity is binding")]
    BindingCapacity,
    /// The same table cell was declared more than once.
    #[error("transportation graph contains duplicate routes")]
    DuplicateRoute,
    /// Checked total arithmetic overflowed.
    #[error("transportation arithmetic overflow")]
    ArithmeticOverflow,
    /// Canonical graph ordering contradicted its public invariant.
    #[error("transportation canonical model invariant failed")]
    ModelInvariant,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeId, FlowNode, UnresolvedFlowEdge};

    fn graph(capacity: u64) -> FlowNetwork {
        let nodes = [("o0", 4), ("o1", 3), ("d0", -2), ("d1", -5)]
            .into_iter()
            .map(|(id, supply)| FlowNode::new(NodeId::parse(id).expect("node"), supply))
            .collect();
        let edges = [
            ("e00", "o0", "d0", 2),
            ("e01", "o0", "d1", 4),
            ("e10", "o1", "d0", -1),
            ("e11", "o1", "d1", 3),
        ]
        .into_iter()
        .map(|(id, from, to, cost)| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost,
        })
        .collect();
        FlowNetwork::new(nodes, edges).expect("transportation graph")
    }

    #[test]
    fn validates_balanced_sparse_transportation_and_nonbinding_capacities() {
        let network = graph(5);
        let model = TransportationGraph::new(
            &network,
            &["o0".to_owned(), "o1".to_owned()],
            &["d0".to_owned(), "d1".to_owned()],
        )
        .expect("balanced model");
        assert_eq!(model.total_shipment(), 7);
        assert_eq!(model.routes().len(), 4);

        let binding = graph(1);
        assert_eq!(
            TransportationGraph::new(
                &binding,
                &["o0".to_owned(), "o1".to_owned()],
                &["d0".to_owned(), "d1".to_owned()],
            ),
            Err(TransportationModelError::BindingCapacity)
        );
    }
}
