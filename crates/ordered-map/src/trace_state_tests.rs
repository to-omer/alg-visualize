#[cfg(test)]
mod tests {
    use visualizer_core::arena::{ArenaKey, GenerationalArena};

    use super::*;
    use crate::model::{NodeId, StructureLink};

    fn arena_key(index: u32) -> ArenaKey {
        let mut arena = GenerationalArena::new();
        let mut current = None;
        for _ in 0..=index {
            current = Some(arena.try_insert(()).expect("fixture allocation succeeds"));
        }
        current.expect("fixture inserts at least one key")
    }

    fn node(index: u32, key: u64, links: Vec<StructureLink>) -> StructureNode {
        let id = NodeId(arena_key(index));
        StructureNode {
            id: StructureEntityId::Node(id),
            role: "binary-node".to_owned(),
            entries: vec![EntryId(arena_key(index))],
            keys: vec![key],
            links,
            metadata: Vec::new(),
        }
    }

    fn link(slot: u32, role: &str, target: u32) -> StructureLink {
        StructureLink {
            slot,
            role: role.to_owned(),
            target: StructureEntityId::Node(NodeId(arena_key(target))),
        }
    }

    fn entry(index: u32, key: u64) -> CanonicalEntry {
        CanonicalEntry {
            id: EntryId(arena_key(index)),
            key,
            value: key.to_string(),
        }
    }

    fn initial_state() -> TraceState {
        TraceState::from_snapshots(
            &StructureSnapshot {
                root: Some(StructureEntityId::Node(NodeId(arena_key(0)))),
                nodes: vec![
                    node(0, 1, vec![link(1, "right", 1)]),
					node(1, 2, Vec::new()),
                ],
            },
            &CanonicalSnapshot {
                entries: vec![entry(0, 1), entry(1, 2)],
                metrics: Metrics::default(),
            },
        )
        .expect("fixture is valid")
    }

    fn trace_event() -> TraceEvent {
        TraceEvent {
            catalog_id: 1,
            kind: crate::model::TraceKind::Compare,
            node: Some(StructureEntityId::Node(NodeId(arena_key(0)))),
            target: None,
            entry: Some(EntryId(arena_key(0))),
            key: Some(2),
            patch_start: 0,
            patch_count: 0,
        }
    }

    #[test]
    fn transaction_round_trips_a_rotation_exactly() {
        let before = initial_state();
        let mut after_root = node(0, 1, Vec::new());
        after_root.links.clear();
        let after_pivot = node(1, 2, vec![link(0, "left", 0)]);
        let records = vec![
            StatePatchRecord::Root {
                before: before.root,
                after: Some(after_pivot.id),
            },
            StatePatchRecord::Node {
                id: after_root.id,
                before: before.nodes.get(&after_root.id).cloned().map(Box::new),
                after: Some(Box::new(after_root)),
            },
            StatePatchRecord::Node {
                id: after_pivot.id,
                before: before.nodes.get(&after_pivot.id).cloned().map(Box::new),
                after: Some(Box::new(after_pivot)),
            },
            StatePatchRecord::Metric {
                ordinal: MetricOrdinal::Rotations,
                before: 0,
                after: 1,
            },
        ];

        let mut state = before.clone();
        state.apply_forward(&records).expect("rotation applies");
        assert_eq!(
            state.structure_snapshot().root,
            Some(StructureEntityId::Node(NodeId(arena_key(1))))
        );
        state.apply_reverse(&records).expect("rotation reverses");
        assert_eq!(state, before);
    }

    #[test]
    fn stale_precondition_rejects_the_whole_transaction() {
        let mut state = initial_state();
        let original = state.clone();
        let records = vec![
            StatePatchRecord::Root {
                before: state.root,
                after: Some(StructureEntityId::Node(NodeId(arena_key(1)))),
            },
            StatePatchRecord::Metric {
                ordinal: MetricOrdinal::Rotations,
                before: 9,
                after: 10,
            },
        ];

        assert!(matches!(
            state.apply_forward(&records),
            Err(MapError::TraceState("metric patch precondition mismatch"))
        ));
        assert_eq!(state, original);
    }

    #[test]
    fn recorder_assigns_exact_spans_and_verifies_independent_final_state() {
        let base = initial_state();
        let structure = base.structure_snapshot();
        let canonical = base.canonical_snapshot();
        let mut recorder =
            OrderedMapTraceRecorder::new(&structure, &canonical).expect("base snapshot is valid");
        recorder
            .record_transition(
                trace_event(),
                vec![StatePatchRecord::Metric {
                    ordinal: MetricOrdinal::Comparisons,
                    before: 0,
                    after: 1,
                }],
            )
            .expect("metric transition is valid");
        let mut final_canonical = canonical;
        final_canonical.metrics.comparisons = 1;
        recorder
            .verify_final(&structure, &final_canonical)
            .expect("patch replay reaches independent final state");
        let (events, patches) = recorder.into_parts();

        assert_eq!(patches.len(), 1);
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].patch_start, events[0].patch_count), (0, 1));
    }

    #[test]
    fn recorder_rejects_an_omitted_visible_change() {
        let base = initial_state();
        let structure = base.structure_snapshot();
        let canonical = base.canonical_snapshot();
        let recorder =
            OrderedMapTraceRecorder::new(&structure, &canonical).expect("base snapshot is valid");
        let mut mismatched = canonical;
        mismatched.metrics.rotations = 1;

        assert!(matches!(
            recorder.verify_final(&structure, &mismatched),
            Err(MapError::TraceState(
                "trace metrics do not match final snapshot"
            ))
        ));
    }

	fn oracle_transition() -> (
		StructureSnapshot,
		CanonicalSnapshot,
		StructureSnapshot,
		CanonicalSnapshot,
		Vec<StatePatchRecord>,
	) {
        let before_structure = StructureSnapshot {
            root: Some(StructureEntityId::Node(NodeId(arena_key(0)))),
            nodes: vec![
				node(
					0,
					1,
					vec![link(1, "right", 1), link(2, "metadata-child", 2)],
				),
				node(1, 2, vec![link(0, "back", 0)]),
                node(2, 3, Vec::new()),
            ],
        };
        let before_canonical = CanonicalSnapshot {
            entries: vec![entry(0, 1), entry(1, 2), entry(2, 3)],
            metrics: Metrics::default(),
        };
        let mut metadata_node = node(2, 3, Vec::new());
        metadata_node.metadata.push(("height".to_owned(), 99));
        let after_structure = StructureSnapshot {
            root: Some(StructureEntityId::Node(NodeId(arena_key(1)))),
            nodes: vec![
				node(
					0,
					1,
					vec![link(1, "forward", 1), link(2, "metadata-child", 2)],
				),
                node(1, 2, vec![link(0, "left", 0)]),
                metadata_node,
            ],
        };
        let mut after_canonical = before_canonical.clone();
        after_canonical.entries[0].value = "updated".to_owned();
        after_canonical.metrics.rotations = 1;
        let base = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("oracle base is valid");
        let complete = base
            .diff_to_snapshots(&after_structure, &after_canonical)
            .expect("independent final snapshot has a canonical patch");
		(
			before_structure,
			before_canonical,
			after_structure,
			after_canonical,
			complete,
		)
	}

	#[test]
	fn final_oracle_rejects_each_omitted_patch_class() {
		let (before_structure, before_canonical, after_structure, after_canonical, complete) =
			oracle_transition();

        let omitted = [
            (
                "root",
                complete
                    .iter()
                    .position(|record| matches!(record, StatePatchRecord::Root { .. }))
                    .unwrap(),
                "trace root does not match final snapshot",
            ),
            (
                "outgoing link",
                complete
                    .iter()
                    .position(|record| {
                        matches!(record, StatePatchRecord::Node { id, .. } if *id == StructureEntityId::Node(NodeId(arena_key(0))))
                    })
                    .unwrap(),
                "trace nodes do not match final snapshot",
            ),
            (
                "incoming link",
                complete
                    .iter()
                    .position(|record| {
                        matches!(record, StatePatchRecord::Node { id, .. } if *id == StructureEntityId::Node(NodeId(arena_key(1))))
                    })
                    .unwrap(),
                "trace nodes do not match final snapshot",
            ),
            (
                "metadata",
                complete
                    .iter()
                    .position(|record| {
                        matches!(record, StatePatchRecord::Node { id, .. } if *id == StructureEntityId::Node(NodeId(arena_key(2))))
                    })
                    .unwrap(),
                "trace nodes do not match final snapshot",
            ),
            (
                "entry value",
                complete
                    .iter()
                    .position(|record| matches!(record, StatePatchRecord::Entry { .. }))
                    .unwrap(),
                "trace entries do not match final snapshot",
            ),
            (
                "metric",
                complete
                    .iter()
                    .position(|record| matches!(record, StatePatchRecord::Metric { .. }))
                    .unwrap(),
                "trace metrics do not match final snapshot",
            ),
        ];

        for (class, omitted_index, expected) in omitted {
            let mut incomplete = complete.clone();
            incomplete.remove(omitted_index);
            let mut recorder = OrderedMapTraceRecorder::new(&before_structure, &before_canonical)
                .expect("base snapshot is valid");
            recorder
                .record_transition(trace_event(), incomplete)
                .unwrap_or_else(|error| panic!("{class} omission must remain replayable: {error}"));
            assert!(
                matches!(
                    recorder.verify_final(&after_structure, &after_canonical),
                    Err(MapError::TraceState(message)) if message == expected
                ),
                "{class} omission reached the independent final oracle"
            );
        }
    }

    #[test]
    fn unrecorded_events_do_not_materialize_projection_snapshots() {
        let mut events = Vec::new();
        let mut target = TraceTarget::Events(&mut events);

        target
            .transition(trace_event(), |_| {
                panic!("unrecorded execution must not build a projection")
            })
            .expect("raw event recording succeeds");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].catalog_id, 1);
    }

    #[test]
    fn trace_event_limit_is_enforced_before_the_extra_record_is_pushed() {
        let base = initial_state();
        let mut recorder =
            OrderedMapTraceRecorder::new(&base.structure_snapshot(), &base.canonical_snapshot())
                .expect("base state is valid");
        for _ in 0..MAX_TRACE_EVENTS {
            recorder.record(trace_event()).expect("limit is inclusive");
        }
        assert!(matches!(
            recorder.record(trace_event()),
            Err(MapError::ResourceLimit("trace event count"))
        ));
        let (events, _) = recorder.into_parts();
        assert_eq!(events.len(), MAX_TRACE_EVENTS);
    }

    #[test]
    fn patch_record_limit_is_a_recoverable_resource_error() {
        let base = initial_state();
        let mut recorder = OrderedMapTraceRecorder::new_with_limits(
            &base.structure_snapshot(),
            &base.canonical_snapshot(),
            1,
            0,
        )
        .expect("base state is valid");

        assert!(matches!(
            recorder.record_transition(
                trace_event(),
                vec![StatePatchRecord::Metric {
                    ordinal: MetricOrdinal::Comparisons,
                    before: 0,
                    after: 1,
                }],
            ),
            Err(MapError::ResourceLimit("state patch record count"))
        ));
    }

    #[test]
    fn failed_recorder_transition_preserves_state_and_tables() {
        let base = initial_state();
        let structure = base.structure_snapshot();
        let canonical = base.canonical_snapshot();
        let mut recorder =
            OrderedMapTraceRecorder::new(&structure, &canonical).expect("base state is valid");
        let removed = base
            .node(StructureEntityId::Node(NodeId(arena_key(1))))
            .cloned()
            .map(Box::new);

        assert!(matches!(
            recorder.record_transition(
                trace_event(),
                vec![StatePatchRecord::Node {
                    id: StructureEntityId::Node(NodeId(arena_key(1))),
                    before: removed,
                    after: None,
                }]
            ),
            Err(MapError::TraceEventState {
                message: "link references missing entity",
                ..
            })
        ));
        assert_eq!(recorder.state(), &base);
        let (events, patches) = recorder.into_parts();
        assert!(events.is_empty());
        assert!(patches.is_empty());
    }

    #[test]
    fn dangling_result_rolls_back_after_validation() {
        let mut state = initial_state();
        let original = state.clone();
        let removed = state
            .nodes
            .get(&StructureEntityId::Node(NodeId(arena_key(1))))
            .cloned()
            .map(Box::new);
        let records = vec![StatePatchRecord::Node {
            id: StructureEntityId::Node(NodeId(arena_key(1))),
            before: removed,
            after: None,
        }];

        assert!(matches!(
            state.apply_forward(&records),
            Err(MapError::TraceState("link references missing entity"))
        ));
        assert_eq!(state, original);
    }
}
