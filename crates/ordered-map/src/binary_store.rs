//! Shared stable storage for binary ordered-map variants.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use visualizer_core::arena::GenerationalArena;

use crate::model::{
    CanonicalEntry, CanonicalSnapshot, EntryId, MapError, Metrics, NodeId, OperationResult,
    StructureEntityId, StructureLink, StructureNode, StructureSnapshot,
};

#[derive(Clone, Debug)]
pub(crate) struct EntryRecord {
    pub(crate) key: u64,
    pub(crate) value: String,
}

#[derive(Clone, Debug)]
pub(crate) struct BinaryNode<M> {
    pub(crate) entry: EntryId,
    pub(crate) key: u64,
    pub(crate) left: Option<NodeId>,
    pub(crate) right: Option<NodeId>,
    pub(crate) metadata: M,
}

#[derive(Clone, Debug)]
pub(crate) struct BinaryStore<M> {
    pub(crate) root: Option<NodeId>,
    pub(crate) nodes: GenerationalArena<BinaryNode<M>>,
    pub(crate) entries: GenerationalArena<EntryRecord>,
    pub(crate) by_key: BTreeMap<u64, (EntryId, NodeId)>,
    pub(crate) metrics: Metrics,
    changed_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
}

impl<M> BinaryStore<M> {
    pub(crate) const fn new() -> Self {
        Self {
            root: None,
            nodes: GenerationalArena::new(),
            entries: GenerationalArena::new(),
            by_key: BTreeMap::new(),
            metrics: Metrics {
                comparisons: 0,
                node_visits: 0,
                bit_tests: 0,
                rotations: 0,
                recolors: 0,
                splits: 0,
                merges: 0,
                rebuild_items: 0,
                allocations: 0,
                frees: 0,
            },
            changed_nodes: BTreeSet::new(),
            removed_nodes: BTreeSet::new(),
        }
    }

    pub(crate) fn node(&self, id: NodeId) -> Result<&BinaryNode<M>, MapError> {
        self.nodes
            .get(id.0)
            .ok_or(MapError::Corrupt("dangling binary node link"))
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> Result<&mut BinaryNode<M>, MapError> {
        self.changed_nodes.insert(id);
        self.nodes
            .get_mut(id.0)
            .ok_or(MapError::Corrupt("dangling binary node link"))
    }

    pub(crate) fn allocate(
        &mut self,
        key: u64,
        value: String,
        metadata: M,
    ) -> Result<(EntryId, NodeId), MapError> {
        let entry = EntryId(self.entries.try_insert(EntryRecord { key, value })?);
        let node = BinaryNode {
            entry,
            key,
            left: None,
            right: None,
            metadata,
        };
        let node = match self.nodes.try_insert(node) {
            Ok(id) => NodeId(id),
            Err(error) => {
                self.entries.remove(entry.0);
                return Err(error.into());
            }
        };
        self.metrics.allocations += 2;
        self.by_key.insert(key, (entry, node));
        Ok((entry, node))
    }

    pub(crate) fn overwrite(&mut self, entry: EntryId, value: String) -> Result<String, MapError> {
        let record = self
            .entries
            .get_mut(entry.0)
            .ok_or(MapError::Corrupt("binary node references missing entry"))?;
        Ok(std::mem::replace(&mut record.value, value))
    }

    pub(crate) fn free_node(&mut self, node: NodeId) -> Result<(), MapError> {
        self.nodes
            .remove(node.0)
            .ok_or(MapError::Corrupt("binary node disappeared before free"))?;
        self.metrics.frees += 1;
        self.changed_nodes.remove(&node);
        self.removed_nodes.insert(node);
        Ok(())
    }

    pub(crate) fn clear_projection_changes(&mut self) {
        self.changed_nodes.clear();
        self.removed_nodes.clear();
    }

    pub(crate) fn take_projection_changes(&mut self) -> (BTreeSet<NodeId>, BTreeSet<NodeId>) {
        (
            std::mem::take(&mut self.changed_nodes),
            std::mem::take(&mut self.removed_nodes),
        )
    }

    pub(crate) fn take_projected_nodes(
        &mut self,
        metadata: impl Fn(&M) -> Vec<(String, u64)>,
    ) -> Result<Vec<(StructureEntityId, Option<StructureNode>)>, MapError> {
        let (changed, removed) = self.take_projection_changes();
        let mut projected = Vec::with_capacity(changed.len().saturating_add(removed.len()));
        for id in changed {
            projected.push((
                StructureEntityId::Node(id),
                Some(self.project_node(id, &metadata)?),
            ));
        }
        projected.extend(
            removed
                .into_iter()
                .map(|id| (StructureEntityId::Node(id), None)),
        );
        Ok(projected)
    }

    pub(crate) fn free_entry(&mut self, key: u64, entry: EntryId) -> Result<String, MapError> {
        self.by_key.remove(&key);
        let record = self
            .entries
            .remove(entry.0)
            .ok_or(MapError::Corrupt("binary entry disappeared before free"))?;
        self.metrics.frees += 1;
        Ok(record.value)
    }

    pub(crate) fn found_result(&self, entry: EntryId) -> Result<OperationResult, MapError> {
        let record = self
            .entries
            .get(entry.0)
            .ok_or(MapError::Corrupt("binary node references missing entry"))?;
        Ok(OperationResult::Found {
            entry,
            key: record.key,
            value: record.value.clone(),
        })
    }

    pub(crate) fn canonical_snapshot(&self) -> CanonicalSnapshot {
        let entries = self
            .by_key
            .iter()
            .filter_map(|(key, (id, _))| {
                self.entries.get(id.0).map(|record| CanonicalEntry {
                    id: *id,
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

    pub(crate) fn project_entry(&self, id: EntryId) -> Result<CanonicalEntry, MapError> {
        let entry = self
            .entries
            .get(id.0)
            .ok_or(MapError::Corrupt("binary node references missing entry"))?;
        Ok(CanonicalEntry {
            id,
            key: entry.key,
            value: entry.value.clone(),
        })
    }

    pub(crate) fn project_node(
        &self,
        id: NodeId,
        metadata: impl Fn(&M) -> Vec<(String, u64)>,
    ) -> Result<StructureNode, MapError> {
        let node = self.node(id)?;
        Ok(StructureNode {
            id: StructureEntityId::Node(id),
            role: "binary-node".to_owned(),
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
            metadata: metadata(&node.metadata),
        })
    }

    pub(crate) fn structure_snapshot(
        &self,
        metadata: impl Fn(&M) -> Vec<(String, u64)>,
    ) -> StructureSnapshot {
        let nodes = self
            .nodes
            .iter()
            .map(|(id, _)| {
                self.project_node(NodeId(id), &metadata)
                    .expect("arena iteration yields a present binary node")
            })
            .collect();
        StructureSnapshot {
            root: self.root.map(StructureEntityId::Node),
            nodes,
        }
    }

    pub(crate) fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    pub(crate) fn estimated_bytes(&self) -> usize {
        self.nodes
            .estimated_bytes()
            .saturating_add(self.entries.estimated_bytes())
            .saturating_add(
                self.by_key
                    .len()
                    .saturating_mul(size_of::<(u64, (EntryId, NodeId))>()),
            )
            .saturating_add(
                self.entries
                    .iter()
                    .map(|(_, entry)| entry.value.capacity())
                    .sum::<usize>(),
            )
    }
}
