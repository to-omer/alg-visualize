//! Closed build-time registry for all ordered-map variants.

use visualizer_core::scenario::AlgorithmSpec;

use crate::trace_state::MAX_PATCH_RECORDS;
use crate::{
    AaMap, AvlMap, BTreeMap, CanonicalSnapshot, InvariantViolation, LlrbMap, MAX_VISUAL_ENTITIES,
    MapError, Operation, OperationResult, OrderedMap, OrderedMapTraceRecorder, ScapegoatMap,
    SkipListMap, SplayMap, StatePatchRecord, StructureSnapshot, TraceEvent, TreapMap, VebMap,
    WbtMap, XFastMap, YFastMap, ZipMap,
};

const CHECKPOINT_ACCOUNTING_SAFETY_FACTOR: usize = 4;
const CHECKPOINT_FIXED_OVERHEAD_BYTES: usize = 4 * 1024;

/// One operation result with its event and reversible patch tables.
#[derive(Clone, Debug)]
pub struct RecordedOperation {
    /// Public operation result.
    pub result: OperationResult,
    /// Atomic semantic events in execution order.
    pub trace: Vec<TraceEvent>,
    /// Commit-local reversible state patch table.
    pub patches: Vec<StatePatchRecord>,
}

/// Statically dispatched ordered-map implementation selected by Scenario.
#[derive(Clone, Debug)]
pub enum AlgorithmInstance {
    /// AVL tree.
    Avl(AvlMap),
    /// Weight-balanced tree.
    Wbt(WbtMap),
    /// AA tree.
    Aa(AaMap),
    /// Left-leaning red-black tree.
    Llrb(LlrbMap),
    /// Treap.
    Treap(TreapMap),
    /// Zip tree.
    Zip(ZipMap),
    /// Splay tree.
    Splay(SplayMap),
    /// Scapegoat tree.
    Scapegoat(ScapegoatMap),
    /// Skip list.
    SkipList(SkipListMap),
    /// B-tree.
    BTree(BTreeMap),
    /// Sparse van Emde Boas tree.
    Veb(VebMap),
    /// X-fast trie.
    XFast(XFastMap),
    /// Y-fast trie.
    YFast(Box<YFastMap>),
}

impl AlgorithmInstance {
    /// Constructs exactly the Scenario-selected implementation.
    ///
    /// # Errors
    ///
    /// Propagates bounded configuration or allocation errors without selecting
    /// a fallback implementation.
    pub fn from_spec(specification: &AlgorithmSpec, seed: u64) -> Result<Self, MapError> {
        match specification {
            AlgorithmSpec::Avl(_) => Ok(Self::Avl(AvlMap::new())),
            AlgorithmSpec::Wbt(_) => Ok(Self::Wbt(WbtMap::new())),
            AlgorithmSpec::Aa(_) => Ok(Self::Aa(AaMap::new())),
            AlgorithmSpec::Llrb(_) => Ok(Self::Llrb(LlrbMap::new())),
            AlgorithmSpec::Treap(_) => Ok(Self::Treap(TreapMap::new(seed))),
            AlgorithmSpec::Zip(_) => Ok(Self::Zip(ZipMap::new(seed))),
            AlgorithmSpec::Splay(_) => Ok(Self::Splay(SplayMap::new())),
            AlgorithmSpec::Scapegoat(configuration) => Ok(Self::Scapegoat(ScapegoatMap::new(
                configuration.alpha_numerator,
                configuration.alpha_denominator,
            )?)),
            AlgorithmSpec::SkipList(configuration) => {
                let denominator = match configuration.promotion.as_str() {
                    "1/2" => 2,
                    "1/4" => 4,
                    _ => return Err(MapError::InvalidConfiguration("skip-list promotion")),
                };
                Ok(Self::SkipList(SkipListMap::new(
                    seed,
                    u8::try_from(configuration.max_level)
                        .map_err(|_| MapError::InvalidConfiguration("skip-list max_level"))?,
                    denominator,
                )?))
            }
            AlgorithmSpec::BTree(configuration) => Ok(Self::BTree(BTreeMap::new(
                u8::try_from(configuration.min_degree)
                    .map_err(|_| MapError::InvalidConfiguration("B-tree minimum degree"))?,
            )?)),
            AlgorithmSpec::Veb(configuration) => Ok(Self::Veb(VebMap::new(
                u8::try_from(configuration.word_bits)
                    .map_err(|_| MapError::InvalidConfiguration("vEB word_bits"))?,
            )?)),
            AlgorithmSpec::XFast(configuration) => Ok(Self::XFast(XFastMap::new(
                seed,
                u8::try_from(configuration.word_bits)
                    .map_err(|_| MapError::InvalidConfiguration("X-fast word_bits"))?,
            )?)),
            AlgorithmSpec::YFast(configuration) => Ok(Self::YFast(Box::new(YFastMap::new(
                seed,
                u8::try_from(configuration.word_bits)
                    .map_err(|_| MapError::InvalidConfiguration("Y-fast word_bits"))?,
            )?))),
        }
    }

    /// Stable algorithm identifier used by plugin catalogs.
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Avl(_) => "avl",
            Self::Wbt(_) => "wbt",
            Self::Aa(_) => "aa",
            Self::Llrb(_) => "llrb",
            Self::Treap(_) => "treap",
            Self::Zip(_) => "zip",
            Self::Splay(_) => "splay",
            Self::Scapegoat(_) => "scapegoat",
            Self::SkipList(_) => "skip-list",
            Self::BTree(_) => "b-tree",
            Self::Veb(_) => "veb",
            Self::XFast(_) => "x-fast",
            Self::YFast(_) => "y-fast",
        }
    }

    /// Applies one operation and records its reversible state transitions.
    ///
    /// # Errors
    ///
    /// Propagates algorithm and trace-state validation failures.
    pub fn apply_recorded(&mut self, operation: Operation) -> Result<RecordedOperation, MapError> {
        let (candidate, recorded) = self.stage_recorded(operation)?;
        *self = candidate;
        Ok(recorded)
    }

    /// Applies one operation to an isolated candidate without changing this instance.
    ///
    /// The caller can perform serialization or other fallible publication work and
    /// install the returned candidate only after that work succeeds.
    ///
    /// # Errors
    ///
    /// Propagates algorithm and trace-state validation failures while preserving
    /// this instance exactly.
    pub fn stage_recorded(
        &self,
        operation: Operation,
    ) -> Result<(Self, RecordedOperation), MapError> {
        self.stage_recorded_with_limits(operation, crate::MAX_TRACE_EVENTS, MAX_PATCH_RECORDS)
    }

    /// Applies and validates one recorded operation without cloning this instance.
    ///
    /// This is the success-path primitive for a session that can reconstruct its
    /// previous committed boundary if this call or later publication fails.
    /// Callers without such a rollback boundary must use [`Self::apply_recorded`].
    ///
    /// # Errors
    ///
    /// Returns the first algorithm or trace validation failure. The caller must
    /// treat the instance as uncommitted and reconstruct it before reuse.
    pub fn apply_recorded_reconstructible(
        &mut self,
        operation: Operation,
    ) -> Result<RecordedOperation, MapError> {
        self.apply_recorded_in_place(operation, crate::MAX_TRACE_EVENTS, MAX_PATCH_RECORDS)
    }

    fn stage_recorded_with_limits(
        &self,
        operation: Operation,
        max_events: usize,
        max_patches: usize,
    ) -> Result<(Self, RecordedOperation), MapError> {
        let mut candidate = self.clone();
        let recorded = candidate.apply_recorded_in_place(operation, max_events, max_patches)?;
        Ok((candidate, recorded))
    }

    fn apply_recorded_in_place(
        &mut self,
        operation: Operation,
        max_events: usize,
        max_patches: usize,
    ) -> Result<RecordedOperation, MapError> {
        if self.structure_entity_count() > MAX_VISUAL_ENTITIES {
            return Err(MapError::ResourceLimit("visual entity count"));
        }
        let before_structure = self.structure_snapshot();
        let before_canonical = self.canonical_snapshot();
        let mut recorder = OrderedMapTraceRecorder::new_with_limits(
            &before_structure,
            &before_canonical,
            max_events,
            max_patches,
        )?;
        let result = match self {
            Self::Avl(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Wbt(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Aa(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Llrb(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Treap(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Zip(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Splay(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Scapegoat(map) => map.apply_traced(operation, &mut recorder)?,
            Self::SkipList(map) => map.apply_traced(operation, &mut recorder)?,
            Self::BTree(map) => map.apply_traced(operation, &mut recorder)?,
            Self::Veb(map) => map.apply_traced(operation, &mut recorder)?,
            Self::XFast(map) => map.apply_traced(operation, &mut recorder)?,
            Self::YFast(map) => map.apply_traced(operation, &mut recorder)?,
        };
        if self.structure_entity_count() > MAX_VISUAL_ENTITIES {
            return Err(MapError::ResourceLimit("visual entity count"));
        }
        recorder.verify_final(&self.structure_snapshot(), &self.canonical_snapshot())?;
        let (trace, patches) = recorder.into_parts();
        Ok(RecordedOperation {
            result,
            trace,
            patches,
        })
    }
}

macro_rules! dispatch {
    ($self:expr, $map:ident => $body:expr) => {
        match $self {
            AlgorithmInstance::Avl($map) => $body,
            AlgorithmInstance::Wbt($map) => $body,
            AlgorithmInstance::Aa($map) => $body,
            AlgorithmInstance::Llrb($map) => $body,
            AlgorithmInstance::Treap($map) => $body,
            AlgorithmInstance::Zip($map) => $body,
            AlgorithmInstance::Splay($map) => $body,
            AlgorithmInstance::Scapegoat($map) => $body,
            AlgorithmInstance::SkipList($map) => $body,
            AlgorithmInstance::BTree($map) => $body,
            AlgorithmInstance::Veb($map) => $body,
            AlgorithmInstance::XFast($map) => $body,
            AlgorithmInstance::YFast($map) => $body,
        }
    };
}

impl OrderedMap for AlgorithmInstance {
    fn apply(
        &mut self,
        operation: Operation,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<OperationResult, MapError> {
        dispatch!(self, map => map.apply(operation, trace))
    }

    fn canonical_snapshot(&self) -> CanonicalSnapshot {
        dispatch!(self, map => map.canonical_snapshot())
    }

    fn structure_snapshot(&self) -> StructureSnapshot {
        dispatch!(self, map => map.structure_snapshot())
    }

    fn structure_entity_count(&self) -> usize {
        dispatch!(self, map => map.structure_entity_count())
    }

    fn check_invariants(&self) -> Result<(), InvariantViolation> {
        dispatch!(self, map => map.check_invariants())
    }

    fn estimated_bytes(&self) -> usize {
        let owned_payload = dispatch!(self, map => map.estimated_bytes());
        owned_payload
            .saturating_mul(CHECKPOINT_ACCOUNTING_SAFETY_FACTOR)
            .saturating_add(CHECKPOINT_FIXED_OVERHEAD_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as Model;
    use std::collections::BTreeSet;

    use visualizer_core::scenario::{
        BTreeConfig, EmptyConfig, ScapegoatConfig, SkipListConfig, WordConfig,
    };

    use super::*;
    use crate::{EntryId, TraceState};

    type PublicModel = Model<u64, (EntryId, String)>;

    fn specifications() -> Vec<AlgorithmSpec> {
        vec![
            AlgorithmSpec::Avl(EmptyConfig::default()),
            AlgorithmSpec::Wbt(EmptyConfig::default()),
            AlgorithmSpec::Aa(EmptyConfig::default()),
            AlgorithmSpec::Llrb(EmptyConfig::default()),
            AlgorithmSpec::Treap(EmptyConfig::default()),
            AlgorithmSpec::Zip(EmptyConfig::default()),
            AlgorithmSpec::Splay(EmptyConfig::default()),
            AlgorithmSpec::Scapegoat(ScapegoatConfig {
                alpha_numerator: 2,
                alpha_denominator: 3,
            }),
            AlgorithmSpec::SkipList(SkipListConfig {
                promotion: "1/2".to_owned(),
                max_level: 16,
            }),
            AlgorithmSpec::BTree(BTreeConfig { min_degree: 3 }),
            AlgorithmSpec::Veb(WordConfig { word_bits: 64 }),
            AlgorithmSpec::XFast(WordConfig { word_bits: 64 }),
            AlgorithmSpec::YFast(WordConfig { word_bits: 64 }),
        ]
    }

    fn apply(map: &mut AlgorithmInstance, operation: Operation) -> OperationResult {
        let mut trace = Vec::new();
        let result = map.apply(operation, &mut trace).unwrap();
        assert!(!trace.is_empty(), "{} emitted an empty trace", map.id());
        map.check_invariants()
            .unwrap_or_else(|error| panic!("{} failed its invariant: {error}", map.id()));
        result
    }

    fn apply_recorded_round_trip(
        map: &mut AlgorithmInstance,
        operation: Operation,
    ) -> RecordedOperation {
        let operation_description = format!("{operation:?}");
        let before_structure = map.structure_snapshot();
        let before_canonical = map.canonical_snapshot();
        let recorded = map.apply_recorded(operation).unwrap_or_else(|error| {
            panic!(
                "{} recorded {operation_description} failed: {error}",
                map.id()
            )
        });
        let mut state = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("independent base snapshot is valid");
        for event in &recorded.trace {
            let start = usize::try_from(event.patch_start).expect("patch offset fits usize");
            let count = usize::try_from(event.patch_count).expect("patch count fits usize");
            let end = start
                .checked_add(count)
                .expect("patch span does not overflow");
            let records = &recorded.patches[start..end];
            let rotation_metrics = records
                .iter()
                .filter_map(|record| match record {
                    StatePatchRecord::Metric {
                        ordinal: crate::MetricOrdinal::Rotations,
                        before,
                        after,
                    } => Some(after - before),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if matches!(
                event.kind,
                crate::TraceKind::RotateLeft | crate::TraceKind::RotateRight
            ) {
                assert_eq!(
                    rotation_metrics,
                    vec![1],
                    "{} rotation event {} must own exactly one rotation",
                    map.id(),
                    event.catalog_id
                );
                let changed_nodes = records
                    .iter()
                    .filter(|record| matches!(record, StatePatchRecord::Node { .. }))
                    .count();
                assert!(
                    (2..=3).contains(&changed_nodes),
                    "{} rotation event {} must change both endpoints",
                    map.id(),
                    event.catalog_id
                );
            } else {
                assert!(
                    rotation_metrics.is_empty(),
                    "{} non-rotation event {} aggregated a rotation",
                    map.id(),
                    event.catalog_id
                );
            }
            state.apply_forward(records).unwrap_or_else(|error| {
                panic!(
                    "{} event {} does not apply forward: {error}",
                    map.id(),
                    event.catalog_id
                )
            });
        }
        let final_state =
            TraceState::from_snapshots(&map.structure_snapshot(), &map.canonical_snapshot())
                .expect("independent final snapshot is valid");
        assert_eq!(state, final_state, "{} forward state", map.id());
        for event in recorded.trace.iter().rev() {
            let start = usize::try_from(event.patch_start).expect("patch offset fits usize");
            let count = usize::try_from(event.patch_count).expect("patch count fits usize");
            let end = start
                .checked_add(count)
                .expect("patch span does not overflow");
            state
                .apply_reverse(&recorded.patches[start..end])
                .unwrap_or_else(|error| {
                    panic!(
                        "{} event {} does not apply in reverse: {error}",
                        map.id(),
                        event.catalog_id
                    )
                });
        }
        let base_state = TraceState::from_snapshots(&before_structure, &before_canonical)
            .expect("independent base snapshot remains valid");
        assert_eq!(state, base_state, "{} reverse state", map.id());
        recorded
    }

    fn assert_snapshot(map: &AlgorithmInstance, model: &PublicModel) {
        let actual: Vec<_> = map
            .canonical_snapshot()
            .entries
            .into_iter()
            .map(|entry| (entry.key, (entry.id, entry.value)))
            .collect();
        let expected = model
            .iter()
            .map(|(key, value)| (*key, value.clone()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{} canonical snapshot", map.id());
    }

    fn insert_matches(
        map: &mut AlgorithmInstance,
        model: &mut PublicModel,
        key: u64,
        value: String,
    ) {
        let expected = model.get(&key).cloned();
        let actual = apply(
            map,
            Operation::Insert {
                key,
                value: value.clone(),
            },
        );
        let entry = match (expected, actual) {
            (None, OperationResult::Inserted { entry }) => entry,
            (
                Some((expected_entry, expected_previous)),
                OperationResult::Overwritten { entry, previous },
            ) => {
                assert_eq!(entry, expected_entry, "{} overwrite identity", map.id());
                assert_eq!(previous, expected_previous, "{} overwrite value", map.id());
                entry
            }
            (expected, actual) => panic!(
                "{} insert({key}) mismatch: expected {expected:?}, got {actual:?}",
                map.id()
            ),
        };
        model.insert(key, (entry, value));
    }

    fn remove_matches(map: &mut AlgorithmInstance, model: &mut PublicModel, key: u64) {
        match (model.remove(&key), apply(map, Operation::Remove { key })) {
            (None, OperationResult::Miss) => {}
            (Some((expected_entry, expected_value)), OperationResult::Removed { entry, value }) => {
                assert_eq!(entry, expected_entry, "{} removed identity", map.id());
                assert_eq!(value, expected_value, "{} removed value", map.id());
            }
            (expected, actual) => panic!(
                "{} remove({key}) mismatch: expected {expected:?}, got {actual:?}",
                map.id()
            ),
        }
    }

    fn get_matches(map: &mut AlgorithmInstance, model: &PublicModel, query: u64) {
        match (model.get(&query), apply(map, Operation::Get { key: query })) {
            (None, OperationResult::Miss) => {}
            (
                Some((expected_entry, expected_value)),
                OperationResult::Found { entry, key, value },
            ) => {
                assert_eq!(entry, *expected_entry, "{} get identity", map.id());
                assert_eq!(key, query, "{} get key", map.id());
                assert_eq!(value, *expected_value, "{} get value", map.id());
            }
            (expected, actual) => panic!(
                "{} get({query}) mismatch: expected {expected:?}, got {actual:?}",
                map.id()
            ),
        }
    }

    fn lower_bound_matches(map: &mut AlgorithmInstance, model: &PublicModel, query: u64) {
        match (
            model.range(query..).next(),
            apply(map, Operation::LowerBound { key: query }),
        ) {
            (None, OperationResult::Miss) => {}
            (
                Some((expected_key, (expected_entry, expected_value))),
                OperationResult::Found { entry, key, value },
            ) => {
                assert_eq!(entry, *expected_entry, "{} lower-bound identity", map.id());
                assert_eq!(key, *expected_key, "{} lower-bound key", map.id());
                assert_eq!(value, *expected_value, "{} lower-bound value", map.id());
            }
            (expected, actual) => panic!(
                "{} lower_bound({query}) mismatch: expected {expected:?}, got {actual:?}",
                map.id()
            ),
        }
    }

    fn exercise_u64_max(map: &mut AlgorithmInstance, model: &mut PublicModel) {
        let boundary = u64::MAX;
        insert_matches(map, model, boundary, "maximum".to_owned());
        insert_matches(map, model, boundary, "overwritten".to_owned());
        get_matches(map, model, boundary);
        assert_snapshot(map, model);
        remove_matches(map, model, boundary);
        get_matches(map, model, boundary);
        assert_snapshot(map, model);
    }

    #[test]
    fn staged_recording_failure_preserves_the_algorithm_exactly() {
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 7)
                .expect("fixture configuration is valid");
            apply(
                &mut map,
                Operation::Insert {
                    key: 4,
                    value: "before".to_owned(),
                },
            );
            let before_structure = map.structure_snapshot();
            let before_canonical = map.canonical_snapshot();
            let before_bytes = map.estimated_bytes();

            assert!(matches!(
                map.stage_recorded_with_limits(
                    Operation::Insert {
                        key: 9,
                        value: "after".to_owned(),
                    },
                    0,
                    MAX_PATCH_RECORDS,
                ),
                Err(MapError::ResourceLimit("trace event count"))
            ));
            assert_eq!(map.structure_snapshot(), before_structure, "{}", map.id());
            assert_eq!(map.canonical_snapshot(), before_canonical, "{}", map.id());
            assert_eq!(map.estimated_bytes(), before_bytes, "{}", map.id());
            map.check_invariants().expect("original map remains valid");
        }
    }

    #[test]
    fn checkpoint_accounting_grows_for_every_registered_variant() {
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 11)
                .expect("fixture configuration is valid");
            let empty_bytes = map.estimated_bytes();
            apply(
                &mut map,
                Operation::Insert {
                    key: 17,
                    value: "owned-payload".repeat(8),
                },
            );
            assert!(
                map.estimated_bytes() > empty_bytes,
                "{} checkpoint accounting ignored owned growth",
                map.id()
            );
        }
    }

    #[test]
    fn shared_contract_fixture_covers_each_algorithm_once() {
        let ids = specifications()
            .into_iter()
            .map(|specification| {
                AlgorithmInstance::from_spec(&specification, 0)
                    .expect("registered fixture configuration is valid")
                    .id()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "avl",
                "wbt",
                "aa",
                "llrb",
                "treap",
                "zip",
                "splay",
                "scapegoat",
                "skip-list",
                "b-tree",
                "veb",
                "x-fast",
                "y-fast",
            ]
        );
    }

    #[test]
    fn every_registered_algorithm_matches_the_same_public_model() {
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 123).unwrap();
            let mut model = PublicModel::new();
            for round in 0_u64..4 {
                for key in (0_u64..128).map(|key| (key * 73 + round * 19) % 128) {
                    insert_matches(&mut map, &mut model, key, format!("{round}:{key}"));
                }
                for key in (0_u64..128).filter(|key| (key + round) % 3 == 0) {
                    remove_matches(&mut map, &mut model, key);
                }
                assert_snapshot(&map, &model);
            }
            for query in 0_u64..130 {
                get_matches(&mut map, &model, query);
                lower_bound_matches(&mut map, &model, query);
            }
            exercise_u64_max(&mut map, &mut model);
        }
    }

    #[test]
    fn recorded_operations_reach_their_independent_final_snapshot() {
        let specifications = [
            AlgorithmSpec::Avl(EmptyConfig::default()),
            AlgorithmSpec::Wbt(EmptyConfig::default()),
            AlgorithmSpec::Aa(EmptyConfig::default()),
        ];
        for specification in specifications {
            let mut map = AlgorithmInstance::from_spec(&specification, 123).unwrap();
            for (key, value) in [(8, "root"), (3, "left"), (12, "right")] {
                apply(
                    &mut map,
                    Operation::Insert {
                        key,
                        value: value.to_owned(),
                    },
                );
            }

            let recorded = map
                .apply_recorded(Operation::Insert {
                    key: 6,
                    value: "new".to_owned(),
                })
                .unwrap_or_else(|error| panic!("{} insert trace failed: {error}", map.id()));
            assert!(!recorded.patches.is_empty(), "{} insert patches", map.id());
            assert_eq!(
                recorded.trace.last().map(|event| event.kind),
                Some(crate::TraceKind::Result),
                "{} terminal result event",
                map.id()
            );
            let covered: usize = recorded
                .trace
                .iter()
                .map(|event| usize::try_from(event.patch_count).unwrap())
                .sum();
            assert_eq!(covered, recorded.patches.len(), "{} patch spans", map.id());

            let query = map
                .apply_recorded(Operation::Get { key: 12 })
                .unwrap_or_else(|error| panic!("{} query trace failed: {error}", map.id()));
            assert!(
                !query.patches.is_empty(),
                "{} query metric patches",
                map.id()
            );
            assert_eq!(
                query.trace.last().map(|event| event.kind),
                Some(crate::TraceKind::Result)
            );

            let removed = map
                .apply_recorded(Operation::Remove { key: 6 })
                .unwrap_or_else(|error| panic!("{} remove trace failed: {error}", map.id()));
            assert!(!removed.patches.is_empty(), "{} remove patches", map.id());
            assert_eq!(
                removed.trace.last().map(|event| event.kind),
                Some(crate::TraceKind::Result)
            );
        }
    }

    #[test]
    fn recorded_mixed_updates_cover_every_registered_algorithm() {
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 456).unwrap();
            for key in (0_u64..48).map(|key| (key * 29) % 48) {
                let recorded = apply_recorded_round_trip(
                    &mut map,
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                );
                assert_eq!(
                    recorded.trace.last().map(|event| event.kind),
                    Some(crate::TraceKind::Result)
                );
            }
            for key in (0_u64..48).filter(|key| key % 3 != 1) {
                apply_recorded_round_trip(&mut map, Operation::Remove { key });
                map.check_invariants()
                    .unwrap_or_else(|error| panic!("{} remove({key}): {error}", map.id()));
            }
        }
    }

    #[test]
    fn recorded_pseudorandom_operations_round_trip_for_every_algorithm() {
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 789).unwrap();
            let mut random = 0x6a09_e667_f3bc_c909_u64;
            for step in 0_u64..96 {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let key = (random >> 17) % 32;
                let operation = match random & 3 {
                    0 => Operation::Insert {
                        key,
                        value: format!("{step}:{key}"),
                    },
                    1 => Operation::Remove { key },
                    2 => Operation::Get { key },
                    _ => Operation::LowerBound { key },
                };
                apply_recorded_round_trip(&mut map, operation);
                map.check_invariants().unwrap_or_else(|error| {
                    panic!("{} operation {step} broke invariants: {error}", map.id())
                });
            }
        }
    }

    #[test]
    fn comparison_traces_do_not_repeat_a_top_down_search() {
        const COMPARISON_STRUCTURES: [&str; 10] = [
            "avl",
            "wbt",
            "aa",
            "llrb",
            "treap",
            "zip",
            "splay",
            "scapegoat",
            "b-tree",
            "y-fast",
        ];
        let operations = [
            Operation::Insert {
                key: 6,
                value: "new".to_owned(),
            },
            Operation::Insert {
                key: 3,
                value: "overwritten".to_owned(),
            },
            Operation::Get { key: 12 },
            Operation::LowerBound { key: 7 },
            Operation::Remove { key: 3 },
        ];

        for specification in specifications() {
            for operation in &operations {
                let mut map = AlgorithmInstance::from_spec(&specification, 1_337).unwrap();
                for key in [8_u64, 3, 12] {
                    apply(
                        &mut map,
                        Operation::Insert {
                            key,
                            value: key.to_string(),
                        },
                    );
                }
                let recorded = apply_recorded_round_trip(&mut map, operation.clone());
                if !COMPARISON_STRUCTURES.contains(&map.id()) {
                    continue;
                }

                let mut compared = BTreeSet::new();
                let mut previous_node = None;
                let mut descended_since_compare = false;
                for event in &recorded.trace {
                    if event.kind == crate::TraceKind::Compare {
                        let identity = (event.node, event.entry, event.key);
                        assert!(
                            compared.insert(identity),
                            "{} {operation:?} repeated comparison {identity:?} without an intervening structural mutation",
                            map.id()
                        );
                        if previous_node.is_some_and(|node| Some(node) != event.node) {
                            assert!(
                                descended_since_compare,
                                "{} {operation:?} changed comparison nodes without a descend event",
                                map.id()
                            );
                        }
                        previous_node = event.node;
                        descended_since_compare = false;
                    } else if event.kind == crate::TraceKind::Descend {
                        descended_since_compare = true;
                    }

                    let start = usize::try_from(event.patch_start).expect("patch offset fits");
                    let end = start + usize::try_from(event.patch_count).expect("patch count fits");
                    if recorded.patches[start..end].iter().any(|record| {
                        matches!(
                            record,
                            StatePatchRecord::Root { .. }
                                | StatePatchRecord::Node { .. }
                                | StatePatchRecord::Entry { .. }
                        )
                    }) {
                        compared.clear();
                        previous_node = None;
                        descended_since_compare = false;
                    }
                }
            }
        }
    }

    #[test]
    fn avl_rotation_patches_remain_local_as_the_tree_grows() {
        for size in [2_u64, 126, 1_022] {
            let mut map = AlgorithmInstance::Avl(AvlMap::new());
            for key in 0..size {
                apply(
                    &mut map,
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                );
            }
            let recorded = map
                .apply_recorded(Operation::Insert {
                    key: size,
                    value: size.to_string(),
                })
                .expect("ascending insertion remains traceable");
            let rotations = recorded
                .trace
                .iter()
                .filter(|event| {
                    matches!(
                        event.kind,
                        crate::TraceKind::RotateLeft | crate::TraceKind::RotateRight
                    )
                })
                .collect::<Vec<_>>();
            assert!(!rotations.is_empty(), "size {size} must rotate");
            for event in rotations {
                let start = usize::try_from(event.patch_start).expect("patch offset fits");
                let end = start + usize::try_from(event.patch_count).expect("patch count fits");
                let changed_nodes = recorded.patches[start..end]
                    .iter()
                    .filter(|record| matches!(record, StatePatchRecord::Node { .. }))
                    .count();
                assert!(
                    (2..=3).contains(&changed_nodes),
                    "size {size} copied {changed_nodes} nodes for one rotation"
                );
            }
        }
    }

    #[test]
    fn navigation_events_preserve_projected_identity_and_real_link_targets() {
        const AUXILIARY_STRUCTURES: [&str; 4] = ["skip-list", "veb", "x-fast", "y-fast"];
        for specification in specifications() {
            let mut map = AlgorithmInstance::from_spec(&specification, 2_021).unwrap();
            if !AUXILIARY_STRUCTURES.contains(&map.id()) {
                continue;
            }
            for key in [1_u64, 3, 7, 9, 12] {
                apply(
                    &mut map,
                    Operation::Insert {
                        key,
                        value: key.to_string(),
                    },
                );
            }
            let structure = map.structure_snapshot();
            let recorded = map
                .apply_recorded(Operation::Get { key: 12 })
                .expect("query trace succeeds");

            for event in &recorded.trace {
                if let Some(node) = event.node {
                    assert!(
                        structure.nodes.iter().any(|candidate| candidate.id == node),
                        "{} emitted an unprojected event identity {node:?}",
                        map.id()
                    );
                }
                if let Some(target) = event.target {
                    let source = event.node.expect("a traversal target has a source");
                    let source = structure
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == source)
                        .expect("source identity is projected");
                    assert!(
                        source.links.iter().any(|link| link.target == target),
                        "{} emitted a traversal that is not a projected link: {:?} -> {target:?}",
                        map.id(),
                        source.id
                    );
                }
            }
        }
    }
}
