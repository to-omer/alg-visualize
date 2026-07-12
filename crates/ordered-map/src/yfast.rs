//! Y-fast trie with an X-fast representative index and Treap buckets.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::ops::Bound::{Excluded, Unbounded};

use visualizer_core::arena::GenerationalArena;
use visualizer_core::rng::RngV1;

use crate::model::{
    AuxiliaryId, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError,
    MetricOrdinal, Metrics, NodeId, Operation, OperationResult, OrderedMap, StructureEntityId,
    StructureLink, StructureNode, StructureSnapshot, TraceEvent, TraceKind,
};
use crate::trace_state::TraceTarget;
use crate::xfast::XFastMap;
use crate::{OrderedMapTraceRecorder, binary_trace};

const COMPARE: u32 = 1101;
const INSERT: u32 = 1102;
const OVERWRITE: u32 = 1103;
const REMOVE: u32 = 1104;
const ROTATE_LEFT: u32 = 1105;
const ROTATE_RIGHT: u32 = 1106;
const REPRESENTATIVE: u32 = 1107;
const RESULT: u32 = 1108;
const DESCEND: u32 = 1109;

#[derive(Clone, Debug)]
struct EntryRecord {
    key: u64,
    value: String,
}

#[derive(Clone, Debug)]
struct BucketNode {
    entry: EntryId,
    key: u64,
    priority: u64,
    left: Option<NodeId>,
    right: Option<NodeId>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Bucket {
    root: Option<NodeId>,
    len: u64,
}

/// Y-fast trie with randomly sampled representatives and extended Treap buckets.
#[derive(Clone, Debug)]
pub struct YFastMap {
    index: XFastMap,
    buckets: BTreeMap<u64, Bucket>,
    nodes: GenerationalArena<BucketNode>,
    entries: GenerationalArena<EntryRecord>,
    by_key: BTreeMap<u64, (EntryId, NodeId)>,
    priority_rng: RngV1,
    representative_rng: RngV1,
    word_bits: u8,
    sentinel: u64,
    metrics: Metrics,
    dirty_nodes: BTreeSet<NodeId>,
    dirty_entries: BTreeSet<EntryId>,
    dirty_representatives: BTreeSet<u64>,
}

impl YFastMap {
    /// Creates an empty Y-fast trie with its structural maximum representative.
    ///
    /// # Errors
    ///
    /// Rejects an invalid word width or representative-index allocation failure.
    pub fn new(seed: u64, word_bits: u8) -> Result<Self, MapError> {
        if !(1..=64).contains(&word_bits) {
            return Err(MapError::InvalidConfiguration("Y-fast word_bits"));
        }
        let sentinel = if word_bits == 64 {
            u64::MAX
        } else {
            (1_u64 << word_bits) - 1
        };
        let mut index = XFastMap::with_hash_domains(
            seed,
            word_bits,
            "hash.algorithm.y-fast.k0",
            "hash.algorithm.y-fast.k1",
        )?;
        index.index_insert(sentinel, &mut Vec::new())?;
        let _ = index.take_structure_delta();
        let mut buckets = BTreeMap::new();
        buckets.insert(sentinel, Bucket::default());
        Ok(Self {
            index,
            buckets,
            nodes: GenerationalArena::new(),
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            priority_rng: RngV1::from_seed(seed, "rng.algorithm.y-fast.bucket-priority"),
            representative_rng: RngV1::from_seed(seed, "rng.algorithm.y-fast.representative"),
            word_bits,
            sentinel,
            metrics: Metrics::default(),
            dirty_nodes: BTreeSet::new(),
            dirty_entries: BTreeSet::new(),
            dirty_representatives: BTreeSet::new(),
        })
    }

    /// Words consumed by bucket priorities.
    pub const fn priority_draws(&self) -> u64 {
        self.priority_rng.draws()
    }

    /// Words consumed by exact representative samples.
    pub const fn representative_draws(&self) -> u64 {
        self.representative_rng.draws()
    }

    fn validate_key(&self, key: u64) -> Result<(), MapError> {
        if key > self.sentinel {
            return Err(MapError::InvalidConfiguration(
                "key exceeds Y-fast universe",
            ));
        }
        Ok(())
    }

    fn node(&self, id: NodeId) -> Result<&BucketNode, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling Y-fast bucket link"))
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut BucketNode, MapError> {
        self.dirty_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling Y-fast bucket link"))
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

    fn project_event(
        &mut self,
        trace: &mut TraceTarget<'_>,
        event: TraceEvent,
    ) -> Result<(), MapError> {
        if !trace.records_patches() {
            let _ = self.index.take_structure_delta();
            self.dirty_nodes.clear();
            self.dirty_entries.clear();
            self.dirty_representatives.clear();
            return trace.record(event);
        }
        let index_delta = self.index.take_structure_delta();
        let mut nodes_after = BTreeMap::new();
        for (id, node) in index_delta.nodes {
            nodes_after.insert(
                Self::index_identity(id),
                node.map(|node| self.project_index_node(node)),
            );
        }
        for representative in std::mem::take(&mut self.dirty_representatives) {
            if let Some(node) = self.index.projected_leaf(representative) {
                let node = self.project_index_node(node);
                nodes_after.insert(node.id, Some(node));
            }
        }
        for id in std::mem::take(&mut self.dirty_nodes) {
            nodes_after.insert(
                StructureEntityId::Node(id),
                self.nodes
                    .get(id.0)
                    .map(|_| self.project_bucket_node(id))
                    .transpose()?,
            );
        }
        let entries_after = std::mem::take(&mut self.dirty_entries)
            .into_iter()
            .map(|id| {
                let after = self.entries.get(id.0).map(|entry| CanonicalEntry {
                    id,
                    key: entry.key,
                    value: entry.value.clone(),
                });
                (id, after)
            })
            .collect();
        let root_after = index_delta.root.map(Self::index_identity);
        let metrics_after = self.combined_metrics();
        trace.transition(event, move |state| {
            state.diff_selected(
                root_after,
                nodes_after.into_iter().collect(),
                entries_after,
                metrics_after,
            )
        })
    }

    const fn index_identity(id: StructureEntityId) -> StructureEntityId {
        match id {
            StructureEntityId::Node(node) => StructureEntityId::Auxiliary(AuxiliaryId(node.0)),
            StructureEntityId::Auxiliary(auxiliary) => StructureEntityId::Auxiliary(auxiliary),
        }
    }

    fn project_index_node(&self, mut node: StructureNode) -> StructureNode {
        node.id = Self::index_identity(node.id);
        node.role = format!("yfast-representative-{}", node.role);
        node.entries.clear();
        for link in &mut node.links {
            link.target = Self::index_identity(link.target);
        }
        if node.role.ends_with("xfast-leaf")
            && let Some(representative) = node.keys.first()
            && let Some(root) = self
                .buckets
                .get(representative)
                .and_then(|bucket| bucket.root)
        {
            node.links.push(StructureLink {
                slot: 2,
                role: "bucket".to_owned(),
                target: StructureEntityId::Node(root),
            });
        }
        node
    }

    fn append_index_trace(
        trace: &mut TraceTarget<'_>,
        mut index_trace: Vec<TraceEvent>,
    ) -> Result<(), MapError> {
        for event in &mut index_trace {
            event.catalog_id = event.catalog_id.saturating_add(100);
            event.node = event.node.map(Self::index_identity);
            event.target = event.target.map(Self::index_identity);
            event.entry = None;
            event.patch_start = 0;
            event.patch_count = 0;
            if event.kind == TraceKind::Descend {
                trace.transition(event.clone(), |state| {
                    binary_trace::metric_increments(
                        state,
                        &[(MetricOrdinal::NodeVisits, 1), (MetricOrdinal::BitTests, 1)],
                    )
                })?;
            }
        }
        Ok(())
    }

    fn representative_for(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<u64, MapError> {
        let mut index_trace = Vec::new();
        let representative =
            self.index
                .index_lower_bound(key, &mut index_trace)?
                .ok_or(MapError::Corrupt(
                    "Y-fast sentinel representative is missing",
                ))?;
        Self::append_index_trace(trace, index_trace)?;
        Ok(representative)
    }

    fn higher(&self, first: NodeId, second: NodeId) -> Result<bool, MapError> {
        let first = self.node(first)?;
        let second = self.node(second)?;
        Ok(first.priority > second.priority
            || (first.priority == second.priority && first.key < second.key))
    }

    fn project_bucket_node(&self, id: NodeId) -> Result<StructureNode, MapError> {
        let node = self.node(id)?;
        Ok(StructureNode {
            id: StructureEntityId::Node(id),
            role: "yfast-bucket-node".to_owned(),
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
            metadata: vec![("priority".to_owned(), node.priority)],
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
        let entry = self.node(node)?.entry;
        trace.transition(
            Self::event(
                COMPARE,
                TraceKind::Compare,
                Some(node),
                Some(entry),
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

    fn rotate_left(
        &mut self,
        root: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let pivot = self.node(root)?.right.ok_or(MapError::Corrupt(
            "Y-fast left rotation requires right child",
        ))?;
        let middle = self.node(pivot)?.left;
        self.node_mut(root)?.right = middle;
        self.node_mut(pivot)?.left = Some(root);
        self.metrics.rotations += 1;
        let pivot_record = self.node(pivot)?;
        let event = Self::event(
            ROTATE_LEFT,
            TraceKind::RotateLeft,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self.project_bucket_node(root)?;
        let after_pivot = self.project_bucket_node(pivot)?;
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
        let pivot = self.node(root)?.left.ok_or(MapError::Corrupt(
            "Y-fast right rotation requires left child",
        ))?;
        let middle = self.node(pivot)?.right;
        self.node_mut(root)?.left = middle;
        self.node_mut(pivot)?.right = Some(root);
        self.metrics.rotations += 1;
        let pivot_record = self.node(pivot)?;
        let event = Self::event(
            ROTATE_RIGHT,
            TraceKind::RotateRight,
            Some(root),
            Some(pivot_record.entry),
            Some(pivot_record.key),
        );
        let after_root = self.project_bucket_node(root)?;
        let after_pivot = self.project_bucket_node(pivot)?;
        trace.transition(event, move |state| {
            binary_trace::rotation(state, root, pivot, after_root, after_pivot)
        })?;
        Ok(pivot)
    }

    fn insert_node(
        &mut self,
        root: NodeId,
        inserted: NodeId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<NodeId, MapError> {
        let key = self.node(inserted)?.key;
        self.compare(key, root, trace)?;
        if key < self.node(root)?.key {
            let child = if let Some(left) = self.node(root)?.left {
                Self::descend(trace, root, left, self.node(root)?.entry, key)?;
                self.insert_node(left, inserted, trace)?
            } else {
                self.node_mut(root)?.left = Some(inserted);
                self.record_insert(trace, inserted)?;
                inserted
            };
            self.node_mut(root)?.left = Some(child);
            if self.higher(child, root)? {
                return self.rotate_right(root, trace);
            }
        } else {
            let child = if let Some(right) = self.node(root)?.right {
                Self::descend(trace, root, right, self.node(root)?.entry, key)?;
                self.insert_node(right, inserted, trace)?
            } else {
                self.node_mut(root)?.right = Some(inserted);
                self.record_insert(trace, inserted)?;
                inserted
            };
            self.node_mut(root)?.right = Some(child);
            if self.higher(child, root)? {
                return self.rotate_left(root, trace);
            }
        }
        Ok(root)
    }

    fn record_insert(&mut self, trace: &mut TraceTarget<'_>, node: NodeId) -> Result<(), MapError> {
        let record = self.node(node)?;
        self.project_event(
            trace,
            Self::event(
                INSERT,
                TraceKind::Insert,
                Some(node),
                Some(record.entry),
                Some(record.key),
            ),
        )
    }

    fn split(
        &mut self,
        root: Option<NodeId>,
        key: u64,
    ) -> Result<(Option<NodeId>, Option<NodeId>), MapError> {
        let Some(root) = root else {
            return Ok((None, None));
        };
        if self.node(root)?.key <= key {
            let right = self.node(root)?.right;
            let (left_part, right_part) = self.split(right, key)?;
            self.node_mut(root)?.right = left_part;
            Ok((Some(root), right_part))
        } else {
            let left = self.node(root)?.left;
            let (left_part, right_part) = self.split(left, key)?;
            self.node_mut(root)?.left = right_part;
            Ok((left_part, Some(root)))
        }
    }

    fn merge(
        &mut self,
        left: Option<NodeId>,
        right: Option<NodeId>,
    ) -> Result<Option<NodeId>, MapError> {
        match (left, right) {
            (None, tree) | (tree, None) => Ok(tree),
            (Some(left), Some(right)) if self.higher(left, right)? => {
                let merged = self.merge(self.node(left)?.right, Some(right))?;
                self.node_mut(left)?.right = merged;
                Ok(Some(left))
            }
            (Some(left), Some(right)) => {
                let merged = self.merge(Some(left), self.node(right)?.left)?;
                self.node_mut(right)?.left = merged;
                Ok(Some(right))
            }
        }
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
        match key.cmp(&self.node(root)?.key) {
            std::cmp::Ordering::Less => {
                let entry = self.node(root)?.entry;
                if let Some(left) = self.node(root)?.left {
                    Self::descend(trace, root, left, entry, key)?;
                }
                let (left, entry) = self.remove_node(self.node(root)?.left, key, trace)?;
                self.node_mut(root)?.left = left;
                Ok((Some(root), entry))
            }
            std::cmp::Ordering::Greater => {
                let entry = self.node(root)?.entry;
                if let Some(right) = self.node(root)?.right {
                    Self::descend(trace, root, right, entry, key)?;
                }
                let (right, entry) = self.remove_node(self.node(root)?.right, key, trace)?;
                self.node_mut(root)?.right = right;
                Ok((Some(root), entry))
            }
            std::cmp::Ordering::Equal => {
                let node = self.node(root)?;
                let (entry, left, right) = (node.entry, node.left, node.right);
                let merged = self.merge(left, right)?;
                self.nodes
                    .remove(root.0)
                    .ok_or(MapError::Corrupt("Y-fast bucket node disappeared"))?;
                self.metrics.frees += 1;
                self.dirty_nodes.insert(root);
                Ok((merged, Some(entry)))
            }
        }
    }

    fn find_in_bucket(
        &mut self,
        root: Option<NodeId>,
        key: u64,
        lower_bound: bool,
        trace: &mut TraceTarget<'_>,
    ) -> Result<Option<EntryId>, MapError> {
        let mut cursor = root;
        let mut candidate = None;
        while let Some(node) = cursor {
            self.compare(key, node, trace)?;
            let record = self.node(node)?;
            let (entry, next) = match key.cmp(&record.key) {
                std::cmp::Ordering::Equal => return Ok(Some(record.entry)),
                std::cmp::Ordering::Less => {
                    if lower_bound {
                        candidate = Some(record.entry);
                    }
                    (record.entry, record.left)
                }
                std::cmp::Ordering::Greater => (record.entry, record.right),
            };
            if let Some(target) = next {
                Self::descend(trace, node, target, entry, key)?;
            }
            cursor = next;
        }
        Ok(candidate)
    }

    fn split_representative_bucket(
        &mut self,
        representative: u64,
        key: u64,
        node: NodeId,
        entry: EntryId,
        trace: &mut TraceTarget<'_>,
    ) -> Result<(), MapError> {
        let bucket = self.buckets[&representative];
        let (left, right) = self.split(bucket.root, key)?;
        let left_len = self.count_nodes(left)?;
        self.buckets.insert(
            key,
            Bucket {
                root: left,
                len: left_len,
            },
        );
        self.buckets.insert(
            representative,
            Bucket {
                root: right,
                len: bucket.len - left_len,
            },
        );
        self.dirty_representatives.insert(key);
        self.dirty_representatives.insert(representative);
        self.project_event(
            trace,
            Self::event(
                REPRESENTATIVE,
                TraceKind::Split,
                Some(node),
                Some(entry),
                Some(key),
            ),
        )
    }

    fn insert(
        &mut self,
        key: u64,
        value: String,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.validate_key(key)?;
        if let Some((entry, node)) = self.by_key.get(&key).copied() {
            let record = self
                .entries
                .get_mut(entry.0)
                .ok_or(MapError::Corrupt("Y-fast entry disappeared"))?;
            let previous = std::mem::replace(&mut record.value, value);
            self.dirty_entries.insert(entry);
            self.project_event(
                trace,
                Self::event(
                    OVERWRITE,
                    TraceKind::Overwrite,
                    Some(node),
                    Some(entry),
                    Some(key),
                ),
            )?;
            return Ok(OperationResult::Overwritten { entry, previous });
        }
        let representative = self.representative_for(key, trace)?;
        let priority = self.priority_rng.next_u64();
        let sampled = self
            .representative_rng
            .bounded_u64(u64::from(self.word_bits))?
            == 0;
        let creates_representative = sampled && key != self.sentinel;
        self.entries.try_reserve(1)?;
        self.nodes.try_reserve(1)?;
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        let node = match self.nodes.try_insert(BucketNode {
            entry,
            key,
            priority,
            left: None,
            right: None,
        }) {
            Ok(node) => NodeId(node),
            Err(error) => {
                self.entries.remove(entry.0);
                return Err(error.into());
            }
        };
        self.dirty_entries.insert(entry);
        self.dirty_nodes.insert(node);
        let bucket = self
            .buckets
            .get(&representative)
            .copied()
            .ok_or(MapError::Corrupt("representative bucket is missing"))?;
        self.by_key.insert(key, (entry, node));
        self.metrics.allocations += 2;
        let root = if let Some(root) = bucket.root {
            self.insert_node(root, node, trace)?
        } else {
            self.buckets.insert(
                representative,
                Bucket {
                    root: Some(node),
                    len: bucket.len + 1,
                },
            );
            self.dirty_representatives.insert(representative);
            self.record_insert(trace, node)?;
            node
        };
        self.buckets.insert(
            representative,
            Bucket {
                root: Some(root),
                len: bucket.len + 1,
            },
        );
        if creates_representative {
            let mut index_trace = Vec::new();
            self.index.index_insert(key, &mut index_trace)?;
            Self::append_index_trace(trace, index_trace)?;
            self.split_representative_bucket(representative, key, node, entry, trace)?;
        }
        Ok(OperationResult::Inserted { entry })
    }

    fn count_nodes(&self, root: Option<NodeId>) -> Result<u64, MapError> {
        let Some(root) = root else {
            return Ok(0);
        };
        self.count_nodes(self.node(root)?.left)?
            .checked_add(self.count_nodes(self.node(root)?.right)?)
            .and_then(|count| count.checked_add(1))
            .ok_or(MapError::ArithmeticOverflow)
    }

    fn remove(
        &mut self,
        key: u64,
        trace: &mut TraceTarget<'_>,
    ) -> Result<OperationResult, MapError> {
        self.validate_key(key)?;
        let Some((entry, node)) = self.by_key.get(&key).copied() else {
            return Ok(OperationResult::Miss);
        };
        let representative = self.representative_for(key, trace)?;
        let bucket = self.buckets[&representative];
        let (root, removed) = self.remove_node(bucket.root, key, trace)?;
        if removed != Some(entry) {
            return Err(MapError::Corrupt("Y-fast removal identity changed"));
        }
        self.buckets.insert(
            representative,
            Bucket {
                root,
                len: bucket.len - 1,
            },
        );
        self.by_key.remove(&key);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("Y-fast entry disappeared before free"))?;
        self.dirty_entries.insert(entry);
        self.metrics.frees += 1;
        self.dirty_representatives.insert(representative);
        self.project_event(
            trace,
            Self::event(
                REMOVE,
                TraceKind::Remove,
                Some(node),
                Some(entry),
                Some(key),
            ),
        )?;
        if representative == key && key != self.sentinel {
            let successor = self
                .buckets
                .range((Excluded(key), Unbounded))
                .next()
                .map(|(key, _)| *key)
                .ok_or(MapError::Corrupt("normal representative has no successor"))?;
            let left = self.buckets.remove(&key).ok_or(MapError::Corrupt(
                "removed representative bucket is missing",
            ))?;
            let right = self.buckets[&successor];
            let root = self.merge(left.root, right.root)?;
            self.buckets.insert(
                successor,
                Bucket {
                    root,
                    len: left.len + right.len,
                },
            );
            let mut index_trace = Vec::new();
            self.index.index_remove(key, &mut index_trace)?;
            Self::append_index_trace(trace, index_trace)?;
            self.dirty_representatives.insert(successor);
            self.project_event(
                trace,
                Self::event(
                    REPRESENTATIVE,
                    TraceKind::Merge,
                    None,
                    Some(entry),
                    Some(key),
                ),
            )?;
        }
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
        let representative = self.representative_for(key, trace)?;
        let bucket = self.buckets[&representative];
        let entry = self.find_in_bucket(bucket.root, key, lower_bound, trace)?;
        let result = entry.map_or(Ok(OperationResult::Miss), |entry| {
            let record = self
                .entries
                .get(entry.0)
                .ok_or(MapError::Corrupt("Y-fast result entry disappeared"))?;
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

    fn validate_bucket(
        &self,
        root: Option<NodeId>,
        bounds: (Option<u64>, Option<u64>),
        nodes: &mut BTreeSet<NodeId>,
        entries: &mut BTreeSet<EntryId>,
    ) -> Result<u64, InvariantViolation> {
        let Some(root) = root else {
            return Ok(0);
        };
        if !nodes.insert(root) {
            return Err(InvariantViolation {
                code: "YFAST_CYCLE",
            });
        }
        let node = self.nodes.get(root.0).ok_or(InvariantViolation {
            code: "YFAST_DANGLING_NODE",
        })?;
        if bounds.0.is_some_and(|bound| node.key <= bound)
            || bounds.1.is_some_and(|bound| node.key >= bound)
            || !entries.insert(node.entry)
        {
            return Err(InvariantViolation {
                code: "YFAST_ORDER",
            });
        }
        for child in [node.left, node.right].into_iter().flatten() {
            let child = self.nodes.get(child.0).ok_or(InvariantViolation {
                code: "YFAST_DANGLING_CHILD",
            })?;
            if child.priority > node.priority
                || (child.priority == node.priority && child.key < node.key)
            {
                return Err(InvariantViolation { code: "YFAST_HEAP" });
            }
        }
        let left = self.validate_bucket(node.left, (bounds.0, Some(node.key)), nodes, entries)?;
        let right = self.validate_bucket(node.right, (Some(node.key), bounds.1), nodes, entries)?;
        Ok(left + right + 1)
    }

    fn combined_metrics(&self) -> Metrics {
        let index = self.index.absolute_metrics();
        Metrics {
            comparisons: self.metrics.comparisons + index.comparisons,
            node_visits: self.metrics.node_visits + index.node_visits,
            bit_tests: self.metrics.bit_tests + index.bit_tests,
            rotations: self.metrics.rotations + index.rotations,
            recolors: self.metrics.recolors + index.recolors,
            splits: self.metrics.splits + index.splits,
            merges: self.metrics.merges + index.merges,
            rebuild_items: self.metrics.rebuild_items + index.rebuild_items,
            allocations: self.metrics.allocations + index.allocations,
            frees: self.metrics.frees + index.frees,
        }
    }
}

#[path = "yfast_ordered_map.rs"]
mod yfast_ordered_map;

#[cfg(test)]
include!("yfast_tests.rs");
