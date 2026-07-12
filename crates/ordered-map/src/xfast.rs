//! X-fast trie with fixed `SipHash` prefix tables and a linked leaf list.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{BuildHasher, Hash, Hasher};
use std::mem::size_of;
use std::ops::Bound::{Excluded, Unbounded};

use hashbrown::HashMap;
use siphasher::sip::SipHasher24;
use visualizer_core::arena::GenerationalArena;
use visualizer_core::rng::derive_hash_key;

use crate::model::{
    AuxiliaryId, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation,
    MAX_VISUAL_ENTITIES, MapError, MetricOrdinal, Metrics, NodeId, Operation, OperationResult,
    OrderedMap, StructureEntityId, StructureLink, StructureNode, StructureSnapshot, TraceEvent,
    TraceKind,
};
use crate::trace_state::TraceTarget;
use crate::{OrderedMapTraceRecorder, binary_trace};

const PREFIX_PROBE: u32 = 1001;
const INSERT: u32 = 1002;
const OVERWRITE: u32 = 1003;
const REMOVE: u32 = 1004;
const RESULT: u32 = 1005;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PrefixKey {
    level: u8,
    prefix: u64,
}

impl Hash for PrefixKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&[self.level]);
        state.write(&self.prefix.to_le_bytes());
    }
}

#[derive(Clone, Debug)]
struct SipBuildHasher {
    key0: u64,
    key1: u64,
}

impl BuildHasher for SipBuildHasher {
    type Hasher = SipHasher24;

    fn build_hasher(&self) -> Self::Hasher {
        SipHasher24::new_with_keys(self.key0, self.key1)
    }
}

type PrefixTable = HashMap<PrefixKey, NodeId, SipBuildHasher>;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Debug)]
enum XNode {
    Prefix {
        level: u8,
        prefix: u64,
        count: u64,
        minimum: u64,
        maximum: u64,
    },
    Leaf {
        entry: EntryId,
        key: u64,
        previous: Option<NodeId>,
        next: Option<NodeId>,
    },
}

/// X-fast trie using every binary prefix level and a doubly-linked leaf list.
#[derive(Clone, Debug)]
pub struct XFastMap {
    nodes: GenerationalArena<XNode>,
    entries: GenerationalArena<EntryRecord>,
    by_key: BTreeMap<u64, (EntryId, NodeId)>,
    dirty_nodes: BTreeMap<NodeId, StructureEntityId>,
    dirty_entries: BTreeSet<EntryId>,
    tables: Vec<PrefixTable>,
    word_bits: u8,
    metrics: Metrics,
}

pub(crate) struct XFastStructureDelta {
    pub(crate) root: Option<StructureEntityId>,
    pub(crate) nodes: Vec<(StructureEntityId, Option<StructureNode>)>,
    entries: Vec<(EntryId, Option<CanonicalEntry>)>,
}

impl XFastMap {
    /// Creates an empty X-fast trie for `1..=64` key bits.
    ///
    /// # Errors
    ///
    /// Rejects an invalid word width.
    pub fn new(seed: u64, word_bits: u8) -> Result<Self, MapError> {
        Self::with_hash_domains(
            seed,
            word_bits,
            "hash.algorithm.x-fast.k0",
            "hash.algorithm.x-fast.k1",
        )
    }

    pub(crate) fn with_hash_domains(
        seed: u64,
        word_bits: u8,
        key0_domain: &str,
        key1_domain: &str,
    ) -> Result<Self, MapError> {
        if !(1..=64).contains(&word_bits) {
            return Err(MapError::InvalidConfiguration("X-fast word_bits"));
        }
        let hasher = SipBuildHasher {
            key0: derive_hash_key(seed, key0_domain),
            key1: derive_hash_key(seed, key1_domain),
        };
        let tables = (0..=word_bits)
            .map(|_| HashMap::with_hasher(hasher.clone()))
            .collect();
        Ok(Self {
            nodes: GenerationalArena::new(),
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            dirty_nodes: BTreeMap::new(),
            dirty_entries: BTreeSet::new(),
            tables,
            word_bits,
            metrics: Metrics::default(),
        })
    }

    pub(crate) fn index_insert(
        &mut self,
        key: u64,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<(), MapError> {
        self.insert(key, String::new(), &mut TraceTarget::Events(trace))
            .map(|_| ())
    }

    pub(crate) fn index_remove(
        &mut self,
        key: u64,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<(), MapError> {
        self.remove(key, &mut TraceTarget::Events(trace))
            .map(|_| ())
    }

    pub(crate) fn index_lower_bound(
        &mut self,
        key: u64,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<Option<u64>, MapError> {
        self.lower_bound_key(key, &mut TraceTarget::Events(trace))
    }

    pub(crate) fn index_keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.by_key.keys().copied()
    }

    pub(crate) const fn absolute_metrics(&self) -> Metrics {
        self.metrics
    }

    fn validate_key(&self, key: u64) -> Result<(), MapError> {
        if self.word_bits < 64 && key >= (1_u64 << self.word_bits) {
            return Err(MapError::InvalidConfiguration(
                "key exceeds X-fast universe",
            ));
        }
        Ok(())
    }

    const fn prefix(word_bits: u8, key: u64, level: u8) -> u64 {
        if level == 0 {
            0
        } else {
            key >> (word_bits - level)
        }
    }

    const fn prefix_key(&self, key: u64, level: u8) -> PrefixKey {
        PrefixKey {
            level,
            prefix: Self::prefix(self.word_bits, key, level),
        }
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
            return trace.record(event);
        }
        let delta = self.take_structure_delta();
        let metrics_after = self.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(delta.root, delta.nodes, delta.entries, metrics_after)
        })
    }

    pub(crate) fn take_structure_delta(&mut self) -> XFastStructureDelta {
        let node_ids = std::mem::take(&mut self.dirty_nodes);
        let nodes = node_ids
            .into_iter()
            .map(|(id, identity)| (identity, self.project_node(id)))
            .collect();
        let entries = std::mem::take(&mut self.dirty_entries)
            .into_iter()
            .map(|id| (id, self.project_entry(id)))
            .collect();
        let root = self.tables[0]
            .get(&PrefixKey {
                level: 0,
                prefix: 0,
            })
            .copied()
            .map(|root| StructureEntityId::Auxiliary(AuxiliaryId(root.0)));
        XFastStructureDelta {
            root,
            nodes,
            entries,
        }
    }

    pub(crate) fn projected_leaf(&self, key: u64) -> Option<StructureNode> {
        let (_, node) = self.by_key.get(&key)?;
        self.project_node(*node)
    }

    fn mark_node(&mut self, id: NodeId) {
        let identity = self.structure_id(id);
        self.dirty_nodes.insert(id, identity);
    }

    fn project_node(&self, id: NodeId) -> Option<StructureNode> {
        self.nodes.get(id.0).map(|node| match node {
            XNode::Prefix {
                level,
                prefix,
                count,
                minimum,
                maximum,
            } => StructureNode {
                id: StructureEntityId::Auxiliary(AuxiliaryId(id.0)),
                role: "xfast-prefix".to_owned(),
                entries: Vec::new(),
                keys: vec![*minimum, *maximum],
                links: [0_u64, 1]
                    .into_iter()
                    .filter_map(|bit| {
                        self.child_node(*level, *prefix, bit)
                            .map(|target| StructureLink {
                                slot: u32::try_from(bit).unwrap_or_default(),
                                role: format!("bit-{bit}"),
                                target: self.structure_id(target),
                            })
                    })
                    .collect(),
                metadata: vec![
                    ("level".to_owned(), u64::from(*level)),
                    ("prefix".to_owned(), *prefix),
                    ("count".to_owned(), *count),
                ],
            },
            XNode::Leaf {
                entry,
                key,
                previous,
                next,
            } => StructureNode {
                id: StructureEntityId::Node(id),
                role: "xfast-leaf".to_owned(),
                entries: vec![*entry],
                keys: vec![*key],
                links: [(0, "previous", *previous), (1, "next", *next)]
                    .into_iter()
                    .filter_map(|(slot, role, target)| {
                        target.map(|target| StructureLink {
                            slot,
                            role: role.to_owned(),
                            target: StructureEntityId::Node(target),
                        })
                    })
                    .collect(),
                metadata: Vec::new(),
            },
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

    fn probe(
        &mut self,
        level: u8,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<NodeId>, MapError> {
        self.metrics.bit_tests += 1;
        self.metrics.node_visits += 1;
        let found = self.tables[usize::from(level)]
            .get(&self.prefix_key(key, level))
            .copied();
        trace.transition(
            self.event(PREFIX_PROBE, TraceKind::Descend, found, None, Some(key)),
            |state| {
                binary_trace::metric_increments(
                    state,
                    &[(MetricOrdinal::NodeVisits, 1), (MetricOrdinal::BitTests, 1)],
                )
            },
        )?;
        Ok(found)
    }

    fn leaf(
        &self,
        node: NodeId,
    ) -> Result<(EntryId, u64, Option<NodeId>, Option<NodeId>), MapError> {
        match self
            .nodes
            .get(node.0)
            .ok_or(MapError::Corrupt("dangling X-fast node"))?
        {
            XNode::Leaf {
                entry,
                key,
                previous,
                next,
            } => Ok((*entry, *key, *previous, *next)),
            XNode::Prefix { .. } => Err(MapError::Corrupt("expected X-fast leaf")),
        }
    }

    fn prefix_record(&self, node: NodeId) -> Result<(u8, u64, u64, u64, u64), MapError> {
        match self
            .nodes
            .get(node.0)
            .ok_or(MapError::Corrupt("dangling X-fast node"))?
        {
            XNode::Prefix {
                level,
                prefix,
                count,
                minimum,
                maximum,
            } => Ok((*level, *prefix, *count, *minimum, *maximum)),
            XNode::Leaf { .. } => Err(MapError::Corrupt("expected X-fast prefix")),
        }
    }

    fn exact_leaf(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<NodeId>, MapError> {
        self.probe(self.word_bits, key, trace)
    }

    fn longest_prefix(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let mut low = 0_u8;
        let mut high = self.word_bits - 1;
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if self.probe(middle, key, trace)?.is_some() {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        self.probe(low, key, trace)?
            .ok_or(MapError::Corrupt("nonempty X-fast trie has no root prefix"))
    }

    fn lower_bound_key(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<u64>, MapError> {
        if let Some(leaf) = self.exact_leaf(key, trace)? {
            return Ok(Some(self.leaf(leaf)?.1));
        }
        if self.by_key.is_empty() {
            return Ok(None);
        }
        let prefix = self.longest_prefix(key, trace)?;
        let (level, _, _, minimum, maximum) = self.prefix_record(prefix)?;
        let next_bit = (key >> (self.word_bits - level - 1)) & 1;
        if next_bit == 0 {
            return Ok(Some(minimum));
        }
        let maximum_leaf = self.tables[usize::from(self.word_bits)]
            .get(&self.prefix_key(maximum, self.word_bits))
            .copied()
            .ok_or(MapError::Corrupt("prefix maximum leaf is missing"))?;
        self.leaf(maximum_leaf)?
            .3
            .map(|next| self.leaf(next).map(|leaf| leaf.1))
            .transpose()
    }

    fn interval(&self, level: u8, prefix: u64) -> (u64, u64) {
        let suffix = self.word_bits - level;
        if suffix == 64 {
            return (0, u64::MAX);
        }
        let start = prefix << suffix;
        let end = start | ((1_u64 << suffix) - 1);
        (start, end)
    }

    fn reserve_insert(&mut self, key: u64) -> Result<(), MapError> {
        let missing = (0..self.word_bits)
            .filter(|level| {
                !self.tables[usize::from(*level)].contains_key(&self.prefix_key(key, *level))
            })
            .count();
        let projected_count = usize::try_from(self.nodes.len())
            .unwrap_or(usize::MAX)
            .saturating_add(missing)
            .saturating_add(1);
        if projected_count > MAX_VISUAL_ENTITIES {
            return Err(MapError::ResourceLimit("visual entity count"));
        }
        self.nodes.try_reserve(missing + 1)?;
        self.entries.try_reserve(1)?;
        for level in 0..=self.word_bits {
            let table = &mut self.tables[usize::from(level)];
            if !table.contains_key(&PrefixKey {
                level,
                prefix: Self::prefix(self.word_bits, key, level),
            }) {
                table
                    .try_reserve(1)
                    .map_err(|_| MapError::AllocationFailed)?;
            }
        }
        Ok(())
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.validate_key(key)?;
        if let Some((entry, leaf)) = self.by_key.get(&key).copied() {
            self.dirty_entries.insert(entry);
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("X-fast entry disappeared"))?;
            let previous = std::mem::replace(&mut record.value, value);
            self.project_event(
                trace,
                self.event(
                    OVERWRITE,
                    TraceKind::Overwrite,
                    Some(leaf),
                    Some(entry),
                    Some(key),
                ),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        self.reserve_insert(key)?;
        let previous = self.by_key.range(..key).next_back().map(|(_, item)| item.1);
        let next = self.by_key.range(key..).next().map(|(_, item)| item.1);
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        self.dirty_entries.insert(entry);
        let leaf = NodeId(self.nodes.try_insert(XNode::Leaf {
            entry,
            key,
            previous,
            next,
        })?);
        self.dirty_nodes.insert(leaf, StructureEntityId::Node(leaf));
        if let Some(previous) = previous {
            self.mark_node(previous);
            let Some(XNode::Leaf { next, .. }) = self.nodes.get_mut(previous.0) else {
                return Err(MapError::Corrupt("X-fast predecessor is not a leaf"));
            };
            *next = Some(leaf);
        }
        if let Some(next) = next {
            self.mark_node(next);
            let Some(XNode::Leaf { previous, .. }) = self.nodes.get_mut(next.0) else {
                return Err(MapError::Corrupt("X-fast successor is not a leaf"));
            };
            *previous = Some(leaf);
        }
        let leaf_key = self.prefix_key(key, self.word_bits);
        self.tables[usize::from(self.word_bits)].insert(leaf_key, leaf);
        self.by_key.insert(key, (entry, leaf));
        self.dirty_entries.insert(entry);
        self.metrics.allocations += 2;
        for level in 0..self.word_bits {
            let prefix_key = self.prefix_key(key, level);
            if let Some(node) = self.tables[usize::from(level)].get(&prefix_key).copied() {
                self.mark_node(node);
                let Some(XNode::Prefix {
                    count,
                    minimum,
                    maximum,
                    ..
                }) = self.nodes.get_mut(node.0)
                else {
                    return Err(MapError::Corrupt("prefix table points to leaf"));
                };
                *count += 1;
                *minimum = (*minimum).min(key);
                *maximum = (*maximum).max(key);
            } else {
                let node = NodeId(self.nodes.try_insert(XNode::Prefix {
                    level,
                    prefix: prefix_key.prefix,
                    count: 1,
                    minimum: key,
                    maximum: key,
                })?);
                self.dirty_nodes
                    .insert(node, StructureEntityId::Auxiliary(AuxiliaryId(node.0)));
                self.tables[usize::from(level)].insert(prefix_key, node);
                self.metrics.allocations += 1;
            }
        }
        self.project_event(
            trace,
            self.event(
                INSERT,
                TraceKind::Insert,
                Some(leaf),
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
        self.validate_key(key)?;
        let Some((entry, leaf)) = self.by_key.remove(&key) else {
            return Ok(OperationResult::Miss);
        };
        let (_, _, previous, next) = self.leaf(leaf)?;
        if let Some(previous) = previous {
            self.mark_node(previous);
            let Some(XNode::Leaf { next: link, .. }) = self.nodes.get_mut(previous.0) else {
                return Err(MapError::Corrupt("X-fast predecessor is not a leaf"));
            };
            *link = next;
        }
        if let Some(next) = next {
            self.mark_node(next);
            let Some(XNode::Leaf { previous: link, .. }) = self.nodes.get_mut(next.0) else {
                return Err(MapError::Corrupt("X-fast successor is not a leaf"));
            };
            *link = previous;
        }
        let leaf_key = self.prefix_key(key, self.word_bits);
        self.tables[usize::from(self.word_bits)].remove(&leaf_key);
        self.nodes
            .remove(leaf.0)
            .ok_or(MapError::Corrupt("X-fast leaf disappeared before free"))?;
        self.dirty_nodes.insert(leaf, StructureEntityId::Node(leaf));
        for level in 0..self.word_bits {
            let prefix_key = self.prefix_key(key, level);
            let node = *self.tables[usize::from(level)]
                .get(&prefix_key)
                .ok_or(MapError::Corrupt("X-fast prefix disappeared"))?;
            let count = match self.nodes.get(node.0) {
                Some(XNode::Prefix { count, .. }) => *count,
                _ => return Err(MapError::Corrupt("prefix table points to leaf")),
            };
            if count == 1 {
                self.dirty_nodes
                    .insert(node, StructureEntityId::Auxiliary(AuxiliaryId(node.0)));
                self.tables[usize::from(level)].remove(&prefix_key);
                self.nodes
                    .remove(node.0)
                    .ok_or(MapError::Corrupt("X-fast prefix disappeared before free"))?;
                self.metrics.frees += 1;
            } else {
                self.mark_node(node);
                let (start, end) = self.interval(level, prefix_key.prefix);
                let minimum = *self
                    .by_key
                    .range(start..=end)
                    .next()
                    .ok_or(MapError::Corrupt("nonempty prefix has no minimum"))?
                    .0;
                let maximum = *self
                    .by_key
                    .range(start..=end)
                    .next_back()
                    .ok_or(MapError::Corrupt("nonempty prefix has no maximum"))?
                    .0;
                let Some(XNode::Prefix {
                    count,
                    minimum: stored_minimum,
                    maximum: stored_maximum,
                    ..
                }) = self.nodes.get_mut(node.0)
                else {
                    return Err(MapError::Corrupt("prefix table points to leaf"));
                };
                *count -= 1;
                *stored_minimum = minimum;
                *stored_maximum = maximum;
            }
        }
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("X-fast entry disappeared before free"))?;
        self.dirty_entries.insert(entry);
        self.metrics.frees += 2;
        self.project_event(
            trace,
            self.event(
                REMOVE,
                TraceKind::Remove,
                Some(leaf),
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
        self.validate_key(key)?;
        let found = if lower_bound {
            self.lower_bound_key(key, trace)?
        } else {
            self.exact_leaf(key, trace)?.map(|_| key)
        };
        let item = found.and_then(|key| self.by_key.get(&key).copied());
        let result = item.map_or(Ok(OperationResult::Miss), |(entry, _)| {
            let record = self
                .entries
                .get(entry.0)
                .ok_or(MapError::Corrupt("X-fast result entry disappeared"))?;
            Ok::<_, MapError>(OperationResult::Found {
                entry,
                key: record.key,
                value: record.value.clone(),
            })
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

    fn child_node(&self, level: u8, prefix: u64, bit: u64) -> Option<NodeId> {
        let child_level = level + 1;
        let key = PrefixKey {
            level: child_level,
            prefix: (prefix << 1) | bit,
        };
        self.tables[usize::from(child_level)].get(&key).copied()
    }

    fn structure_id(&self, node: NodeId) -> StructureEntityId {
        match self.nodes.get(node.0) {
            Some(XNode::Prefix { .. }) => StructureEntityId::Auxiliary(AuxiliaryId(node.0)),
            Some(XNode::Leaf { .. }) | None => StructureEntityId::Node(node),
        }
    }
}

impl OrderedMap for XFastMap {
    fn apply(
        &mut self,
        operation: Operation,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<OperationResult, MapError> {
        let result = self.apply_operation(operation, &mut TraceTarget::Events(trace));
        let _ = self.take_structure_delta();
        result
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
        let root = self.tables[0]
            .get(&PrefixKey {
                level: 0,
                prefix: 0,
            })
            .copied();
        StructureSnapshot {
            root: root.map(|root| StructureEntityId::Auxiliary(AuxiliaryId(root.0))),
            nodes,
        }
    }

    fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let expected_count = self.by_key.len();
        if self.tables[usize::from(self.word_bits)].len() != expected_count
            || self.entries.len() != u32::try_from(expected_count).unwrap_or(u32::MAX)
        {
            return Err(InvariantViolation {
                code: "XFAST_COUNT",
            });
        }
        for level in 0..self.word_bits {
            let mut expected: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
            for key in self.by_key.keys() {
                expected
                    .entry(Self::prefix(self.word_bits, *key, level))
                    .or_default()
                    .push(*key);
            }
            if self.tables[usize::from(level)].len() != expected.len() {
                return Err(InvariantViolation {
                    code: "XFAST_PREFIX_COUNT",
                });
            }
            for (prefix, keys) in expected {
                let node = self.tables[usize::from(level)]
                    .get(&PrefixKey { level, prefix })
                    .and_then(|node| self.nodes.get(node.0))
                    .ok_or(InvariantViolation {
                        code: "XFAST_PREFIX_MISSING",
                    })?;
                let XNode::Prefix {
                    count,
                    minimum,
                    maximum,
                    ..
                } = node
                else {
                    return Err(InvariantViolation {
                        code: "XFAST_PREFIX_KIND",
                    });
                };
                if *count != u64::try_from(keys.len()).unwrap_or(u64::MAX)
                    || Some(minimum) != keys.first()
                    || Some(maximum) != keys.last()
                {
                    return Err(InvariantViolation {
                        code: "XFAST_PREFIX_RANGE",
                    });
                }
            }
        }
        let mut previous = None;
        for (key, (entry, leaf)) in &self.by_key {
            let (stored_entry, stored_key, stored_previous, next) =
                self.leaf(*leaf).map_err(|_| InvariantViolation {
                    code: "XFAST_LEAF_MISSING",
                })?;
            if stored_entry != *entry || stored_key != *key || stored_previous != previous {
                return Err(InvariantViolation {
                    code: "XFAST_LEAF_LINK",
                });
            }
            if let Some(next) = next {
                let next_key = self
                    .leaf(next)
                    .map_err(|_| InvariantViolation {
                        code: "XFAST_NEXT_MISSING",
                    })?
                    .1;
                if self
                    .by_key
                    .range((Excluded(*key), Unbounded))
                    .next()
                    .map(|(key, _)| *key)
                    != Some(next_key)
                {
                    return Err(InvariantViolation {
                        code: "XFAST_NEXT_ORDER",
                    });
                }
            }
            previous = Some(*leaf);
        }
        let expected_nodes = expected_count
            + self.tables[..usize::from(self.word_bits)]
                .iter()
                .map(HashMap::len)
                .sum::<usize>();
        if usize::try_from(self.nodes.len()).unwrap_or(usize::MAX) != expected_nodes {
            return Err(InvariantViolation {
                code: "XFAST_NODE_COUNT",
            });
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.nodes
            .estimated_bytes()
            .saturating_add(self.entries.estimated_bytes())
            .saturating_add(
                self.tables
                    .iter()
                    .map(|table| {
                        table
                            .capacity()
                            .saturating_mul(size_of::<(PrefixKey, NodeId)>())
                    })
                    .sum::<usize>(),
            )
            .saturating_add(
                self.entries
                    .iter()
                    .map(|(_, entry)| entry.value.capacity())
                    .sum::<usize>(),
            )
            .saturating_add(
                self.tables
                    .len()
                    .saturating_mul(size_of::<SipBuildHasher>()),
            )
    }
}

#[cfg(test)]
include!("xfast_tests.rs");
