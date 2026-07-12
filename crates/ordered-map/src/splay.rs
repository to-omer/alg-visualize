//! Top-down Splay tree with specified hit and miss side effects.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::OrderedMapTraceRecorder;
use crate::binary_store::BinaryStore;
use crate::binary_trace;
use crate::model::{
    CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal, NodeId, Operation,
    OperationResult, OrderedMap, StructureEntityId, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const EVENT_COMPARE: u32 = 201;
const EVENT_INSERT: u32 = 202;
const EVENT_OVERWRITE: u32 = 203;
const EVENT_REMOVE: u32 = 204;
const EVENT_ROTATE_LEFT: u32 = 205;
const EVENT_ROTATE_RIGHT: u32 = 206;
const EVENT_RESULT: u32 = 207;
const EVENT_DESCEND: u32 = 208;

#[derive(Clone, Copy)]
enum SplayContinuation {
    LeftLeft { root: NodeId, left: NodeId },
    LeftRight { root: NodeId, left: NodeId },
    RightRight { root: NodeId, right: NodeId },
    RightLeft { root: NodeId, right: NodeId },
}

enum SplayStep {
    Descend {
        cursor: NodeId,
        continuation: SplayContinuation,
    },
    Complete(NodeId),
}

/// Top-down self-adjusting binary search tree.
#[derive(Clone, Debug)]
pub struct SplayMap {
    store: BinaryStore<()>,
}

impl Default for SplayMap {
    fn default() -> Self {
        Self::new()
    }
}

impl SplayMap {
    /// Creates an empty Splay tree.
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
        &self,
        trace: &mut TraceTarget<'_>,
        node: NodeId,
        target: NodeId,
        key: u64,
    ) -> Result<(), MapError> {
        let entry = self.store.node(node)?.entry;
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

    fn rotate_left(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self.store.node(root)?.right.ok_or(MapError::Corrupt(
            "Splay left rotation requires right child",
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
        let after_root = self.store.project_node(root, |()| Vec::new())?;
        let after_pivot = self.store.project_node(pivot, |()| Vec::new())?;
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
            "Splay right rotation requires left child",
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
        let after_root = self.store.project_node(root, |()| Vec::new())?;
        let after_pivot = self.store.project_node(pivot, |()| Vec::new())?;
        trace.transition(event, move |state| {
            binary_trace::rotation(state, root, pivot, after_root, after_pivot)
        })?;
        Ok(pivot)
    }

    fn splay(
        &mut self,
        root: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let mut continuations = Vec::new();
        let mut cursor = root;
        let mut result = loop {
            self.compare(key, cursor, trace)?;
            let step = match key.cmp(&self.store.node(cursor)?.key) {
                Ordering::Less => self.splay_left(cursor, key, trace)?,
                Ordering::Greater => self.splay_right(cursor, key, trace)?,
                Ordering::Equal => SplayStep::Complete(cursor),
            };
            match step {
                SplayStep::Descend {
                    cursor: next,
                    continuation,
                } => {
                    continuations.push(continuation);
                    cursor = next;
                }
                SplayStep::Complete(root) => break root,
            }
        };

        while let Some(continuation) = continuations.pop() {
            result = self.resume_splay(continuation, result, trace)?;
        }
        Ok(result)
    }

    fn splay_left(
        &mut self,
        root: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<SplayStep, MapError> {
        let Some(left) = self.store.node(root)?.left else {
            return Ok(SplayStep::Complete(root));
        };
        self.descend(trace, root, left, key)?;
        self.compare(key, left, trace)?;
        match key.cmp(&self.store.node(left)?.key) {
            Ordering::Less => {
                if let Some(next) = self.store.node(left)?.left {
                    self.descend(trace, left, next, key)?;
                    return Ok(SplayStep::Descend {
                        cursor: next,
                        continuation: SplayContinuation::LeftLeft { root, left },
                    });
                }
                let rotated = self.rotate_right(root, trace)?;
                Ok(SplayStep::Complete(
                    if self.store.node(rotated)?.left.is_some() {
                        self.rotate_right(rotated, trace)?
                    } else {
                        rotated
                    },
                ))
            }
            Ordering::Greater => {
                if let Some(next) = self.store.node(left)?.right {
                    self.descend(trace, left, next, key)?;
                    return Ok(SplayStep::Descend {
                        cursor: next,
                        continuation: SplayContinuation::LeftRight { root, left },
                    });
                }
                if self.store.node(left)?.right.is_some() {
                    let child = self.rotate_left(left, trace)?;
                    self.store.node_mut(root)?.left = Some(child);
                }
                Ok(SplayStep::Complete(
                    if self.store.node(root)?.left.is_some() {
                        self.rotate_right(root, trace)?
                    } else {
                        root
                    },
                ))
            }
            Ordering::Equal => Ok(SplayStep::Complete(self.rotate_right(root, trace)?)),
        }
    }

    fn splay_right(
        &mut self,
        root: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<SplayStep, MapError> {
        let Some(right) = self.store.node(root)?.right else {
            return Ok(SplayStep::Complete(root));
        };
        self.descend(trace, root, right, key)?;
        self.compare(key, right, trace)?;
        match key.cmp(&self.store.node(right)?.key) {
            Ordering::Greater => {
                if let Some(next) = self.store.node(right)?.right {
                    self.descend(trace, right, next, key)?;
                    return Ok(SplayStep::Descend {
                        cursor: next,
                        continuation: SplayContinuation::RightRight { root, right },
                    });
                }
                let rotated = self.rotate_left(root, trace)?;
                Ok(SplayStep::Complete(
                    if self.store.node(rotated)?.right.is_some() {
                        self.rotate_left(rotated, trace)?
                    } else {
                        rotated
                    },
                ))
            }
            Ordering::Less => {
                if let Some(next) = self.store.node(right)?.left {
                    self.descend(trace, right, next, key)?;
                    return Ok(SplayStep::Descend {
                        cursor: next,
                        continuation: SplayContinuation::RightLeft { root, right },
                    });
                }
                if self.store.node(right)?.left.is_some() {
                    let child = self.rotate_right(right, trace)?;
                    self.store.node_mut(root)?.right = Some(child);
                }
                Ok(SplayStep::Complete(
                    if self.store.node(root)?.right.is_some() {
                        self.rotate_left(root, trace)?
                    } else {
                        root
                    },
                ))
            }
            Ordering::Equal => Ok(SplayStep::Complete(self.rotate_left(root, trace)?)),
        }
    }

    fn resume_splay(
        &mut self,
        continuation: SplayContinuation,
        result: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        match continuation {
            SplayContinuation::LeftLeft { root, left } => {
                self.store.node_mut(left)?.left = Some(result);
                let rotated = self.rotate_right(root, trace)?;
                if self.store.node(rotated)?.left.is_some() {
                    self.rotate_right(rotated, trace)
                } else {
                    Ok(rotated)
                }
            }
            SplayContinuation::LeftRight { root, left } => {
                self.store.node_mut(left)?.right = Some(result);
                if self.store.node(left)?.right.is_some() {
                    let child = self.rotate_left(left, trace)?;
                    self.store.node_mut(root)?.left = Some(child);
                }
                if self.store.node(root)?.left.is_some() {
                    self.rotate_right(root, trace)
                } else {
                    Ok(root)
                }
            }
            SplayContinuation::RightRight { root, right } => {
                self.store.node_mut(right)?.right = Some(result);
                let rotated = self.rotate_left(root, trace)?;
                if self.store.node(rotated)?.right.is_some() {
                    self.rotate_left(rotated, trace)
                } else {
                    Ok(rotated)
                }
            }
            SplayContinuation::RightLeft { root, right } => {
                self.store.node_mut(right)?.left = Some(result);
                if self.store.node(right)?.left.is_some() {
                    let child = self.rotate_right(right, trace)?;
                    self.store.node_mut(root)?.right = Some(child);
                }
                if self.store.node(root)?.right.is_some() {
                    self.rotate_left(root, trace)
                } else {
                    Ok(root)
                }
            }
        }
    }

    fn splay_root(&mut self, key: u64, trace: &mut TraceTarget<'_>) -> Result<(), MapError> {
        if let Some(root) = self.store.root {
            self.store.root = Some(self.splay(root, key, trace)?);
        }
        Ok(())
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.splay_root(key, trace)?;
        if let Some(root) = self.store.root
            && self.store.node(root)?.key == key
        {
            let entry = self.store.node(root)?.entry;
            let previous = self.store.overwrite(entry, value)?;
            let after = self.store.project_entry(entry)?;
            trace.transition(
                Self::event(
                    EVENT_OVERWRITE,
                    TraceKind::Overwrite,
                    Some(root),
                    Some(entry),
                    Some(key),
                ),
                move |state| binary_trace::entry_change(state, after),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let (entry, node) = self.store.allocate(key, value, ())?;
        let previous_root = self.store.root;
        if let Some(root) = previous_root {
            if key < self.store.node(root)?.key {
                let left = self.store.node(root)?.left;
                self.store.node_mut(node)?.left = left;
                self.store.node_mut(node)?.right = Some(root);
                self.store.node_mut(root)?.left = None;
            } else {
                let right = self.store.node(root)?.right;
                self.store.node_mut(node)?.right = right;
                self.store.node_mut(node)?.left = Some(root);
                self.store.node_mut(root)?.right = None;
            }
        }
        self.store.root = Some(node);
        let node_after = self.store.project_node(node, |()| Vec::new())?;
        let entry_after = self.store.project_entry(entry)?;
        let previous_root_after = previous_root
            .map(|root| self.store.project_node(root, |()| Vec::new()))
            .transpose()?;
        trace.transition(
            Self::event(
                EVENT_INSERT,
                TraceKind::Insert,
                Some(node),
                Some(entry),
                Some(key),
            ),
            move |state| {
                binary_trace::insertion(
                    state,
                    Some(node),
                    previous_root_after,
                    node_after,
                    entry_after,
                )
            },
        )?;
        Ok(OperationResult::Inserted { entry })
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.splay_root(key, trace)?;
        let Some(root) = self.store.root else {
            return Ok(OperationResult::Miss);
        };
        if self.store.node(root)?.key != key {
            return Ok(OperationResult::Miss);
        }
        let entry = self.store.node(root)?.entry;
        let left = self.store.node(root)?.left;
        let right = self.store.node(root)?.right;
        let (new_root, joined_node, join_key) = if let Some(left) = left {
            let mut maximum = left;
            while let Some(next) = self.store.node(maximum)?.right {
                maximum = next;
            }
            let maximum_key = self.store.node(maximum)?.key;
            self.store.node_mut(maximum)?.right = right;
            (
                Some(left),
                Some(self.store.project_node(maximum, |()| Vec::new())?),
                Some(maximum_key),
            )
        } else {
            (right, None, None)
        };
        self.store.free_node(root)?;
        let value = self.store.free_entry(key, entry)?;
        self.store.root = new_root;
        trace.transition(
            Self::event(
                EVENT_REMOVE,
                TraceKind::Remove,
                Some(root),
                Some(entry),
                Some(key),
            ),
            move |state| {
                binary_trace::removal(
                    state,
                    root,
                    entry,
                    binary_trace::RootUpdate::Set(new_root),
                    joined_node.into_iter().collect(),
                )
            },
        )?;
        if let Some(join_key) = join_key {
            self.splay_root(join_key, trace)?;
        }
        Ok(OperationResult::Removed { entry, value })
    }

    fn find_lower_bound(&self, key: u64) -> Result<Option<u64>, MapError> {
        let mut cursor = self.store.root;
        let mut candidate = None;
        while let Some(id) = cursor {
            let node = self.store.node(id)?;
            if node.key >= key {
                candidate = Some(node.key);
                cursor = node.left;
            } else {
                cursor = node.right;
            }
        }
        Ok(candidate)
    }

    fn query(
        &mut self,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let target = if lower_bound {
            self.find_lower_bound(key)?.unwrap_or(key)
        } else {
            key
        };
        self.splay_root(target, trace)?;
        let result = if let Some(root) = self.store.root {
            let root_key = self.store.node(root)?.key;
            let is_match = if lower_bound {
                root_key >= key
            } else {
                root_key == key
            };
            if is_match {
                self.store.found_result(self.store.node(root)?.entry)?
            } else {
                OperationResult::Miss
            }
        } else {
            OperationResult::Miss
        };
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
            return Err(InvariantViolation {
                code: "SPLAY_CYCLE",
            });
        }
        let node = self.store.nodes.get(id.0).ok_or(InvariantViolation {
            code: "SPLAY_DANGLING_NODE",
        })?;
        if minimum.is_some_and(|bound| node.key <= bound)
            || maximum.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation {
                code: "SPLAY_ORDER",
            });
        }
        if !entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "SPLAY_DUPLICATE_ENTRY",
            });
        }
        let entry = self
            .store
            .entries
            .get(node.entry.0)
            .ok_or(InvariantViolation {
                code: "SPLAY_DANGLING_ENTRY",
            })?;
        if entry.key != node.key {
            return Err(InvariantViolation {
                code: "SPLAY_ENTRY_KEY",
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

impl OrderedMap for SplayMap {
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
        self.store.structure_snapshot(|()| Vec::new())
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
            return Err(InvariantViolation {
                code: "SPLAY_COUNT",
            });
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.store.estimated_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{BinaryTopology, binary_topology};
    use crate::{StructureEntityId, TraceState};

    fn root_key(map: &SplayMap) -> Option<u64> {
        map.store
            .root
            .and_then(|root| map.store.nodes.get(root.0))
            .map(|node| node.key)
    }

    fn three_node_fixture(root: u64, left: u64, grandchild: u64, zig_zag: bool) -> SplayMap {
        let mut map = SplayMap::new();
        let (_, root_node) = map
            .store
            .allocate(root, root.to_string(), ())
            .expect("fixture root allocation succeeds");
        let (_, left_node) = map
            .store
            .allocate(left, left.to_string(), ())
            .expect("fixture child allocation succeeds");
        let (_, grandchild_node) = map
            .store
            .allocate(grandchild, grandchild.to_string(), ())
            .expect("fixture grandchild allocation succeeds");
        map.store.root = Some(root_node);
        map.store
            .node_mut(root_node)
            .expect("fixture root exists")
            .left = Some(left_node);
        if zig_zag {
            map.store
                .node_mut(left_node)
                .expect("fixture child exists")
                .right = Some(grandchild_node);
        } else {
            map.store
                .node_mut(left_node)
                .expect("fixture child exists")
                .left = Some(grandchild_node);
        }
        map.check_invariants()
            .expect("fixture is a valid Splay tree");
        map
    }

    fn recorded_rotation_topologies(
        map: &mut SplayMap,
        key: u64,
    ) -> Vec<(TraceKind, BinaryTopology)> {
        let before_structure = map.structure_snapshot();
        let before_canonical = map.canonical_snapshot();
        let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
            .expect("base state is valid");

        let result = map
            .apply_traced(Operation::Get { key }, &mut recorder)
            .expect("traced query succeeds");
        assert!(matches!(result, OperationResult::Found { key: found, .. } if found == key));
        recorder
            .verify_final(&map.structure_snapshot(), &map.canonical_snapshot())
            .expect("trace reaches independent final state");
        let (events, patches) = recorder.into_parts();
        let mut replay = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("base state replays");
        let mut rotations = Vec::new();
        for event in &events {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            replay
                .apply_forward(&patches[start..end])
                .expect("event patch applies");
            if matches!(event.kind, TraceKind::RotateLeft | TraceKind::RotateRight) {
                rotations.push((event.kind, binary_topology(&replay.structure_snapshot())));
            }
        }
        for event in events.iter().rev() {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            replay
                .apply_reverse(&patches[start..end])
                .expect("event patch reverses");
        }
        assert_eq!(replay.structure_snapshot(), before_structure);
        assert_eq!(replay.canonical_snapshot(), before_canonical);
        rotations
    }

    #[test]
    fn traced_zig_zig_and_zig_zag_expose_each_intermediate_topology() {
        let mut same_direction = three_node_fixture(3, 2, 1, false);
        assert_eq!(
            recorded_rotation_topologies(&mut same_direction, 1),
            vec![
                (
                    TraceKind::RotateRight,
                    (
                        2,
                        vec![(2, "left".to_owned(), 1), (2, "right".to_owned(), 3)]
                    )
                ),
                (
                    TraceKind::RotateRight,
                    (
                        1,
                        vec![(1, "right".to_owned(), 2), (2, "right".to_owned(), 3)]
                    )
                ),
            ]
        );

        let mut opposite_direction = three_node_fixture(3, 1, 2, true);
        assert_eq!(
            recorded_rotation_topologies(&mut opposite_direction, 2),
            vec![
                (
                    TraceKind::RotateLeft,
                    (
                        3,
                        vec![(2, "left".to_owned(), 1), (3, "left".to_owned(), 2)]
                    )
                ),
                (
                    TraceKind::RotateRight,
                    (
                        2,
                        vec![(2, "left".to_owned(), 1), (2, "right".to_owned(), 3)]
                    )
                ),
            ]
        );
    }

    #[test]
    fn hit_miss_and_lower_bound_splay_the_specified_node() {
        let mut map = SplayMap::new();
        for key in [10, 5, 20, 15, 30] {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .unwrap();
        }
        map.apply(Operation::Get { key: 15 }, &mut Vec::new())
            .unwrap();
        assert_eq!(root_key(&map), Some(15));
        map.apply(Operation::Get { key: 17 }, &mut Vec::new())
            .unwrap();
        assert!(matches!(root_key(&map), Some(15 | 20)));
        map.apply(Operation::LowerBound { key: 17 }, &mut Vec::new())
            .unwrap();
        assert_eq!(root_key(&map), Some(20));
        map.apply(Operation::LowerBound { key: 99 }, &mut Vec::new())
            .unwrap();
        assert_eq!(root_key(&map), Some(30));
        map.apply(Operation::Remove { key: 10 }, &mut Vec::new())
            .unwrap();
        map.check_invariants().unwrap();
    }

    #[test]
    fn accepted_degenerate_tree_splays_without_using_the_call_stack() {
        let mut map = SplayMap::new();
        for key in 0..10_000_u64 {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .expect("accepted initial entry inserts");
        }

        let mut trace = Vec::new();
        let result = map
            .apply(Operation::Get { key: 0 }, &mut trace)
            .expect("deepest accepted key splays");

        assert!(matches!(result, OperationResult::Found { key: 0, .. }));
        assert_eq!(root_key(&map), Some(0));
        assert_eq!(map.canonical_snapshot().entries.len(), 10_000);
        assert!(
            trace
                .iter()
                .any(|event| event.kind == TraceKind::RotateRight)
        );
    }

    #[test]
    fn removal_is_visible_before_join_rotations() {
        let keys = [0_u64, 3, 6, 9, 12, 15, 2, 5, 8, 11, 14, 1, 4, 7, 10, 13];
        let mut map = SplayMap::new();
        for key in keys {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .expect("fixture insert succeeds");
        }
        let removed_node = map.store.by_key.get(&3).expect("key 3 exists").1;
        let removed_entry = map.store.node(removed_node).expect("node 3 exists").entry;
        let before_structure = map.structure_snapshot();
        let before_canonical = map.canonical_snapshot();
        let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
            .expect("base state is valid");

        map.apply_traced(Operation::Remove { key: 3 }, &mut recorder)
            .expect("traced removal succeeds");
        recorder
            .verify_final(&map.structure_snapshot(), &map.canonical_snapshot())
            .expect("trace reaches independent final state");
        let (events, patches) = recorder.into_parts();
        let mut replay = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("base state replays");
        let mut removal_seen = false;
        let mut post_remove_roots = Vec::new();
        for event in &events {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            replay
                .apply_forward(&patches[start..end])
                .expect("event patch applies");
            if event.kind == TraceKind::Remove {
                removal_seen = true;
                assert!(
                    replay.node(StructureEntityId::Node(removed_node)).is_none(),
                    "the removed node must disappear at the remove event"
                );
                assert!(
                    replay.entry(removed_entry).is_none(),
                    "the removed entry must disappear at the remove event"
                );
            }
            if removal_seen && matches!(event.kind, TraceKind::Remove | TraceKind::RotateLeft) {
                let root_key = replay
                    .root()
                    .and_then(|root| replay.node(root))
                    .and_then(|node| node.keys.first())
                    .copied()
                    .expect("nonempty fixture has a keyed root");
                post_remove_roots.push((event.kind, root_key));
            }
        }

        assert_eq!(
            post_remove_roots,
            vec![(TraceKind::Remove, 1), (TraceKind::RotateLeft, 2)]
        );
        for event in events.iter().rev() {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            replay
                .apply_reverse(&patches[start..end])
                .expect("event patch reverses");
        }
        assert_eq!(replay.structure_snapshot(), before_structure);
        assert_eq!(replay.canonical_snapshot(), before_canonical);
    }
}
