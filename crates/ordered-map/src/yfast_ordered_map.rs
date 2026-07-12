use super::{
    BTreeSet, Bucket, CanonicalEntry, CanonicalSnapshot, InvariantViolation, MapError, NodeId,
    Operation, OperationResult, OrderedMap, StructureEntityId, StructureLink, StructureNode,
    StructureSnapshot, TraceEvent, TraceTarget, YFastMap, size_of,
};

impl OrderedMap for YFastMap {
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
            metrics: self.combined_metrics(),
        }
    }

    fn structure_snapshot(&self) -> StructureSnapshot {
        let mut index = self.index.structure_snapshot();
        for node in &mut index.nodes {
            *node = self.project_index_node(node.clone());
        }
        index.root = index.root.map(Self::index_identity);
        index.nodes.extend(self.nodes.iter().map(|(id, node)| {
            StructureNode {
                id: StructureEntityId::Node(NodeId(id)),
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
            }
        }));
        index
    }

    fn structure_entity_count(&self) -> usize {
        self.index
            .structure_entity_count()
            .saturating_add(usize::try_from(self.nodes.len()).unwrap_or(usize::MAX))
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        let index_keys: BTreeSet<_> = self.index.index_keys().collect();
        let bucket_keys: BTreeSet<_> = self.buckets.keys().copied().collect();
        if index_keys != bucket_keys || !bucket_keys.contains(&self.sentinel) {
            return Err(InvariantViolation {
                code: "YFAST_REPRESENTATIVES",
            });
        }
        let mut nodes = BTreeSet::new();
        let mut entries = BTreeSet::new();
        let mut previous = None;
        for (representative, bucket) in &self.buckets {
            let upper = representative.checked_add(1);
            let count =
                self.validate_bucket(bucket.root, (previous, upper), &mut nodes, &mut entries)?;
            if count != bucket.len {
                return Err(InvariantViolation {
                    code: "YFAST_BUCKET_COUNT",
                });
            }
            if *representative != self.sentinel {
                let maximum = bucket.root.and_then(|root| {
                    let mut node = root;
                    while let Some(right) = self.nodes.get(node.0).and_then(|node| node.right) {
                        node = right;
                    }
                    self.nodes.get(node.0).map(|node| node.key)
                });
                if maximum != Some(*representative) {
                    return Err(InvariantViolation {
                        code: "YFAST_REPRESENTATIVE_MAX",
                    });
                }
            }
            previous = Some(*representative);
        }
        if nodes.len() != self.by_key.len()
            || entries.len() != self.by_key.len()
            || self.entries.len() != u32::try_from(self.by_key.len()).unwrap_or(u32::MAX)
        {
            return Err(InvariantViolation {
                code: "YFAST_COUNT",
            });
        }
        for (key, (entry, node)) in &self.by_key {
            let node_record = self.nodes.get(node.0).ok_or(InvariantViolation {
                code: "YFAST_INDEX_NODE",
            })?;
            let entry_record = self.entries.get(entry.0).ok_or(InvariantViolation {
                code: "YFAST_INDEX_ENTRY",
            })?;
            if node_record.key != *key || node_record.entry != *entry || entry_record.key != *key {
                return Err(InvariantViolation {
                    code: "YFAST_INDEX",
                });
            }
        }
        Ok(())
    }

    fn estimated_bytes(&self) -> usize {
        self.index
            .estimated_bytes()
            .saturating_add(self.nodes.estimated_bytes())
            .saturating_add(self.entries.estimated_bytes())
            .saturating_add(
                self.buckets
                    .len()
                    .saturating_mul(size_of::<(u64, Bucket)>()),
            )
            .saturating_add(
                self.entries
                    .iter()
                    .map(|(_, entry)| entry.value.capacity())
                    .sum::<usize>(),
            )
    }
}
