//! Splay-based dynamic forest with stable edge slots and exact path aggregates.
//!
//! Sleator and Tarjan define the rooted-forest operations `link`, `cut`,
//! `evert`, path-cost update, and minimum-cost edge query. This implementation
//! uses the paper's permitted self-adjusting-path representation and represents
//! every edge by its own auxiliary node. Edge identity and value therefore stay
//! attached to the edge when a represented tree is rerooted.

use std::mem;

use num_bigint::BigInt;
use num_traits::Zero;
use thiserror::Error;

/// Stable vertex identity in one [`LinkCutForest`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DynamicTreeVertex(usize);

impl DynamicTreeVertex {
    pub(crate) fn index(self) -> usize {
        self.0
    }
}

/// Stable reusable edge slot in one [`LinkCutForest`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct DynamicTreeEdge(usize);

impl DynamicTreeEdge {
    pub(crate) fn index(self) -> usize {
        self.0
    }
}

/// Exact minimum on a represented-tree path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PathMinimum {
    pub(crate) edge: DynamicTreeEdge,
    pub(crate) value: BigInt,
}

/// Rejected dynamic-forest operation. No represented edge or value is changed.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(crate) enum LinkCutError {
    #[error("dynamic-tree vertex {0} is out of range")]
    InvalidVertex(usize),
    #[error("dynamic-tree edge slot {0} is out of range")]
    InvalidEdge(usize),
    #[error("dynamic-tree edge slot {0} is already linked")]
    EdgeAlreadyLinked(usize),
    #[error("dynamic-tree edge slot {0} is not linked")]
    EdgeNotLinked(usize),
    #[error("dynamic-tree self-loops are not forest edges")]
    SelfLoop,
    #[error("dynamic-tree link would create a represented cycle")]
    Cycle,
    #[error("dynamic-tree rooted link child is not a represented-tree root")]
    ChildNotRoot,
    #[error("dynamic-tree vertices are in different represented trees")]
    Disconnected,
    #[error("dynamic-tree internal endpoint invariant was violated")]
    EndpointInvariant,
}

#[derive(Clone, Debug)]
struct Node {
    children: [Option<usize>; 2],
    parent: Option<usize>,
    reversed: bool,
    lazy_add: BigInt,
    edge_slot: Option<usize>,
    active_edge: bool,
    value: BigInt,
    edge_count: usize,
    path_sum: BigInt,
    minimum: Option<(BigInt, usize)>,
    rootward_minimum: Option<(BigInt, usize)>,
    vertex_weight: usize,
    virtual_vertex_count: usize,
    represented_vertex_count: usize,
}

impl Node {
    fn vertex() -> Self {
        Self {
            children: [None, None],
            parent: None,
            reversed: false,
            lazy_add: BigInt::zero(),
            edge_slot: None,
            active_edge: false,
            value: BigInt::zero(),
            edge_count: 0,
            path_sum: BigInt::zero(),
            minimum: None,
            rootward_minimum: None,
            vertex_weight: 1,
            virtual_vertex_count: 0,
            represented_vertex_count: 1,
        }
    }

    fn edge(edge_slot: usize, value: BigInt, active_edge: bool) -> Self {
        let aggregate = active_edge.then(|| (value.clone(), edge_slot));
        Self {
            children: [None, None],
            parent: None,
            reversed: false,
            lazy_add: BigInt::zero(),
            edge_slot: Some(edge_slot),
            active_edge,
            value,
            edge_count: usize::from(active_edge),
            path_sum: if active_edge {
                aggregate
                    .as_ref()
                    .map_or_else(BigInt::zero, |(value, _)| value.clone())
            } else {
                BigInt::zero()
            },
            minimum: aggregate.clone(),
            rootward_minimum: aggregate,
            vertex_weight: 0,
            virtual_vertex_count: 0,
            represented_vertex_count: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct EdgeRecord {
    endpoints: Option<(usize, usize)>,
    rooted_child: Option<usize>,
}

/// A collection of unrooted represented trees backed by preferred-path splay trees.
///
/// Vertex ordinals and edge-slot ordinals are fixed at construction. A slot may be
/// cut and linked again without changing its identity. All path values and lazy
/// updates use arbitrary-precision signed integers.
#[derive(Clone, Debug)]
pub(crate) struct LinkCutForest {
    vertex_count: usize,
    nodes: Vec<Node>,
    edges: Vec<EdgeRecord>,
}

impl LinkCutForest {
    pub(crate) fn new(vertex_count: usize, edge_slot_count: usize) -> Self {
        let mut nodes = Vec::with_capacity(vertex_count.saturating_add(edge_slot_count));
        nodes.extend((0..vertex_count).map(|_| Node::vertex()));
        nodes.extend((0..edge_slot_count).map(|slot| Node::edge(slot, BigInt::zero(), false)));
        Self {
            vertex_count,
            nodes,
            edges: vec![EdgeRecord::default(); edge_slot_count],
        }
    }

    pub(crate) fn vertex(&self, index: usize) -> Result<DynamicTreeVertex, LinkCutError> {
        (index < self.vertex_count)
            .then_some(DynamicTreeVertex(index))
            .ok_or(LinkCutError::InvalidVertex(index))
    }

    pub(crate) fn edge(&self, index: usize) -> Result<DynamicTreeEdge, LinkCutError> {
        (index < self.edges.len())
            .then_some(DynamicTreeEdge(index))
            .ok_or(LinkCutError::InvalidEdge(index))
    }

    #[cfg(test)]
    pub(crate) fn is_linked(&self, edge: DynamicTreeEdge) -> Result<bool, LinkCutError> {
        self.validate_edge(edge)?;
        Ok(self.edges[edge.index()].endpoints.is_some())
    }

    pub(crate) fn connected(
        &mut self,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
    ) -> Result<bool, LinkCutError> {
        self.validate_vertex(left)?;
        self.validate_vertex(right)?;
        if left == right {
            return Ok(true);
        }
        Ok(self.find_root(left.index()) == self.find_root(right.index()))
    }

    pub(crate) fn link(
        &mut self,
        edge: DynamicTreeEdge,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
        value: BigInt,
    ) -> Result<(), LinkCutError> {
        self.validate_edge(edge)?;
        self.validate_vertex(left)?;
        self.validate_vertex(right)?;
        if self.edges[edge.index()].endpoints.is_some() {
            return Err(LinkCutError::EdgeAlreadyLinked(edge.index()));
        }
        if left == right {
            return Err(LinkCutError::SelfLoop);
        }
        if self.connected(left, right)? {
            return Err(LinkCutError::Cycle);
        }

        let edge_node = self.edge_node(edge);
        self.nodes[edge_node] = Node::edge(edge.index(), value, true);
        self.link_nodes(edge_node, left.index())?;
        self.link_nodes(right.index(), edge_node)?;
        self.edges[edge.index()].endpoints = Some((left.index(), right.index()));
        Ok(())
    }

    pub(crate) fn link_rooted(
        &mut self,
        edge: DynamicTreeEdge,
        child: DynamicTreeVertex,
        parent: DynamicTreeVertex,
        value: BigInt,
    ) -> Result<(), LinkCutError> {
        self.validate_edge(edge)?;
        self.validate_vertex(child)?;
        self.validate_vertex(parent)?;
        if self.edges[edge.index()].endpoints.is_some() {
            return Err(LinkCutError::EdgeAlreadyLinked(edge.index()));
        }
        if child == parent {
            return Err(LinkCutError::SelfLoop);
        }
        if self.find_root(child.index()) != child.index() {
            return Err(LinkCutError::ChildNotRoot);
        }
        if self.find_root(parent.index()) == child.index() {
            return Err(LinkCutError::Cycle);
        }

        let edge_node = self.edge_node(edge);
        self.nodes[edge_node] = Node::edge(edge.index(), value, true);
        self.link_rooted_nodes(child.index(), edge_node);
        self.link_rooted_nodes(edge_node, parent.index());
        self.edges[edge.index()] = EdgeRecord {
            endpoints: Some((child.index(), parent.index())),
            rooted_child: Some(child.index()),
        };
        Ok(())
    }

    pub(crate) fn cut(&mut self, edge: DynamicTreeEdge) -> Result<BigInt, LinkCutError> {
        self.validate_edge(edge)?;
        let (left, right) = self.edges[edge.index()]
            .endpoints
            .ok_or(LinkCutError::EdgeNotLinked(edge.index()))?;
        let edge_node = self.edge_node(edge);
        let value = self.edge_value(edge)?;
        self.cut_nodes(edge_node, left)?;
        self.cut_nodes(edge_node, right)?;
        self.nodes[edge_node] = Node::edge(edge.index(), value.clone(), false);
        self.edges[edge.index()] = EdgeRecord::default();
        Ok(value)
    }

    pub(crate) fn cut_rooted(&mut self, edge: DynamicTreeEdge) -> Result<BigInt, LinkCutError> {
        self.validate_edge(edge)?;
        let child = self.edges[edge.index()]
            .rooted_child
            .ok_or(LinkCutError::EdgeNotLinked(edge.index()))?;
        let edge_node = self.edge_node(edge);
        let value = self.edge_value(edge)?;
        self.cut_parent_node(child)?;
        self.cut_parent_node(edge_node)?;
        self.nodes[edge_node] = Node::edge(edge.index(), value.clone(), false);
        self.edges[edge.index()] = EdgeRecord::default();
        Ok(value)
    }

    #[cfg(test)]
    pub(crate) fn reroot(&mut self, vertex: DynamicTreeVertex) -> Result<(), LinkCutError> {
        self.validate_vertex(vertex)?;
        self.make_root(vertex.index());
        Ok(())
    }

    pub(crate) fn represented_root(
        &mut self,
        vertex: DynamicTreeVertex,
    ) -> Result<DynamicTreeVertex, LinkCutError> {
        self.validate_vertex(vertex)?;
        let root = self.find_root(vertex.index());
        if root >= self.vertex_count {
            return Err(LinkCutError::EndpointInvariant);
        }
        Ok(DynamicTreeVertex(root))
    }

    pub(crate) fn edge_value(&mut self, edge: DynamicTreeEdge) -> Result<BigInt, LinkCutError> {
        self.validate_edge(edge)?;
        if self.edges[edge.index()].endpoints.is_none() {
            return Err(LinkCutError::EdgeNotLinked(edge.index()));
        }
        let node = self.edge_node(edge);
        self.access(node);
        Ok(self.nodes[node].value.clone())
    }

    /// Replaces one linked edge value while preserving its stable slot.
    #[cfg(test)]
    pub(crate) fn set_edge_value(
        &mut self,
        edge: DynamicTreeEdge,
        value: BigInt,
    ) -> Result<(), LinkCutError> {
        self.validate_edge(edge)?;
        if self.edges[edge.index()].endpoints.is_none() {
            return Err(LinkCutError::EdgeNotLinked(edge.index()));
        }
        let node = self.edge_node(edge);
        self.access(node);
        self.nodes[node].value = value;
        self.pull(node);
        Ok(())
    }

    /// Returns the exact sum of edge values on one represented-tree path.
    pub(crate) fn path_sum(
        &mut self,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
    ) -> Result<BigInt, LinkCutError> {
        let root = self.expose_path(left, right)?;
        Ok(self.nodes[root].path_sum.clone())
    }

    pub(crate) fn path_minimum(
        &mut self,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
    ) -> Result<Option<PathMinimum>, LinkCutError> {
        let root = self.expose_path(left, right)?;
        Ok(self.nodes[root]
            .minimum
            .as_ref()
            .map(|(value, edge)| PathMinimum {
                edge: DynamicTreeEdge(*edge),
                value: value.clone(),
            }))
    }

    pub(crate) fn path_add(
        &mut self,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
        delta: &BigInt,
    ) -> Result<(), LinkCutError> {
        let root = self.expose_path(left, right)?;
        self.apply_add(root, delta);
        Ok(())
    }

    pub(crate) fn root_path_minimum(
        &mut self,
        vertex: DynamicTreeVertex,
    ) -> Result<Option<PathMinimum>, LinkCutError> {
        self.validate_vertex(vertex)?;
        self.access(vertex.index());
        Ok(self.nodes[vertex.index()]
            .minimum
            .as_ref()
            .map(|(value, edge)| PathMinimum {
                edge: DynamicTreeEdge(*edge),
                value: value.clone(),
            }))
    }

    /// Returns the exact sum from the represented root to one vertex.
    pub(crate) fn root_path_sum(
        &mut self,
        vertex: DynamicTreeVertex,
    ) -> Result<BigInt, LinkCutError> {
        self.validate_vertex(vertex)?;
        self.access(vertex.index());
        Ok(self.nodes[vertex.index()].path_sum.clone())
    }

    /// Returns the minimum-valued root-path edge, breaking ties toward the
    /// represented root as required by Goldberg--Tarjan `find-min`.
    pub(crate) fn root_path_minimum_closest_to_root(
        &mut self,
        vertex: DynamicTreeVertex,
    ) -> Result<Option<PathMinimum>, LinkCutError> {
        self.validate_vertex(vertex)?;
        self.access(vertex.index());
        Ok(self.nodes[vertex.index()]
            .rootward_minimum
            .as_ref()
            .map(|(value, edge)| PathMinimum {
                edge: DynamicTreeEdge(*edge),
                value: value.clone(),
            }))
    }

    /// Returns the number of original vertices in the represented tree.
    pub(crate) fn represented_vertex_count(
        &mut self,
        vertex: DynamicTreeVertex,
    ) -> Result<usize, LinkCutError> {
        self.validate_vertex(vertex)?;
        self.access(vertex.index());
        Ok(self.nodes[vertex.index()].represented_vertex_count)
    }

    pub(crate) fn root_path_add(
        &mut self,
        vertex: DynamicTreeVertex,
        delta: &BigInt,
    ) -> Result<(), LinkCutError> {
        self.validate_vertex(vertex)?;
        self.access(vertex.index());
        self.apply_add(vertex.index(), delta);
        Ok(())
    }

    fn validate_vertex(&self, vertex: DynamicTreeVertex) -> Result<(), LinkCutError> {
        if vertex.index() < self.vertex_count {
            Ok(())
        } else {
            Err(LinkCutError::InvalidVertex(vertex.index()))
        }
    }

    fn validate_edge(&self, edge: DynamicTreeEdge) -> Result<(), LinkCutError> {
        if edge.index() < self.edges.len() {
            Ok(())
        } else {
            Err(LinkCutError::InvalidEdge(edge.index()))
        }
    }

    fn edge_node(&self, edge: DynamicTreeEdge) -> usize {
        self.vertex_count + edge.index()
    }

    fn expose_path(
        &mut self,
        left: DynamicTreeVertex,
        right: DynamicTreeVertex,
    ) -> Result<usize, LinkCutError> {
        self.validate_vertex(left)?;
        self.validate_vertex(right)?;
        if !self.connected(left, right)? {
            return Err(LinkCutError::Disconnected);
        }
        self.make_root(left.index());
        self.access(right.index());
        Ok(right.index())
    }

    fn link_nodes(&mut self, child: usize, parent: usize) -> Result<(), LinkCutError> {
        self.make_root(child);
        if self.find_root(parent) == child {
            return Err(LinkCutError::Cycle);
        }
        self.access(parent);
        self.access(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].virtual_vertex_count += self.nodes[child].represented_vertex_count;
        self.pull(parent);
        Ok(())
    }

    fn link_rooted_nodes(&mut self, child: usize, parent: usize) {
        self.access(child);
        self.access(parent);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].virtual_vertex_count += self.nodes[child].represented_vertex_count;
        self.pull(parent);
    }

    fn cut_nodes(&mut self, left: usize, right: usize) -> Result<(), LinkCutError> {
        self.make_root(left);
        self.access(right);
        self.push(right);
        self.push(left);
        if self.nodes[right].children[0] != Some(left) || self.nodes[left].children[1].is_some() {
            return Err(LinkCutError::EndpointInvariant);
        }
        self.nodes[right].children[0] = None;
        self.nodes[left].parent = None;
        self.pull(right);
        Ok(())
    }

    fn cut_parent_node(&mut self, node: usize) -> Result<(), LinkCutError> {
        self.access(node);
        let ancestors = self.nodes[node].children[0]
            .take()
            .ok_or(LinkCutError::EndpointInvariant)?;
        self.nodes[ancestors].parent = None;
        self.pull(node);
        Ok(())
    }

    fn make_root(&mut self, node: usize) {
        self.access(node);
        self.apply_reverse(node);
    }

    fn find_root(&mut self, node: usize) -> usize {
        self.access(node);
        let mut current = node;
        self.push(current);
        while let Some(left) = self.nodes[current].children[0] {
            current = left;
            self.push(current);
        }
        self.splay(current);
        current
    }

    fn access(&mut self, node: usize) {
        let mut last: Option<usize> = None;
        let mut current = Some(node);
        while let Some(value) = current {
            self.splay(value);
            let path_parent = self.nodes[value].parent;
            if let Some(previous_preferred) = self.nodes[value].children[1] {
                self.nodes[value].virtual_vertex_count +=
                    self.nodes[previous_preferred].represented_vertex_count;
            }
            if let Some(next_preferred) = last {
                self.nodes[value].virtual_vertex_count -=
                    self.nodes[next_preferred].represented_vertex_count;
            }
            self.nodes[value].children[1] = last;
            if let Some(last_node) = last {
                self.nodes[last_node].parent = Some(value);
            }
            self.pull(value);
            last = Some(value);
            current = path_parent;
        }
        self.splay(node);
    }

    fn splay(&mut self, node: usize) {
        let mut ancestors = vec![node];
        let mut current = node;
        while !self.is_auxiliary_root(current) {
            let parent = self.nodes[current]
                .parent
                .expect("a non-root auxiliary node has a parent");
            ancestors.push(parent);
            current = parent;
        }
        for ancestor in ancestors.into_iter().rev() {
            self.push(ancestor);
        }

        while !self.is_auxiliary_root(node) {
            let parent = self.nodes[node]
                .parent
                .expect("a non-root auxiliary node has a parent");
            if !self.is_auxiliary_root(parent) {
                let grandparent = self.nodes[parent]
                    .parent
                    .expect("a non-root auxiliary parent has a parent");
                let node_is_right = self.nodes[parent].children[1] == Some(node);
                let parent_is_right = self.nodes[grandparent].children[1] == Some(parent);
                if node_is_right == parent_is_right {
                    self.rotate(parent);
                } else {
                    self.rotate(node);
                }
            }
            self.rotate(node);
        }
    }

    fn rotate(&mut self, node: usize) {
        let parent = self.nodes[node]
            .parent
            .expect("an auxiliary rotation node has a parent");
        let grandparent = self.nodes[parent].parent;
        let direction = usize::from(self.nodes[parent].children[1] == Some(node));
        let opposite = direction ^ 1;
        let middle = self.nodes[node].children[opposite];

        if !self.is_auxiliary_root(parent) {
            let grandparent = grandparent.expect("non-root auxiliary parent has a parent");
            let parent_direction = usize::from(self.nodes[grandparent].children[1] == Some(parent));
            self.nodes[grandparent].children[parent_direction] = Some(node);
        }
        self.nodes[node].parent = grandparent;
        self.nodes[node].children[opposite] = Some(parent);
        self.nodes[parent].parent = Some(node);
        self.nodes[parent].children[direction] = middle;
        if let Some(middle) = middle {
            self.nodes[middle].parent = Some(parent);
        }
        self.pull(parent);
        self.pull(node);
    }

    fn is_auxiliary_root(&self, node: usize) -> bool {
        self.nodes[node].parent.is_none_or(|parent| {
            self.nodes[parent].children[0] != Some(node)
                && self.nodes[parent].children[1] != Some(node)
        })
    }

    fn apply_reverse(&mut self, node: usize) {
        self.nodes[node].children.swap(0, 1);
        self.nodes[node].reversed = !self.nodes[node].reversed;
    }

    fn apply_add(&mut self, node: usize, delta: &BigInt) {
        if self.nodes[node].edge_count == 0 || delta.is_zero() {
            return;
        }
        if self.nodes[node].active_edge {
            self.nodes[node].value += delta;
        }
        let edge_count = self.nodes[node].edge_count;
        self.nodes[node].path_sum += delta * BigInt::from(edge_count);
        if let Some((minimum, _)) = &mut self.nodes[node].minimum {
            *minimum += delta;
        }
        if let Some((minimum, _)) = &mut self.nodes[node].rootward_minimum {
            *minimum += delta;
        }
        self.nodes[node].lazy_add += delta;
    }

    fn push(&mut self, node: usize) {
        if self.nodes[node].reversed {
            let children = self.nodes[node].children;
            for child in children.into_iter().flatten() {
                self.apply_reverse(child);
            }
            self.nodes[node].reversed = false;
        }
        let delta = mem::take(&mut self.nodes[node].lazy_add);
        if !delta.is_zero() {
            let children = self.nodes[node].children;
            for child in children.into_iter().flatten() {
                self.apply_add(child, &delta);
            }
        }
    }

    fn pull(&mut self, node: usize) {
        let children = self.nodes[node].children;
        let left = children[0].map(|child| {
            (
                self.nodes[child].edge_count,
                self.nodes[child].path_sum.clone(),
                self.nodes[child].minimum.clone(),
                self.nodes[child].rootward_minimum.clone(),
                self.nodes[child].represented_vertex_count,
            )
        });
        let right = children[1].map(|child| {
            (
                self.nodes[child].edge_count,
                self.nodes[child].path_sum.clone(),
                self.nodes[child].minimum.clone(),
                self.nodes[child].rootward_minimum.clone(),
                self.nodes[child].represented_vertex_count,
            )
        });
        let own_minimum = self.nodes[node].active_edge.then(|| {
            (
                self.nodes[node].value.clone(),
                self.nodes[node].edge_slot.unwrap(),
            )
        });
        self.nodes[node].edge_count = usize::from(self.nodes[node].active_edge)
            + left.as_ref().map_or(0, |summary| summary.0)
            + right.as_ref().map_or(0, |summary| summary.0);
        let own_value = if self.nodes[node].active_edge {
            self.nodes[node].value.clone()
        } else {
            BigInt::zero()
        };
        self.nodes[node].path_sum = left
            .as_ref()
            .map_or_else(BigInt::zero, |summary| summary.1.clone())
            + own_value
            + right
                .as_ref()
                .map_or_else(BigInt::zero, |summary| summary.1.clone());
        self.nodes[node].minimum = [
            left.as_ref().and_then(|summary| summary.2.clone()),
            own_minimum,
            right.as_ref().and_then(|summary| summary.2.clone()),
        ]
        .into_iter()
        .flatten()
        .min_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        self.nodes[node].rootward_minimum = [
            left.as_ref().and_then(|summary| summary.3.clone()),
            self.nodes[node].active_edge.then(|| {
                (
                    self.nodes[node].value.clone(),
                    self.nodes[node].edge_slot.unwrap(),
                )
            }),
            right.as_ref().and_then(|summary| summary.3.clone()),
        ]
        .into_iter()
        .flatten()
        .reduce(|best, candidate| {
            if candidate.0 < best.0 {
                candidate
            } else {
                best
            }
        });
        self.nodes[node].represented_vertex_count = self.nodes[node].vertex_weight
            + self.nodes[node].virtual_vertex_count
            + left.as_ref().map_or(0, |summary| summary.4)
            + right.as_ref().map_or(0, |summary| summary.4);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[derive(Clone, Debug)]
    struct ModelEdge {
        endpoints: Option<(usize, usize)>,
        value: BigInt,
    }

    fn model_path(
        vertex_count: usize,
        edges: &[ModelEdge],
        source: usize,
        sink: usize,
    ) -> Option<Vec<usize>> {
        let mut previous = vec![None; vertex_count];
        let mut queue = VecDeque::from([source]);
        previous[source] = Some((source, usize::MAX));
        while let Some(node) = queue.pop_front() {
            if node == sink {
                break;
            }
            for (edge, record) in edges.iter().enumerate() {
                let Some((left, right)) = record.endpoints else {
                    continue;
                };
                let next = if left == node {
                    right
                } else if right == node {
                    left
                } else {
                    continue;
                };
                if previous[next].is_none() {
                    previous[next] = Some((node, edge));
                    queue.push_back(next);
                }
            }
        }
        previous[sink]?;
        let mut path = Vec::new();
        let mut node = sink;
        while node != source {
            let (parent, edge) = previous[node]?;
            path.push(edge);
            node = parent;
        }
        Some(path)
    }

    fn model_minimum(edges: &[ModelEdge], path: &[usize]) -> Option<PathMinimum> {
        path.iter()
            .map(|edge| (*edge, edges[*edge].value.clone()))
            .min_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)))
            .map(|(edge, value)| PathMinimum {
                edge: DynamicTreeEdge(edge),
                value,
            })
    }

    fn bounded(draw: u64, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("test bound fits u64");
        usize::try_from(draw % bound).expect("reduced test draw fits usize")
    }

    #[test]
    fn exact_path_operations_keep_values_attached_to_edges_across_reroots() {
        let mut forest = LinkCutForest::new(4, 4);
        let vertices = (0..4)
            .map(|index| forest.vertex(index).expect("vertex exists"))
            .collect::<Vec<_>>();
        let edges = (0..4)
            .map(|index| forest.edge(index).expect("edge exists"))
            .collect::<Vec<_>>();

        forest
            .link(edges[2], vertices[0], vertices[1], BigInt::from(6))
            .expect("first forest edge links");
        forest
            .link(edges[0], vertices[1], vertices[2], BigInt::from(4))
            .expect("second forest edge links");
        forest
            .link(edges[3], vertices[2], vertices[3], BigInt::from(4))
            .expect("third forest edge links");
        assert_eq!(
            forest.path_minimum(vertices[0], vertices[3]),
            Ok(Some(PathMinimum {
                edge: edges[0],
                value: BigInt::from(4),
            }))
        );

        forest
            .path_add(vertices[1], vertices[3], &BigInt::from(-7))
            .expect("exact negative path update succeeds");
        assert_eq!(forest.edge_value(edges[2]), Ok(BigInt::from(6)));
        assert_eq!(forest.edge_value(edges[0]), Ok(BigInt::from(-3)));
        assert_eq!(forest.edge_value(edges[3]), Ok(BigInt::from(-3)));
        assert_eq!(
            forest.path_sum(vertices[0], vertices[3]),
            Ok(BigInt::zero())
        );
        forest
            .set_edge_value(edges[0], BigInt::from(11))
            .expect("stable edge replacement succeeds");
        assert_eq!(
            forest.path_sum(vertices[0], vertices[3]),
            Ok(BigInt::from(14))
        );

        forest.reroot(vertices[3]).expect("reroot succeeds");
        forest.reroot(vertices[0]).expect("reroot succeeds again");
        assert_eq!(forest.edge_value(edges[0]), Ok(BigInt::from(11)));
        assert_eq!(forest.edge_value(edges[3]), Ok(BigInt::from(-3)));
        assert_eq!(
            forest.link(edges[1], vertices[0], vertices[3], BigInt::from(1)),
            Err(LinkCutError::Cycle)
        );
        assert_eq!(forest.is_linked(edges[1]), Ok(false));

        assert_eq!(forest.cut(edges[0]), Ok(BigInt::from(11)));
        assert_eq!(forest.connected(vertices[0], vertices[3]), Ok(false));
        assert_eq!(
            forest.path_minimum(vertices[0], vertices[3]),
            Err(LinkCutError::Disconnected)
        );
        assert_eq!(forest.cut(edges[0]), Err(LinkCutError::EdgeNotLinked(0)));
    }

    #[test]
    fn rooted_paths_follow_paper_link_update_mincost_and_cut_contract() {
        let mut forest = LinkCutForest::new(4, 3);
        let vertices = (0..4)
            .map(|index| forest.vertex(index).expect("vertex exists"))
            .collect::<Vec<_>>();
        let edges = (0..3)
            .map(|index| forest.edge(index).expect("edge exists"))
            .collect::<Vec<_>>();
        let huge: BigInt = BigInt::from(1_u8) << 300_usize;

        forest
            .link_rooted(edges[2], vertices[0], vertices[1], huge.clone() + 9)
            .expect("root child links to its parent");
        forest
            .link_rooted(edges[0], vertices[1], vertices[2], huge.clone() + 4)
            .expect("extended rooted path links");
        forest
            .link_rooted(edges[1], vertices[2], vertices[3], huge.clone() + 4)
            .expect("rooted path reaches final root");
        assert_eq!(forest.represented_root(vertices[0]), Ok(vertices[3]));
        assert_eq!(forest.represented_vertex_count(vertices[0]), Ok(4));
        assert_eq!(forest.root_path_sum(vertices[0]), Ok(huge.clone() * 3 + 17));
        assert_eq!(
            forest.root_path_minimum(vertices[0]),
            Ok(Some(PathMinimum {
                edge: edges[0],
                value: huge.clone() + 4,
            }))
        );
        assert_eq!(
            forest.root_path_minimum_closest_to_root(vertices[0]),
            Ok(Some(PathMinimum {
                edge: edges[1],
                value: huge.clone() + 4,
            }))
        );

        forest
            .root_path_add(vertices[0], &(-huge.clone() - 4))
            .expect("arbitrary-precision root-path update succeeds");
        assert_eq!(forest.edge_value(edges[0]), Ok(BigInt::zero()));
        assert_eq!(forest.edge_value(edges[1]), Ok(BigInt::zero()));
        assert_eq!(forest.edge_value(edges[2]), Ok(BigInt::from(5)));
        assert_eq!(forest.root_path_sum(vertices[0]), Ok(BigInt::from(5)));
        assert_eq!(forest.cut_rooted(edges[0]), Ok(BigInt::zero()));
        assert_eq!(forest.represented_root(vertices[0]), Ok(vertices[1]));
        assert_eq!(forest.represented_root(vertices[2]), Ok(vertices[3]));
        assert_eq!(forest.represented_vertex_count(vertices[0]), Ok(2));
        assert_eq!(forest.represented_vertex_count(vertices[3]), Ok(2));
        assert_eq!(
            forest.link_rooted(edges[0], vertices[0], vertices[3], BigInt::from(1)),
            Err(LinkCutError::ChildNotRoot)
        );
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one deterministic sequence keeps mutation and full oracle comparison adjacent"
    )]
    fn model_based_sequences_match_an_independent_adjacency_forest() {
        const VERTICES: usize = 8;
        const EDGES: usize = 12;
        let mut forest = LinkCutForest::new(VERTICES, EDGES);
        let vertices = (0..VERTICES)
            .map(|index| forest.vertex(index).expect("vertex exists"))
            .collect::<Vec<_>>();
        let edge_handles = (0..EDGES)
            .map(|index| forest.edge(index).expect("edge exists"))
            .collect::<Vec<_>>();
        let mut model = (0..EDGES)
            .map(|_| ModelEdge {
                endpoints: None,
                value: BigInt::zero(),
            })
            .collect::<Vec<_>>();
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut draw = || {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state
        };

        for _ in 0..2_000 {
            match draw() % 5 {
                0 | 1 => {
                    let edge = bounded(draw(), EDGES);
                    let left = bounded(draw(), VERTICES);
                    let right = bounded(draw(), VERTICES);
                    if model[edge].endpoints.is_none()
                        && left != right
                        && model_path(VERTICES, &model, left, right).is_none()
                    {
                        let value = BigInt::from(i64::try_from(draw() % 101).unwrap() - 50);
                        forest
                            .link(
                                edge_handles[edge],
                                vertices[left],
                                vertices[right],
                                value.clone(),
                            )
                            .expect("model-accepted link succeeds");
                        model[edge] = ModelEdge {
                            endpoints: Some((left, right)),
                            value,
                        };
                    }
                }
                2 => {
                    let edge = bounded(draw(), EDGES);
                    if model[edge].endpoints.is_some() {
                        assert_eq!(
                            forest.cut(edge_handles[edge]),
                            Ok(model[edge].value.clone())
                        );
                        model[edge].endpoints = None;
                    }
                }
                3 => {
                    let left = bounded(draw(), VERTICES);
                    let right = bounded(draw(), VERTICES);
                    if let Some(path) = model_path(VERTICES, &model, left, right) {
                        let delta = BigInt::from(i64::try_from(draw() % 15).unwrap() - 7);
                        forest
                            .path_add(vertices[left], vertices[right], &delta)
                            .expect("connected path update succeeds");
                        for edge in path {
                            model[edge].value += &delta;
                        }
                    }
                }
                _ => {
                    let vertex = bounded(draw(), VERTICES);
                    forest
                        .reroot(vertices[vertex])
                        .expect("arbitrary reroot succeeds");
                }
            }

            for left in 0..VERTICES {
                for right in 0..VERTICES {
                    match model_path(VERTICES, &model, left, right) {
                        Some(path) => assert_eq!(
                            forest.path_minimum(vertices[left], vertices[right]),
                            Ok(model_minimum(&model, &path)),
                            "path {left}..{right}",
                        ),
                        None => assert_eq!(
                            forest.path_minimum(vertices[left], vertices[right]),
                            Err(LinkCutError::Disconnected),
                            "path {left}..{right}",
                        ),
                    }
                }
                let expected_size = (0..VERTICES)
                    .filter(|&right| model_path(VERTICES, &model, left, right).is_some())
                    .count();
                assert_eq!(
                    forest.represented_vertex_count(vertices[left]),
                    Ok(expected_size),
                    "component containing {left}",
                );
            }
            for edge in 0..EDGES {
                if model[edge].endpoints.is_some() {
                    assert_eq!(
                        forest.edge_value(edge_handles[edge]),
                        Ok(model[edge].value.clone())
                    );
                }
            }
        }
    }
}
