//! Stable residual-arc identities and checked flow mutation.

use thiserror::Error;

use crate::model::{EdgeId, FlowEdge, FlowNetwork, NodeIndex};

/// Direction of a residual arc derived from one original edge.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResidualDirection {
    /// Adds flow to the original edge.
    Forward,
    /// Removes flow from the original edge down to its lower bound.
    Reverse,
}

/// Stable residual identity derived from original edge identity and direction.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResidualArcId {
    original_edge: EdgeId,
    direction: ResidualDirection,
}

impl ResidualArcId {
    /// Creates a stable residual identity.
    #[must_use]
    pub const fn new(original_edge: EdgeId, direction: ResidualDirection) -> Self {
        Self {
            original_edge,
            direction,
        }
    }

    /// Returns the original edge identity.
    #[must_use]
    pub const fn original_edge(&self) -> &EdgeId {
        &self.original_edge
    }

    /// Returns whether this arc adds or removes original flow.
    #[must_use]
    pub const fn direction(&self) -> ResidualDirection {
        self.direction
    }
}

/// Materialized positive-capacity residual arc.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidualArc {
    /// Stable residual identity.
    pub id: ResidualArcId,
    /// Tail node.
    pub from: NodeIndex,
    /// Head node.
    pub to: NodeIndex,
    /// Available augmentation amount.
    pub capacity: u64,
    /// Exact residual unit cost.
    pub cost: i128,
}

/// Mutable flow vector over an immutable canonical original network.
#[derive(Clone, Debug)]
pub struct ResidualState<'graph> {
    graph: &'graph FlowNetwork,
    capacities: Box<[u64]>,
    flows: Box<[u64]>,
}

impl<'graph> ResidualState<'graph> {
    /// Creates a state at every original edge's lower bound.
    #[must_use]
    pub fn at_lower_bounds(graph: &'graph FlowNetwork) -> Self {
        Self {
            graph,
            capacities: graph
                .edges()
                .iter()
                .map(FlowEdge::capacity)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            flows: graph
                .edges()
                .iter()
                .map(FlowEdge::lower)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    /// Creates a state from an externally checked candidate flow vector.
    ///
    /// # Errors
    ///
    /// Rejects wrong length or any value outside its original lower/upper bound.
    pub fn from_flows(graph: &'graph FlowNetwork, flows: &[u64]) -> Result<Self, ResidualError> {
        if flows.len() != graph.edges().len() {
            return Err(ResidualError::FlowVectorLength);
        }
        for (edge, &flow) in graph.edges().iter().zip(flows) {
            if flow < edge.lower() || flow > edge.capacity() {
                return Err(ResidualError::FlowBounds);
            }
        }
        Ok(Self {
            graph,
            capacities: graph
                .edges()
                .iter()
                .map(FlowEdge::capacity)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            flows: flows.to_vec().into_boxed_slice(),
        })
    }

    /// Creates a state with current capacities bounded by the immutable graph.
    ///
    /// The graph capacities act as an envelope for stable edge identities.
    /// A flow may exceed its current capacity only while a dynamic update is
    /// being repaired; it must remain within the immutable envelope and above
    /// the original lower bound.
    ///
    /// # Errors
    ///
    /// Rejects wrong vector lengths, a current capacity outside the original
    /// lower/envelope bounds, or a flow outside the lower/envelope bounds.
    pub fn from_current_capacities_and_flows(
        graph: &'graph FlowNetwork,
        capacities: &[u64],
        flows: &[u64],
    ) -> Result<Self, ResidualError> {
        if capacities.len() != graph.edges().len() {
            return Err(ResidualError::CapacityVectorLength);
        }
        if flows.len() != graph.edges().len() {
            return Err(ResidualError::FlowVectorLength);
        }
        for ((edge, &capacity), &flow) in graph.edges().iter().zip(capacities).zip(flows) {
            if capacity < edge.lower() || capacity > edge.capacity() {
                return Err(ResidualError::CurrentCapacityBounds);
            }
            if flow < edge.lower() || flow > edge.capacity() {
                return Err(ResidualError::FlowBounds);
            }
        }
        Ok(Self {
            graph,
            capacities: capacities.to_vec().into_boxed_slice(),
            flows: flows.to_vec().into_boxed_slice(),
        })
    }

    /// Returns the immutable original graph.
    #[must_use]
    pub const fn graph(&self) -> &'graph FlowNetwork {
        self.graph
    }

    /// Returns original-edge flows in canonical edge-ID order.
    #[must_use]
    pub fn flows(&self) -> &[u64] {
        &self.flows
    }

    /// Returns current capacities in canonical edge-ID order.
    #[must_use]
    pub fn capacities(&self) -> &[u64] {
        &self.capacities
    }

    /// Replaces one current capacity within the immutable graph envelope.
    ///
    /// Capacity decreases are allowed to create a temporary `flow > capacity`
    /// violation so a dynamic solver can expose and repair it explicitly.
    ///
    /// # Errors
    ///
    /// Rejects a missing edge or a value outside its lower/envelope bounds.
    pub fn set_current_capacity(
        &mut self,
        edge_id: &EdgeId,
        capacity: u64,
    ) -> Result<(), ResidualError> {
        let edge_index = self
            .graph
            .edge_index(edge_id)
            .ok_or(ResidualError::MissingArc)?;
        let edge = self
            .graph
            .edge(edge_index)
            .ok_or(ResidualError::MissingArc)?;
        if capacity < edge.lower() || capacity > edge.capacity() {
            return Err(ResidualError::CurrentCapacityBounds);
        }
        *self
            .capacities
            .get_mut(edge_index.as_usize())
            .ok_or(ResidualError::MissingArc)? = capacity;
        Ok(())
    }

    /// Returns the amount by which one flow exceeds its current capacity.
    #[must_use]
    pub fn capacity_violation(&self, edge_id: &EdgeId) -> Option<u64> {
        let edge_index = self.graph.edge_index(edge_id)?;
        let flow = *self.flows.get(edge_index.as_usize())?;
        let capacity = *self.capacities.get(edge_index.as_usize())?;
        Some(flow.saturating_sub(capacity))
    }

    /// Resolves one residual arc, including zero-capacity arcs.
    #[must_use]
    pub fn arc(&self, id: &ResidualArcId) -> Option<ResidualArc> {
        let edge_index = self.graph.edge_index(id.original_edge())?;
        let edge = self.graph.edge(edge_index)?;
        let flow = *self.flows.get(edge_index.as_usize())?;
        let capacity = *self.capacities.get(edge_index.as_usize())?;
        Some(match id.direction() {
            ResidualDirection::Forward => ResidualArc {
                id: id.clone(),
                from: edge.from(),
                to: edge.to(),
                capacity: capacity.saturating_sub(flow),
                cost: i128::from(edge.cost()),
            },
            ResidualDirection::Reverse => ResidualArc {
                id: id.clone(),
                from: edge.to(),
                to: edge.from(),
                capacity: flow - edge.lower(),
                cost: -i128::from(edge.cost()),
            },
        })
    }

    /// Returns positive-capacity residual arcs leaving a node in stable order.
    #[must_use]
    pub fn outgoing_arcs(&self, node: NodeIndex) -> Vec<ResidualArc> {
        let mut ids = Vec::with_capacity(
            self.graph.outgoing_edges(node).len() + self.graph.incoming_edges(node).len(),
        );
        ids.extend(self.graph.outgoing_edges(node).iter().filter_map(|&index| {
            self.graph
                .edge(index)
                .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Forward))
        }));
        ids.extend(self.graph.incoming_edges(node).iter().filter_map(|&index| {
            self.graph
                .edge(index)
                .map(|edge| ResidualArcId::new(edge.id().clone(), ResidualDirection::Reverse))
        }));
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| self.arc(&id))
            .filter(|arc| arc.capacity > 0)
            .collect()
    }

    /// Atomically augments a residual walk by one positive amount.
    ///
    /// # Errors
    ///
    /// Rejects zero augmentation, a missing arc, insufficient residual capacity,
    /// a discontinuous walk, or checked objective overflow without mutation.
    pub fn augment(&mut self, path: &[ResidualArcId], amount: u64) -> Result<i128, ResidualError> {
        if amount == 0 || path.is_empty() {
            return Err(ResidualError::EmptyAugmentation);
        }
        let mut arcs = Vec::with_capacity(path.len());
        let mut previous_to = None;
        let mut unit_cost = 0_i128;
        for id in path {
            let arc = self.arc(id).ok_or(ResidualError::MissingArc)?;
            if arc.capacity < amount {
                return Err(ResidualError::InsufficientCapacity);
            }
            if previous_to.is_some_and(|node| node != arc.from) {
                return Err(ResidualError::DiscontinuousPath);
            }
            previous_to = Some(arc.to);
            unit_cost = unit_cost
                .checked_add(arc.cost)
                .ok_or(ResidualError::CostOverflow)?;
            arcs.push(arc);
        }
        let total_cost = unit_cost
            .checked_mul(i128::from(amount))
            .ok_or(ResidualError::CostOverflow)?;
        for arc in arcs {
            let edge_index = self
                .graph
                .edge_index(arc.id.original_edge())
                .ok_or(ResidualError::MissingArc)?;
            let flow = self
                .flows
                .get_mut(edge_index.as_usize())
                .ok_or(ResidualError::MissingArc)?;
            match arc.id.direction() {
                ResidualDirection::Forward => *flow += amount,
                ResidualDirection::Reverse => *flow -= amount,
            }
        }
        Ok(total_cost)
    }
}

/// Residual-state validation or mutation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ResidualError {
    /// Current-capacity vector length differs from original edge count.
    #[error("current-capacity vector length does not match original edge count")]
    CapacityVectorLength,
    /// Flow vector length differs from original edge count.
    #[error("flow vector length does not match original edge count")]
    FlowVectorLength,
    /// An original flow violates its lower or upper bound.
    #[error("original flow is outside edge bounds")]
    FlowBounds,
    /// A current capacity violates its original lower/envelope bound.
    #[error("current capacity is outside edge envelope bounds")]
    CurrentCapacityBounds,
    /// Empty paths and zero amounts are not augmentations.
    #[error("residual augmentation must have a path and positive amount")]
    EmptyAugmentation,
    /// A residual identity is not present in the graph.
    #[error("residual arc identity is missing")]
    MissingArc,
    /// A residual arc cannot carry the requested amount.
    #[error("residual arc capacity is insufficient")]
    InsufficientCapacity,
    /// Consecutive residual arcs do not form a walk.
    #[error("residual path is discontinuous")]
    DiscontinuousPath,
    /// Exact cost arithmetic exceeded i128.
    #[error("residual cost arithmetic overflow")]
    CostOverflow,
}

#[cfg(test)]
mod tests {
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};

    use super::*;

    fn graph() -> FlowNetwork {
        let nodes = ["s", "a", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("valid node"), 0))
            .collect();
        let edge = |id: &str, from: &str, to: &str, lower, capacity, cost| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("valid edge"),
            from: NodeId::parse(from).expect("valid tail"),
            to: NodeId::parse(to).expect("valid head"),
            lower,
            capacity,
            cost,
        };
        FlowNetwork::new(
            nodes,
            vec![
                edge("e1", "s", "a", 1, 5, i64::MIN),
                edge("e2", "a", "t", 0, 7, 3),
            ],
        )
        .expect("valid graph")
    }

    #[test]
    fn forward_and_reverse_ids_keep_cost_and_lower_bound_exact() {
        let graph = graph();
        let mut state = ResidualState::at_lower_bounds(&graph);
        let forward = ResidualArcId::new(
            EdgeId::parse("e1").expect("valid edge"),
            ResidualDirection::Forward,
        );
        let reverse = ResidualArcId::new(
            EdgeId::parse("e1").expect("valid edge"),
            ResidualDirection::Reverse,
        );

        assert_eq!(state.arc(&forward).expect("arc exists").capacity, 4);
        assert_eq!(state.arc(&reverse).expect("arc exists").capacity, 0);
        state
            .augment(std::slice::from_ref(&forward), 2)
            .expect("augment");
        assert_eq!(state.arc(&reverse).expect("arc exists").capacity, 2);
        assert_eq!(
            state.arc(&reverse).expect("arc exists").cost,
            -i128::from(i64::MIN)
        );
    }

    #[test]
    fn failed_multi_arc_augmentation_is_atomic() {
        let graph = graph();
        let mut state = ResidualState::at_lower_bounds(&graph);
        let before = state.flows().to_vec();
        let path = [
            ResidualArcId::new(
                EdgeId::parse("e1").expect("valid edge"),
                ResidualDirection::Forward,
            ),
            ResidualArcId::new(
                EdgeId::parse("e2").expect("valid edge"),
                ResidualDirection::Forward,
            ),
        ];

        assert_eq!(
            state.augment(&path, 6),
            Err(ResidualError::InsufficientCapacity)
        );
        assert_eq!(state.flows(), before);
    }

    #[test]
    fn self_loop_exposes_both_stable_directions() {
        let node_id = NodeId::parse("v").expect("valid node");
        let loop_id = EdgeId::parse("loop").expect("valid edge");
        let graph = FlowNetwork::new(
            vec![FlowNode::new(node_id.clone(), 0)],
            vec![UnresolvedFlowEdge {
                id: loop_id.clone(),
                from: node_id.clone(),
                to: node_id.clone(),
                lower: 0,
                capacity: 3,
                cost: -1,
            }],
        )
        .expect("valid graph");
        let node = graph.node_index(&node_id).expect("node exists");
        let state = ResidualState::from_flows(&graph, &[1]).expect("bounded flow");
        let arcs = state.outgoing_arcs(node);

        assert_eq!(arcs.len(), 2);
        assert_eq!(
            arcs.iter().map(|arc| arc.id.clone()).collect::<Vec<_>>(),
            vec![
                ResidualArcId::new(loop_id.clone(), ResidualDirection::Forward),
                ResidualArcId::new(loop_id, ResidualDirection::Reverse),
            ]
        );
        assert!(arcs.iter().all(|arc| arc.from == node && arc.to == node));
    }

    #[test]
    fn current_capacity_decrease_exposes_exact_reverse_repair() {
        let graph = graph();
        let edge = EdgeId::parse("e1").expect("valid edge");
        let reverse = ResidualArcId::new(edge.clone(), ResidualDirection::Reverse);
        let forward = ResidualArcId::new(edge.clone(), ResidualDirection::Forward);
        let mut state = ResidualState::from_flows(&graph, &[4, 0]).expect("envelope flow");

        state
            .set_current_capacity(&edge, 2)
            .expect("capacity decrease");
        assert_eq!(state.capacities(), &[2, 7]);
        assert_eq!(state.capacity_violation(&edge), Some(2));
        assert_eq!(state.arc(&forward).expect("forward").capacity, 0);
        assert_eq!(state.arc(&reverse).expect("reverse").capacity, 3);

        state
            .augment(std::slice::from_ref(&reverse), 2)
            .expect("exact reverse repair");
        assert_eq!(state.flows(), &[2, 0]);
        assert_eq!(state.capacity_violation(&edge), Some(0));
        assert_eq!(state.arc(&forward).expect("forward").capacity, 0);
        assert_eq!(state.arc(&reverse).expect("reverse").capacity, 1);
    }

    #[test]
    fn dynamic_state_rejects_values_outside_the_immutable_envelope() {
        let graph = graph();
        assert_eq!(
            ResidualState::from_current_capacities_and_flows(&graph, &[5], &[1, 0])
                .expect_err("capacity length"),
            ResidualError::CapacityVectorLength
        );
        assert_eq!(
            ResidualState::from_current_capacities_and_flows(&graph, &[0, 7], &[1, 0])
                .expect_err("capacity below lower"),
            ResidualError::CurrentCapacityBounds
        );
        assert_eq!(
            ResidualState::from_current_capacities_and_flows(&graph, &[6, 7], &[1, 0])
                .expect_err("capacity above envelope"),
            ResidualError::CurrentCapacityBounds
        );
        assert_eq!(
            ResidualState::from_current_capacities_and_flows(&graph, &[2, 7], &[6, 0])
                .expect_err("flow above envelope"),
            ResidualError::FlowBounds
        );

        let state = ResidualState::from_current_capacities_and_flows(&graph, &[2, 7], &[4, 0])
            .expect("temporary capacity violation");
        assert_eq!(
            state.capacity_violation(&EdgeId::parse("e1").expect("edge")),
            Some(2)
        );
    }
}
