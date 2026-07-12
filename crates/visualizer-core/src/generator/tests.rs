use std::collections::HashSet;

use super::*;

fn initial_spec() -> InitialGeneratorSpec {
    InitialGeneratorSpec {
        seed: "7".to_owned(),
        count: 10,
        key_min: "0".to_owned(),
        key_max: "31".to_owned(),
        distribution: KeyDistribution::Uniform,
        value_prefix: "v-".to_owned(),
        value_max_scalar_values: 8,
        overwrite_rate_bps: 3_000,
    }
}

fn operation_spec() -> OperationGeneratorSpec {
    OperationGeneratorSpec {
        seed: "11".to_owned(),
        count: 40,
        key_min: "0".to_owned(),
        key_max: "31".to_owned(),
        distribution: KeyDistribution::Hotspot,
        value_prefix: "x".to_owned(),
        value_max_scalar_values: 6,
        weights: OperationWeights {
            insert: 3,
            remove: 2,
            get: 4,
            lower_bound: 1,
        },
        get_hit_rate_bps: 7_500,
        remove_hit_rate_bps: 5_000,
        insert_overwrite_rate_bps: 2_500,
    }
}

#[test]
fn materialization_is_exactly_reproducible_with_requested_counts() {
    let initial = generate_initial(&initial_spec()).expect("feasible initial generator");
    let operations = generate_operations(&operation_spec(), &initial.entries)
        .expect("feasible operation generator");
    let repeated_initial = generate_initial(&initial_spec()).expect("repeat initial");
    let repeated_operations =
        generate_operations(&operation_spec(), &initial.entries).expect("repeat operations");

    assert_eq!(
        serde_json::to_value(&initial.entries).unwrap(),
        serde_json::to_value(&repeated_initial.entries).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&operations.operations).unwrap(),
        serde_json::to_value(&repeated_operations.operations).unwrap()
    );
    assert_eq!(initial.stats.achieved_insert_overwrites, 3);
    assert_eq!(
        [
            operations.stats.insert_count,
            operations.stats.remove_count,
            operations.stats.get_count,
            operations.stats.lower_bound_count,
        ],
        [12, 8, 16, 4]
    );
    assert_eq!(operations.stats.achieved_insert_overwrites, 3);
    assert_eq!(operations.stats.achieved_remove_hits, 4);
    assert_eq!(operations.stats.achieved_get_hits, 12);
    assert_eq!(
        initial.provenance.generator_revision,
        SUPPORTED_GENERATOR_REVISION
    );
}

#[test]
fn zero_weight_operations_are_excluded_from_the_materialized_stream() {
    let initial = generate_initial(&initial_spec()).expect("feasible initial generator");
    let spec = OperationGeneratorSpec {
        weights: OperationWeights {
            insert: 0,
            remove: 0,
            get: 3,
            lower_bound: 1,
        },
        ..operation_spec()
    };
    let generated =
        generate_operations(&spec, &initial.entries).expect("selected operations are feasible");

    assert_eq!(
        [
            generated.stats.insert_count,
            generated.stats.remove_count,
            generated.stats.get_count,
            generated.stats.lower_bound_count,
        ],
        [0, 0, 30, 10]
    );
    assert!(generated.operations.iter().all(|operation| matches!(
        operation,
        Operation::Get { .. } | Operation::LowerBound { .. }
    )));
}

#[test]
fn initial_counterexample_schedules_new_before_overwrite() {
    let spec = InitialGeneratorSpec {
        count: 2,
        key_max: "0".to_owned(),
        overwrite_rate_bps: 5_000,
        ..initial_spec()
    };
    let generated = generate_initial(&spec).expect("new then overwrite is feasible");

    assert_eq!(generated.entries.len(), 2);
    assert_eq!(generated.entries[0].key, "0");
    assert_eq!(generated.entries[1].key, "0");
    assert_eq!(generated.stats.final_unique_count, 1);
}

#[test]
fn full_u64_universe_is_sampled_without_materialization() {
    let spec = InitialGeneratorSpec {
        count: 2,
        key_min: "0".to_owned(),
        key_max: u64::MAX.to_string(),
        overwrite_rate_bps: 0,
        ..initial_spec()
    };
    let generated = generate_initial(&spec).expect("full universe is supported");

    assert_eq!(generated.entries.len(), 2);
    assert_ne!(generated.entries[0].key, generated.entries[1].key);
}

#[test]
fn infeasible_rates_are_rejected_without_fallback() {
    let spec = InitialGeneratorSpec {
        count: 1,
        overwrite_rate_bps: 10_000,
        ..initial_spec()
    };

    assert!(matches!(
        generate_initial(&spec),
        Err(GenerationError::Schedule(ScheduleError::Infeasible))
    ));
}

#[test]
fn overwrite_priority_cannot_block_required_new_insert() {
    let descriptors = [
        Descriptor {
            id: 0,
            priority: 10,
            class: TransitionClass::NewInsert,
        },
        Descriptor {
            id: 1,
            priority: 0,
            class: TransitionClass::RequiresPresent,
        },
    ];
    let mut scheduler = DescriptorScheduler::new(
        FeasibilityState {
            size: 0,
            capacity: 1,
        },
        descriptors,
    )
    .expect("counterexample is feasible");

    assert_eq!(
        scheduler.next_descriptor().expect("schedule must continue"),
        Some(descriptors[0])
    );
    assert_eq!(
        scheduler.next_descriptor().expect("schedule must continue"),
        Some(descriptors[1])
    );
}

#[test]
fn type_local_ordinals_fix_hit_and_new_assignment() {
    let mut rng = RngV1::from_seed(5, "rng.generator.operations.descriptor-order");
    let descriptors = build_descriptors(GeneratedOperationKind::Insert, 4, 2, &mut rng)
        .expect("valid descriptor counts");

    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.class)
            .collect::<Vec<_>>(),
        [
            TransitionClass::RequiresPresent,
            TransitionClass::RequiresPresent,
            TransitionClass::NewInsert,
            TransitionClass::NewInsert,
        ]
    );
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn scheduling_never_redraws_descriptor_priorities() {
    let mut rng = RngV1::from_seed(9, "rng.generator.operations.descriptor-order");
    let mut descriptors = build_descriptors(GeneratedOperationKind::Insert, 2, 1, &mut rng)
        .expect("valid descriptor counts");
    descriptors[0].priority = 0;
    descriptors[1].priority = 0;
    let draws_before_schedule = rng.draws();
    let mut scheduler = DescriptorScheduler::new(
        FeasibilityState {
            size: 0,
            capacity: 1,
        },
        descriptors,
    )
    .expect("new then overwrite is feasible");

    let first = scheduler
        .next_descriptor()
        .expect("first descriptor")
        .expect("nonempty");
    let second = scheduler
        .next_descriptor()
        .expect("second descriptor")
        .expect("nonempty");

    assert_eq!(first.class, TransitionClass::NewInsert);
    assert_eq!(second.class, TransitionClass::RequiresPresent);
    assert_eq!(rng.draws(), draws_before_schedule);
}

#[test]
fn feasibility_predicate_matches_exhaustive_oracle() {
    for capacity in 0_u32..=8 {
        for size in 0..=capacity {
            for inserts in 0..=8 {
                for removes in 0..=8 - inserts {
                    for present in 0..=8 - inserts - removes {
                        for absent in 0..=8 - inserts - removes - present {
                            for always in 0..=8 - inserts - removes - present - absent {
                                let remaining = [inserts, removes, present, absent, always];
                                let state = FeasibilityState {
                                    size: u128::from(size),
                                    capacity: u128::from(capacity),
                                };
                                assert_eq!(
                                    can_complete(state, remaining),
                                    exhaustive_oracle(state, remaining, &mut HashSet::new()),
                                    "state={state:?}, remaining={remaining:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn exhaustive_oracle(
    state: FeasibilityState,
    remaining: [u32; CLASS_COUNT],
    seen: &mut HashSet<(u128, [u32; CLASS_COUNT])>,
) -> bool {
    if remaining.iter().all(|count| *count == 0) {
        return true;
    }
    if !seen.insert((state.size, remaining)) {
        return false;
    }
    TransitionClass::ALL.into_iter().any(|class| {
        if remaining[class.index()] == 0 {
            return false;
        }
        let Some(next_state) = apply_transition(state, class) else {
            return false;
        };
        let mut next_remaining = remaining;
        next_remaining[class.index()] -= 1;
        exhaustive_oracle(next_state, next_remaining, seen)
    })
}
