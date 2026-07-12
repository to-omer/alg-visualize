//! Andersson AA tree with deterministic bottom-up deletion repair.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::OrderedMapTraceRecorder;
use crate::binary_store::{BinaryNode, BinaryStore};
use crate::binary_trace;
use crate::model::{
    CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal, NodeId, Operation,
    OperationResult, OrderedMap, StructureEntityId, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const EVENT_COMPARE: u32 = 301;
const EVENT_INSERT: u32 = 302;
const EVENT_OVERWRITE: u32 = 303;
const EVENT_REMOVE: u32 = 304;
const EVENT_SKEW: u32 = 305;
const EVENT_SPLIT: u32 = 306;
const EVENT_LEVEL: u32 = 307;
const EVENT_RESULT: u32 = 308;
const EVENT_DESCEND: u32 = 309;

/// Andersson AA tree representing a 2–3 tree with integer levels.
#[derive(Clone, Debug)]
pub struct AaMap {
    store: BinaryStore<u32>,
}

impl Default for AaMap {
    fn default() -> Self {
        Self::new()
    }
}

impl AaMap {
    /// Creates an empty AA tree.
    pub const fn new() -> Self {
        Self {
            store: BinaryStore::new(),
        }
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

    fn compare(
        &mut self,
        key: u64,
        node: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.store.metrics.comparisons += 1;
        self.store.metrics.node_visits += 1;
        let entry = self.store.nodes.get(node.0).map(|record| record.entry);
        trace.transition(
            Self::event(
                EVENT_COMPARE,
                TraceKind::Compare,
                Some(node),
                entry,
                Some(key),
            ),
            |state| {
                binary_trace::metric_increments(
                    state,
                    &[
                        (MetricOrdinal::Comparisons, 1),
                        (MetricOrdinal::NodeVisits, 1),
                    ],
                )
            },
        )
    }

    fn descend(
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        target: NodeId,
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
            .with_target(Some(StructureEntityId::Node(target))),
        )
    }

    fn level(&self, node: Option<NodeId>) -> Result<u32, MapError> {
        node.map_or(Ok(0), |id| Ok(self.store.node(id)?.metadata))
    }

    fn rotate_left(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self
            .store
            .node(root)?
            .right
            .ok_or(MapError::Corrupt("AA split requires right child"))?;
        let middle = self.store.node(pivot)?.left;
        self.store.node_mut(root)?.right = middle;
        self.store.node_mut(pivot)?.left = Some(root);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            EVENT_SPLIT,
            TraceKind::RotateLeft,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self
            .store
            .project_node(root, |level| vec![("level".to_owned(), u64::from(*level))])?;
        let after_pivot = self
            .store
            .project_node(pivot, |level| vec![("level".to_owned(), u64::from(*level))])?;
        trace.transition(event, move |state| {
            binary_trace::rotation(state, root, pivot, after_root, after_pivot)
        })?;
        Ok(pivot)
    }

    fn rotate_right(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self
            .store
            .node(root)?
            .left
            .ok_or(MapError::Corrupt("AA skew requires left child"))?;
        let middle = self.store.node(pivot)?.right;
        self.store.node_mut(root)?.left = middle;
        self.store.node_mut(pivot)?.right = Some(root);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            EVENT_SKEW,
            TraceKind::RotateRight,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self
            .store
            .project_node(root, |level| vec![("level".to_owned(), u64::from(*level))])?;
        let after_pivot = self
            .store
            .project_node(pivot, |level| vec![("level".to_owned(), u64::from(*level))])?;
        trace.transition(event, move |state| {
            binary_trace::rotation(state, root, pivot, after_root, after_pivot)
        })?;
        Ok(pivot)
    }

    fn skew(&mut self, root: NodeId, trace: &mut TraceTarget<'_>) -> Result<NodeId, MapError> {
        let left = self.store.node(root)?.left;
        if left.is_some() && self.level(left)? == self.level(Some(root))? {
            self.rotate_right(root, trace)
        } else {
            Ok(root)
        }
    }

    fn split(&mut self, root: NodeId, trace: &mut TraceTarget<'_>) -> Result<NodeId, MapError> {
        let right = self.store.node(root)?.right;
        let right_right = right.and_then(|id| self.store.nodes.get(id.0)?.right);
        if right_right.is_some() && self.level(right_right)? == self.level(Some(root))? {
            let pivot = self.rotate_left(root, trace)?;
            self.store.node_mut(pivot)?.metadata = self
                .store
                .node(pivot)?
                .metadata
                .checked_add(1)
                .ok_or(MapError::ArithmeticOverflow)?;
            let record = self.store.node(pivot)?;
            let after = self
                .store
                .project_node(pivot, |level| vec![("level".to_owned(), u64::from(*level))])?;
            trace.transition(
                Self::event(
                    EVENT_LEVEL,
                    TraceKind::UpdateMetadata,
                    Some(pivot),
                    Some(record.entry),
                    Some(record.key),
                ),
                move |state| binary_trace::metadata_change(state, after),
            )?;
            Ok(pivot)
        } else {
            Ok(root)
        }
    }

    fn insert_node(
        &mut self,
        root: NodeId,
        inserted: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.compare(key, root, trace)?;
        if key < self.store.node(root)?.key {
            let child = if let Some(left) = self.store.node(root)?.left {
                Self::descend(trace, root, left, self.store.node(root)?.entry, key)?;
                self.insert_node(left, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.left = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.left = Some(child);
        } else {
            let child = if let Some(right) = self.store.node(root)?.right {
                Self::descend(trace, root, right, self.store.node(root)?.entry, key)?;
                self.insert_node(right, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.right = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.right = Some(child);
        }
        let root = self.skew(root, trace)?;
        self.split(root, trace)
    }

    fn record_insert(
        &self,
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        parent: Option<NodeId>,
        root_after: Option<NodeId>,
    ) -> Result<(), MapError> {
        let node_after = self
            .store
            .project_node(node, |level| vec![("level".to_owned(), u64::from(*level))])?;
        let entry_after = self.store.project_entry(self.store.node(node)?.entry)?;
        let parent_after = parent
            .map(|id| {
                self.store
                    .project_node(id, |level| vec![("level".to_owned(), u64::from(*level))])
            })
            .transpose()?;
        let entry = entry_after.id;
        let key = entry_after.key;
        trace.transition(
            Self::event(
                EVENT_INSERT,
                TraceKind::Insert,
                Some(node),
                Some(entry),
                Some(key),
            ),
            move |state| {
                binary_trace::insertion(state, root_after, parent_after, node_after, entry_after)
            },
        )
    }

    fn decrease_level(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let node = self.store.node(root)?;
        let expected = self.level(node.left)?.min(self.level(node.right)?) + 1;
        if expected >= node.metadata {
            return Ok(());
        }
        self.store.node_mut(root)?.metadata = expected;
        if let Some(right) = self.store.node(root)?.right
            && self.store.node(right)?.metadata > expected
        {
            self.store.node_mut(right)?.metadata = expected;
        }
        let record = self.store.node(root)?;
        let after_root = self
            .store
            .project_node(root, |level| vec![("level".to_owned(), u64::from(*level))])?;
        let right = record.right;
        let after_right = right
            .map(|id| {
                self.store
                    .project_node(id, |level| vec![("level".to_owned(), u64::from(*level))])
            })
            .transpose()?;
        trace.transition(
            Self::event(
                EVENT_LEVEL,
                TraceKind::UpdateMetadata,
                Some(root),
                Some(record.entry),
                Some(record.key),
            ),
            move |state| {
                let mut records = vec![binary_trace::node_change(state, after_root)?];
                if let Some(after_right) = after_right
                    && state.node(after_right.id) != Some(&after_right)
                {
                    records.push(binary_trace::node_change(state, after_right)?);
                }
                records.sort_by_key(|record| match record {
                    crate::StatePatchRecord::Node { id, .. } => *id,
                    _ => unreachable!("level transition contains only node patches"),
                });
                Ok(records)
            },
        )?;
        Ok(())
    }

    fn repair_after_delete(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.decrease_level(root, trace)?;
        let root = self.skew(root, trace)?;
        if let Some(right) = self.store.node(root)?.right {
            let right = self.skew(right, trace)?;
            self.store.node_mut(root)?.right = Some(right);
            if let Some(right_right) = self.store.node(right)?.right {
                let repaired = self.skew(right_right, trace)?;
                self.store.node_mut(right)?.right = Some(repaired);
            }
        }
        let root = self.split(root, trace)?;
        if let Some(right) = self.store.node(root)?.right {
            let right = self.split(right, trace)?;
            self.store.node_mut(root)?.right = Some(right);
        }
        Ok(root)
    }

    fn detach_min_unbalanced(
        &mut self,
        root: NodeId,
        affected: &mut Vec<NodeId>,
    ) -> Result<(Option<NodeId>, BinaryNode<u32>), MapError> {
        if let Some(left) = self.store.node(root)?.left {
            let (left, minimum) = self.detach_min_unbalanced(left, affected)?;
            self.store.node_mut(root)?.left = left;
            affected.push(root);
            return Ok((Some(root), minimum));
        }
        let record = self.store.node(root)?.clone();
        let right = record.right;
        self.store.free_node(root)?;
        Ok((right, record))
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
        let (root_key, left, right, entry) = {
            let node = self.store.node(root)?;
            (node.key, node.left, node.right, node.entry)
        };
        let removed = match key.cmp(&root_key) {
            Ordering::Less => {
                if let Some(left) = left {
                    Self::descend(trace, root, left, entry, key)?;
                }
                let (child, removed) = self.remove_node_unbalanced(left, key, trace, affected)?;
                if removed.is_none() {
                    return Ok((Some(root), None));
                }
                self.store.node_mut(root)?.left = child;
                affected.push(root);
                removed
            }
            Ordering::Greater => {
                if let Some(right) = right {
                    Self::descend(trace, root, right, entry, key)?;
                }
                let (child, removed) = self.remove_node_unbalanced(right, key, trace, affected)?;
                if removed.is_none() {
                    return Ok((Some(root), None));
                }
                self.store.node_mut(root)?.right = child;
                affected.push(root);
                removed
            }
            Ordering::Equal => match (left, right) {
                (None, child) | (child, None) => {
                    self.store.free_node(root)?;
                    return Ok((child, Some(entry)));
                }
                (Some(_), Some(right)) => {
                    let (right, successor) = self.detach_min_unbalanced(right, affected)?;
                    let node = self.store.node_mut(root)?;
                    node.key = successor.key;
                    node.entry = successor.entry;
                    node.right = right;
                    self.store
                        .by_key
                        .insert(successor.key, (successor.entry, root));
                    affected.push(root);
                    Some(entry)
                }
            },
        };
        Ok((Some(root), removed))
    }

    fn repair_affected(
        &mut self,
        affected: Vec<NodeId>,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        for node in affected {
            if self.store.nodes.get(node.0).is_none() {
                continue;
            }
            let parent = if self.store.root == Some(node) {
                None
            } else {
                self.store.nodes.iter().find_map(|(id, record)| {
                    (record.left == Some(node) || record.right == Some(node)).then_some(NodeId(id))
                })
            };
            let repaired = self.repair_after_delete(node, trace)?;
            if repaired == node {
                continue;
            }
            if let Some(parent) = parent {
                let record = self.store.node_mut(parent)?;
                if record.left == Some(node) {
                    record.left = Some(repaired);
                } else if record.right == Some(node) {
                    record.right = Some(repaired);
                } else {
                    return Err(MapError::Corrupt("AA repair parent link changed"));
                }
            } else {
                self.store.root = Some(repaired);
            }
        }
        Ok(())
    }

    fn find(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<EntryId>, MapError> {
        let mut cursor = self.store.root;
        let mut candidate = None;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.store.node(id)?;
            if node.key == key {
                return Ok(Some(node.entry));
            }
            let (entry, next) = if key < node.key {
                if lower_bound {
                    candidate = Some(node.entry);
                }
                (node.entry, node.left)
            } else {
                (node.entry, node.right)
            };
            if let Some(target) = next {
                Self::descend(trace, id, target, entry, key)?;
            }
            cursor = next;
        }
        Ok(candidate)
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        if self.store.by_key.contains_key(&key) {
            let entry = self
                .find(key, false, trace)?
                .ok_or(MapError::Corrupt("indexed AA entry is not in the tree"))?;
            let previous = self.store.overwrite(entry, value)?;
            let after = self.store.project_entry(entry)?;
            trace.transition(
                Self::event(
                    EVENT_OVERWRITE,
                    TraceKind::Overwrite,
                    None,
                    Some(entry),
                    Some(key),
                ),
                move |state| binary_trace::entry_change(state, after),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let (entry, node) = self.store.allocate(key, value, 1)?;
        self.store.root = if let Some(root) = self.store.root {
            Some(self.insert_node(root, node, key, trace)?)
        } else {
            self.store.root = Some(node);
            self.record_insert(trace, node, None, self.store.root)?;
            Some(node)
        };
        Ok(OperationResult::Inserted { entry })
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.store.clear_projection_changes();
        let mut affected = Vec::new();
        let (root, entry) =
            self.remove_node_unbalanced(self.store.root, key, trace, &mut affected)?;
        self.store.root = root;
        let Some(entry) = entry else {
            return Ok(OperationResult::Miss);
        };
        let value = self.store.free_entry(key, entry)?;
        let event = Self::event(
            EVENT_REMOVE,
            TraceKind::Remove,
            None,
            Some(entry),
            Some(key),
        );
        if !trace.records_patches() {
            self.store.clear_projection_changes();
            trace.record(event)?;
            self.repair_affected(affected, trace)?;
            return Ok(OperationResult::Removed { entry, value });
        }
        let nodes_after = self
            .store
            .take_projected_nodes(|level| vec![("level".to_owned(), u64::from(*level))])?;
        let root_after = self.store.root.map(crate::StructureEntityId::Node);
        let metrics_after = self.store.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, vec![(entry, None)], metrics_after)
        })?;
        self.repair_affected(affected, trace)?;
        Ok(OperationResult::Removed { entry, value })
    }

    fn query(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let result = self
            .find(key, lower_bound, trace)?
            .map_or(Ok(OperationResult::Miss), |entry| {
                self.store.found_result(entry)
            })?;
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
            EVENT_RESULT,
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
        id: NodeId,
        minimum: Option<u64>,
        maximum: Option<u64>,
        nodes: &mut BTreeSet<NodeId>,
        entries: &mut BTreeSet<EntryId>,
    ) -> Result<(), InvariantViolation> {
        if !nodes.insert(id) {
            return Err(InvariantViolation { code: "AA_CYCLE" });
        }
        let node = self.store.nodes.get(id.0).ok_or(InvariantViolation {
            code: "AA_DANGLING_NODE",
        })?;
        if minimum.is_some_and(|bound| node.key <= bound)
            || maximum.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation { code: "AA_ORDER" });
        }
        if !entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "AA_DUPLICATE_ENTRY",
            });
        }
        let left_level = node
            .left
            .and_then(|id| self.store.nodes.get(id.0))
            .map_or(0, |child| child.metadata);
        let right_level = node
            .right
            .and_then(|id| self.store.nodes.get(id.0))
            .map_or(0, |child| child.metadata);
        let right_right_level = node
            .right
            .and_then(|id| self.store.nodes.get(id.0))
            .and_then(|right| right.right)
            .and_then(|id| self.store.nodes.get(id.0))
            .map_or(0, |child| child.metadata);
        if node.metadata == 0 || left_level + 1 != node.metadata {
            return Err(InvariantViolation {
                code: "AA_LEFT_LEVEL",
            });
        }
        if !(right_level == node.metadata || right_level + 1 == node.metadata) {
            return Err(InvariantViolation {
                code: "AA_RIGHT_LEVEL",
            });
        }
        if right_right_level >= node.metadata {
            return Err(InvariantViolation {
                code: "AA_RIGHT_GRANDCHILD",
            });
        }
        if node.metadata > 1 && (node.left.is_none() || node.right.is_none()) {
            return Err(InvariantViolation {
                code: "AA_LEVEL_CHILDREN",
            });
        }
        if let Some(left) = node.left {
            self.validate_node(left, minimum, Some(node.key), nodes, entries)?;
        }
        if let Some(right) = node.right {
            self.validate_node(right, Some(node.key), maximum, nodes, entries)?;
        }
        Ok(())
    }
}

impl OrderedMap for AaMap {
    fn apply(
        &mut self,
        operation: Operation,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<OperationResult, MapError> {
        self.apply_operation(operation, &mut TraceTarget::Events(trace))
    }

    fn canonical_snapshot(&self) -> CanonicalSnapshot {
        self.store.canonical_snapshot()
    }

    fn structure_snapshot(&self) -> StructureSnapshot {
        self.store
            .structure_snapshot(|level| vec![("level".to_owned(), u64::from(*level))])
    }

    fn structure_entity_count(&self) -> usize {
        self.store.structure_entity_count()
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        if let Some(root) = self.store.root {
            self.validate_node(root, None, None, &mut nodes, &mut entries)?;
        }
        if nodes.len() != usize::try_from(self.store.nodes.len()).unwrap_or(usize::MAX)
            || entries.len() != self.store.by_key.len()
        {
            return Err(InvariantViolation { code: "AA_COUNT" });
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.store.estimated_bytes()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn mixed_updates_match_model_and_preserve_levels() {
        let mut map = AaMap::new();
        let mut model = BTreeMap::new();
        for round in 0_u64..12 {
            for key in (0_u64..96).map(|key| (key * 53 + round * 17) % 96) {
                let value = format!("{round}:{key}");
                map.apply(
                    Operation::Insert {
                        key,
                        value: value.clone(),
                    },
                    &mut Vec::new(),
                )
                .unwrap();
                model.insert(key, value);
                map.check_invariants().unwrap();
            }
            for key in (0_u64..96).filter(|key| (key + round) % 4 == 0) {
                map.apply(Operation::Remove { key }, &mut Vec::new())
                    .unwrap();
                model.remove(&key);
                map.check_invariants().unwrap();
            }
        }
        let actual: Vec<_> = map
            .canonical_snapshot()
            .entries
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect();
        assert_eq!(actual, model.into_iter().collect::<Vec<_>>());
    }
}
