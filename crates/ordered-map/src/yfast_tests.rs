#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as Model;

    use super::*;

    #[test]
    fn randomized_representatives_and_bucket_merges_match_model() {
        let mut map = YFastMap::new(83, 10).unwrap();
        let mut model = Model::new();
        for round in 0_u64..5 {
            for key in (0_u64..512).map(|key| (key * 277 + round * 31) % 512) {
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
            for key in (0_u64..512).filter(|key| (key + round) % 3 == 0) {
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
    fn maximum_user_key_never_replaces_structural_sentinel() {
        let mut map = YFastMap::new(7, 3).unwrap();
        let maximum = 7;
        map.apply(
            Operation::Insert {
                key: maximum,
                value: "max".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        assert!(map.buckets.contains_key(&maximum));
        map.apply(Operation::Remove { key: maximum }, &mut Vec::new())
            .unwrap();
        assert!(map.buckets.contains_key(&maximum));
        assert_eq!(map.buckets.len(), 1);
        map.check_invariants().unwrap();
    }

    #[test]
    fn overwrite_and_reads_do_not_consume_rng() {
        let mut map = YFastMap::new(19, 8).unwrap();
        map.apply(
            Operation::Insert {
                key: 7,
                value: "a".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        let draws = (map.priority_draws(), map.representative_draws());
        map.apply(
            Operation::Insert {
                key: 7,
                value: "b".to_owned(),
            },
            &mut Vec::new(),
        )
        .unwrap();
        map.apply(Operation::Get { key: 7 }, &mut Vec::new())
            .unwrap();
        assert_eq!((map.priority_draws(), map.representative_draws()), draws);
    }
}
