#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;
    use crate::test_support::binary_topology;

    fn apply_model(
        model: &mut BTreeMap<u64, String>,
        operation: &Operation,
    ) -> Option<(u64, String)> {
        match operation {
            Operation::Insert { key, value } => {
                model.insert(*key, value.clone()).map(|old| (*key, old))
            }
            Operation::Remove { key } => model.remove(key).map(|value| (*key, value)),
            Operation::Get { key } => model
                .get_key_value(key)
                .map(|(key, value)| (*key, value.clone())),
            Operation::LowerBound { key } => model
                .range(*key..)
                .next()
                .map(|(key, value)| (*key, value.clone())),
        }
    }

    #[test]
    fn rotations_and_successor_entry_movement_preserve_invariants() {
        let mut map = AvlMap::new();
        let mut trace = Vec::new();
        let mut ids = BTreeMap::new();
        for key in [30, 20, 10, 25, 40, 35, 50] {
            let result = map
                .apply(
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                    &mut trace,
                )
                .unwrap();
            if let OperationResult::Inserted { entry } = result {
                ids.insert(key, entry);
            }
            map.check_invariants().unwrap();
        }
        let removed = map
            .apply(Operation::Remove { key: 30 }, &mut trace)
            .unwrap();
        assert_eq!(
            removed,
            OperationResult::Removed {
                entry: ids[&30],
                value: "30".to_owned()
            }
        );
        assert_eq!(map.by_key[&35].0, ids[&35]);
        map.check_invariants().unwrap();
        assert!(
            trace
                .iter()
                .any(|event| matches!(event.kind, TraceKind::RotateLeft | TraceKind::RotateRight))
        );
    }

    #[test]
    fn traced_double_rotation_exposes_each_exact_intermediate_topology() {
        let mut map = AvlMap::new();
        let mut setup = Vec::new();
        map.apply(
            Operation::Insert {
                key: 3,
                value: "three".to_owned(),
            },
            &mut setup,
        )
        .expect("first setup insert succeeds");
        map.apply(
            Operation::Insert {
                key: 1,
                value: "one".to_owned(),
            },
            &mut setup,
        )
        .expect("second setup insert succeeds");
        let before_structure = map.structure_snapshot();
        let before_canonical = map.canonical_snapshot();
        let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
            .expect("base state is valid");

        map.apply_traced(
            Operation::Insert {
                key: 2,
                value: "two".to_owned(),
            },
            &mut recorder,
        )
        .expect("traced insert succeeds");
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
                rotations.push(binary_topology(&replay.structure_snapshot()));
            }
        }

        assert_eq!(
            rotations,
            vec![
                (
                    3,
                    vec![(2, "left".to_owned(), 1), (3, "left".to_owned(), 2)]
                ),
                (
                    2,
                    vec![(2, "left".to_owned(), 1), (2, "right".to_owned(), 3)]
                ),
            ]
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

    proptest! {
        #[test]
        fn matches_btree_map(operations in prop::collection::vec((0_u8..4, any::<u8>(), any::<u16>()), 0..300)) {
            let mut map = AvlMap::new();
            let mut model = BTreeMap::new();
            for (kind, raw_key, raw_value) in operations {
                let key = u64::from(raw_key);
                let operation = match kind {
                    0 => Operation::Insert { key, value: raw_value.to_string() },
                    1 => Operation::Remove { key },
                    2 => Operation::Get { key },
                    _ => Operation::LowerBound { key },
                };
                let expected = apply_model(&mut model, &operation);
                let result = map.apply(operation.clone(), &mut Vec::new()).unwrap();
                let result_matches = match &operation {
                    Operation::Insert { .. } => matches!(
                        (result, expected),
                        (OperationResult::Inserted { .. }, None)
                            | (OperationResult::Overwritten { .. }, Some(_))
                    ),
                    Operation::Remove { .. } => matches!(
                        (result, expected),
                        (OperationResult::Removed { .. }, Some(_))
                            | (OperationResult::Miss, None)
                    ),
                    Operation::Get { .. } | Operation::LowerBound { .. } => matches!(
                        (result, expected),
                        (OperationResult::Found { .. }, Some(_))
                            | (OperationResult::Miss, None)
                    ),
                };
                prop_assert!(result_matches, "operation result class differed from model");
                map.check_invariants().unwrap();
                let actual: Vec<_> = map.canonical_snapshot().entries.into_iter().map(|entry| (entry.key, entry.value)).collect();
                let expected: Vec<_> = model.iter().map(|(key, value)| (*key, value.clone())).collect();
                prop_assert_eq!(actual, expected);
            }
        }
    }
}
