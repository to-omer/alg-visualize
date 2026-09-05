//! Capacity-update preparation for source-scoped Dynamic Excesses IBFS.

use std::collections::BTreeMap;

use thiserror::Error;

use super::{EibfsError, validate_eibfs_graph};
use crate::model::{EdgeId, FlowEdge, FlowModelError, FlowNetwork, NodeIndex, UnresolvedFlowEdge};
pub use crate::scenario::DYNAMIC_EIBFS_MAX_UPDATES;

/// One stable-edge current-capacity replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicCapacityUpdate {
    /// Existing immutable edge identity.
    pub edge: EdgeId,
    /// New current capacity for this prefix.
    pub capacity: u64,
}

impl DynamicCapacityUpdate {
    /// Creates a typed capacity update.
    #[must_use]
    pub const fn new(edge: EdgeId, capacity: u64) -> Self {
        Self { edge, capacity }
    }
}

/// Validated immutable topology plus the sequential current-capacity contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicEibfsProblem {
    envelope: FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    initial_capacities: Vec<u64>,
    updates: Vec<DynamicCapacityUpdate>,
}

impl DynamicEibfsProblem {
    /// Immutable topology whose capacities dominate every update prefix.
    #[must_use]
    pub const fn envelope(&self) -> &FlowNetwork {
        &self.envelope
    }

    /// Source index in the canonical envelope node order.
    #[must_use]
    pub const fn source(&self) -> NodeIndex {
        self.source
    }

    /// Sink index in the canonical envelope node order.
    #[must_use]
    pub const fn sink(&self) -> NodeIndex {
        self.sink
    }

    /// Initial current capacities in canonical edge-ID order.
    #[must_use]
    pub fn initial_capacities(&self) -> &[u64] {
        &self.initial_capacities
    }

    /// Sequential update order; it is semantic and is never sorted.
    #[must_use]
    pub fn updates(&self) -> &[DynamicCapacityUpdate] {
        &self.updates
    }

    /// Materializes one current-capacity graph; prefix zero is the initial graph.
    ///
    /// # Errors
    ///
    /// Rejects a prefix beyond the validated update sequence or an internal
    /// capacity/envelope mismatch.
    pub fn graph_at_prefix(&self, prefix: usize) -> Result<FlowNetwork, DynamicEibfsError> {
        if prefix > self.updates.len() {
            return Err(DynamicEibfsError::PrefixIndex);
        }
        let mut capacities = self.initial_capacities.clone();
        for update in self.updates.iter().take(prefix) {
            let index = self
                .envelope
                .edge_index(&update.edge)
                .ok_or(DynamicEibfsError::MissingEdge)?;
            capacities[index.as_usize()] = update.capacity;
        }
        materialize_current_graph(&self.envelope, &capacities)
    }
}

/// Dynamic EIBFS input preparation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DynamicEibfsError {
    /// At least one update is required to select the dynamic model.
    #[error("Dynamic EIBFS requires a non-empty capacity-update sequence")]
    EmptyUpdateSequence,
    /// The bounded first revision admits at most 256 updates.
    #[error("Dynamic EIBFS update limit exceeded")]
    UpdateLimit,
    /// An update names an edge outside the immutable topology.
    #[error("Dynamic EIBFS update refers to a missing edge")]
    MissingEdge,
    /// A current capacity is below the immutable lower bound.
    #[error("Dynamic EIBFS capacity is below the edge lower bound")]
    CapacityBelowLowerBound,
    /// A materialized prefix capacity vector does not match the envelope.
    #[error("Dynamic EIBFS current-capacity vector is outside the envelope")]
    CurrentCapacityShape,
    /// Requested materialized prefix is outside `0..=updates.len()`.
    #[error("Dynamic EIBFS prefix index is outside the update sequence")]
    PrefixIndex,
    /// Rebuilding the checked envelope graph failed.
    #[error(transparent)]
    Model(#[from] FlowModelError),
    /// Static EIBFS graph requirements are inherited unchanged.
    #[error(transparent)]
    Static(#[from] EibfsError),
}

/// Validates capacity updates and constructs their immutable envelope graph.
///
/// # Errors
///
/// Rejects an empty or overlong update sequence, missing edge identities,
/// values below lower bounds, an invalid envelope, or any static EIBFS graph
/// requirement. Node/edge identities, endpoints, costs, and terminals never
/// change in this revision.
pub fn prepare_dynamic_eibfs(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    updates: &[DynamicCapacityUpdate],
) -> Result<DynamicEibfsProblem, DynamicEibfsError> {
    validate_eibfs_graph(graph, source, sink)?;
    if updates.is_empty() {
        return Err(DynamicEibfsError::EmptyUpdateSequence);
    }
    if updates.len() > DYNAMIC_EIBFS_MAX_UPDATES {
        return Err(DynamicEibfsError::UpdateLimit);
    }

    let mut envelope_capacities = graph
        .edges()
        .iter()
        .map(|edge| (edge.id().clone(), edge.capacity()))
        .collect::<BTreeMap<_, _>>();
    for update in updates {
        let edge_index = graph
            .edge_index(&update.edge)
            .ok_or(DynamicEibfsError::MissingEdge)?;
        let edge = graph
            .edge(edge_index)
            .ok_or(DynamicEibfsError::MissingEdge)?;
        if update.capacity < edge.lower() {
            return Err(DynamicEibfsError::CapacityBelowLowerBound);
        }
        let envelope = envelope_capacities
            .get_mut(&update.edge)
            .ok_or(DynamicEibfsError::MissingEdge)?;
        *envelope = (*envelope).max(update.capacity);
    }

    let source_id = graph
        .node(source)
        .ok_or(EibfsError::GraphRequirement("distinct source and sink"))?
        .id()
        .clone();
    let sink_id = graph
        .node(sink)
        .ok_or(EibfsError::GraphRequirement("distinct source and sink"))?
        .id()
        .clone();
    let unresolved_edges = graph
        .edges()
        .iter()
        .map(|edge| {
            let from = graph
                .node(edge.from())
                .ok_or(FlowModelError::DanglingEndpoint)?
                .id()
                .clone();
            let to = graph
                .node(edge.to())
                .ok_or(FlowModelError::DanglingEndpoint)?
                .id()
                .clone();
            Ok(UnresolvedFlowEdge {
                id: edge.id().clone(),
                from,
                to,
                lower: edge.lower(),
                capacity: *envelope_capacities
                    .get(edge.id())
                    .ok_or(FlowModelError::DanglingEndpoint)?,
                cost: edge.cost(),
            })
        })
        .collect::<Result<Vec<_>, FlowModelError>>()?;
    let envelope = FlowNetwork::new(graph.nodes().to_vec(), unresolved_edges)?;
    let envelope_source = envelope
        .node_index(&source_id)
        .ok_or(FlowModelError::DanglingEndpoint)?;
    let envelope_sink = envelope
        .node_index(&sink_id)
        .ok_or(FlowModelError::DanglingEndpoint)?;
    validate_eibfs_graph(&envelope, envelope_source, envelope_sink)?;

    Ok(DynamicEibfsProblem {
        envelope,
        source: envelope_source,
        sink: envelope_sink,
        initial_capacities: graph.edges().iter().map(FlowEdge::capacity).collect(),
        updates: updates.to_vec(),
    })
}

pub(crate) fn materialize_current_graph(
    envelope: &FlowNetwork,
    capacities: &[u64],
) -> Result<FlowNetwork, DynamicEibfsError> {
    if capacities.len() != envelope.edges().len() {
        return Err(DynamicEibfsError::CurrentCapacityShape);
    }
    let unresolved_edges = envelope
        .edges()
        .iter()
        .zip(capacities)
        .map(|(edge, &capacity)| {
            if capacity < edge.lower() || capacity > edge.capacity() {
                return Err(DynamicEibfsError::CurrentCapacityShape);
            }
            let from = envelope
                .node(edge.from())
                .ok_or(FlowModelError::DanglingEndpoint)?
                .id()
                .clone();
            let to = envelope
                .node(edge.to())
                .ok_or(FlowModelError::DanglingEndpoint)?
                .id()
                .clone();
            Ok(UnresolvedFlowEdge {
                id: edge.id().clone(),
                from,
                to,
                lower: edge.lower(),
                capacity,
                cost: edge.cost(),
            })
        })
        .collect::<Result<Vec<_>, DynamicEibfsError>>()?;
    FlowNetwork::new(envelope.nodes().to_vec(), unresolved_edges).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use crate::model::{FlowNode, NodeId};

    use super::*;

    fn graph() -> (FlowNetwork, NodeIndex, NodeIndex) {
        let nodes = ["s", "a", "t"]
            .into_iter()
            .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
            .collect();
        let edge = |id: &str, from: &str, to: &str, capacity| UnresolvedFlowEdge {
            id: EdgeId::parse(id).expect("edge"),
            from: NodeId::parse(from).expect("from"),
            to: NodeId::parse(to).expect("to"),
            lower: 0,
            capacity,
            cost: 0,
        };
        let graph = FlowNetwork::new(
            nodes,
            vec![edge("sa", "s", "a", 2), edge("at", "a", "t", 3)],
        )
        .expect("graph");
        let source = graph
            .node_index(&NodeId::parse("s").expect("source"))
            .expect("source index");
        let sink = graph
            .node_index(&NodeId::parse("t").expect("sink"))
            .expect("sink index");
        (graph, source, sink)
    }

    #[test]
    fn envelope_preserves_identity_and_dominates_every_prefix() {
        let (graph, source, sink) = graph();
        let updates = vec![
            DynamicCapacityUpdate::new(EdgeId::parse("sa").expect("edge"), 7),
            DynamicCapacityUpdate::new(EdgeId::parse("sa").expect("edge"), 1),
            DynamicCapacityUpdate::new(EdgeId::parse("at").expect("edge"), 5),
        ];
        let prepared =
            prepare_dynamic_eibfs(&graph, source, sink, &updates).expect("prepared problem");

        assert_eq!(prepared.initial_capacities(), &[3, 2]);
        assert_eq!(prepared.updates(), updates);
        assert_eq!(
            prepared
                .envelope()
                .edges()
                .iter()
                .map(|edge| (edge.id().as_str(), edge.capacity()))
                .collect::<Vec<_>>(),
            vec![("at", 5), ("sa", 7)]
        );
        assert_eq!(
            prepared
                .envelope()
                .node(prepared.source())
                .expect("source")
                .id()
                .as_str(),
            "s"
        );
        assert_eq!(
            prepared
                .envelope()
                .node(prepared.sink())
                .expect("sink")
                .id()
                .as_str(),
            "t"
        );
    }

    #[test]
    fn preparation_rejects_empty_missing_and_overlong_updates() {
        let (graph, source, sink) = graph();
        assert_eq!(
            prepare_dynamic_eibfs(&graph, source, sink, &[]),
            Err(DynamicEibfsError::EmptyUpdateSequence)
        );
        assert_eq!(
            prepare_dynamic_eibfs(
                &graph,
                source,
                sink,
                &[DynamicCapacityUpdate::new(
                    EdgeId::parse("missing").expect("edge"),
                    1,
                )],
            ),
            Err(DynamicEibfsError::MissingEdge)
        );
        let too_many = (0..=DYNAMIC_EIBFS_MAX_UPDATES)
            .map(|value| {
                DynamicCapacityUpdate::new(
                    EdgeId::parse("sa").expect("edge"),
                    u64::try_from(value).expect("bounded value"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            prepare_dynamic_eibfs(&graph, source, sink, &too_many),
            Err(DynamicEibfsError::UpdateLimit)
        );
    }
}
