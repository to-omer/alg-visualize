//! Pugh ordered Skip list with deterministic promotion draws.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use visualizer_core::arena::GenerationalArena;
use visualizer_core::rng::RngV1;

use crate::OrderedMapTraceRecorder;
use crate::binary_trace;
use crate::model::{
    AuxiliaryId, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError,
    MetricOrdinal, Metrics, NodeId, Operation, OperationResult, OrderedMap, StructureEntityId,
    StructureLink, StructureNode, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const COMPARE: u32 = 701;
const DESCEND: u32 = 702;
const INSERT: u32 = 703;
const OVERWRITE: u32 = 704;
const REMOVE: u32 = 705;
const RESULT: u32 = 706;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Debug)]
struct SkipNode {
    entry: Option<EntryId>,
    key: Option<u64>,
    forward: Vec<Option<NodeId>>,
}

/// Ordered Skip list with a structural head sentinel.
#[derive(Clone, Debug)]
pub struct SkipListMap {
    nodes: GenerationalArena<SkipNode>,
    entries: GenerationalArena<EntryRecord>,
    by_key: BTreeMap<u64, (EntryId, NodeId)>,
    dirty_nodes: BTreeSet<NodeId>,
    dirty_entries: BTreeSet<EntryId>,
    head: NodeId,
    max_level: u8,
    promotion_denominator: u64,
    rng: RngV1,
    metrics: Metrics,
}

impl SkipListMap {
    /// Creates an empty Skip list.
    ///
    /// # Errors
    ///
    /// Rejects levels outside `1..=64`, promotion denominators other than two
    /// or four, and arena allocation failure.
    pub fn new(seed: u64, max_level: u8, promotion_denominator: u64) -> Result<Self, MapError> {
        if !(1..=64).contains(&max_level) || !matches!(promotion_denominator, 2 | 4) {
            return Err(MapError::InvalidConfiguration("skip-list configuration"));
        }
        let mut nodes = GenerationalArena::new();
        let head = NodeId(nodes.try_insert(SkipNode {
            entry: None,
            key: None,
            forward: vec![None; usize::from(max_level)],
        })?);
        Ok(Self {
            nodes,
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            dirty_nodes: BTreeSet::new(),
            dirty_entries: BTreeSet::new(),
            head,
            max_level,
            promotion_denominator,
            rng: RngV1::from_seed(seed, "rng.algorithm.skip-list.height"),
            metrics: Metrics {
                allocations: 1,
                ..Metrics::default()
            },
        })
    }

    /// Number of random words consumed by promotion trials.
    pub const fn rng_draws(&self) -> u64 {
        self.rng.draws()
    }

    fn node(&self, id: NodeId) -> Result<&SkipNode, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling skip-list link"))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut SkipNode, MapError> {
        self.dirty_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling skip-list link"))
    }

    fn event(
        &self,
        catalog_id: u32,
        kind: TraceKind,
        node: Option<NodeId>,
        entry: Option<EntryId>,
        key: Option<u64>,
    ) -> TraceEvent {
        TraceEvent {
            catalog_id,
            kind,
            node: node.map(|node| self.structure_id(node)),
            target: None,
            entry,
            key,
            patch_start: 0,
            patch_count: 0,
        }
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
            .map(|id| (self.structure_id(id), self.project_node(id)))
            .collect();
        let entries_after = entry_ids
            .into_iter()
            .map(|id| (id, self.project_entry(id)))
            .collect();
        let root_after = Some(self.structure_id(self.head));
        let metrics_after = self.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, entries_after, metrics_after)
        })
    }

    fn structure_id(&self, id: NodeId) -> StructureEntityId {
        if id == self.head {
            StructureEntityId::Auxiliary(AuxiliaryId(id.0))
        } else {
            StructureEntityId::Node(id)
        }
    }

    fn project_node(&self, id: NodeId) -> Option<StructureNode> {
        self.nodes.get(id.0).map(|node| StructureNode {
            id: self.structure_id(id),
            role: if node.entry.is_some() {
                "skip-node".to_owned()
            } else {
                "head-sentinel".to_owned()
            },
            entries: node.entry.into_iter().collect(),
            keys: node.key.into_iter().collect(),
            links: node
                .forward
                .iter()
                .enumerate()
                .filter_map(|(level, target)| {
                    target.map(|target| StructureLink {
                        slot: u32::try_from(level).unwrap_or(u32::MAX),
                        role: format!("level-{level}"),
                        target: StructureEntityId::Node(target),
                    })
                })
                .collect(),
            metadata: vec![(
                "height".to_owned(),
                u64::try_from(node.forward.len()).unwrap_or(u64::MAX),
            )],
        })
    }

    fn project_entry(&self, id: EntryId) -> Option<CanonicalEntry> {
        let record = self.entries.get(id.0)?;
        self.by_key
            .get(&record.key)
            .is_some_and(|(entry, _)| *entry == id)
            .then(|| CanonicalEntry {
                id,
                key: record.key,
                value: record.value.clone(),
            })
    }

    fn compare(
        &mut self,
        key: u64,
        node: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.metrics.comparisons += 1;
        self.metrics.node_visits += 1;
        trace.transition(
            self.event(
                COMPARE,
                TraceKind::Compare,
                Some(node),
                self.nodes.get(node.0).and_then(|record| record.entry),
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

    fn predecessors(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(Vec<NodeId>, Option<NodeId>), MapError> {
        let mut update = vec![self.head; usize::from(self.max_level)];
        let mut current = self.head;
        for level in (0..usize::from(self.max_level)).rev() {
            loop {
                let next = self.node(current)?.forward.get(level).copied().flatten();
                let Some(next) = next else {
                    break;
                };
                self.compare(key, next, trace)?;
                let next_key = self
                    .node(next)?
                    .key
                    .ok_or(MapError::Corrupt("head sentinel appears as target"))?;
                if next_key >= key {
                    break;
                }
                trace.record(
                    self.event(
                        DESCEND,
                        TraceKind::Descend,
                        Some(current),
                        self.node(next)?.entry,
                        Some(key),
                    )
                    .with_target(Some(self.structure_id(next))),
                )?;
                current = next;
            }
            update[level] = current;
        }
        let candidate = self.node(update[0])?.forward[0];
        Ok((update, candidate))
    }

    fn found_result(&self, entry: EntryId) -> Result<OperationResult, MapError> {
        let record = self
            .entries
            .get(entry.0)
            .ok_or(MapError::Corrupt("skip-list node references missing entry"))?;
        Ok(OperationResult::Found {
            entry,
            key: record.key,
            value: record.value.clone(),
        })
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let (update, candidate) = self.predecessors(key, trace)?;
        if let Some(candidate) = candidate
            && self.node(candidate)?.key == Some(key)
        {
            let entry = self
                .node(candidate)?
                .entry
                .ok_or(MapError::Corrupt("skip-list value node has no entry"))?;
            self.dirty_entries.insert(entry);
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("skip-list entry disappeared"))?;
            let previous = std::mem::replace(&mut record.value, value);
            self.project_event(
                trace,
                self.event(
                    OVERWRITE,
                    TraceKind::Overwrite,
                    Some(candidate),
                    Some(entry),
                    Some(key),
                ),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let height = self
            .rng
            .skip_list_height(self.max_level, self.promotion_denominator)?;
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        self.dirty_entries.insert(entry);
        let node = match self.nodes.try_insert(SkipNode {
            entry: Some(entry),
            key: Some(key),
            forward: vec![None; usize::from(height)],
        }) {
            Ok(id) => {
                let node = NodeId(id);
                self.dirty_nodes.insert(node);
                node
            }
            Err(error) => {
                self.entries.remove(entry.0);
                return Err(error.into());
            }
        };
        for (level, predecessor) in update.iter().copied().enumerate().take(usize::from(height)) {
            let next = self.node(predecessor)?.forward[level];
            self.node_mut(node)?.forward[level] = next;
            self.node_mut(predecessor)?.forward[level] = Some(node);
        }
        self.by_key.insert(key, (entry, node));
        self.dirty_entries.insert(entry);
        self.metrics.allocations += 2;
        self.project_event(
            trace,
            self.event(
                INSERT,
                TraceKind::Insert,
                Some(node),
                Some(entry),
                Some(key),
            ),
        )?;
        Ok(OperationResult::Inserted { entry })
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        let (update, candidate) = self.predecessors(key, trace)?;
        let Some(node) = candidate else {
            return Ok(OperationResult::Miss);
        };
        if self.node(node)?.key != Some(key) {
            return Ok(OperationResult::Miss);
        }
        let entry = self
            .node(node)?
            .entry
            .ok_or(MapError::Corrupt("skip-list value node has no entry"))?;
        let height = self.node(node)?.forward.len();
        for (level, predecessor) in update.iter().copied().enumerate().take(height) {
            if self.node(predecessor)?.forward[level] == Some(node) {
                let next = self.node(node)?.forward[level];
                self.node_mut(predecessor)?.forward[level] = next;
            }
        }
        self.nodes
            .remove(node.0)
            .ok_or(MapError::Corrupt("skip-list node disappeared before free"))?;
        self.dirty_nodes.insert(node);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("skip-list entry disappeared before free"))?;
        self.dirty_entries.insert(entry);
        self.by_key.remove(&key);
        self.metrics.frees += 2;
        self.project_event(
            trace,
            self.event(
                REMOVE,
                TraceKind::Remove,
                Some(node),
                Some(entry),
                Some(key),
            ),
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
        let (_, candidate) = self.predecessors(key, trace)?;
        let entry = candidate.and_then(|node| {
            let record = self.nodes.get(node.0)?;
            if lower_bound || record.key == Some(key) {
                record.entry
            } else {
                None
            }
        });
        let result = entry.map_or(Ok(OperationResult::Miss), |entry| self.found_result(entry))?;
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
        trace.record(self.event(RESULT, TraceKind::Result, None, None, Some(key)))?;
        Ok(result)
    }

    pub(crate) fn apply_traced(
        &mut self,
        operation: Operation,
        trace: &mut OrderedMapTraceRecorder,
    ) -> Result<OperationResult, MapError> {
        self.apply_operation(operation, &mut TraceTarget::Recorder(trace))
    }
}

impl OrderedMap for SkipListMap {
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
            .filter_map(|(key, (entry, _))| {
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
            root: Some(StructureEntityId::Auxiliary(AuxiliaryId(self.head.0))),
            nodes,
        }
    }

    fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let head = self.nodes.get(self.head.0).ok_or(InvariantViolation {
            code: "SKIP_HEAD_MISSING",
        })?;
        if head.entry.is_some()
            || head.key.is_some()
            || head.forward.len() != usize::from(self.max_level)
        {
            return Err(InvariantViolation {
                code: "SKIP_HEAD_INVALID",
            });
        }
        let mut base_nodes = BTreeSet::new();
        for level in 0..usize::from(self.max_level) {
            let mut seen = BTreeSet::new();
            let mut current = self.head;
            let mut previous = None;
            loop {
                if !seen.insert(current) {
                    return Err(InvariantViolation { code: "SKIP_CYCLE" });
                }
                let Some(next) = self
                    .nodes
                    .get(current.0)
                    .and_then(|node| node.forward.get(level))
                    .copied()
                    .flatten()
                else {
                    break;
                };
                let node = self.nodes.get(next.0).ok_or(InvariantViolation {
                    code: "SKIP_DANGLING_LINK",
                })?;
                let key = node.key.ok_or(InvariantViolation {
                    code: "SKIP_SENTINEL_TARGET",
                })?;
                if previous.is_some_and(|previous| key <= previous) || node.forward.len() <= level {
                    return Err(InvariantViolation {
                        code: "SKIP_LEVEL_ORDER",
                    });
                }
                if level == 0 {
                    base_nodes.insert(next);
                }
                previous = Some(key);
                current = next;
            }
        }
        if base_nodes.len() != self.by_key.len()
            || self.nodes.len() != u32::try_from(self.by_key.len() + 1).unwrap_or(u32::MAX)
            || self.entries.len() != u32::try_from(self.by_key.len()).unwrap_or(u32::MAX)
        {
            return Err(InvariantViolation { code: "SKIP_COUNT" });
        }
        for (key, (entry, node)) in &self.by_key {
            let node_record = self.nodes.get(node.0).ok_or(InvariantViolation {
                code: "SKIP_INDEX_NODE",
            })?;
            let entry_record = self.entries.get(entry.0).ok_or(InvariantViolation {
                code: "SKIP_INDEX_ENTRY",
            })?;
            if node_record.key != Some(*key)
                || node_record.entry != Some(*entry)
                || entry_record.key != *key
                || !base_nodes.contains(node)
            {
                return Err(InvariantViolation { code: "SKIP_INDEX" });
            }
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
                        node.forward
                            .capacity()
                            .saturating_mul(size_of::<Option<NodeId>>())
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
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn mixed_updates_match_model_and_all_levels_remain_ordered() {
        let mut map = SkipListMap::new(41, 16, 2).unwrap();
        let mut model = BTreeMap::new();
        for round in 0_u64..8 {
            for key in (0_u64..128).map(|key| (key * 67 + round * 29) % 128) {
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
            for key in (0_u64..128).filter(|key| (key + round) % 4 == 0) {
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
    fn max_level_one_and_overwrite_consume_no_draws() {
        let mut map = SkipListMap::new(9, 1, 2).unwrap();
        map.apply(
            Operation::Insert {
                key: 1,
                value: "a".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(map.rng_draws(), 0);
        map.apply(
            Operation::Insert {
                key: 1,
                value: "b".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(map.rng_draws(), 0);
    }
}
