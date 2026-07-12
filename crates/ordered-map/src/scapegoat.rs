//! Galperin–Rivest Scapegoat tree with exact rational depth thresholds.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use num_bigint::BigUint;

use crate::OrderedMapTraceRecorder;
use crate::binary_store::{BinaryNode, BinaryStore};
use crate::binary_trace;
use crate::model::{
    CanonicalSnapshot, EntryId, InvariantViolation, MapError, MetricOrdinal, NodeId, Operation,
    OperationResult, OrderedMap, StructureEntityId, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const COMPARE: u32 = 601;
const INSERT: u32 = 602;
const OVERWRITE: u32 = 603;
const REMOVE: u32 = 604;
const SIZE: u32 = 605;
const REBUILD: u32 = 606;
const RESULT: u32 = 607;
const DESCEND: u32 = 608;

/// Scapegoat tree parameterized by an exact reduced rational `alpha`.
#[derive(Clone, Debug)]
pub struct ScapegoatMap {
    store: BinaryStore<u64>,
    numerator: u32,
    denominator: u32,
    q: u64,
    q_min: Vec<BigUint>,
    numerator_power: BigUint,
    denominator_power: BigUint,
}

impl ScapegoatMap {
    /// Creates an empty Scapegoat tree.
    ///
    /// # Errors
    ///
    /// Rejects a non-reduced fraction, a denominator above 64, or alpha not
    /// strictly between one half and one.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, MapError> {
        if denominator > 64
            || numerator == 0
            || numerator >= denominator
            || u64::from(numerator) * 2 <= u64::from(denominator)
            || gcd(numerator, denominator) != 1
        {
            return Err(MapError::InvalidConfiguration("scapegoat alpha"));
        }
        Ok(Self {
            store: BinaryStore::new(),
            numerator,
            denominator,
            q: 0,
            q_min: vec![BigUint::from(1_u8)],
            numerator_power: BigUint::from(1_u8),
            denominator_power: BigUint::from(1_u8),
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

    fn size(&self, node: Option<NodeId>) -> Result<u64, MapError> {
        node.map_or(Ok(0), |id| Ok(self.store.node(id)?.metadata))
    }

    fn update_size(&mut self, node: NodeId, trace: &mut TraceTarget<'_>) -> Result<(), MapError> {
        let record = self.store.node(node)?;
        let expected = self
            .size(record.left)?
            .checked_add(self.size(record.right)?)
            .and_then(|size| size.checked_add(1))
            .ok_or(MapError::ArithmeticOverflow)?;
        if record.metadata != expected {
            self.store.node_mut(node)?.metadata = expected;
            let record = self.store.node(node)?;
            let after = self
                .store
                .project_node(node, |size| vec![("size".to_owned(), *size)])?;
            trace.transition(
                Self::event(
                    SIZE,
                    TraceKind::UpdateMetadata,
                    Some(node),
                    Some(record.entry),
                    Some(record.key),
                ),
                move |state| binary_trace::metadata_change(state, after),
            )?;
        }
        Ok(())
    }

    fn extend_thresholds_through(&mut self, q: u64) {
        let q = BigUint::from(q);
        while self.q_min.last().is_some_and(|threshold| threshold <= &q) {
            self.numerator_power *= self.numerator;
            self.denominator_power *= self.denominator;
            let ceiling = (&self.denominator_power + &self.numerator_power - BigUint::from(1_u8))
                / &self.numerator_power;
            self.q_min.push(ceiling);
        }
    }

    fn maximum_depth(&mut self) -> usize {
        self.extend_thresholds_through(self.q);
        let q = BigUint::from(self.q);
        self.q_min.partition_point(|threshold| threshold <= &q) - 1
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

    fn gather_in_order(
        &self,
        root: Option<NodeId>,
        output: &mut Vec<NodeId>,
    ) -> Result<(), MapError> {
        if let Some(root) = root {
            let node = self.store.node(root)?;
            self.gather_in_order(node.left, output)?;
            output.push(root);
            self.gather_in_order(node.right, output)?;
        }
        Ok(())
    }

    fn build_balanced(&mut self, nodes: &[NodeId]) -> Result<Option<NodeId>, MapError> {
        if nodes.is_empty() {
            return Ok(None);
        }
        let middle = nodes.len() / 2;
        let root = nodes[middle];
        let left = self.build_balanced(&nodes[..middle])?;
        let right = self.build_balanced(&nodes[middle + 1..])?;
        let size = u64::try_from(nodes.len()).map_err(|_| MapError::ArithmeticOverflow)?;
        let node = self.store.node_mut(root)?;
        node.left = left;
        node.right = right;
        node.metadata = size;
        Ok(Some(root))
    }

    fn rebuild(
        &mut self,
        root: NodeId,
        parent: Option<NodeId>,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let key = self.store.node(root)?.key;
        let mut nodes = Vec::new();
        self.gather_in_order(Some(root), &mut nodes)?;
        let rebuilt = self
            .build_balanced(&nodes)?
            .ok_or(MapError::Corrupt("nonempty rebuild produced no root"))?;
        if let Some(parent) = parent {
            if key < self.store.node(parent)?.key {
                self.store.node_mut(parent)?.left = Some(rebuilt);
            } else {
                self.store.node_mut(parent)?.right = Some(rebuilt);
            }
        } else {
            self.store.root = Some(rebuilt);
        }
        self.store.metrics.rebuild_items = self
            .store
            .metrics
            .rebuild_items
            .checked_add(u64::try_from(nodes.len()).map_err(|_| MapError::ArithmeticOverflow)?)
            .ok_or(MapError::ArithmeticOverflow)?;
        let rebuilt_record = self.store.node(rebuilt)?;
        let event = Self::event(
            REBUILD,
            TraceKind::Rebuild,
            Some(rebuilt),
            Some(rebuilt_record.entry),
            Some(rebuilt_record.key),
        );
        if let Some(parent) = parent {
            nodes.push(parent);
        }
        let nodes_after = nodes
            .into_iter()
            .map(|id| {
                self.store
                    .project_node(id, |size| vec![("size".to_owned(), *size)])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let root_after = self.store.root;
        let rebuild_count = u64::try_from(nodes_after.len() - usize::from(parent.is_some()))
            .map_err(|_| MapError::ArithmeticOverflow)?;
        trace.transition(event, move |state| {
            binary_trace::projection_changes(
                state,
                root_after,
                nodes_after,
                &[(MetricOrdinal::RebuildItems, rebuild_count)],
            )
        })?;
        Ok(rebuilt)
    }

    fn is_alpha_violation(&self, child: NodeId, parent: NodeId) -> Result<bool, MapError> {
        Ok(
            u128::from(self.size(Some(child))?) * u128::from(self.denominator)
                > u128::from(self.size(Some(parent))?) * u128::from(self.numerator),
        )
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let mut path = Vec::new();
        let mut cursor = self.store.root;
        while let Some(id) = cursor {
            self.compare(key, id, trace)?;
            let node = self.store.node(id)?;
            if key == node.key {
                let entry = node.entry;
                let previous = self.store.overwrite(entry, value)?;
                let after = self.store.project_entry(entry)?;
                trace.transition(
                    Self::event(
                        OVERWRITE,
                        TraceKind::Overwrite,
                        Some(id),
                        Some(entry),
                        Some(key),
                    ),
                    move |state| binary_trace::entry_change(state, after),
                )?;
                return Ok(OperationResult::Overwritten { entry, previous });
            }
            path.push(id);
            let next = if key < node.key {
                node.left
            } else {
                node.right
            };
            if let Some(target) = next {
                Self::descend(trace, id, target, node.entry, key)?;
            }
            cursor = next;
        }
        let (entry, node) = self.store.allocate(key, value, 1)?;
        if let Some(parent) = path.last().copied() {
            if key < self.store.node(parent)?.key {
                self.store.node_mut(parent)?.left = Some(node);
            } else {
                self.store.node_mut(parent)?.right = Some(node);
            }
        } else {
            self.store.root = Some(node);
        }
        self.record_insert(trace, node, path.last().copied(), self.store.root)?;
        for ancestor in path.iter().rev().copied() {
            self.update_size(ancestor, trace)?;
        }
        self.q = self.q.max(u64::from(self.store.entries.len()));
        let depth = path.len();
        if depth > self.maximum_depth() {
            let mut child = node;
            for index in (0..path.len()).rev() {
                let parent = path[index];
                if self.is_alpha_violation(child, parent)? {
                    let grandparent = index.checked_sub(1).map(|position| path[position]);
                    self.rebuild(parent, grandparent, trace)?;
                    for ancestor in path[..index].iter().rev().copied() {
                        self.update_size(ancestor, trace)?;
                    }
                    break;
                }
                child = parent;
            }
        }
        Ok(OperationResult::Inserted { entry })
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
            .project_node(node, |size| vec![("size".to_owned(), *size)])?;
        let entry_after = self.store.project_entry(self.store.node(node)?.entry)?;
        let parent_after = parent
            .map(|id| {
                self.store
                    .project_node(id, |size| vec![("size".to_owned(), *size)])
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

    fn detach_min_unbalanced(
        &mut self,
        root: NodeId,
        affected: &mut Vec<NodeId>,
    ) -> Result<(Option<NodeId>, BinaryNode<u64>), MapError> {
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
        let node = self.store.node(root)?;
        let (root_key, left, right, entry) = (node.key, node.left, node.right, node.entry);
        let removed = match key.cmp(&root_key) {
            Ordering::Less => {
                if let Some(left) = left {
                    Self::descend(trace, root, left, entry, key)?;
                }
                let (left, removed) = self.remove_node_unbalanced(left, key, trace, affected)?;
                if removed.is_none() {
                    return Ok((Some(root), None));
                }
                self.store.node_mut(root)?.left = left;
                affected.push(root);
                removed
            }
            Ordering::Greater => {
                if let Some(right) = right {
                    Self::descend(trace, root, right, entry, key)?;
                }
                let (right, removed) = self.remove_node_unbalanced(right, key, trace, affected)?;
                if removed.is_none() {
                    return Ok((Some(root), None));
                }
                self.store.node_mut(root)?.right = right;
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
        let event = Self::event(REMOVE, TraceKind::Remove, None, Some(entry), Some(key));
        if trace.records_patches() {
            let nodes_after = self
                .store
                .take_projected_nodes(|size| vec![("size".to_owned(), *size)])?;
            let root_after = self.store.root.map(crate::StructureEntityId::Node);
            let metrics_after = self.store.metrics;
            trace.transition(event, move |state| {
                state.diff_selected(root_after, nodes_after, vec![(entry, None)], metrics_after)
            })?;
        } else {
            self.store.clear_projection_changes();
            trace.record(event)?;
        }
        for node in affected {
            if self.store.nodes.get(node.0).is_some() {
                self.update_size(node, trace)?;
            }
        }
        let n = u64::from(self.store.entries.len());
        if u128::from(n) * u128::from(self.denominator)
            < u128::from(self.q) * u128::from(self.numerator)
        {
            if let Some(root) = self.store.root {
                self.rebuild(root, None, trace)?;
            }
            self.q = n;
        }
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
        nodes: &mut BTreeSet<NodeId>,
        entries: &mut BTreeSet<EntryId>,
    ) -> Result<u64, InvariantViolation> {
        if !nodes.insert(id) {
            return Err(InvariantViolation {
                code: "SCAPEGOAT_CYCLE",
            });
        }
        let node = self.store.nodes.get(id.0).ok_or(InvariantViolation {
            code: "SCAPEGOAT_DANGLING_NODE",
        })?;
        if bounds.0.is_some_and(|bound| node.key <= bound)
            || bounds.1.is_some_and(|bound| node.key >= bound)
        {
            return Err(InvariantViolation {
                code: "SCAPEGOAT_ORDER",
            });
        }
        if !entries.insert(node.entry) {
            return Err(InvariantViolation {
                code: "SCAPEGOAT_DUPLICATE_ENTRY",
            });
        }
        let left = node.left.map_or(Ok(0), |left| {
            self.validate_node(left, (bounds.0, Some(node.key)), nodes, entries)
        })?;
        let right = node.right.map_or(Ok(0), |right| {
            self.validate_node(right, (Some(node.key), bounds.1), nodes, entries)
        })?;
        let expected = left + right + 1;
        if node.metadata != expected {
            return Err(InvariantViolation {
                code: "SCAPEGOAT_SIZE",
            });
        }
        Ok(expected)
    }
}

impl OrderedMap for ScapegoatMap {
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
            .structure_snapshot(|size| vec![("size".to_owned(), *size)])
    }

    fn structure_entity_count(&self) -> usize {
        self.store.structure_entity_count()
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        if let Some(root) = self.store.root {
            self.validate_node(root, (None, None), &mut nodes, &mut entries)?;
        }
        if nodes.len() != usize::try_from(self.store.nodes.len()).unwrap_or(usize::MAX)
            || entries.len() != self.store.by_key.len()
            || self.q < u64::from(self.store.entries.len())
        {
            return Err(InvariantViolation {
                code: "SCAPEGOAT_COUNT",
            });
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.store.estimated_bytes()
            + self
                .q_min
                .iter()
                .map(|integer| integer.to_bytes_le().capacity())
                .sum::<usize>()
    }
}

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::TraceState;
    use crate::test_support::binary_topology;

    #[test]
    fn traced_rebuild_exposes_the_balanced_subtree_at_its_event_boundary() {
        let mut map = ScapegoatMap::new(2, 3).expect("fixture alpha is valid");
        for key in 0_u64..4 {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .expect("fixture insert succeeds");
        }
        let before_structure = map.structure_snapshot();
        let before_canonical = map.canonical_snapshot();
        let rebuild_items = before_canonical.metrics.rebuild_items;
        let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
            .expect("base state is valid");

        map.apply_traced(
            Operation::Insert {
                key: 4,
                value: "4".to_owned(),
            },
            &mut recorder,
        )
        .expect("traced insert succeeds");
        recorder
            .verify_final(&map.structure_snapshot(), &map.canonical_snapshot())
            .expect("rebuild trace reaches its independent final state");
        let (events, patches) = recorder.into_parts();
        let mut replay = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("base state replays");
        let mut rebuild_count = 0;
        for event in &events {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            replay
                .apply_forward(&patches[start..end])
                .expect("event patch applies");
            if event.kind == TraceKind::Rebuild {
                rebuild_count += 1;
                assert_eq!(
                    binary_topology(&replay.structure_snapshot()),
                    (
                        0,
                        vec![
                            (0, "right".to_owned(), 3),
                            (2, "left".to_owned(), 1),
                            (3, "left".to_owned(), 2),
                            (3, "right".to_owned(), 4),
                        ]
                    )
                );
                assert_eq!(
                    replay.canonical_snapshot().metrics.rebuild_items,
                    rebuild_items + 4
                );
                let metadata = replay
                    .structure_snapshot()
                    .nodes
                    .into_iter()
                    .map(|node| (node.keys[0], node.metadata))
                    .collect::<BTreeMap<_, _>>();
                assert_eq!(
                    metadata,
                    BTreeMap::from([
                        (0, vec![("size".to_owned(), 5)]),
                        (1, vec![("size".to_owned(), 1)]),
                        (2, vec![("size".to_owned(), 2)]),
                        (3, vec![("size".to_owned(), 4)]),
                        (4, vec![("size".to_owned(), 1)]),
                    ])
                );
            }
        }
        assert_eq!(rebuild_count, 1, "fixture must produce one rebuild event");
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

    #[test]
    fn monotone_updates_rebuild_and_match_model() {
        let mut map = ScapegoatMap::new(2, 3).unwrap();
        let mut model = BTreeMap::new();
        let mut trace = Vec::new();
        for key in 0_u64..256 {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut trace,
            )
            .unwrap();
            model.insert(key, key.to_string());
            map.check_invariants().unwrap();
        }
        assert!(trace.iter().any(|event| event.kind == TraceKind::Rebuild));
        for key in (0_u64..256).step_by(2) {
            map.apply(Operation::Remove { key }, &mut trace).unwrap();
            model.remove(&key);
            map.check_invariants().unwrap();
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
    fn rejects_invalid_alpha() {
        assert!(ScapegoatMap::new(1, 2).is_err());
        assert!(ScapegoatMap::new(4, 6).is_err());
        assert!(ScapegoatMap::new(64, 65).is_err());
    }
}
