//! CLRS-style B-tree with stable entry identities.

use std::collections::{BTreeMap as OrderedIndex, BTreeSet};
use std::mem::size_of;

use visualizer_core::arena::GenerationalArena;

use crate::OrderedMapTraceRecorder;
use crate::binary_trace;
use crate::model::{
    CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal,
    Metrics, NodeId, Operation, OperationResult, OrderedMap, StructureEntityId, StructureLink,
    StructureNode, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const COMPARE: u32 = 801;
const INSERT: u32 = 802;
const OVERWRITE: u32 = 803;
const REMOVE: u32 = 804;
const SPLIT: u32 = 805;
const MERGE: u32 = 806;
const MOVE_ENTRY: u32 = 807;
const RESULT: u32 = 808;
const DESCEND: u32 = 809;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Debug)]
struct BNode {
    entries: Vec<(u64, EntryId)>,
    children: Vec<NodeId>,
    leaf: bool,
}

/// CLRS B-tree with configurable minimum degree `2..=16`.
#[derive(Clone, Debug)]
pub struct BTreeMap {
    nodes: GenerationalArena<BNode>,
    entries: GenerationalArena<EntryRecord>,
    by_key: OrderedIndex<u64, EntryId>,
    dirty_nodes: BTreeSet<NodeId>,
    dirty_entries: BTreeSet<EntryId>,
    root: NodeId,
    degree: usize,
    metrics: Metrics,
}

impl BTreeMap {
    /// Creates an empty B-tree containing one empty leaf root.
    ///
    /// # Errors
    ///
    /// Rejects a minimum degree outside `2..=16` or arena allocation failure.
    pub fn new(minimum_degree: u8) -> Result<Self, MapError> {
        if !(2..=16).contains(&minimum_degree) {
            return Err(MapError::InvalidConfiguration("B-tree minimum degree"));
        }
        let mut nodes = GenerationalArena::new();
        let root = NodeId(nodes.try_insert(BNode {
            entries: Vec::new(),
            children: Vec::new(),
            leaf: true,
        })?);
        Ok(Self {
            nodes,
            entries: GenerationalArena::new(),
            by_key: OrderedIndex::new(),
            dirty_nodes: BTreeSet::new(),
            dirty_entries: BTreeSet::new(),
            root,
            degree: usize::from(minimum_degree),
            metrics: Metrics {
                allocations: 1,
                ..Metrics::default()
            },
        })
    }

    fn node(&self, id: NodeId) -> Result<&BNode, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling B-tree child"))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut BNode, MapError> {
        self.dirty_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling B-tree child"))
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

    fn descend(
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        target: NodeId,
        key: u64,
    ) -> Result<(), MapError> {
        trace.record(
            Self::event(DESCEND, TraceKind::Descend, Some(node), None, Some(key))
                .with_target(Some(StructureEntityId::Node(target))),
        )
    }

    fn project_event(
        &mut self,
        trace: &mut TraceTarget<'_>,
        event: TraceEvent,
    ) -> Result<(), MapError> {
        if !trace.records_patches() {
            self.dirty_nodes.clear();
            self.dirty_entries.clear();
            return trace.record(event);
        }
        let node_ids = std::mem::take(&mut self.dirty_nodes);
        let entry_ids = std::mem::take(&mut self.dirty_entries);
        let nodes_after = node_ids
            .into_iter()
            .map(|id| (StructureEntityId::Node(id), self.project_node(id)))
            .collect();
        let entries_after = entry_ids
            .into_iter()
            .map(|id| (id, self.project_entry(id)))
            .collect();
        let root_after = Some(StructureEntityId::Node(self.root));
        let metrics_after = self.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, entries_after, metrics_after)
        })
    }

    fn project_node(&self, id: NodeId) -> Option<StructureNode> {
        self.nodes.get(id.0).map(|node| StructureNode {
            id: StructureEntityId::Node(id),
            role: if node.leaf {
                "btree-leaf".to_owned()
            } else {
                "btree-internal".to_owned()
            },
            entries: node.entries.iter().map(|(_, entry)| *entry).collect(),
            keys: node.entries.iter().map(|(key, _)| *key).collect(),
            links: node
                .children
                .iter()
                .enumerate()
                .map(|(slot, target)| StructureLink {
                    slot: u32::try_from(slot).unwrap_or(u32::MAX),
                    role: format!("child-{slot}"),
                    target: StructureEntityId::Node(*target),
                })
                .collect(),
            metadata: vec![("leaf".to_owned(), u64::from(node.leaf))],
        })
    }

    fn project_entry(&self, id: EntryId) -> Option<CanonicalEntry> {
        let record = self.entries.get(id.0)?;
        (self.by_key.get(&record.key) == Some(&id)).then(|| CanonicalEntry {
            id,
            key: record.key,
            value: record.value.clone(),
        })
    }

    fn locate_in_node(
        &mut self,
        node: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(usize, bool), MapError> {
        let entries = self.node(node)?.entries.clone();
        self.metrics.node_visits += 1;
        if entries.is_empty() {
            trace.transition(
                Self::event(COMPARE, TraceKind::Compare, Some(node), None, Some(key)),
                |state| binary_trace::metric_increments(state, &[(MetricOrdinal::NodeVisits, 1)]),
            )?;
        }
        for (index, (entry_key, entry)) in entries.iter().enumerate() {
            self.metrics.comparisons += 1;
            trace.transition(
                Self::event(
                    COMPARE,
                    TraceKind::Compare,
                    Some(node),
                    Some(*entry),
                    Some(key),
                ),
                |state| {
                    binary_trace::metric_increments(
                        state,
                        &[
                            (MetricOrdinal::Comparisons, 1),
                            (MetricOrdinal::NodeVisits, u64::from(index == 0)),
                        ],
                    )
                },
            )?;
            if key <= *entry_key {
                return Ok((index, key == *entry_key));
            }
        }
        Ok((entries.len(), false))
    }

    fn find(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<EntryId>, MapError> {
        let mut node = self.root;
        let mut candidate = None;
        loop {
            let (index, equal) = self.locate_in_node(node, key, trace)?;
            let record = self.node(node)?;
            if equal {
                return Ok(Some(record.entries[index].1));
            }
            if lower_bound && index < record.entries.len() {
                candidate = Some(record.entries[index].1);
            }
            if record.leaf {
                return Ok(candidate);
            }
            let child = *record
                .children
                .get(index)
                .ok_or(MapError::Corrupt("B-tree child slot is missing"))?;
            Self::descend(trace, node, child, key)?;
            node = child;
        }
    }

    fn found_result(&self, entry: EntryId) -> Result<OperationResult, MapError> {
        let record = self
            .entries
            .get(entry.0)
            .ok_or(MapError::Corrupt("B-tree node references missing entry"))?;
        Ok(OperationResult::Found {
            entry,
            key: record.key,
            value: record.value.clone(),
        })
    }

    fn allocate_node(&mut self, node: BNode) -> Result<NodeId, MapError> {
        let node = NodeId(self.nodes.try_insert(node)?);
        self.dirty_nodes.insert(node);
        self.metrics.allocations += 1;
        Ok(node)
    }

    fn free_node(&mut self, id: NodeId) -> Result<BNode, MapError> {
        let node = self
            .nodes
            .remove(id.0)
            .ok_or(MapError::Corrupt("B-tree node disappeared before free"))?;
        self.dirty_nodes.insert(id);
        self.metrics.frees += 1;
        Ok(node)
    }

    fn split_child(
        &mut self,
        parent: NodeId,
        index: usize,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let child = *self
            .node(parent)?
            .children
            .get(index)
            .ok_or(MapError::Corrupt("split child slot is missing"))?;
        if self.node(child)?.entries.len() != 2 * self.degree - 1 {
            return Err(MapError::Corrupt("split child is not full"));
        }
        let (leaf, right_entries, right_children, median) = {
            let degree = self.degree;
            let child = self.node_mut(child)?;
            let right_entries = child.entries.split_off(degree);
            let median = child
                .entries
                .pop()
                .ok_or(MapError::Corrupt("full child has no median"))?;
            let right_children = if child.leaf {
                Vec::new()
            } else {
                child.children.split_off(degree)
            };
            (child.leaf, right_entries, right_children, median)
        };
        let right = self.allocate_node(BNode {
            entries: right_entries,
            children: right_children,
            leaf,
        })?;
        let parent_node = self.node_mut(parent)?;
        parent_node.entries.insert(index, median);
        parent_node.children.insert(index + 1, right);
        self.metrics.splits += 1;
        self.project_event(
            trace,
            Self::event(
                SPLIT,
                TraceKind::Split,
                Some(child),
                Some(median.1),
                Some(median.0),
            ),
        )?;
        Ok(())
    }

    fn insert_nonfull(
        &mut self,
        node: NodeId,
        item: (u64, EntryId),
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let (mut index, equal) = self.locate_in_node(node, item.0, trace)?;
        if equal {
            return Err(MapError::Corrupt("duplicate reached B-tree insertion"));
        }
        if self.node(node)?.leaf {
            self.node_mut(node)?.entries.insert(index, item);
            return Ok(());
        }
        let child = self.node(node)?.children[index];
        if self.node(child)?.entries.len() == 2 * self.degree - 1 {
            self.split_child(node, index, trace)?;
            self.metrics.comparisons += 1;
            let separator = self.node(node)?.entries[index];
            trace.transition(
                Self::event(
                    COMPARE,
                    TraceKind::Compare,
                    Some(node),
                    Some(separator.1),
                    Some(item.0),
                ),
                |state| binary_trace::metric_increments(state, &[(MetricOrdinal::Comparisons, 1)]),
            )?;
            if item.0 > separator.0 {
                index += 1;
            }
        }
        let child = self.node(node)?.children[index];
        Self::descend(trace, node, child, item.0)?;
        self.insert_nonfull(child, item, trace)
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        if self.by_key.contains_key(&key) {
            let entry = self
                .find(key, false, trace)?
                .ok_or(MapError::Corrupt("indexed B-tree entry is not in the tree"))?;
            self.dirty_entries.insert(entry);
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("found B-tree entry disappeared"))?;
            let previous = std::mem::replace(&mut record.value, value);
            self.project_event(
                trace,
                Self::event(
                    OVERWRITE,
                    TraceKind::Overwrite,
                    None,
                    Some(entry),
                    Some(key),
                ),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        self.dirty_entries.insert(entry);
        self.metrics.allocations += 1;
        if self.node(self.root)?.entries.len() == 2 * self.degree - 1 {
            let old_root = self.root;
            let root = match self.allocate_node(BNode {
                entries: Vec::new(),
                children: vec![old_root],
                leaf: false,
            }) {
                Ok(root) => root,
                Err(error) => {
                    self.entries.remove(entry.0);
                    self.metrics.frees += 1;
                    return Err(error);
                }
            };
            self.root = root;
            self.split_child(root, 0, trace)?;
        }
        self.insert_nonfull(self.root, (key, entry), trace)?;
        self.by_key.insert(key, entry);
        self.dirty_entries.insert(entry);
        self.project_event(
            trace,
            Self::event(INSERT, TraceKind::Insert, None, Some(entry), Some(key)),
        )?;
        Ok(OperationResult::Inserted { entry })
    }

    fn borrow_left(
        &mut self,
        parent: NodeId,
        index: usize,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let (left, child, separator) = {
            let node = self.node(parent)?;
            (
                node.children[index - 1],
                node.children[index],
                node.entries[index - 1],
            )
        };
        let borrowed = self
            .node_mut(left)?
            .entries
            .pop()
            .ok_or(MapError::Corrupt("left sibling has no borrowable entry"))?;
        let moved_child = if self.node(left)?.leaf {
            None
        } else {
            self.node_mut(left)?.children.pop()
        };
        self.node_mut(parent)?.entries[index - 1] = borrowed;
        self.node_mut(child)?.entries.insert(0, separator);
        if let Some(moved_child) = moved_child {
            self.node_mut(child)?.children.insert(0, moved_child);
        }
        self.project_event(
            trace,
            Self::event(
                MOVE_ENTRY,
                TraceKind::MoveEntry,
                Some(child),
                Some(borrowed.1),
                Some(borrowed.0),
            ),
        )?;
        Ok(())
    }

    fn borrow_right(
        &mut self,
        parent: NodeId,
        index: usize,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let (child, right, separator) = {
            let node = self.node(parent)?;
            (
                node.children[index],
                node.children[index + 1],
                node.entries[index],
            )
        };
        let borrowed = self.node_mut(right)?.entries.remove(0);
        let moved_child = if self.node(right)?.leaf {
            None
        } else {
            Some(self.node_mut(right)?.children.remove(0))
        };
        self.node_mut(parent)?.entries[index] = borrowed;
        self.node_mut(child)?.entries.push(separator);
        if let Some(moved_child) = moved_child {
            self.node_mut(child)?.children.push(moved_child);
        }
        self.project_event(
            trace,
            Self::event(
                MOVE_ENTRY,
                TraceKind::MoveEntry,
                Some(child),
                Some(borrowed.1),
                Some(borrowed.0),
            ),
        )?;
        Ok(())
    }

    fn merge_children(
        &mut self,
        parent: NodeId,
        index: usize,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let (left, right, separator) = {
            let node = self.node_mut(parent)?;
            let separator = node.entries.remove(index);
            let right = node.children.remove(index + 1);
            (node.children[index], right, separator)
        };
        let right_node = self.free_node(right)?;
        let left_node = self.node_mut(left)?;
        left_node.entries.push(separator);
        left_node.entries.extend(right_node.entries);
        left_node.children.extend(right_node.children);
        self.metrics.merges += 1;
        self.project_event(
            trace,
            Self::event(
                MERGE,
                TraceKind::Merge,
                Some(left),
                Some(separator.1),
                Some(separator.0),
            ),
        )?;
        Ok(left)
    }

    fn prepare_child(
        &mut self,
        parent: NodeId,
        index: usize,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(NodeId, usize), MapError> {
        let children_len = self.node(parent)?.children.len();
        if index > 0
            && self
                .node(self.node(parent)?.children[index - 1])?
                .entries
                .len()
                >= self.degree
        {
            self.borrow_left(parent, index, trace)?;
            return Ok((self.node(parent)?.children[index], index));
        }
        if index + 1 < children_len
            && self
                .node(self.node(parent)?.children[index + 1])?
                .entries
                .len()
                >= self.degree
        {
            self.borrow_right(parent, index, trace)?;
            return Ok((self.node(parent)?.children[index], index));
        }
        if index + 1 < children_len {
            return Ok((self.merge_children(parent, index, trace)?, index));
        }
        Ok((self.merge_children(parent, index - 1, trace)?, index - 1))
    }

    fn minimum_entry(&self, mut node: NodeId) -> Result<(u64, EntryId), MapError> {
        loop {
            let record = self.node(node)?;
            if record.leaf {
                return record
                    .entries
                    .first()
                    .copied()
                    .ok_or(MapError::Corrupt("minimum B-tree leaf is empty"));
            }
            node = *record
                .children
                .first()
                .ok_or(MapError::Corrupt("minimum B-tree child is missing"))?;
        }
    }

    fn maximum_entry(&self, mut node: NodeId) -> Result<(u64, EntryId), MapError> {
        loop {
            let record = self.node(node)?;
            if record.leaf {
                return record
                    .entries
                    .last()
                    .copied()
                    .ok_or(MapError::Corrupt("maximum B-tree leaf is empty"));
            }
            node = *record
                .children
                .last()
                .ok_or(MapError::Corrupt("maximum B-tree child is missing"))?;
        }
    }

    fn delete_entry(
        &mut self,
        node: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<EntryId, MapError> {
        let (index, equal) = self.locate_in_node(node, key, trace)?;
        if equal {
            if self.node(node)?.leaf {
                return Ok(self.node_mut(node)?.entries.remove(index).1);
            }
            let (left, right, target) = {
                let record = self.node(node)?;
                (
                    record.children[index],
                    record.children[index + 1],
                    record.entries[index],
                )
            };
            if self.node(left)?.entries.len() >= self.degree {
                let predecessor = self.maximum_entry(left)?;
                let moved = self.delete_entry(left, predecessor.0, trace)?;
                if moved != predecessor.1 {
                    return Err(MapError::Corrupt("predecessor identity changed"));
                }
                self.node_mut(node)?.entries[index] = predecessor;
                self.project_event(
                    trace,
                    Self::event(
                        MOVE_ENTRY,
                        TraceKind::MoveEntry,
                        Some(node),
                        Some(predecessor.1),
                        Some(predecessor.0),
                    ),
                )?;
                return Ok(target.1);
            }
            if self.node(right)?.entries.len() >= self.degree {
                let successor = self.minimum_entry(right)?;
                let moved = self.delete_entry(right, successor.0, trace)?;
                if moved != successor.1 {
                    return Err(MapError::Corrupt("successor identity changed"));
                }
                self.node_mut(node)?.entries[index] = successor;
                self.project_event(
                    trace,
                    Self::event(
                        MOVE_ENTRY,
                        TraceKind::MoveEntry,
                        Some(node),
                        Some(successor.1),
                        Some(successor.0),
                    ),
                )?;
                return Ok(target.1);
            }
            let merged = self.merge_children(node, index, trace)?;
            return self.delete_entry(merged, key, trace);
        }
        if self.node(node)?.leaf {
            return Err(MapError::Corrupt("existing B-tree key disappeared"));
        }
        let mut child = self.node(node)?.children[index];
        if self.node(child)?.entries.len() == self.degree - 1 {
            (child, _) = self.prepare_child(node, index, trace)?;
        }
        Self::descend(trace, node, child, key)?;
        self.delete_entry(child, key, trace)
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        if !self.by_key.contains_key(&key) {
            if self.find(key, false, trace)?.is_some() {
                return Err(MapError::Corrupt("unindexed B-tree entry is in the tree"));
            }
            return Ok(OperationResult::Miss);
        }
        let entry = self.delete_entry(self.root, key, trace)?;
        if self.node(self.root)?.entries.is_empty() && !self.node(self.root)?.leaf {
            let old_root = self.root;
            self.root = self.node(old_root)?.children[0];
            self.free_node(old_root)?;
        }
        self.by_key.remove(&key);
        self.dirty_entries.insert(entry);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("removed B-tree entry disappeared"))?;
        self.metrics.frees += 1;
        self.project_event(
            trace,
            Self::event(REMOVE, TraceKind::Remove, None, Some(entry), Some(key)),
        )?;
        Ok(OperationResult::Removed {
            entry,
            value: record.value,
        })
    }

    fn query(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let result = self
            .find(key, lower_bound, trace)?
            .map_or(Ok(OperationResult::Miss), |entry| self.found_result(entry))?;
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
            Operation::Insert { key, value } => self.insert(key, value, trace),
            Operation::Remove { key } => self.remove(key, trace),
            Operation::Get { key } => self.query(key, false, trace),
            Operation::LowerBound { key } => self.query(key, true, trace),
        }?;
        trace.record(Self::event(
            RESULT,
            TraceKind::Result,
            None,
            None,
            Some(key),
        ))?;
        Ok(result)
    }

    pub(crate) fn apply_traced(
        &mut self,
        operation: Operation,
        trace: &mut OrderedMapTraceRecorder,
    ) -> Result<OperationResult, MapError> {
        self.apply_operation(operation, &mut TraceTarget::Recorder(trace))
    }

    fn validate_node(
        &self,
        node: NodeId,
        bounds: (Option<u64>, Option<u64>),
        depth: usize,
        leaf_depth: &mut Option<usize>,
        nodes: &mut BTreeSet<NodeId>,
        entries: &mut BTreeSet<EntryId>,
    ) -> Result<(), InvariantViolation> {
        if !nodes.insert(node) {
            return Err(InvariantViolation {
                code: "BTREE_CYCLE",
            });
        }
        let record = self.nodes.get(node.0).ok_or(InvariantViolation {
            code: "BTREE_DANGLING_NODE",
        })?;
        let is_root = node == self.root;
        let minimum = if is_root { 0 } else { self.degree - 1 };
        if record.entries.len() < minimum || record.entries.len() > 2 * self.degree - 1 {
            return Err(InvariantViolation {
                code: "BTREE_OCCUPANCY",
            });
        }
        if record.leaf != record.children.is_empty()
            || (!record.leaf && record.children.len() != record.entries.len() + 1)
        {
            return Err(InvariantViolation {
                code: "BTREE_ARITY",
            });
        }
        let mut previous = bounds.0;
        for (key, entry) in &record.entries {
            if previous.is_some_and(|bound| *key <= bound) {
                return Err(InvariantViolation {
                    code: "BTREE_LOWER_BOUND",
                });
            }
            if bounds.1.is_some_and(|bound| *key >= bound) {
                return Err(InvariantViolation {
                    code: "BTREE_UPPER_BOUND",
                });
            }
            if !entries.insert(*entry) {
                return Err(InvariantViolation {
                    code: "BTREE_DUPLICATE_ENTRY",
                });
            }
            let entry_record = self.entries.get(entry.0).ok_or(InvariantViolation {
                code: "BTREE_DANGLING_ENTRY",
            })?;
            if entry_record.key != *key {
                return Err(InvariantViolation {
                    code: "BTREE_ENTRY_KEY",
                });
            }
            previous = Some(*key);
        }
        if record.leaf {
            if leaf_depth.is_some_and(|expected| expected != depth) {
                return Err(InvariantViolation {
                    code: "BTREE_LEAF_DEPTH",
                });
            }
            *leaf_depth = Some(depth);
            return Ok(());
        }
        for (index, child) in record.children.iter().enumerate() {
            let lower = if index == 0 {
                bounds.0
            } else {
                Some(record.entries[index - 1].0)
            };
            let upper = if index == record.entries.len() {
                bounds.1
            } else {
                Some(record.entries[index].0)
            };
            self.validate_node(
                *child,
                (lower, upper),
                depth + 1,
                leaf_depth,
                nodes,
                entries,
            )?;
        }
        Ok(())
    }
}

impl OrderedMap for BTreeMap {
    fn apply(
        &mut self,
        operation: Operation,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<OperationResult, MapError> {
        self.apply_operation(operation, &mut TraceTarget::Events(trace))
    }

    fn canonical_snapshot(&self) -> CanonicalSnapshot {
        let entries = self
            .by_key
            .iter()
            .filter_map(|(key, entry)| {
                self.entries.get(entry.0).map(|record| CanonicalEntry {
                    id: *entry,
                    key: *key,
                    value: record.value.clone(),
                })
            })
            .collect();
        CanonicalSnapshot {
            entries,
            metrics: self.metrics,
        }
    }

    fn structure_snapshot(&self) -> StructureSnapshot {
        let nodes = self
            .nodes
            .iter()
            .filter_map(|(id, _)| self.project_node(NodeId(id)))
            .collect();
        StructureSnapshot {
            root: Some(StructureEntityId::Node(self.root)),
            nodes,
        }
    }

    fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        self.validate_node(
            self.root,
            (None, None),
            0,
            &mut None,
            &mut nodes,
            &mut entries,
        )?;
        if nodes.len() != usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
            || entries.len() != self.by_key.len()
            || entries.len() != usize::try_from(self.entries.len()).unwrap_or(usize::MAX)
        {
            return Err(InvariantViolation {
                code: "BTREE_COUNT",
            });
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.nodes
            .estimated_bytes()
            .saturating_add(self.entries.estimated_bytes())
            .saturating_add(
                self.nodes
                    .iter()
                    .map(|(_, node)| {
                        node.entries
                            .capacity()
                            .saturating_mul(size_of::<(u64, EntryId)>())
                            .saturating_add(
                                node.children.capacity().saturating_mul(size_of::<NodeId>()),
                            )
                    })
                    .sum::<usize>(),
            )
            .saturating_add(
                self.entries
                    .iter()
                    .map(|(_, entry)| entry.value.capacity())
                    .sum::<usize>(),
            )
    }
}

#[cfg(test)]
include!("btree_tests.rs");
