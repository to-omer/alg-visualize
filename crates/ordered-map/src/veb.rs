//! Sparse, lazily materialized van Emde Boas tree.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use visualizer_core::arena::GenerationalArena;

use crate::OrderedMapTraceRecorder;
use crate::binary_trace;
use crate::model::{
    AuxiliaryId, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError,
    MetricOrdinal, Metrics, NodeId, Operation, OperationResult, OrderedMap, StructureEntityId,
    StructureLink, StructureNode, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;

const VISIT: u32 = 901;
const INSERT: u32 = 902;
const OVERWRITE: u32 = 903;
const REMOVE: u32 = 904;
const MATERIALIZE: u32 = 905;
const RESULT: u32 = 906;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VebRole {
    Root,
    Summary,
    Cluster,
}

#[derive(Clone, Debug)]
struct VebNode {
    bits: u8,
    min: Option<u64>,
    max: Option<u64>,
    summary: Option<NodeId>,
    clusters: BTreeMap<u64, NodeId>,
    role: VebRole,
}

/// Sparse recursive van Emde Boas map. Empty substructures are not materialized.
#[derive(Clone, Debug)]
pub struct VebMap {
    nodes: GenerationalArena<VebNode>,
    entries: GenerationalArena<EntryRecord>,
    by_key: BTreeMap<u64, EntryId>,
    dirty_nodes: BTreeSet<NodeId>,
    dirty_entries: BTreeSet<EntryId>,
    root: NodeId,
    word_bits: u8,
    metrics: Metrics,
}

impl VebMap {
    /// Creates an empty sparse vEB map for `1..=64` key bits.
    ///
    /// # Errors
    ///
    /// Rejects an invalid word width or arena allocation failure.
    pub fn new(word_bits: u8) -> Result<Self, MapError> {
        if !(1..=64).contains(&word_bits) {
            return Err(MapError::InvalidConfiguration("vEB word_bits"));
        }
        let mut nodes = GenerationalArena::new();
        let root = NodeId(nodes.try_insert(VebNode {
            bits: word_bits,
            min: None,
            max: None,
            summary: None,
            clusters: BTreeMap::new(),
            role: VebRole::Root,
        })?);
        Ok(Self {
            nodes,
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            dirty_nodes: BTreeSet::new(),
            dirty_entries: BTreeSet::new(),
            root,
            word_bits,
            metrics: Metrics {
                allocations: 1,
                ..Metrics::default()
            },
        })
    }

    fn node(&self, id: NodeId) -> Result<&VebNode, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling vEB substructure"))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut VebNode, MapError> {
        self.dirty_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling vEB substructure"))
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
            node: node.map(Self::structure_id),
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
            .map(|id| (Self::structure_id(id), self.project_node(id)))
            .collect();
        let entries_after = entry_ids
            .into_iter()
            .map(|id| (id, self.project_entry(id)))
            .collect();
        let root_after = Some(Self::structure_id(self.root));
        let metrics_after = self.metrics;
        trace.transition(event, move |state| {
            state.diff_selected(root_after, nodes_after, entries_after, metrics_after)
        })
    }

    fn structure_id(id: NodeId) -> StructureEntityId {
        StructureEntityId::Auxiliary(AuxiliaryId(id.0))
    }

    fn project_node(&self, id: NodeId) -> Option<StructureNode> {
        self.nodes.get(id.0).map(|node| {
            let mut links =
                Vec::with_capacity(node.clusters.len() + usize::from(node.summary.is_some()));
            if let Some(summary) = node.summary {
                links.push(StructureLink {
                    slot: u32::MAX,
                    role: "summary".to_owned(),
                    target: Self::structure_id(summary),
                });
            }
            links.extend(node.clusters.iter().map(|(high, target)| StructureLink {
                slot: u32::try_from(*high).unwrap_or(u32::MAX - 1),
                role: format!("cluster-{high}"),
                target: Self::structure_id(*target),
            }));
            StructureNode {
                id: Self::structure_id(id),
                role: match node.role {
                    VebRole::Root => "veb-root",
                    VebRole::Summary => "veb-summary",
                    VebRole::Cluster => "veb-cluster",
                }
                .to_owned(),
                entries: Vec::new(),
                keys: [node.min, node.max]
                    .into_iter()
                    .flatten()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                links,
                metadata: vec![("word-bits".to_owned(), u64::from(node.bits))],
            }
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

    fn visit(
        &mut self,
        source: Option<NodeId>,
        node: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.metrics.node_visits += 1;
        self.metrics.bit_tests += 1;
        trace.transition(
            Self::event(
                VISIT,
                TraceKind::Descend,
                Some(source.unwrap_or(node)),
                self.by_key.get(&key).copied(),
                Some(key),
            )
            .with_target(source.map(|_| Self::structure_id(node))),
            |state| {
                binary_trace::metric_increments(
                    state,
                    &[(MetricOrdinal::NodeVisits, 1), (MetricOrdinal::BitTests, 1)],
                )
            },
        )
    }

    const fn low_bits(bits: u8) -> u8 {
        bits / 2
    }

    const fn high_bits(bits: u8) -> u8 {
        bits - Self::low_bits(bits)
    }

    fn split(bits: u8, key: u64) -> (u64, u64) {
        let low_bits = Self::low_bits(bits);
        let mask = (1_u64 << low_bits) - 1;
        (key >> low_bits, key & mask)
    }

    const fn combine(bits: u8, high: u64, low: u64) -> u64 {
        (high << Self::low_bits(bits)) | low
    }

    fn validate_key(&self, key: u64) -> Result<(), MapError> {
        if self.word_bits < 64 && key >= (1_u64 << self.word_bits) {
            return Err(MapError::InvalidConfiguration("key exceeds vEB universe"));
        }
        Ok(())
    }

    fn allocate_node(&mut self, bits: u8, role: VebRole) -> Result<NodeId, MapError> {
        let node = NodeId(self.nodes.try_insert(VebNode {
            bits,
            min: None,
            max: None,
            summary: None,
            clusters: BTreeMap::new(),
            role,
        })?);
        self.dirty_nodes.insert(node);
        self.metrics.allocations += 1;
        Ok(node)
    }

    fn empty_insert(&mut self, node: NodeId, key: u64) -> Result<(), MapError> {
        let record = self.node_mut(node)?;
        record.min = Some(key);
        record.max = Some(key);
        Ok(())
    }

    fn insert_set(
        &mut self,
        source: Option<NodeId>,
        node: NodeId,
        mut key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.visit(source, node, key, trace)?;
        let (bits, minimum, maximum) = {
            let record = self.node(node)?;
            (record.bits, record.min, record.max)
        };
        let Some(mut minimum) = minimum else {
            return self.empty_insert(node, key);
        };
        if key < minimum {
            std::mem::swap(&mut key, &mut minimum);
            self.node_mut(node)?.min = Some(minimum);
        }
        if bits > 1 && key != minimum {
            let (high, low) = Self::split(bits, key);
            let cluster = if let Some(cluster) = self.node(node)?.clusters.get(&high).copied() {
                cluster
            } else {
                let summary = if let Some(summary) = self.node(node)?.summary {
                    summary
                } else {
                    let summary = self.allocate_node(Self::high_bits(bits), VebRole::Summary)?;
                    self.node_mut(node)?.summary = Some(summary);
                    self.project_event(
                        trace,
                        Self::event(MATERIALIZE, TraceKind::Insert, Some(summary), None, None),
                    )?;
                    summary
                };
                self.insert_set(Some(node), summary, high, trace)?;
                let cluster = self.allocate_node(Self::low_bits(bits), VebRole::Cluster)?;
                self.node_mut(node)?.clusters.insert(high, cluster);
                self.empty_insert(cluster, low)?;
                self.project_event(
                    trace,
                    Self::event(
                        MATERIALIZE,
                        TraceKind::Insert,
                        Some(cluster),
                        None,
                        Some(key),
                    ),
                )?;
                cluster
            };
            if self.node(cluster)?.min.is_none() {
                self.empty_insert(cluster, low)?;
            } else {
                self.insert_set(Some(node), cluster, low, trace)?;
            }
        }
        if maximum.is_none_or(|maximum| key > maximum) {
            self.node_mut(node)?.max = Some(key);
        }
        Ok(())
    }

    fn contains_set(
        &mut self,
        source: Option<NodeId>,
        node: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<bool, MapError> {
        self.visit(source, node, key, trace)?;
        let record = self.node(node)?;
        if record.min == Some(key) || record.max == Some(key) {
            return Ok(true);
        }
        if record.bits <= 1 {
            return Ok(false);
        }
        let (high, low) = Self::split(record.bits, key);
        let cluster = record.clusters.get(&high).copied();
        cluster.map_or(Ok(false), |cluster| {
            self.contains_set(Some(node), cluster, low, trace)
        })
    }

    fn successor_set(
        &mut self,
        source: Option<NodeId>,
        node: NodeId,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<u64>, MapError> {
        self.visit(source, node, key, trace)?;
        let record = self.node(node)?;
        let (bits, minimum, maximum, summary) =
            (record.bits, record.min, record.max, record.summary);
        if bits <= 1 {
            return Ok((key == 0 && maximum == Some(1)).then_some(1));
        }
        if minimum.is_some_and(|minimum| key < minimum) {
            return Ok(minimum);
        }
        let (high, low) = Self::split(bits, key);
        if let Some(cluster) = self.node(node)?.clusters.get(&high).copied()
            && self.node(cluster)?.max.is_some_and(|maximum| low < maximum)
        {
            let successor = self
                .successor_set(Some(node), cluster, low, trace)?
                .ok_or(MapError::Corrupt("cluster maximum promised a successor"))?;
            return Ok(Some(Self::combine(bits, high, successor)));
        }
        let Some(summary) = summary else {
            return Ok(None);
        };
        let Some(next_high) = self.successor_set(Some(node), summary, high, trace)? else {
            return Ok(None);
        };
        let cluster = *self
            .node(node)?
            .clusters
            .get(&next_high)
            .ok_or(MapError::Corrupt("summary references missing cluster"))?;
        let low = self
            .node(cluster)?
            .min
            .ok_or(MapError::Corrupt("summary references empty cluster"))?;
        Ok(Some(Self::combine(bits, next_high, low)))
    }

    fn remove_empty_node(&mut self, node: NodeId) -> Result<(), MapError> {
        let record = self.node(node)?;
        if record.min.is_some() || record.summary.is_some() || !record.clusters.is_empty() {
            return Err(MapError::Corrupt("attempted to free nonempty vEB node"));
        }
        self.nodes
            .remove(node.0)
            .ok_or(MapError::Corrupt("vEB node disappeared before free"))?;
        self.dirty_nodes.insert(node);
        self.metrics.frees += 1;
        Ok(())
    }

    fn remove_set(
        &mut self,
        source: Option<NodeId>,
        node: NodeId,
        mut key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        self.visit(source, node, key, trace)?;
        let record = self.node(node)?;
        let (bits, minimum, maximum) = (record.bits, record.min, record.max);
        if minimum == maximum {
            let record = self.node_mut(node)?;
            record.min = None;
            record.max = None;
            return Ok(());
        }
        if bits <= 1 {
            let remaining = u64::from(key == 0);
            let record = self.node_mut(node)?;
            record.min = Some(remaining);
            record.max = Some(remaining);
            return Ok(());
        }
        if minimum == Some(key) {
            let summary = self
                .node(node)?
                .summary
                .ok_or(MapError::Corrupt("nontrivial vEB node has no summary"))?;
            let first_high = self
                .node(summary)?
                .min
                .ok_or(MapError::Corrupt("vEB summary is unexpectedly empty"))?;
            let cluster = *self
                .node(node)?
                .clusters
                .get(&first_high)
                .ok_or(MapError::Corrupt("summary minimum cluster is missing"))?;
            let first_low = self
                .node(cluster)?
                .min
                .ok_or(MapError::Corrupt("summary minimum cluster is empty"))?;
            key = Self::combine(bits, first_high, first_low);
            self.node_mut(node)?.min = Some(key);
        }
        let (high, low) = Self::split(bits, key);
        let cluster = *self
            .node(node)?
            .clusters
            .get(&high)
            .ok_or(MapError::Corrupt("vEB key cluster is missing"))?;
        self.remove_set(Some(node), cluster, low, trace)?;
        if self.node(cluster)?.min.is_none() {
            let summary = self
                .node(node)?
                .summary
                .ok_or(MapError::Corrupt("empty cluster has no summary"))?;
            self.remove_set(Some(node), summary, high, trace)?;
            self.node_mut(node)?.clusters.remove(&high);
            self.remove_empty_node(cluster)?;
            if self.node(summary)?.min.is_none() {
                self.node_mut(node)?.summary = None;
                self.remove_empty_node(summary)?;
            }
            if maximum == Some(key) {
                let new_max = if let Some(summary) = self.node(node)?.summary {
                    let high = self
                        .node(summary)?
                        .max
                        .ok_or(MapError::Corrupt("nonempty summary has no maximum"))?;
                    let cluster = self.node(node)?.clusters[&high];
                    let low = self
                        .node(cluster)?
                        .max
                        .ok_or(MapError::Corrupt("nonempty cluster has no maximum"))?;
                    Self::combine(bits, high, low)
                } else {
                    self.node(node)?
                        .min
                        .ok_or(MapError::Corrupt("vEB lost its remaining minimum"))?
                };
                self.node_mut(node)?.max = Some(new_max);
            }
        } else if maximum == Some(key) {
            let low = self
                .node(cluster)?
                .max
                .ok_or(MapError::Corrupt("nonempty cluster has no maximum"))?;
            self.node_mut(node)?.max = Some(Self::combine(bits, high, low));
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
        if let Some(entry) = self.by_key.get(&key).copied() {
            self.dirty_entries.insert(entry);
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("vEB entry disappeared"))?;
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
        self.insert_set(None, self.root, key, trace)?;
        self.by_key.insert(key, entry);
        self.dirty_entries.insert(entry);
        self.project_event(
            trace,
            Self::event(
                INSERT,
                TraceKind::Insert,
                Some(self.root),
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
        let Some(entry) = self.by_key.get(&key).copied() else {
            return Ok(OperationResult::Miss);
        };
        self.remove_set(None, self.root, key, trace)?;
        self.by_key.remove(&key);
        self.dirty_entries.insert(entry);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("removed vEB entry disappeared"))?;
        self.metrics.frees += 1;
        self.project_event(
            trace,
            Self::event(
                REMOVE,
                TraceKind::Remove,
                Some(self.root),
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
        let found = if self.contains_set(None, self.root, key, trace)? {
            Some(key)
        } else if lower_bound {
            self.successor_set(None, self.root, key, trace)?
        } else {
            None
        };
        let entry = found.and_then(|found| self.by_key.get(&found).copied());
        let result = entry.map_or(Ok::<_, MapError>(OperationResult::Miss), |entry| {
            let record = self
                .entries
                .get(entry.0)
                .ok_or(MapError::Corrupt("vEB result entry disappeared"))?;
            Ok(OperationResult::Found {
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

    fn collect_set(
        &self,
        node: NodeId,
        seen: &mut BTreeSet<NodeId>,
    ) -> Result<BTreeSet<u64>, InvariantViolation> {
        if !seen.insert(node) {
            return Err(InvariantViolation { code: "VEB_CYCLE" });
        }
        let record = self.nodes.get(node.0).ok_or(InvariantViolation {
            code: "VEB_DANGLING_NODE",
        })?;
        if record.min.is_none() != record.max.is_none()
            || record
                .min
                .zip(record.max)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(InvariantViolation {
                code: "VEB_MIN_MAX",
            });
        }
        if record.min.is_none() {
            if record.summary.is_some() || !record.clusters.is_empty() {
                return Err(InvariantViolation {
                    code: "VEB_EMPTY_MATERIALIZED",
                });
            }
            return Ok(BTreeSet::new());
        }
        let mut values = BTreeSet::new();
        values.insert(record.min.unwrap_or_default());
        if record.bits <= 1 {
            values.insert(record.max.unwrap_or_default());
        }
        let mut cluster_keys = BTreeSet::new();
        for (high, cluster) in &record.clusters {
            let lows = self.collect_set(*cluster, seen)?;
            if lows.is_empty() {
                return Err(InvariantViolation {
                    code: "VEB_EMPTY_CLUSTER",
                });
            }
            cluster_keys.insert(*high);
            for low in lows {
                values.insert(Self::combine(record.bits, *high, low));
            }
        }
        let summary_keys = record.summary.map_or(Ok(BTreeSet::new()), |summary| {
            self.collect_set(summary, seen)
        })?;
        if summary_keys != cluster_keys {
            return Err(InvariantViolation {
                code: "VEB_SUMMARY_CLUSTERS",
            });
        }
        if record.max.is_some_and(|maximum| !values.contains(&maximum)) {
            return Err(InvariantViolation {
                code: "VEB_MAX_NOT_MATERIALIZED",
            });
        }
        Ok(values)
    }
}

impl OrderedMap for VebMap {
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
            root: Some(StructureEntityId::Auxiliary(AuxiliaryId(self.root.0))),
            nodes,
        }
    }

    fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut seen = BTreeSet::new();
        let keys = self.collect_set(self.root, &mut seen)?;
        let expected: BTreeSet<_> = self.by_key.keys().copied().collect();
        if keys != expected
            || seen.len() != usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
            || self.entries.len() != u32::try_from(self.by_key.len()).unwrap_or(u32::MAX)
        {
            return Err(InvariantViolation {
                code: "VEB_CONTENTS",
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
                        node.clusters
                            .len()
                            .saturating_mul(size_of::<(u64, NodeId)>())
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
    use std::collections::BTreeMap as Model;

    use super::*;

    #[test]
    fn odd_splits_boundaries_and_sparse_cleanup_match_model() {
        for bits in [1, 2, 3, 7, 16] {
            let mut map = VebMap::new(bits).unwrap();
            let universe = if bits == 16 { 512 } else { 1_u64 << bits };
            let mut model = Model::new();
            for key in (0..universe).map(|key| (key * 277) % universe) {
                map.apply(
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                    &mut Vec::new(),
                )
                .unwrap();
                model.insert(key, key.to_string());
                map.check_invariants().unwrap();
            }
            for key in (0..universe).map(|key| (key * 181) % universe) {
                map.apply(Operation::Remove { key }, &mut Vec::new())
                    .unwrap();
                model.remove(&key);
                map.check_invariants().unwrap();
            }
            assert!(model.is_empty());
            assert_eq!(map.nodes.len(), 1);
        }
    }

    #[test]
    fn full_u64_boundaries_work_without_shift_overflow() {
        let mut map = VebMap::new(64).unwrap();
        for key in [0, 1, u64::MAX - 1, u64::MAX] {
            map.apply(
                Operation::Insert {
                    key,
                    value: key.to_string(),
                },
                &mut Vec::new(),
            )
            .unwrap();
        }
        let result = map
            .apply(Operation::LowerBound { key: u64::MAX - 2 }, &mut Vec::new())
            .unwrap();
        assert!(matches!(result, OperationResult::Found { key, .. } if key == u64::MAX - 1));
        map.check_invariants().unwrap();
    }
}
