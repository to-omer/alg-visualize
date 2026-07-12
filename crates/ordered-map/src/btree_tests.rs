#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as Model;

    use super::*;
    use crate::TraceState;

    fn root_layer(snapshot: &StructureSnapshot) -> (Vec<u64>, Vec<Vec<u64>>) {
        let nodes = snapshot
            .nodes
            .iter()
            .map(|node| (node.id, node))
            .collect::<Model<_, _>>();
        let root = snapshot
            .root
            .and_then(|root| nodes.get(&root).copied())
            .expect("fixture tree has a root");
        let children = root
            .links
            .iter()
            .map(|link| {
                nodes
                    .get(&link.target)
                    .expect("fixture child exists")
                    .keys
                    .clone()
            })
            .collect();
        (root.keys.clone(), children)
    }

    fn semantic_topology(snapshot: &StructureSnapshot) -> Vec<(Vec<u64>, Vec<Vec<u64>>)> {
        let by_id = snapshot
            .nodes
            .iter()
            .map(|node| (node.id, node.keys.clone()))
            .collect::<Model<_, _>>();
        let mut topology = snapshot
            .nodes
            .iter()
            .map(|node| {
                let children = node
                    .links
                    .iter()
                    .map(|link| by_id.get(&link.target).cloned().unwrap_or_default())
                    .collect::<Vec<_>>();
                (node.keys.clone(), children)
            })
            .collect::<Vec<_>>();
        topology.sort();
        topology
    }

    fn canonical_keys(state: &TraceState) -> Vec<u64> {
        state
            .canonical_snapshot()
            .entries
            .into_iter()
            .map(|entry| entry.key)
            .collect()
    }

    fn raw_insert(map: &mut BTreeMap, key: u64) {
        map.apply(
            Operation::Insert {
                key,
                value: key.to_string(),
            },
            &mut Vec::new(),
        )
        .expect("fixture insert succeeds");
    }

    fn assert_traced_split(map: &mut BTreeMap) {
        let before_split_structure = map.structure_snapshot();
        let before_split_canonical = map.canonical_snapshot();
        let split_count = before_split_canonical.metrics.splits;
        let mut split_recorder =
            OrderedMapTraceRecorder::new(&before_split_structure, &before_split_canonical)
                .expect("split base state is valid");
        map.apply_traced(
            Operation::Insert {
                key: 4,
                value: "4".to_owned(),
            },
            &mut split_recorder,
        )
        .expect("traced split insert succeeds");
        split_recorder
            .verify_final(&map.structure_snapshot(), &map.canonical_snapshot())
            .expect("split trace reaches its independent final state");
        let (split_events, split_patches) = split_recorder.into_parts();
        let mut split_replay =
            TraceState::from_snapshots(&before_split_structure, &before_split_canonical)
                .expect("split base state replays");
        let mut split_observed = false;
        for event in &split_events {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            split_replay
                .apply_forward(&split_patches[start..end])
                .expect("split event patch applies");
            if event.kind == TraceKind::Split {
                split_observed = true;
                assert_eq!(
                    root_layer(&split_replay.structure_snapshot()),
                    (vec![2], vec![vec![1], vec![3]])
                );
                assert_eq!(canonical_keys(&split_replay), vec![1, 2, 3]);
                assert_eq!(
                    split_replay.canonical_snapshot().metrics.splits,
                    split_count + 1
                );
            }
        }
        assert!(split_observed, "fixture must produce a split event");
        assert_eq!(
            root_layer(&split_replay.structure_snapshot()),
            (vec![2], vec![vec![1], vec![3, 4]])
        );
        assert_eq!(canonical_keys(&split_replay), vec![1, 2, 3, 4]);
        for event in split_events.iter().rev() {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            split_replay
                .apply_reverse(&split_patches[start..end])
                .expect("split event patch reverses");
        }
        assert_eq!(split_replay.structure_snapshot(), before_split_structure);
        assert_eq!(split_replay.canonical_snapshot(), before_split_canonical);
    }

    fn assert_traced_merge(map: &mut BTreeMap) {
        map.apply(Operation::Remove { key: 4 }, &mut Vec::new())
            .expect("fixture preparation remove succeeds");
        let before_merge_structure = map.structure_snapshot();
        let before_merge_canonical = map.canonical_snapshot();
        let merge_count = before_merge_canonical.metrics.merges;
        let mut merge_recorder =
            OrderedMapTraceRecorder::new(&before_merge_structure, &before_merge_canonical)
                .expect("merge base state is valid");
        map.apply_traced(Operation::Remove { key: 1 }, &mut merge_recorder)
            .expect("traced merge remove succeeds");
        merge_recorder
            .verify_final(&map.structure_snapshot(), &map.canonical_snapshot())
            .expect("merge trace reaches its independent final state");
        let (merge_events, merge_patches) = merge_recorder.into_parts();
        let mut merge_replay =
            TraceState::from_snapshots(&before_merge_structure, &before_merge_canonical)
                .expect("merge base state replays");
        let mut merge_observed = false;
        for event in &merge_events {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            merge_replay
                .apply_forward(&merge_patches[start..end])
                .expect("merge event patch applies");
            if event.kind == TraceKind::Merge {
                merge_observed = true;
                assert_eq!(
                    root_layer(&merge_replay.structure_snapshot()),
                    (Vec::new(), vec![vec![1, 2, 3]])
                );
                assert_eq!(canonical_keys(&merge_replay), vec![1, 2, 3]);
                assert_eq!(
                    merge_replay.canonical_snapshot().metrics.merges,
                    merge_count + 1
                );
            }
        }
        assert!(merge_observed, "fixture must produce a merge event");
        assert_eq!(
            root_layer(&merge_replay.structure_snapshot()),
            (vec![2, 3], Vec::new())
        );
        assert_eq!(canonical_keys(&merge_replay), vec![2, 3]);
        for event in merge_events.iter().rev() {
            let start = usize::try_from(event.patch_start).expect("patch offset fits");
            let end = start + usize::try_from(event.patch_count).expect("patch count fits");
            merge_replay
                .apply_reverse(&merge_patches[start..end])
                .expect("merge event patch reverses");
        }
        assert_eq!(merge_replay.structure_snapshot(), before_merge_structure);
        assert_eq!(merge_replay.canonical_snapshot(), before_merge_canonical);
    }

    #[test]
    fn traced_split_and_merge_expose_each_intermediate_multiway_topology() {
        let mut map = BTreeMap::new(2).expect("minimum degree is valid");
        for key in 1..=3 {
            raw_insert(&mut map, key);
        }

        assert_traced_split(&mut map);
        assert_traced_merge(&mut map);
    }

    #[test]
    fn consecutive_split_and_merge_fixture() {
        let fixtures = [
            (
                17_u64,
                Operation::Insert {
                    key: 17,
                    value: "17".to_owned(),
                },
                TraceKind::Split,
                vec![
                    vec![
                        (vec![0], vec![]),
                        (vec![1], vec![vec![0], vec![2]]),
                        (vec![2], vec![]),
                        (vec![3], vec![vec![1], vec![5]]),
                        (vec![4], vec![]),
                        (vec![5], vec![vec![4], vec![6]]),
                        (vec![6], vec![]),
                        (vec![7], vec![vec![3], vec![11]]),
                        (vec![8], vec![]),
                        (vec![9], vec![vec![8], vec![10]]),
                        (vec![10], vec![]),
                        (vec![11], vec![vec![9], vec![13]]),
                        (vec![12], vec![]),
                        (vec![13], vec![vec![12], vec![14, 15, 16]]),
                        (vec![14, 15, 16], vec![]),
                    ],
                    vec![
                        (vec![0], vec![]),
                        (vec![1], vec![vec![0], vec![2]]),
                        (vec![2], vec![]),
                        (vec![3], vec![vec![1], vec![5]]),
                        (vec![4], vec![]),
                        (vec![5], vec![vec![4], vec![6]]),
                        (vec![6], vec![]),
                        (vec![7], vec![vec![3], vec![11]]),
                        (vec![8], vec![]),
                        (vec![9], vec![vec![8], vec![10]]),
                        (vec![10], vec![]),
                        (vec![11], vec![vec![9], vec![13, 15]]),
                        (vec![12], vec![]),
                        (vec![13, 15], vec![vec![12], vec![14], vec![16]]),
                        (vec![14], vec![]),
                        (vec![16], vec![]),
                    ],
                ],
            ),
            (
                9_u64,
                Operation::Remove { key: 0 },
                TraceKind::Merge,
                vec![
                    vec![
                        (vec![], vec![vec![1, 3, 5]]),
                        (vec![0], vec![]),
                        (
                            vec![1, 3, 5],
                            vec![vec![0], vec![2], vec![4], vec![6, 7, 8]],
                        ),
                        (vec![2], vec![]),
                        (vec![4], vec![]),
                        (vec![6, 7, 8], vec![]),
                    ],
                    vec![
                        (vec![], vec![vec![3, 5]]),
                        (vec![0, 1, 2], vec![]),
                        (vec![3, 5], vec![vec![0, 1, 2], vec![4], vec![6, 7, 8]]),
                        (vec![4], vec![]),
                        (vec![6, 7, 8], vec![]),
                    ],
                ],
            ),
        ];
        for (size, operation, kind, expected) in fixtures {
            let mut map = BTreeMap::new(2).expect("minimum degree is valid");
            for key in 0..size {
                raw_insert(&mut map, key);
            }
            let before_structure = map.structure_snapshot();
            let before_canonical = map.canonical_snapshot();
            let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
                .expect("fixture base is valid");
            map.apply_traced(operation, &mut recorder)
                .expect("fixture operation succeeds");
            let (events, patches) = recorder.into_parts();
            let mut replay = TraceState::from_snapshots(&before_structure, &before_canonical)
                .expect("fixture replays");
            let mut actual = Vec::new();
            for event in events {
                let start = usize::try_from(event.patch_start).expect("patch offset");
                let end = start + usize::try_from(event.patch_count).expect("patch count");
                replay
                    .apply_forward(&patches[start..end])
                    .expect("event applies");
                if event.kind == kind {
                    actual.push(semantic_topology(&replay.structure_snapshot()));
                }
            }
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn split_borrow_merge_and_root_shrink_match_model() {
        for degree in [2, 3, 8, 16] {
            let mut map = BTreeMap::new(degree).unwrap();
            let mut model = Model::new();
            for key in (0_u64..512).map(|key| (key * 277) % 512) {
                map.apply(
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                    &mut Vec::new(),
                )
                .unwrap();
                model.insert(key, key.to_string());
                map.check_invariants().unwrap_or_else(|error| {
                    panic!("degree {degree}, invariant after inserting {key}: {error}")
                });
            }
            for key in (0_u64..512).map(|key| (key * 181) % 512) {
                map.apply(Operation::Remove { key }, &mut Vec::new())
                    .unwrap();
                model.remove(&key);
                map.check_invariants().unwrap_or_else(|error| {
                    panic!("degree {degree}, invariant after removing {key}: {error}")
                });
            }
            assert!(model.is_empty());
            assert!(map.canonical_snapshot().entries.is_empty());
            assert!(map.node(map.root).unwrap().leaf);
        }
    }

    #[test]
    fn missing_remove_is_structurally_read_only() {
        let mut map = BTreeMap::new(3).unwrap();
        for key in 0_u64..40 {
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
            map.apply(Operation::Remove { key: 100 }, &mut Vec::new())
                .unwrap(),
            OperationResult::Miss
        );
        assert_eq!(map.structure_snapshot(), before);
    }
}
