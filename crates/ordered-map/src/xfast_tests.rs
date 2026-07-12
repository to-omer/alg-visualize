#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as Model;

    use super::*;

    #[test]
    fn prefix_search_and_leaf_links_match_model() {
        for bits in [1, 3, 8, 16] {
            let universe = if bits == 16 { 1024 } else { 1_u64 << bits };
            let mut map = XFastMap::new(73, bits).unwrap();
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
            for query in 0..universe {
                let actual = map
                    .apply(Operation::LowerBound { key: query }, &mut Vec::new())
                    .unwrap();
                let expected = model.range(query..).next().map(|(key, _)| *key);
                assert_eq!(
                    match actual {
                        OperationResult::Found { key, .. } => Some(key),
                        OperationResult::Miss => None,
                        _ => panic!("unexpected query result"),
                    },
                    expected
                );
            }
            for key in (0..universe).map(|key| (key * 181) % universe) {
                map.apply(Operation::Remove { key }, &mut Vec::new())
                    .unwrap();
                model.remove(&key);
                map.check_invariants().unwrap();
            }
        }
    }

    #[test]
    fn high_entropy_word64_input_stops_before_the_visual_entity_limit() {
        let mut map = XFastMap::new(73, 64).expect("word size is valid");
        let mut rejected = false;
        for index in 0..10_000_u64 {
            let key = index.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(17);
            let before = map.structure_entity_count();
            match map.apply(
                Operation::Insert {
                    key,
                    value: String::new(),
                },
                &mut Vec::new(),
            ) {
                Ok(_) => assert!(map.structure_entity_count() <= MAX_VISUAL_ENTITIES),
                Err(MapError::ResourceLimit("visual entity count")) => {
                    assert_eq!(map.structure_entity_count(), before);
                    rejected = true;
                    break;
                }
                Err(error) => panic!("unexpected insertion error: {error}"),
            }
        }
        assert!(rejected, "fixture must cross the visual entity limit");
        map.check_invariants()
            .expect("rejection preserves the trie");
    }

    #[test]
    fn full_u64_boundaries_use_canonical_prefix_bytes() {
        let mut map = XFastMap::new(11, 64).unwrap();
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
        map.check_invariants().unwrap();
        assert!(matches!(
            map.apply(Operation::LowerBound { key: u64::MAX - 2 }, &mut Vec::new())
                .unwrap(),
            OperationResult::Found { key, .. } if key == u64::MAX - 1
        ));
    }
}
