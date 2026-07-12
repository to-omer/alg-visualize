//! Treap and Zip-tree implementations sharing a stable Cartesian-tree engine.

use std::collections::BTreeSet;

use visualizer_core::rng::RngV1;

use crate::OrderedMapTraceRecorder;
use crate::binary_store::BinaryStore;
use crate::binary_trace;
use crate::model::{
    CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal, NodeId, Operation,
    OperationResult, OrderedMap, StructureEntityId, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const EVENT_COMPARE: u32 = 101;
const EVENT_DESCEND: u32 = 102;
const EVENT_INSERT: u32 = 103;
const EVENT_OVERWRITE: u32 = 104;
const EVENT_REMOVE: u32 = 105;
const EVENT_ROTATE_LEFT: u32 = 106;
const EVENT_ROTATE_RIGHT: u32 = 107;
const EVENT_RESULT: u32 = 108;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RankSource {
    Priority,
    Geometric,
}

#[derive(Clone, Debug)]
struct RandomizedTree {
    store: BinaryStore<u64>,
    rng: RngV1,
    source: RankSource,
}

impl RandomizedTree {
    fn new(seed: u64, source: RankSource) -> Self {
        let domain = match source {
            RankSource::Priority => "rng.algorithm.treap.priority",
            RankSource::Geometric => "rng.algorithm.zip.rank",
        };
        Self {
            store: BinaryStore::new(),
            rng: RngV1::from_seed(seed, domain),
            source,
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

    fn metadata_label(&self) -> &'static str {
        match self.source {
            RankSource::Priority => "priority",
            RankSource::Geometric => "rank",
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

    fn higher(&self, first: NodeId, second: NodeId) -> Result<bool, MapError> {
        let first = self.store.node(first)?;
        let second = self.store.node(second)?;
        Ok(first.metadata > second.metadata
            || (first.metadata == second.metadata && first.key < second.key))
    }

    fn rotate_left(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self.store.node(root)?.right.ok_or(MapError::Corrupt(
            "Cartesian left rotation requires right child",
        ))?;
        let middle = self.store.node(pivot)?.left;
        self.store.node_mut(root)?.right = middle;
        self.store.node_mut(pivot)?.left = Some(root);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            EVENT_ROTATE_LEFT,
            TraceKind::RotateLeft,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let label = self.metadata_label();
        let after_root = self
            .store
            .project_node(root, |value| vec![(label.to_owned(), *value)])?;
        let after_pivot = self
            .store
            .project_node(pivot, |value| vec![(label.to_owned(), *value)])?;
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
        let pivot = self.store.node(root)?.left.ok_or(MapError::Corrupt(
            "Cartesian right rotation requires left child",
        ))?;
        let middle = self.store.node(pivot)?.right;
        self.store.node_mut(root)?.left = middle;
        self.store.node_mut(pivot)?.right = Some(root);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            EVENT_ROTATE_RIGHT,
            TraceKind::RotateRight,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let label = self.metadata_label();
        let after_root = self
            .store
            .project_node(root, |value| vec![(label.to_owned(), *value)])?;
        let after_pivot = self
            .store
            .project_node(pivot, |value| vec![(label.to_owned(), *value)])?;
        trace.transition(event, move |state| {
            binary_trace::rotation(state, root, pivot, after_root, after_pivot)
        })?;
        Ok(pivot)
    }

    fn find(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<(NodeId, EntryId)>, MapError> {
        let mut cursor = self.store.root;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.store.node(id)?;
            if key == node.key {
                return Ok(Some((id, node.entry)));
            }
            cursor = if key < node.key {
                node.left
            } else {
                node.right
            };
            trace.record(
                Self::event(
                    EVENT_DESCEND,
                    TraceKind::Descend,
                    Some(id),
                    Some(node.entry),
                    Some(key),
                )
                .with_target(cursor.map(StructureEntityId::Node)),
            )?;
        }
        Ok(None)
    }

    fn lower_bound(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<EntryId>, MapError> {
        let mut cursor = self.store.root;
        let mut candidate = None;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.store.node(id)?;
            if node.key >= key {
                candidate = Some(node.entry);
                cursor = node.left;
            } else {
                cursor = node.right;
            }
            trace.record(
                Self::event(
                    EVENT_DESCEND,
                    TraceKind::Descend,
                    Some(id),
                    Some(node.entry),
                    Some(key),
                )
                .with_target(cursor.map(StructureEntityId::Node)),
            )?;
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
        let root_key = self.store.node(root)?.key;
        if key < root_key {
            let child = if let Some(left) = self.store.node(root)?.left {
                let entry = self.store.node(root)?.entry;
                trace.record(
                    Self::event(
                        EVENT_DESCEND,
                        TraceKind::Descend,
                        Some(root),
                        Some(entry),
                        Some(key),
                    )
                    .with_target(Some(StructureEntityId::Node(left))),
                )?;
                self.insert_node(left, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.left = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.left = Some(child);
            if self.higher(child, root)? {
                return self.rotate_right(root, trace);
            }
        } else {
            let child = if let Some(right) = self.store.node(root)?.right {
                let entry = self.store.node(root)?.entry;
                trace.record(
                    Self::event(
                        EVENT_DESCEND,
                        TraceKind::Descend,
                        Some(root),
                        Some(entry),
                        Some(key),
                    )
                    .with_target(Some(StructureEntityId::Node(right))),
                )?;
                self.insert_node(right, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.right = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.right = Some(child);
            if self.higher(child, root)? {
                return self.rotate_left(root, trace);
            }
        }
        Ok(root)
    }

    fn record_insert(
        &self,
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        parent: Option<NodeId>,
        root_after: Option<NodeId>,
    ) -> Result<(), MapError> {
        let label = self.metadata_label();
        let node_after = self
            .store
            .project_node(node, |value| vec![(label.to_owned(), *value)])?;
        let entry_after = self.store.project_entry(self.store.node(node)?.entry)?;
        let parent_after = parent
            .map(|id| {
                self.store
                    .project_node(id, |value| vec![(label.to_owned(), *value)])
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

    fn remove_node(
        &mut self,
        root: Option<NodeId>,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(Option<NodeId>, Option<EntryId>), MapError> {
        let Some(root) = root else {
            return Ok((None, None));
        };
        self.compare(key, root, trace)?;
        let (root_key, left, right, entry) = {
            let node = self.store.node(root)?;
            (node.key, node.left, node.right, node.entry)
        };
        if key < root_key {
            if let Some(left) = left {
                trace.record(
                    Self::event(
                        EVENT_DESCEND,
                        TraceKind::Descend,
                        Some(root),
                        Some(entry),
                        Some(key),
                    )
                    .with_target(Some(StructureEntityId::Node(left))),
                )?;
            }
            let (child, removed) = self.remove_node(left, key, trace)?;
            self.store.node_mut(root)?.left = child;
            return Ok((Some(root), removed));
        }
        if key > root_key {
            if let Some(right) = right {
                trace.record(
                    Self::event(
                        EVENT_DESCEND,
                        TraceKind::Descend,
                        Some(root),
                        Some(entry),
                        Some(key),
                    )
                    .with_target(Some(StructureEntityId::Node(right))),
                )?;
            }
            let (child, removed) = self.remove_node(right, key, trace)?;
            self.store.node_mut(root)?.right = child;
            return Ok((Some(root), removed));
        }
        match (left, right) {
            (None, child) | (child, None) => {
                self.store.free_node(root)?;
                Ok((child, Some(entry)))
            }
            (Some(left), Some(right)) if self.higher(left, right)? => {
                let new_root = self.rotate_right(root, trace)?;
                let (child, removed) = self.remove_node(Some(root), key, trace)?;
                self.store.node_mut(new_root)?.right = child;
                Ok((Some(new_root), removed))
            }
            (Some(_), Some(_)) => {
                let new_root = self.rotate_left(root, trace)?;
                let (child, removed) = self.remove_node(Some(root), key, trace)?;
                self.store.node_mut(new_root)?.left = child;
                Ok((Some(new_root), removed))
            }
        }
    }

    fn sample_rank(&mut self) -> Result<u64, MapError> {
        match self.source {
            RankSource::Priority => Ok(self.rng.next_u64()),
            RankSource::Geometric => Ok(self.rng.zip_rank()?),
        }
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        if self.store.by_key.contains_key(&key) {
            let (_, entry) = self.find(key, trace)?.ok_or(MapError::Corrupt(
                "indexed randomized entry is not in the tree",
            ))?;
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
        let rank = self.sample_rank()?;
        let (entry, node) = self.store.allocate(key, value, rank)?;
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
        let (root, removed) = self.remove_node(self.store.root, key, trace)?;
        self.store.root = root;
        let Some(entry) = removed else {
            return Ok(OperationResult::Miss);
        };
        let value = self.store.free_entry(key, entry)?;
        let label = self.metadata_label();
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
            return Ok(OperationResult::Removed { entry, value });
        }
        let nodes_after = self
            .store
            .take_projected_nodes(|value| vec![(label.to_owned(), *value)])?;
        let root_after = self.store.root.map(crate::StructureEntityId::Node);
        let metrics_after = self.store.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, vec![(entry, None)], metrics_after)
        })?;
        Ok(OperationResult::Removed { entry, value })
    }

    fn query(
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
        let result = entry.map_or(Ok(OperationResult::Miss), |entry| {
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

    fn apply_traced(
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
    ) -> Result<(), InvariantViolation> {
        if !seen_nodes.insert(id) {
            return Err(InvariantViolation {
                code: "CARTESIAN_CYCLE",
            });
        }
        let node = self.store.nodes.get(id.0).ok_or(InvariantViolation {
            code: "CARTESIAN_DANGLING_NODE",
        })?;
        if minimum.is_some_and(|bound| node.key <= bound)
            || maximum.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation {
                code: "CARTESIAN_ORDER",
            });
        }
        if !seen_entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "CARTESIAN_DUPLICATE_ENTRY",
            });
        }
        let entry = self
            .store
            .entries
            .get(node.entry.0)
            .ok_or(InvariantViolation {
                code: "CARTESIAN_DANGLING_ENTRY",
            })?;
        if entry.key != node.key {
            return Err(InvariantViolation {
                code: "CARTESIAN_ENTRY_KEY",
            });
        }
        for (child, min, max) in [
            (node.left, minimum, Some(node.key)),
            (node.right, Some(node.key), maximum),
        ] {
            if let Some(child) = child {
                let child_node = self.store.nodes.get(child.0).ok_or(InvariantViolation {
                    code: "CARTESIAN_DANGLING_CHILD",
                })?;
                if child_node.metadata > node.metadata
                    || (child_node.metadata == node.metadata && child_node.key < node.key)
                {
                    return Err(InvariantViolation {
                        code: "CARTESIAN_HEAP",
                    });
                }
                self.validate_node(child, min, max, seen_nodes, seen_entries)?;
            }
        }
        Ok(())
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        if let Some(root) = self.store.root {
            self.validate_node(root, None, None, &mut nodes, &mut entries)?;
        }
        if nodes.len() != usize::try_from(self.store.nodes.len()).unwrap_or(usize::MAX)
            || entries.len() != usize::try_from(self.store.entries.len()).unwrap_or(usize::MAX)
            || entries.len() != self.store.by_key.len()
        {
            return Err(InvariantViolation {
                code: "CARTESIAN_COUNT",
            });
        }
        for (key, (entry, node)) in &self.store.by_key {
            let record = self.store.nodes.get(node.0).ok_or(InvariantViolation {
                code: "CARTESIAN_INDEX_NODE",
            })?;
            if record.key != *key || record.entry != *entry {
                return Err(InvariantViolation {
                    code: "CARTESIAN_INDEX",
                });
            }
        }
        Ok(())
    }
}

/// Max-priority Cartesian tree. Equal priorities put the smaller key above.
#[derive(Clone, Debug)]
pub struct TreapMap(RandomizedTree);

impl TreapMap {
    /// Creates an empty Treap with a domain-separated deterministic RNG.
    pub fn new(seed: u64) -> Self {
        Self(RandomizedTree::new(seed, RankSource::Priority))
    }

    /// Number of priority words consumed. Overwrites and reads do not change it.
    pub const fn rng_draws(&self) -> u64 {
        self.0.rng.draws()
    }

    pub(crate) fn apply_traced(
        &mut self,
        operation: Operation,
        trace: &mut OrderedMapTraceRecorder,
    ) -> Result<OperationResult, MapError> {
        self.0.apply_traced(operation, trace)
    }
}

/// Geometric-rank Zip tree. Equal ranks put the smaller key above.
#[derive(Clone, Debug)]
pub struct ZipMap(RandomizedTree);

impl ZipMap {
    /// Creates an empty Zip tree with a domain-separated deterministic RNG.
    pub fn new(seed: u64) -> Self {
        Self(RandomizedTree::new(seed, RankSource::Geometric))
    }

    /// Number of rank words consumed. Overwrites and reads do not change it.
    pub const fn rng_draws(&self) -> u64 {
        self.0.rng.draws()
    }

    pub(crate) fn apply_traced(
        &mut self,
        operation: Operation,
        trace: &mut OrderedMapTraceRecorder,
    ) -> Result<OperationResult, MapError> {
        self.0.apply_traced(operation, trace)
    }
}

macro_rules! impl_ordered_map {
    ($map:ty) => {
        impl OrderedMap for $map {
            fn apply(
                &mut self,
                operation: Operation,
                trace: &mut Vec<TraceEvent>,
            ) -> Result<OperationResult, MapError> {
                self.0
                    .apply_operation(operation, &mut TraceTarget::Events(trace))
            }

            fn canonical_snapshot(&self) -> CanonicalSnapshot {
                self.0.store.canonical_snapshot()
            }

            fn structure_snapshot(&self) -> StructureSnapshot {
                let label = self.0.metadata_label();
                self.0
                    .store
                    .structure_snapshot(|value| vec![(label.to_owned(), *value)])
            }

            fn structure_entity_count(&self) -> usize {
                self.0.store.structure_entity_count()
            }

            fn check_invariants(&self) -> Result<(), InvariantViolation> {
                self.0.check_invariants()
            }

            fn estimated_bytes(&self) -> usize {
                self.0.store.estimated_bytes()
            }
        }
    };
}

impl_ordered_map!(TreapMap);
impl_ordered_map!(ZipMap);

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn exercise(map: &mut impl OrderedMap) {
        let mut model = BTreeMap::new();
        for round in 0_u64..20 {
            for key in (0_u64..64).map(|key| (key * 37 + round * 11) % 64) {
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
            for key in (0_u64..64).filter(|key| (key + round) % 3 == 0) {
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
        let expected: Vec<_> = model.into_iter().collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn treap_matches_model_and_only_new_insert_draws() {
        let mut map = TreapMap::new(17);
        map.apply(
            Operation::Insert {
                key: 7,
                value: "first".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        let draws = map.rng_draws();
        map.apply(
            Operation::Insert {
                key: 7,
                value: "second".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        map.apply(Operation::Get { key: 7 }, &mut Vec::new())
            .unwrap();
        assert_eq!(map.rng_draws(), draws);
        exercise(&mut map);
    }

    #[test]
    fn zip_matches_model_and_rank_is_exposed() {
        let mut map = ZipMap::new(29);
        exercise(&mut map);
        assert!(map.rng_draws() > 0);
        assert!(
            map.structure_snapshot()
                .nodes
                .iter()
                .all(|node| node.metadata[0].0 == "rank")
        );
    }
}
