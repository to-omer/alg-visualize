//! Sedgewick 2–3-tree left-leaning red-black tree.

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

const COMPARE: u32 = 501;
const INSERT: u32 = 502;
const OVERWRITE: u32 = 503;
const REMOVE: u32 = 504;
const ROTATE_LEFT: u32 = 505;
const ROTATE_RIGHT: u32 = 506;
const RECOLOR: u32 = 507;
const RESULT: u32 = 508;
const DETACH: u32 = 509;
const DESCEND: u32 = 510;

/// Left-leaning red-black tree representing a 2–3 tree.
#[derive(Clone, Debug)]
pub struct LlrbMap {
    store: BinaryStore<bool>,
}

impl Default for LlrbMap {
    fn default() -> Self {
        Self::new()
    }
}

impl LlrbMap {
    /// Creates an empty LLRB tree.
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
        trace.transition(
            Self::event(
                COMPARE,
                TraceKind::Compare,
                Some(node),
                self.store.nodes.get(node.0).map(|record| record.entry),
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
                DESCEND,
                TraceKind::Descend,
                Some(node),
                Some(entry),
                Some(key),
            )
            .with_target(Some(StructureEntityId::Node(target))),
        )
    }

    fn is_red(&self, node: Option<NodeId>) -> bool {
        node.and_then(|id| self.store.nodes.get(id.0))
            .is_some_and(|record| record.metadata)
    }

    fn set_red(
        &mut self,
        node: NodeId,
        red: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        if !self.set_red_value(node, red)? {
            return Ok(());
        }
        let record = self.store.node(node)?;
        let after = self
            .store
            .project_node(node, |red| vec![("red".to_owned(), u64::from(*red))])?;
        trace.transition(
            Self::event(
                RECOLOR,
                TraceKind::UpdateMetadata,
                Some(node),
                Some(record.entry),
                Some(record.key),
            ),
            move |state| {
                let mut records = binary_trace::metadata_change(state, after)?;
                records.extend(binary_trace::metric_increments(
                    state,
                    &[(MetricOrdinal::Recolors, 1)],
                )?);
                Ok(records)
            },
        )?;
        Ok(())
    }

    fn set_red_value(&mut self, node: NodeId, red: bool) -> Result<bool, MapError> {
        if self.store.node(node)?.metadata == red {
            return Ok(false);
        }
        self.store.node_mut(node)?.metadata = red;
        self.store.metrics.recolors += 1;
        Ok(true)
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
            .ok_or(MapError::Corrupt("LLRB left rotation requires right child"))?;
        let color = self.store.node(root)?.metadata;
        let middle = self.store.node(pivot)?.left;
        self.store.node_mut(root)?.right = middle;
        self.store.node_mut(pivot)?.left = Some(root);
        if self.store.root == Some(root) {
            self.store.root = Some(pivot);
        }
        let recolors = u64::from(self.set_red_value(pivot, color)?)
            + u64::from(self.set_red_value(root, true)?);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            ROTATE_LEFT,
            TraceKind::RotateLeft,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self
            .store
            .project_node(root, |red| vec![("red".to_owned(), u64::from(*red))])?;
        let after_pivot = self
            .store
            .project_node(pivot, |red| vec![("red".to_owned(), u64::from(*red))])?;
        trace.transition(event, move |state| {
            binary_trace::rotation_with_metrics(
                state,
                root,
                pivot,
                after_root,
                after_pivot,
                &[(MetricOrdinal::Recolors, recolors)],
            )
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
            .ok_or(MapError::Corrupt("LLRB right rotation requires left child"))?;
        let color = self.store.node(root)?.metadata;
        let middle = self.store.node(pivot)?.right;
        self.store.node_mut(root)?.left = middle;
        self.store.node_mut(pivot)?.right = Some(root);
        if self.store.root == Some(root) {
            self.store.root = Some(pivot);
        }
        let recolors = u64::from(self.set_red_value(pivot, color)?)
            + u64::from(self.set_red_value(root, true)?);
        self.store.metrics.rotations += 1;
        let pivot_record = self.store.node(pivot)?;
        let event = Self::event(
            ROTATE_RIGHT,
            TraceKind::RotateRight,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self
            .store
            .project_node(root, |red| vec![("red".to_owned(), u64::from(*red))])?;
        let after_pivot = self
            .store
            .project_node(pivot, |red| vec![("red".to_owned(), u64::from(*red))])?;
        trace.transition(event, move |state| {
            binary_trace::rotation_with_metrics(
                state,
                root,
                pivot,
                after_root,
                after_pivot,
                &[(MetricOrdinal::Recolors, recolors)],
            )
        })?;
        Ok(pivot)
    }

    fn flip_colors(&mut self, root: NodeId, trace: &mut TraceTarget<'_>) -> Result<(), MapError> {
        let node = self.store.node(root)?;
        let (color, left, right) = (node.metadata, node.left, node.right);
        self.set_red(root, !color, trace)?;
        for child in [left, right].into_iter().flatten() {
            let color = self.store.node(child)?.metadata;
            self.set_red(child, !color, trace)?;
        }
        Ok(())
    }

    fn left_left(&self, root: NodeId) -> Option<NodeId> {
        self.store
            .nodes
            .get(root.0)?
            .left
            .and_then(|left| self.store.nodes.get(left.0)?.left)
    }

    fn right_left(&self, root: NodeId) -> Option<NodeId> {
        self.store
            .nodes
            .get(root.0)?
            .right
            .and_then(|right| self.store.nodes.get(right.0)?.left)
    }

    fn fix_up(
        &mut self,
        mut root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        if self.is_red(self.store.node(root)?.right) {
            root = self.rotate_left(root, trace)?;
        }
        if self.is_red(self.store.node(root)?.left) && self.is_red(self.left_left(root)) {
            root = self.rotate_right(root, trace)?;
        }
        if self.is_red(self.store.node(root)?.left) && self.is_red(self.store.node(root)?.right) {
            self.flip_colors(root, trace)?;
        }
        Ok(root)
    }

    fn move_red_left(
        &mut self,
        mut root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.flip_colors(root, trace)?;
        if self.is_red(self.right_left(root)) {
            let right = self
                .store
                .node(root)?
                .right
                .ok_or(MapError::Corrupt("moveRedLeft requires right child"))?;
            let right = self.rotate_right(right, trace)?;
            self.store.node_mut(root)?.right = Some(right);
            root = self.rotate_left(root, trace)?;
            self.flip_colors(root, trace)?;
        }
        Ok(root)
    }

    fn move_red_right(
        &mut self,
        mut root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.flip_colors(root, trace)?;
        if self.is_red(self.left_left(root)) {
            root = self.rotate_right(root, trace)?;
            self.flip_colors(root, trace)?;
        }
        Ok(root)
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
            let (entry, next) = match key.cmp(&node.key) {
                Ordering::Equal => return Ok(Some(node.entry)),
                Ordering::Less => {
                    if lower_bound {
                        candidate = Some(node.entry);
                    }
                    (node.entry, node.left)
                }
                Ordering::Greater => (node.entry, node.right),
            };
            if let Some(target) = next {
                Self::descend(trace, id, target, entry, key)?;
            }
            cursor = next;
        }
        Ok(candidate)
    }

    fn insert_node(
        &mut self,
        mut root: NodeId,
        inserted: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        self.compare(key, root, trace)?;
        if key < self.store.node(root)?.key {
            let left = if let Some(left) = self.store.node(root)?.left {
                Self::descend(trace, root, left, self.store.node(root)?.entry, key)?;
                self.insert_node(left, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.left = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.left = Some(left);
        } else {
            let right = if let Some(right) = self.store.node(root)?.right {
                Self::descend(trace, root, right, self.store.node(root)?.entry, key)?;
                self.insert_node(right, inserted, key, trace)?
            } else {
                self.store.node_mut(root)?.right = Some(inserted);
                self.record_insert(trace, inserted, Some(root), self.store.root)?;
                inserted
            };
            self.store.node_mut(root)?.right = Some(right);
        }
        if self.is_red(self.store.node(root)?.right) && !self.is_red(self.store.node(root)?.left) {
            root = self.rotate_left(root, trace)?;
        }
        if self.is_red(self.store.node(root)?.left) && self.is_red(self.left_left(root)) {
            root = self.rotate_right(root, trace)?;
        }
        if self.is_red(self.store.node(root)?.left) && self.is_red(self.store.node(root)?.right) {
            self.flip_colors(root, trace)?;
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
        let node_after = self
            .store
            .project_node(node, |red| vec![("red".to_owned(), u64::from(*red))])?;
        let entry_after = self.store.project_entry(self.store.node(node)?.entry)?;
        let parent_after = parent
            .map(|id| {
                self.store
                    .project_node(id, |red| vec![("red".to_owned(), u64::from(*red))])
            })
            .transpose()?;
        let entry = entry_after.id;
        let key = entry_after.key;
        trace.transition(
            Self::event(
                INSERT,
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

    fn delete_min(
        &mut self,
        mut root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(Option<NodeId>, BinaryNode<bool>, NodeId, bool), MapError> {
        if self.store.node(root)?.left.is_none() {
            let record = self.store.node(root)?.clone();
            let right = record.right;
            self.store.free_node(root)?;
            return Ok((right, record, root, true));
        }
        if !self.is_red(self.store.node(root)?.left) && !self.is_red(self.left_left(root)) {
            root = self.move_red_left(root, trace)?;
        }
        let left = self
            .store
            .node(root)?
            .left
            .ok_or(MapError::Corrupt("deleteMin lost left child"))?;
        let (left, minimum, removed_node, pending) = self.delete_min(left, trace)?;
        self.store.node_mut(root)?.left = left;
        self.record_pending_child_removal(
            trace,
            root,
            removed_node,
            minimum.entry,
            minimum.key,
            pending,
        )?;
        Ok((
            Some(self.fix_up(root, trace)?),
            minimum,
            removed_node,
            false,
        ))
    }

    fn remove_node(
        &mut self,
        mut root: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(Option<NodeId>, EntryId, NodeId, bool), MapError> {
        self.compare(key, root, trace)?;
        if key < self.store.node(root)?.key {
            if !self.is_red(self.store.node(root)?.left) && !self.is_red(self.left_left(root)) {
                root = self.move_red_left(root, trace)?;
            }
            let left = self
                .store
                .node(root)?
                .left
                .ok_or(MapError::Corrupt("existing key lost LLRB left path"))?;
            Self::descend(trace, root, left, self.store.node(root)?.entry, key)?;
            let (left, removed, removed_node, pending) = self.remove_node(left, key, trace)?;
            self.store.node_mut(root)?.left = left;
            self.record_pending_child_removal(trace, root, removed_node, removed, key, pending)?;
            return Ok((
                Some(self.fix_up(root, trace)?),
                removed,
                removed_node,
                false,
            ));
        }
        if self.is_red(self.store.node(root)?.left) {
            root = self.rotate_right(root, trace)?;
        }
        let entry = self.store.node(root)?.entry;
        if key == self.store.node(root)?.key && self.store.node(root)?.right.is_none() {
            self.store.free_node(root)?;
            return Ok((None, entry, root, true));
        }
        if !self.is_red(self.store.node(root)?.right) && !self.is_red(self.right_left(root)) {
            root = self.move_red_right(root, trace)?;
        }
        if key == self.store.node(root)?.key {
            let removed = self.store.node(root)?.entry;
            let right = self
                .store
                .node(root)?
                .right
                .ok_or(MapError::Corrupt("LLRB successor path is missing"))?;
            let (right, successor, removed_node, pending) = self.delete_min(right, trace)?;
            let node = self.store.node_mut(root)?;
            node.key = successor.key;
            node.entry = successor.entry;
            node.right = right;
            self.store
                .by_key
                .insert(successor.key, (successor.entry, root));
            self.record_pending_child_removal(
                trace,
                root,
                removed_node,
                successor.entry,
                successor.key,
                pending,
            )?;
            Ok((
                Some(self.fix_up(root, trace)?),
                removed,
                removed_node,
                false,
            ))
        } else {
            let right = self
                .store
                .node(root)?
                .right
                .ok_or(MapError::Corrupt("existing key lost LLRB right path"))?;
            Self::descend(trace, root, right, self.store.node(root)?.entry, key)?;
            let (right, removed, removed_node, pending) = self.remove_node(right, key, trace)?;
            self.store.node_mut(root)?.right = right;
            self.record_pending_child_removal(trace, root, removed_node, removed, key, pending)?;
            Ok((
                Some(self.fix_up(root, trace)?),
                removed,
                removed_node,
                false,
            ))
        }
    }

    fn record_structure_removal(
        trace: &mut TraceTarget<'_>,
        removed_node: NodeId,
        entry: EntryId,
        key: u64,
        root_update: binary_trace::RootUpdate,
        nodes_after: Vec<crate::StructureNode>,
    ) -> Result<(), MapError> {
        trace.transition(
            Self::event(DETACH, TraceKind::Remove, None, Some(entry), Some(key)),
            move |state| binary_trace::node_removal(state, removed_node, root_update, nodes_after),
        )
    }

    fn record_pending_child_removal(
        &self,
        trace: &mut TraceTarget<'_>,
        parent: NodeId,
        removed_node: NodeId,
        entry: EntryId,
        key: u64,
        pending: bool,
    ) -> Result<(), MapError> {
        if !pending {
            return Ok(());
        }
        let after = self
            .store
            .project_node(parent, |red| vec![("red".to_owned(), u64::from(*red))])?;
        Self::record_structure_removal(
            trace,
            removed_node,
            entry,
            key,
            binary_trace::RootUpdate::Preserve,
            vec![after],
        )
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
                .ok_or(MapError::Corrupt("indexed LLRB entry is not in the tree"))?;
            let previous = self.store.overwrite(entry, value)?;
            let after = self.store.project_entry(entry)?;
            trace.transition(
                Self::event(
                    OVERWRITE,
                    TraceKind::Overwrite,
                    None,
                    Some(entry),
                    Some(key),
                ),
                move |state| binary_trace::entry_change(state, after),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let (entry, node) = self.store.allocate(key, value, true)?;
        self.store.root = if let Some(root) = self.store.root {
            Some(self.insert_node(root, node, key, trace)?)
        } else {
            self.store.root = Some(node);
            self.record_insert(trace, node, None, self.store.root)?;
            Some(node)
        };
        let root = self
            .store
            .root
            .ok_or(MapError::Corrupt("insert lost root"))?;
        self.set_red(root, false, trace)?;
        Ok(OperationResult::Inserted { entry })
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.store.clear_projection_changes();
        if !self.store.by_key.contains_key(&key) {
            if self.find(key, false, trace)?.is_some() {
                return Err(MapError::Corrupt("unindexed LLRB entry is in the tree"));
            }
            return Ok(OperationResult::Miss);
        }
        let root = self
            .store
            .root
            .ok_or(MapError::Corrupt("found key without LLRB root"))?;
        if !self.is_red(self.store.node(root)?.left) && !self.is_red(self.store.node(root)?.right) {
            self.set_red(root, true, trace)?;
        }
        let (root, entry, removed_node, pending) = self.remove_node(root, key, trace)?;
        self.store.root = root;
        if pending {
            Self::record_structure_removal(
                trace,
                removed_node,
                entry,
                key,
                binary_trace::RootUpdate::Set(root),
                Vec::new(),
            )?;
        }
        if let Some(root) = root {
            self.set_red(root, false, trace)?;
        }
        let value = self.store.free_entry(key, entry)?;
        let nodes_after = if trace.records_patches() {
            self.store
                .take_projected_nodes(|red| vec![("red".to_owned(), u64::from(*red))])?
        } else {
            self.store.clear_projection_changes();
            Vec::new()
        };
        let root_after = self.store.root.map(crate::StructureEntityId::Node);
        let metrics_after = self.store.metrics;
        trace.transition(
            Self::event(REMOVE, TraceKind::Remove, None, Some(entry), Some(key)),
            move |state| {
                state.diff_selected(root_after, nodes_after, vec![(entry, None)], metrics_after)
            },
        )?;
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
        id: NodeId,
        bounds: (Option<u64>, Option<u64>),
        parent_red: bool,
        nodes: &mut BTreeSet<NodeId>,
        entries: &mut BTreeSet<EntryId>,
    ) -> Result<u32, InvariantViolation> {
        if !nodes.insert(id) {
            return Err(InvariantViolation { code: "LLRB_CYCLE" });
        }
        let node = self.store.nodes.get(id.0).ok_or(InvariantViolation {
            code: "LLRB_DANGLING_NODE",
        })?;
        if bounds.0.is_some_and(|bound| node.key <= bound)
            || bounds.1.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation { code: "LLRB_ORDER" });
        }
        if !entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "LLRB_DUPLICATE_ENTRY",
            });
        }
        if self.is_red(node.right) {
            return Err(InvariantViolation {
                code: "LLRB_RIGHT_RED",
            });
        }
        if parent_red && node.metadata {
            return Err(InvariantViolation {
                code: "LLRB_CONSECUTIVE_RED",
            });
        }
        let left_black = node.left.map_or(Ok(1), |left| {
            self.validate_node(
                left,
                (bounds.0, Some(node.key)),
                node.metadata,
                nodes,
                entries,
            )
        })?;
        let right_black = node.right.map_or(Ok(1), |right| {
            self.validate_node(
                right,
                (Some(node.key), bounds.1),
                node.metadata,
                nodes,
                entries,
            )
        })?;
        if left_black != right_black {
            return Err(InvariantViolation {
                code: "LLRB_BLACK_HEIGHT",
            });
        }
        Ok(left_black + u32::from(!node.metadata))
    }
}

impl OrderedMap for LlrbMap {
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
            .structure_snapshot(|red| vec![("red".to_owned(), u64::from(*red))])
    }

    fn structure_entity_count(&self) -> usize {
        self.store.structure_entity_count()
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        if self.store.root.is_some_and(|root| self.is_red(Some(root))) {
            return Err(InvariantViolation {
                code: "LLRB_ROOT_RED",
            });
        }
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        if let Some(root) = self.store.root {
            self.validate_node(root, (None, None), false, &mut nodes, &mut entries)?;
        }
        if nodes.len() != usize::try_from(self.store.nodes.len()).unwrap_or(usize::MAX)
            || entries.len() != self.store.by_key.len()
        {
            return Err(InvariantViolation { code: "LLRB_COUNT" });
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
    fn top_down_delete_matches_model_and_preserves_black_height() {
        let mut map = LlrbMap::new();
        let mut model = BTreeMap::new();
        for round in 0_u64..10 {
            for key in (0_u64..128).map(|key| (key * 71 + round * 23) % 128) {
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
            for key in (0_u64..128).filter(|key| (key + round) % 3 == 0) {
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

    #[test]
    fn missing_remove_is_structurally_read_only() {
        let mut map = LlrbMap::new();
        for key in [4, 2, 6, 1, 3, 5, 7] {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .unwrap();
        }
        let before = map.structure_snapshot();
        assert_eq!(
            map.apply(Operation::Remove { key: 99 }, &mut Vec::new())
                .unwrap(),
            OperationResult::Miss
        );
        assert_eq!(map.structure_snapshot(), before);
    }
}
