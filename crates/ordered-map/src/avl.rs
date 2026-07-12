//! Bottom-up AVL tree with stable node and entry identities.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use visualizer_core::arena::GenerationalArena;

use crate::model::{
    CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal,
    Metrics, NodeId, Operation, OperationResult, OrderedMap, StatePatchRecord, StructureEntityId,
    StructureLink, StructureNode, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;
use crate::{OrderedMapTraceRecorder, TraceState};

const EVENT_COMPARE: u32 = 1;
const EVENT_DESCEND: u32 = 2;
const EVENT_INSERT: u32 = 3;
const EVENT_OVERWRITE: u32 = 4;
const EVENT_REMOVE: u32 = 5;
const EVENT_ROTATE_LEFT: u32 = 6;
const EVENT_ROTATE_RIGHT: u32 = 7;
const EVENT_UPDATE_HEIGHT: u32 = 8;
const EVENT_RESULT: u32 = 9;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Debug)]
struct Node {
    entry: EntryId,
    key: u64,
    left: Option<NodeId>,
    right: Option<NodeId>,
    height: u32,
}

/// Traceable bottom-up AVL ordered map.
#[derive(Clone, Debug)]
pub struct AvlMap {
    root: Option<NodeId>,
    nodes: GenerationalArena<Node>,
    entries: GenerationalArena<EntryRecord>,
    by_key: BTreeMap<u64, (EntryId, NodeId)>,
    metrics: Metrics,
    changed_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
}

impl Default for AvlMap {
    fn default() -> Self {
        Self::new()
    }
}

impl AvlMap {
    /// Creates an empty AVL tree.
    pub const fn new() -> Self {
        Self {
            root: None,
            nodes: GenerationalArena::new(),
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            metrics: Metrics {
                comparisons: 0,
                node_visits: 0,
                bit_tests: 0,
                rotations: 0,
                recolors: 0,
                splits: 0,
                merges: 0,
                rebuild_items: 0,
                allocations: 0,
                frees: 0,
            },
            changed_nodes: BTreeSet::new(),
            removed_nodes: BTreeSet::new(),
        }
    }

    fn node(&self, id: NodeId) -> Result<&Node, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling node link"))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, MapError> {
        self.changed_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling node link"))
    }

    fn height(&self, id: Option<NodeId>) -> Result<u32, MapError> {
        id.map_or(Ok(0), |node| Ok(self.node(node)?.height))
    }

    fn emit(
        trace: &mut TraceTarget<'_>,
        catalog_id: u32,
        kind: TraceKind,
        node: Option<NodeId>,
        entry: Option<EntryId>,
        key: Option<u64>,
    ) -> Result<(), MapError> {
        trace.record(Self::event(catalog_id, kind, node, entry, key))
    }

    fn emit_descend(
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        target: Option<NodeId>,
        entry: EntryId,
        key: u64,
    ) -> Result<(), MapError> {
        trace.record(
            Self::event(
                EVENT_DESCEND,
                TraceKind::Descend,
                Some(node),
                Some(entry),
                Some(key),
            )
            .with_target(target.map(StructureEntityId::Node)),
        )
    }

    fn compare(
        &mut self,
        query: u64,
        node: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.metrics.comparisons += 1;
        self.metrics.node_visits += 1;
        let entry = self.nodes.get(node.0).map(|record| record.entry);
        let event = Self::event(
            EVENT_COMPARE,
            TraceKind::Compare,
            Some(node),
            entry,
            Some(query),
        );
        trace.transition(event, |state| {
            Ok(vec![
                StatePatchRecord::Metric {
                    ordinal: MetricOrdinal::Comparisons,
                    before: state.metric_value(MetricOrdinal::Comparisons),
                    after: state
                        .metric_value(MetricOrdinal::Comparisons)
                        .checked_add(1)
                        .ok_or(MapError::ArithmeticOverflow)?,
                },
                StatePatchRecord::Metric {
                    ordinal: MetricOrdinal::NodeVisits,
                    before: state.metric_value(MetricOrdinal::NodeVisits),
                    after: state
                        .metric_value(MetricOrdinal::NodeVisits)
                        .checked_add(1)
                        .ok_or(MapError::ArithmeticOverflow)?,
                },
            ])
        })
    }

    fn event(
        catalog_id: u32,
        kind: TraceKind,
        node: Option<NodeId>,
        entry: Option<EntryId>,
        key: Option<u64>,
    ) -> TraceEvent {
        TraceEvent {
            catalog_id,
            kind,
            node: node.map(StructureEntityId::Node),
            target: None,
            entry,
            key,
            patch_start: 0,
            patch_count: 0,
        }
    }

    fn project_node(&self, id: NodeId) -> Result<StructureNode, MapError> {
        let node = self.node(id)?;
        Ok(StructureNode {
            id: StructureEntityId::Node(id),
            role: "binary-node".to_owned(),
            entries: vec![node.entry],
            keys: vec![node.key],
            links: [(0, "left", node.left), (1, "right", node.right)]
                .into_iter()
                .filter_map(|(slot, role, target)| {
                    target.map(|target| StructureLink {
                        slot,
                        role: role.to_owned(),
                        target: StructureEntityId::Node(target),
                    })
                })
                .collect(),
            metadata: vec![("height".to_owned(), u64::from(node.height))],
        })
    }

    fn project_entry(&self, id: EntryId) -> Result<CanonicalEntry, MapError> {
        let entry = self
            .entries
            .get(id.0)
            .ok_or(MapError::Corrupt("node references missing entry"))?;
        Ok(CanonicalEntry {
            id,
            key: entry.key,
            value: entry.value.clone(),
        })
    }

    fn node_change(state: &TraceState, after: StructureNode) -> Result<StatePatchRecord, MapError> {
        let before = state
            .node(after.id)
            .ok_or(MapError::TraceState(
                "changed node is missing from trace state",
            ))?
            .clone();
        Ok(StatePatchRecord::Node {
            id: after.id,
            before: Some(Box::new(before)),
            after: Some(Box::new(after)),
        })
    }

    fn rotation_patch(
        state: &TraceState,
        root: NodeId,
        pivot: NodeId,
        after_root: StructureNode,
        after_pivot: StructureNode,
    ) -> Result<Vec<StatePatchRecord>, MapError> {
        let root_id = StructureEntityId::Node(root);
        let pivot_id = StructureEntityId::Node(pivot);
        let mut records = Vec::with_capacity(4);
        if state.root() == Some(root_id) {
            records.push(StatePatchRecord::Root {
                before: Some(root_id),
                after: Some(pivot_id),
            });
        } else {
            let mut incoming = state
                .nodes()
                .filter(|node| node.links.iter().any(|link| link.target == root_id));
            let parent = incoming
                .next()
                .ok_or(MapError::TraceState("rotation root has no incoming link"))?;
            if incoming.next().is_some() {
                return Err(MapError::TraceState(
                    "rotation root has multiple incoming links",
                ));
            }
            let mut after_parent = parent.clone();
            let link = after_parent
                .links
                .iter_mut()
                .find(|link| link.target == root_id)
                .ok_or(MapError::TraceState("rotation incoming link disappeared"))?;
            link.target = pivot_id;
            records.push(StatePatchRecord::Node {
                id: parent.id,
                before: Some(Box::new(parent.clone())),
                after: Some(Box::new(after_parent)),
            });
        }
        records.push(Self::node_change(state, after_root)?);
        records.push(Self::node_change(state, after_pivot)?);
        records.sort_by_key(|record| match record {
            StatePatchRecord::Node { id, .. } => Some(*id),
            StatePatchRecord::Root { .. }
            | StatePatchRecord::Entry { .. }
            | StatePatchRecord::Metric { .. } => None,
        });
        let rotations = state.metric_value(MetricOrdinal::Rotations);
        records.push(StatePatchRecord::Metric {
            ordinal: MetricOrdinal::Rotations,
            before: rotations,
            after: rotations
                .checked_add(1)
                .ok_or(MapError::ArithmeticOverflow)?,
        });
        Ok(records)
    }

    fn insert_patch(
        state: &TraceState,
        root_after: Option<StructureEntityId>,
        parent_after: Option<StructureNode>,
        node_after: StructureNode,
        entry_after: CanonicalEntry,
    ) -> Result<Vec<StatePatchRecord>, MapError> {
        let mut records = Vec::with_capacity(5);
        if state.root() != root_after {
            records.push(StatePatchRecord::Root {
                before: state.root(),
                after: root_after,
            });
        }
        if let Some(parent_after) = parent_after {
            records.push(Self::node_change(state, parent_after)?);
        }
        if state.node(node_after.id).is_some() {
            return Err(MapError::TraceState("inserted node already exists"));
        }
        records.push(StatePatchRecord::Node {
            id: node_after.id,
            before: None,
            after: Some(Box::new(node_after)),
        });
        records.sort_by_key(|record| match record {
            StatePatchRecord::Node { id, .. } => Some(*id),
            StatePatchRecord::Root { .. }
            | StatePatchRecord::Entry { .. }
            | StatePatchRecord::Metric { .. } => None,
        });
        if state.entry(entry_after.id).is_some() {
            return Err(MapError::TraceState("inserted entry already exists"));
        }
        records.push(StatePatchRecord::Entry {
            id: entry_after.id,
            before: None,
            after: Some(Box::new(entry_after)),
        });
        let allocations = state.metric_value(MetricOrdinal::Allocations);
        records.push(StatePatchRecord::Metric {
            ordinal: MetricOrdinal::Allocations,
            before: allocations,
            after: allocations
                .checked_add(2)
                .ok_or(MapError::ArithmeticOverflow)?,
        });
        Ok(records)
    }

    fn record_insert(
        &self,
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        parent: Option<NodeId>,
        root_after: Option<NodeId>,
    ) -> Result<(), MapError> {
        let projected_node = self.project_node(node)?;
        let projected_entry = self.project_entry(self.node(node)?.entry)?;
        let projected_parent = parent.map(|id| self.project_node(id)).transpose()?;
        let entry = projected_entry.id;
        let key = projected_entry.key;
        trace.transition(
            Self::event(
                EVENT_INSERT,
                TraceKind::Insert,
                Some(node),
                Some(entry),
                Some(key),
            ),
            move |state| {
                Self::insert_patch(
                    state,
                    root_after.map(StructureEntityId::Node),
                    projected_parent,
                    projected_node,
                    projected_entry,
                )
            },
        )
    }

    fn recompute_height(&mut self, id: NodeId) -> Result<bool, MapError> {
        let (left, right, old_height) = {
            let node = self.node(id)?;
            (node.left, node.right, node.height)
        };
        let height = 1_u32
            .checked_add(self.height(left)?.max(self.height(right)?))
            .ok_or(MapError::ArithmeticOverflow)?;
        if height == old_height {
            return Ok(false);
        }
        self.node_mut(id)?.height = height;
        Ok(true)
    }

    fn update_height(&mut self, id: NodeId, trace: &mut TraceTarget<'_>) -> Result<(), MapError> {
        let (entry, key) = {
            let node = self.node(id)?;
            (node.entry, node.key)
        };
        if self.recompute_height(id)? {
            let after = self.project_node(id)?;
            trace.transition(
                Self::event(
                    EVENT_UPDATE_HEIGHT,
                    TraceKind::UpdateMetadata,
                    Some(id),
                    Some(entry),
                    Some(key),
                ),
                move |state| Ok(vec![Self::node_change(state, after)?]),
            )?;
        }
        Ok(())
    }

    fn balance_factor(&self, id: NodeId) -> Result<i64, MapError> {
        let node = self.node(id)?;
        Ok(i64::from(self.height(node.left)?) - i64::from(self.height(node.right)?))
    }

    fn rotate_left(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self
            .node(root)?
            .right
            .ok_or(MapError::Corrupt("left rotation requires right child"))?;
        let middle = self.node(pivot)?.left;
        self.node_mut(root)?.right = middle;
        self.node_mut(pivot)?.left = Some(root);
        self.recompute_height(root)?;
        self.recompute_height(pivot)?;
        self.metrics.rotations += 1;
        let (pivot_entry, pivot_key) = {
            let pivot_node = self.node(pivot)?;
            (pivot_node.entry, pivot_node.key)
        };
        let after_root = self.project_node(root)?;
        let after_pivot = self.project_node(pivot)?;
        trace.transition(
            Self::event(
                EVENT_ROTATE_LEFT,
                TraceKind::RotateLeft,
                Some(root),
                Some(pivot_entry),
                Some(pivot_key),
            ),
            move |state| Self::rotation_patch(state, root, pivot, after_root, after_pivot),
        )?;
        Ok(pivot)
    }

    fn rotate_right(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self
            .node(root)?
            .left
            .ok_or(MapError::Corrupt("right rotation requires left child"))?;
        let middle = self.node(pivot)?.right;
        self.node_mut(root)?.left = middle;
        self.node_mut(pivot)?.right = Some(root);
        self.recompute_height(root)?;
        self.recompute_height(pivot)?;
        self.metrics.rotations += 1;
        let (pivot_entry, pivot_key) = {
            let pivot_node = self.node(pivot)?;
            (pivot_node.entry, pivot_node.key)
        };
        let after_root = self.project_node(root)?;
        let after_pivot = self.project_node(pivot)?;
        trace.transition(
            Self::event(
                EVENT_ROTATE_RIGHT,
                TraceKind::RotateRight,
                Some(root),
                Some(pivot_entry),
                Some(pivot_key),
            ),
            move |state| Self::rotation_patch(state, root, pivot, after_root, after_pivot),
        )?;
        Ok(pivot)
    }

    fn balance(&mut self, root: NodeId, trace: &mut TraceTarget<'_>) -> Result<NodeId, MapError> {
        self.update_height(root, trace)?;
        let factor = self.balance_factor(root)?;
        if factor > 1 {
            let left = self
                .node(root)?
                .left
                .ok_or(MapError::Corrupt("positive balance without left child"))?;
            if self.balance_factor(left)? < 0 {
                let rotated = self.rotate_left(left, trace)?;
                self.node_mut(root)?.left = Some(rotated);
            }
            return self.rotate_right(root, trace);
        }
        if factor < -1 {
            let right = self
                .node(root)?
                .right
                .ok_or(MapError::Corrupt("negative balance without right child"))?;
            if self.balance_factor(right)? > 0 {
                let rotated = self.rotate_right(right, trace)?;
                self.node_mut(root)?.right = Some(rotated);
            }
            return self.rotate_left(root, trace);
        }
        Ok(root)
    }

    fn find(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<(NodeId, EntryId)>, MapError> {
        let mut cursor = self.root;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.node(id)?;
            if key == node.key {
                return Ok(Some((id, node.entry)));
            }
            cursor = if key < node.key {
                node.left
            } else {
                node.right
            };
            Self::emit_descend(trace, id, cursor, node.entry, key)?;
        }
        Ok(None)
    }

    fn lower_bound(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<EntryId>, MapError> {
        let mut cursor = self.root;
        let mut candidate = None;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.node(id)?;
            if node.key >= key {
                candidate = Some(node.entry);
                cursor = node.left;
            } else {
                cursor = node.right;
            }
            Self::emit_descend(trace, id, cursor, node.entry, key)?;
        }
        Ok(candidate)
    }

    fn insert_node(
        &mut self,
        root: NodeId,
        inserted: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.compare(key, root, trace)?;
        let root_key = self.node(root)?.key;
        if key < root_key {
            let child = if let Some(left) = self.node(root)?.left {
                let entry = self.node(root)?.entry;
                Self::emit_descend(trace, root, Some(left), entry, key)?;
                self.insert_node(left, inserted, key, trace)?
            } else {
                self.node_mut(root)?.left = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.root)?;
                inserted
            };
            self.node_mut(root)?.left = Some(child);
        } else {
            let child = if let Some(right) = self.node(root)?.right {
                let entry = self.node(root)?.entry;
                Self::emit_descend(trace, root, Some(right), entry, key)?;
                self.insert_node(right, inserted, key, trace)?
            } else {
                self.node_mut(root)?.right = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.root)?;
                inserted
            };
            self.node_mut(root)?.right = Some(child);
        }
        self.balance(root, trace)
    }

    fn detach_min_unbalanced(
        &mut self,
        root: NodeId,
        affected: &mut Vec<NodeId>,
    ) -> Result<(Option<NodeId>, Node), MapError> {
        let left = self.node(root)?.left;
        if let Some(left) = left {
            let (new_left, minimum) = self.detach_min_unbalanced(left, affected)?;
            self.node_mut(root)?.left = new_left;
            affected.push(root);
            return Ok((Some(root), minimum));
        }
        let right = self.node(root)?.right;
        let removed = self
            .nodes
            .remove(root.0)
            .ok_or(MapError::Corrupt("minimum node disappeared"))?;
        self.metrics.frees += 1;
        self.changed_nodes.remove(&root);
        self.removed_nodes.insert(root);
        Ok((right, removed))
    }

    fn remove_node_unbalanced(
        &mut self,
        root: Option<NodeId>,
        key: u64,
        trace: &mut TraceTarget<'_>,
        affected: &mut Vec<NodeId>,
    ) -> Result<(Option<NodeId>, Option<EntryId>), MapError> {
        let Some(root) = root else {
            return Ok((None, None));
        };
        self.compare(key, root, trace)?;
        let (root_key, left, right, root_entry) = {
            let node = self.node(root)?;
            (node.key, node.left, node.right, node.entry)
        };
        if key < root_key {
            if let Some(left) = left {
                Self::emit_descend(trace, root, Some(left), root_entry, key)?;
            }
            let (new_left, removed) = self.remove_node_unbalanced(left, key, trace, affected)?;
            if removed.is_none() {
                return Ok((Some(root), None));
            }
            self.node_mut(root)?.left = new_left;
            affected.push(root);
            return Ok((Some(root), removed));
        }
        if key > root_key {
            if let Some(right) = right {
                Self::emit_descend(trace, root, Some(right), root_entry, key)?;
            }
            let (new_right, removed) = self.remove_node_unbalanced(right, key, trace, affected)?;
            if removed.is_none() {
                return Ok((Some(root), None));
            }
            self.node_mut(root)?.right = new_right;
            affected.push(root);
            return Ok((Some(root), removed));
        }

        match (left, right) {
            (None, child) | (child, None) => {
                self.nodes
                    .remove(root.0)
                    .ok_or(MapError::Corrupt("removed node disappeared"))?;
                self.metrics.frees += 1;
                self.changed_nodes.remove(&root);
                self.removed_nodes.insert(root);
                Ok((child, Some(root_entry)))
            }
            (Some(_), Some(right)) => {
                let (new_right, successor) = self.detach_min_unbalanced(right, affected)?;
                let successor_entry = successor.entry;
                let successor_key = successor.key;
                {
                    let node = self.node_mut(root)?;
                    node.entry = successor_entry;
                    node.key = successor_key;
                    node.right = new_right;
                }
                self.by_key.insert(successor_key, (successor_entry, root));
                affected.push(root);
                Ok((Some(root), Some(root_entry)))
            }
        }
    }

    fn rebalance_affected(
        &mut self,
        affected: Vec<NodeId>,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        for node in affected {
            if self.nodes.get(node.0).is_none() {
                continue;
            }
            let parent = if self.root == Some(node) {
                None
            } else {
                self.nodes.iter().find_map(|(id, record)| {
                    (record.left == Some(node) || record.right == Some(node)).then_some(NodeId(id))
                })
            };
            let balanced = self.balance(node, trace)?;
            if balanced == node {
                continue;
            }
            if let Some(parent) = parent {
                let record = self.node_mut(parent)?;
                if record.left == Some(node) {
                    record.left = Some(balanced);
                } else if record.right == Some(node) {
                    record.right = Some(balanced);
                } else {
                    return Err(MapError::Corrupt("AVL rebalance parent link changed"));
                }
            } else {
                self.root = Some(balanced);
            }
        }
        Ok(())
    }

    fn entry_result(&self, entry: EntryId) -> Result<OperationResult, MapError> {
        let record = self
            .entries
            .get(entry.0)
            .ok_or(MapError::Corrupt("node references missing entry"))?;
        Ok(OperationResult::Found {
            entry,
            key: record.key,
            value: record.value.clone(),
        })
    }

    fn apply_insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        if self.by_key.contains_key(&key) {
            let (_, entry) = self
                .find(key, trace)?
                .ok_or(MapError::Corrupt("indexed AVL entry is not in the tree"))?;
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("found entry disappeared"))?;
            let previous = std::mem::replace(&mut record.value, value);
            let after = self.project_entry(entry)?;
            trace.transition(
                Self::event(
                    EVENT_OVERWRITE,
                    TraceKind::Overwrite,
                    None,
                    Some(entry),
                    Some(key),
                ),
                move |state| {
                    let before = state.entry(entry).ok_or(MapError::TraceState(
                        "overwritten entry is missing from trace state",
                    ))?;
                    Ok(vec![StatePatchRecord::Entry {
                        id: entry,
                        before: Some(Box::new(before.clone())),
                        after: Some(Box::new(after)),
                    }])
                },
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        let node_record = Node {
            entry,
            key,
            left: None,
            right: None,
            height: 1,
        };
        let node = match self.nodes.try_insert(node_record) {
            Ok(id) => NodeId(id),
            Err(error) => {
                self.entries.remove(entry.0);
                return Err(error.into());
            }
        };
        self.metrics.allocations += 2;
        self.root = if let Some(root) = self.root {
            Some(self.insert_node(root, node, key, trace)?)
        } else {
            self.root = Some(node);
            self.record_insert(trace, node, None, self.root)?;
            Some(node)
        };
        self.by_key.insert(key, (entry, node));
        Ok(OperationResult::Inserted { entry })
    }

    fn apply_remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.changed_nodes.clear();
        self.removed_nodes.clear();
        let mut affected = Vec::new();
        let (root, removed) = self.remove_node_unbalanced(self.root, key, trace, &mut affected)?;
        self.root = root;
        let Some(entry) = removed else {
            return Ok(OperationResult::Miss);
        };
        self.by_key.remove(&key);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("removed entry disappeared"))?;
        self.metrics.frees += 1;
        let event = Self::event(
            EVENT_REMOVE,
            TraceKind::Remove,
            None,
            Some(entry),
            Some(key),
        );
        if !trace.records_patches() {
            self.changed_nodes.clear();
            self.removed_nodes.clear();
            trace.record(event)?;
            self.rebalance_affected(affected, trace)?;
            return Ok(OperationResult::Removed {
                entry,
                value: record.value,
            });
        }
        let changed = std::mem::take(&mut self.changed_nodes);
        let removed_nodes = std::mem::take(&mut self.removed_nodes);
        let mut nodes_after = Vec::with_capacity(changed.len() + removed_nodes.len());
        for node in changed {
            nodes_after.push((
                StructureEntityId::Node(node),
                Some(self.project_node(node)?),
            ));
        }
        nodes_after.extend(
            removed_nodes
                .into_iter()
                .map(|node| (StructureEntityId::Node(node), None)),
        );
        let root_after = self.root.map(StructureEntityId::Node);
        let metrics_after = self.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, vec![(entry, None)], metrics_after)
        })?;
        self.rebalance_affected(affected, trace)?;
        Ok(OperationResult::Removed {
            entry,
            value: record.value,
        })
    }

    fn apply_query(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let entry = if lower_bound {
            self.lower_bound(key, trace)?
        } else {
            self.find(key, trace)?.map(|(_, entry)| entry)
        };
        let result = entry.map_or(Ok(OperationResult::Miss), |entry| self.entry_result(entry))?;
        Ok(result)
    }

    fn apply_operation(
        &mut self,
        operation: Operation,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let key = match &operation {
            Operation::Insert { key, .. }
            | Operation::Remove { key }
            | Operation::Get { key }
            | Operation::LowerBound { key } => *key,
        };
        let result = match operation {
            Operation::Insert { key, value } => self.apply_insert(key, value, trace),
            Operation::Remove { key } => self.apply_remove(key, trace),
            Operation::Get { key } => self.apply_query(key, false, trace),
            Operation::LowerBound { key } => self.apply_query(key, true, trace),
        }?;
        Self::emit(
            trace,
            EVENT_RESULT,
            TraceKind::Result,
            None,
            None,
            Some(key),
        )?;
        Ok(result)
    }

    /// Applies one operation while producing reversible event state patches.
    ///
    /// # Errors
    ///
    /// Propagates map and trace-state contract failures.
    pub fn apply_traced(
        &mut self,
        operation: Operation,
        trace: &mut OrderedMapTraceRecorder,
    ) -> Result<OperationResult, MapError> {
        self.apply_operation(operation, &mut TraceTarget::Recorder(trace))
    }

    fn validate_node(
        &self,
        id: NodeId,
        minimum: Option<u64>,
        maximum: Option<u64>,
        seen_nodes: &mut BTreeSet<NodeId>,
        seen_entries: &mut BTreeSet<EntryId>,
    ) -> Result<u32, InvariantViolation> {
        if !seen_nodes.insert(id) {
            return Err(InvariantViolation { code: "AVL_CYCLE" });
        }
        let node = self.nodes.get(id.0).ok_or(InvariantViolation {
            code: "AVL_DANGLING_NODE",
        })?;
        if minimum.is_some_and(|bound| node.key <= bound)
            || maximum.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation { code: "AVL_ORDER" });
        }
        if !seen_entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "AVL_DUPLICATE_ENTRY",
            });
        }
        let entry = self.entries.get(node.entry.0).ok_or(InvariantViolation {
            code: "AVL_DANGLING_ENTRY",
        })?;
        if entry.key != node.key {
            return Err(InvariantViolation {
                code: "AVL_ENTRY_KEY",
            });
        }
        let left_height = node.left.map_or(Ok(0), |left| {
            self.validate_node(left, minimum, Some(node.key), seen_nodes, seen_entries)
        })?;
        let right_height = node.right.map_or(Ok(0), |right| {
            self.validate_node(right, Some(node.key), maximum, seen_nodes, seen_entries)
        })?;
        if left_height.abs_diff(right_height) > 1 {
            return Err(InvariantViolation {
                code: "AVL_BALANCE",
            });
        }
        let expected = 1 + left_height.max(right_height);
        if node.height != expected {
            return Err(InvariantViolation { code: "AVL_HEIGHT" });
        }
        Ok(expected)
    }
}

#[path = "avl_ordered_map.rs"]
mod avl_ordered_map;

#[cfg(test)]
include!("avl_tests.rs");
