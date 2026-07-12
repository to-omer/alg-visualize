use super::{
    AvlMap, BTreeSet, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MapError,
    NodeId, Operation, OperationResult, OrderedMap, StructureEntityId, StructureLink,
    StructureNode, StructureSnapshot, TraceEvent, TraceTarget, size_of,
};

impl OrderedMap for AvlMap {
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

    fn structure_snapshot(&self) -> StructureSnapshot {
        let nodes = self
            .nodes
            .iter()
            .map(|(id, node)| StructureNode {
                id: StructureEntityId::Node(NodeId(id)),
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
                metadata: vec![("height".to_owned(), u64::from(node.height))],
            })
            .collect();
        StructureSnapshot {
            root: self.root.map(StructureEntityId::Node),
            nodes,
        }
    }

    fn structure_entity_count(&self) -> usize {
        usize::try_from(self.nodes.len()).unwrap_or(usize::MAX)
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        if let Some(root) = self.root {
            self.validate_node(root, None, None, &mut nodes, &mut entries)?;
        }
        if nodes.len() != usize::try_from(self.nodes.len()).unwrap_or(usize::MAX) {
            return Err(InvariantViolation {
                code: "AVL_UNREACHABLE_NODE",
            });
        }
        if entries.len() != usize::try_from(self.entries.len()).unwrap_or(usize::MAX)
            || entries.len() != self.by_key.len()
        {
            return Err(InvariantViolation {
                code: "AVL_ENTRY_COUNT",
            });
        }
        for (key, (entry, node)) in &self.by_key {
            let entry_record = self.entries.get(entry.0).ok_or(InvariantViolation {
                code: "AVL_INDEX_ENTRY",
            })?;
            let node_record = self.nodes.get(node.0).ok_or(InvariantViolation {
                code: "AVL_INDEX_NODE",
            })?;
            if entry_record.key != *key || node_record.key != *key || node_record.entry != *entry {
                return Err(InvariantViolation {
                    code: "AVL_INDEX_MISMATCH",
                });
            }
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
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
