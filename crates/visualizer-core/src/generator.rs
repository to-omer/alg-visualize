//! Deterministic transition-class scheduler for generated operations.

use std::array;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::index::{IndexError, UniverseIndex};
use crate::provenance::{
    GeneratorProvenanceV1, GeneratorStats, MaterializedItem, ProvenanceError,
    SUPPORTED_GENERATOR_REVISION, materialized_digest,
};
use crate::rng::{RngError, RngV1};
use crate::scenario::{Entry, GeneratorProvenanceJson, Operation};

const CLASS_COUNT: usize = 5;

/// State transition required by a generated descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[repr(u8)]
pub enum TransitionClass {
    /// Insert a currently absent key and increase the set size.
    NewInsert = 0,
    /// Remove a present key and decrease the set size.
    HitRemove = 1,
    /// Perform a zero-size transition that needs at least one present key.
    RequiresPresent = 2,
    /// Perform a zero-size transition that needs at least one absent key.
    RequiresAbsent = 3,
    /// Perform an operation independent of present/absent candidates.
    AlwaysFeasible = 4,
}

/// Operation type used to derive canonical descriptor IDs and transition classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GeneratedOperationKind {
    /// Insert new or overwrite.
    Insert = 0,
    /// Remove hit or miss.
    Remove = 1,
    /// Get hit or miss.
    Get = 2,
    /// Lower-bound is feasible independently of current set size.
    LowerBound = 3,
}

impl TransitionClass {
    const ALL: [Self; CLASS_COUNT] = [
        Self::NewInsert,
        Self::HitRemove,
        Self::RequiresPresent,
        Self::RequiresAbsent,
        Self::AlwaysFeasible,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

/// Generated descriptor identity and its one-time priority draw.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Descriptor {
    /// Canonical ID derived from operation type and type-local ordinal.
    pub id: u32,
    /// Exactly one draw from the descriptor-order RNG stream.
    pub priority: u64,
    /// Size transition class used for scheduling.
    pub class: TransitionClass,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct QueueEntry {
    priority: u64,
    id: u32,
}

impl From<Descriptor> for QueueEntry {
    fn from(descriptor: Descriptor) -> Self {
        Self {
            priority: descriptor.priority,
            id: descriptor.id,
        }
    }
}

/// Set-size state needed by the exact feasibility predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FeasibilityState {
    /// Current number of present keys.
    pub size: u128,
    /// Number of keys in the inclusive generator universe.
    pub capacity: u128,
}

/// Generator schedule validation or execution error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ScheduleError {
    /// Initial size exceeds the key universe.
    #[error("generator size exceeds universe capacity")]
    InvalidState,
    /// No ordering can satisfy the requested transition counts.
    #[error("generator descriptors are infeasible")]
    Infeasible,
    /// Two descriptors used the same canonical ID.
    #[error("duplicate generator descriptor ID")]
    DuplicateDescriptorId,
    /// A requested hit/overwrite count exceeds its operation-type count.
    #[error("generator special outcome count exceeds operation count")]
    InvalidOutcomeCount,
}

/// Key-selection distribution for generated inputs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "kind", rename_all = "snake_case")]
pub enum KeyDistribution {
    /// Uniformly sample the feasible candidate set.
    Uniform,
    /// Select the next feasible key in ascending order, wrapping at the end.
    Ascending,
    /// Select the next feasible key in descending order, wrapping at the start.
    Descending,
    /// Sample the first fifth with probability four fifths, otherwise all keys.
    Hotspot,
}

/// Initial-entry generator contract.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InitialGeneratorSpec {
    /// Generator seed as canonical unsigned decimal.
    pub seed: String,
    /// Number of materialized insert entries.
    pub count: u32,
    /// Inclusive minimum key as canonical unsigned decimal.
    pub key_min: String,
    /// Inclusive maximum key as canonical unsigned decimal.
    pub key_max: String,
    /// Candidate selection distribution.
    pub distribution: KeyDistribution,
    /// Prefix retained in every generated value.
    pub value_prefix: String,
    /// Maximum generated Unicode scalar-value count.
    pub value_max_scalar_values: u32,
    /// Target overwrite rate in basis points.
    pub overwrite_rate_bps: u32,
}

/// Non-negative operation weights.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OperationWeights {
    /// Insert weight.
    pub insert: u32,
    /// Remove weight.
    pub remove: u32,
    /// Get weight.
    pub get: u32,
    /// Lower-bound weight.
    pub lower_bound: u32,
}

/// Operation-stream generator contract.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OperationGeneratorSpec {
    /// Generator seed as canonical unsigned decimal.
    pub seed: String,
    /// Number of materialized operations.
    pub count: u32,
    /// Inclusive minimum key as canonical unsigned decimal.
    pub key_min: String,
    /// Inclusive maximum key as canonical unsigned decimal.
    pub key_max: String,
    /// Candidate selection distribution.
    pub distribution: KeyDistribution,
    /// Prefix retained in every generated insert value.
    pub value_prefix: String,
    /// Maximum generated Unicode scalar-value count.
    pub value_max_scalar_values: u32,
    /// Operation-type weights.
    pub weights: OperationWeights,
    /// Target successful-get rate in basis points.
    pub get_hit_rate_bps: u32,
    /// Target successful-remove rate in basis points.
    pub remove_hit_rate_bps: u32,
    /// Target insert-overwrite rate in basis points.
    pub insert_overwrite_rate_bps: u32,
}

/// Materialized initial entries and verifiable provenance.
#[derive(Clone, Debug, Serialize)]
pub struct GeneratedInitial {
    /// Generated entry sequence.
    pub entries: Vec<Entry>,
    /// Generator provenance persisted with the materialized sequence.
    pub provenance: GeneratorProvenanceJson,
    /// Exact achieved statistics.
    pub stats: GeneratorStats,
}

/// Materialized operations and verifiable provenance.
#[derive(Clone, Debug, Serialize)]
pub struct GeneratedOperations {
    /// Generated operation sequence.
    pub operations: Vec<Operation>,
    /// Generator provenance persisted with the materialized sequence.
    pub provenance: GeneratorProvenanceJson,
    /// Exact achieved statistics.
    pub stats: GeneratorStats,
}

/// Generator input, feasibility, or bounded-randomness failure.
#[derive(Debug, Error)]
pub enum GenerationError {
    /// A bounded generator field is invalid.
    #[error("invalid generator setting: {0}")]
    Invalid(&'static str),
    /// The requested transition multiset has no feasible ordering.
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
    /// The present/absent rank index rejected a transition.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// A bounded RNG primitive could not complete.
    #[error(transparent)]
    Rng(#[from] RngError),
    /// Materialized provenance could not be encoded.
    #[error(transparent)]
    Provenance(#[from] ProvenanceError),
    /// A typed provenance payload could not be represented as JSON.
    #[error("generator provenance serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Clone, Copy)]
struct CommonSpec<'a> {
    seed: &'a str,
    count: u32,
    key_min: &'a str,
    key_max: &'a str,
    distribution: KeyDistribution,
    value_prefix: &'a str,
    value_max_scalar_values: u32,
    count_limit: u32,
}

#[derive(Clone, Copy)]
struct ValidatedCommon {
    seed: u64,
    count: u32,
    minimum: u64,
    maximum: u64,
    capacity: u128,
    distribution: KeyDistribution,
    suffix_capacity: u32,
}

struct OperationSchedule {
    counts: [u32; 4],
    special: [u32; 4],
    descriptors: Vec<Descriptor>,
}

/// Generates deterministic initial inserts from an empty map.
///
/// # Errors
///
/// Rejects invalid bounds/rates/value limits and infeasible overwrite targets.
pub fn generate_initial(spec: &InitialGeneratorSpec) -> Result<GeneratedInitial, GenerationError> {
    let common = validate_common(CommonSpec {
        seed: &spec.seed,
        count: spec.count,
        key_min: &spec.key_min,
        key_max: &spec.key_max,
        distribution: spec.distribution,
        value_prefix: &spec.value_prefix,
        value_max_scalar_values: spec.value_max_scalar_values,
        count_limit: 10_000,
    })?;
    validate_rate(spec.overwrite_rate_bps)?;
    let overwrite_count = target_count(common.count, spec.overwrite_rate_bps)?;
    let mut descriptor_rng =
        RngV1::from_seed(common.seed, "rng.generator.initial.descriptor-order");
    let descriptors = build_descriptors(
        GeneratedOperationKind::Insert,
        common.count,
        overwrite_count,
        &mut descriptor_rng,
    )?;
    let mut scheduler = DescriptorScheduler::new(
        FeasibilityState {
            size: 0,
            capacity: common.capacity,
        },
        descriptors,
    )?;
    let mut index = UniverseIndex::new(common.minimum, common.maximum)?;
    let mut selector = KeySelector::new(common);
    let mut key_rng = RngV1::from_seed(common.seed, "rng.generator.initial.key-selection");
    let mut value_rng = RngV1::from_seed(common.seed, "rng.generator.initial.value");
    let mut entries = Vec::with_capacity(common.count as usize);

    while let Some(descriptor) = scheduler.next_descriptor()? {
        let candidate = candidate_for(descriptor.class);
        let key = selector.select(&index, candidate, &mut key_rng)?;
        if descriptor.class == TransitionClass::NewInsert {
            index.insert(key)?;
        }
        entries.push(Entry {
            key: key.to_string(),
            value: generate_value(
                spec.value_prefix.as_str(),
                common.suffix_capacity,
                &mut value_rng,
            )?,
        });
    }

    let stats = GeneratorStats {
        item_count: common.count,
        final_unique_count: u32::try_from(index.present_len())
            .map_err(|_| GenerationError::Invalid("generated unique count overflow"))?,
        insert_count: common.count,
        achieved_insert_overwrites: overwrite_count,
        ..GeneratorStats::default()
    };
    let materialized = entries
        .iter()
        .map(|entry| MaterializedItem::Entry {
            key: entry.key.clone(),
            value: entry.value.clone(),
        })
        .collect::<Vec<_>>();
    let provenance = make_provenance(spec, &materialized, &stats)?;
    Ok(GeneratedInitial {
        entries,
        provenance,
        stats,
    })
}

/// Generates deterministic operations from the map state produced by `initial`.
///
/// # Errors
///
/// Rejects invalid settings, an initial key outside `u64`, and any infeasible
/// requested hit/miss/overwrite mix.
pub fn generate_operations(
    spec: &OperationGeneratorSpec,
    initial: &[Entry],
) -> Result<GeneratedOperations, GenerationError> {
    let common = validate_common(CommonSpec {
        seed: &spec.seed,
        count: spec.count,
        key_min: &spec.key_min,
        key_max: &spec.key_max,
        distribution: spec.distribution,
        value_prefix: &spec.value_prefix,
        value_max_scalar_values: spec.value_max_scalar_values,
        count_limit: 100_000,
    })?;
    let schedule = operation_descriptors(common, spec)?;
    let counts = schedule.counts;
    let special = schedule.special;
    let descriptors = schedule.descriptors;

    let mut values = BTreeMap::new();
    let mut index = UniverseIndex::new(common.minimum, common.maximum)?;
    for entry in initial {
        let key = parse_canonical_u64(&entry.key)?;
        values.insert(key, entry.value.clone());
        if (common.minimum..=common.maximum).contains(&key) {
            match index.insert(key) {
                Ok(()) | Err(IndexError::AlreadyPresent) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    let mut scheduler = DescriptorScheduler::new(
        FeasibilityState {
            size: u128::from(index.present_len()),
            capacity: common.capacity,
        },
        descriptors,
    )?;
    let mut selector = KeySelector::new(common);
    let mut key_rng = RngV1::from_seed(common.seed, "rng.generator.operations.key-selection");
    let mut value_rng = RngV1::from_seed(common.seed, "rng.generator.operations.value");
    let mut operations = Vec::with_capacity(common.count as usize);

    while let Some(descriptor) = scheduler.next_descriptor()? {
        let kind = descriptor_kind(descriptor.id)?;
        let key = selector.select(&index, candidate_for(descriptor.class), &mut key_rng)?;
        let key_string = key.to_string();
        let operation = match kind {
            GeneratedOperationKind::Insert => {
                let value = generate_value(
                    spec.value_prefix.as_str(),
                    common.suffix_capacity,
                    &mut value_rng,
                )?;
                if descriptor.class == TransitionClass::NewInsert {
                    index.insert(key)?;
                }
                values.insert(key, value.clone());
                Operation::Insert {
                    key: key_string,
                    value,
                }
            }
            GeneratedOperationKind::Remove => {
                if descriptor.class == TransitionClass::HitRemove {
                    index.remove(key)?;
                    values.remove(&key);
                }
                Operation::Remove { key: key_string }
            }
            GeneratedOperationKind::Get => Operation::Get { key: key_string },
            GeneratedOperationKind::LowerBound => Operation::LowerBound { key: key_string },
        };
        operations.push(operation);
    }

    let stats = GeneratorStats {
        item_count: common.count,
        final_unique_count: u32::try_from(values.len())
            .map_err(|_| GenerationError::Invalid("generated unique count overflow"))?,
        insert_count: counts[0],
        remove_count: counts[1],
        get_count: counts[2],
        lower_bound_count: counts[3],
        achieved_get_hits: special[2],
        achieved_remove_hits: special[1],
        achieved_insert_overwrites: special[0],
    };
    let materialized = operations
        .iter()
        .map(materialized_operation)
        .collect::<Vec<_>>();
    let provenance = make_provenance(spec, &materialized, &stats)?;
    Ok(GeneratedOperations {
        operations,
        provenance,
        stats,
    })
}

fn validate_common(spec: CommonSpec<'_>) -> Result<ValidatedCommon, GenerationError> {
    if spec.count > spec.count_limit {
        return Err(GenerationError::Invalid("generator item count limit"));
    }
    if spec.value_max_scalar_values > 256 {
        return Err(GenerationError::Invalid("generator value length limit"));
    }
    let prefix_scalars = u32::try_from(spec.value_prefix.chars().count())
        .map_err(|_| GenerationError::Invalid("generator value prefix length"))?;
    if prefix_scalars > spec.value_max_scalar_values {
        return Err(GenerationError::Invalid(
            "value prefix exceeds maximum scalar count",
        ));
    }
    let seed = parse_canonical_u64(spec.seed)?;
    let minimum = parse_canonical_u64(spec.key_min)?;
    let maximum = parse_canonical_u64(spec.key_max)?;
    if minimum > maximum {
        return Err(GenerationError::Invalid("generator key range is reversed"));
    }
    Ok(ValidatedCommon {
        seed,
        count: spec.count,
        minimum,
        maximum,
        capacity: u128::from(maximum) - u128::from(minimum) + 1,
        distribution: spec.distribution,
        suffix_capacity: spec.value_max_scalar_values - prefix_scalars,
    })
}

fn validate_rate(rate: u32) -> Result<(), GenerationError> {
    if rate > 10_000 {
        return Err(GenerationError::Invalid(
            "generator rate exceeds 10,000 basis points",
        ));
    }
    Ok(())
}

fn parse_canonical_u64(value: &str) -> Result<u64, GenerationError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(GenerationError::Invalid("noncanonical generator u64"));
    }
    value
        .parse()
        .map_err(|_| GenerationError::Invalid("generator u64 overflow"))
}

fn target_count(count: u32, rate_bps: u32) -> Result<u32, GenerationError> {
    u32::try_from((u64::from(count) * u64::from(rate_bps) + 5_000) / 10_000)
        .map_err(|_| GenerationError::Invalid("generator target count overflow"))
}

fn operation_descriptors(
    common: ValidatedCommon,
    spec: &OperationGeneratorSpec,
) -> Result<OperationSchedule, GenerationError> {
    validate_rate(spec.get_hit_rate_bps)?;
    validate_rate(spec.remove_hit_rate_bps)?;
    validate_rate(spec.insert_overwrite_rate_bps)?;
    let counts = allocate_operation_counts(common.count, spec.weights)?;
    let special = [
        target_count(counts[0], spec.insert_overwrite_rate_bps)?,
        target_count(counts[1], spec.remove_hit_rate_bps)?,
        target_count(counts[2], spec.get_hit_rate_bps)?,
        0,
    ];
    let mut rng = RngV1::from_seed(common.seed, "rng.generator.operations.descriptor-order");
    let mut descriptors = Vec::with_capacity(common.count as usize);
    for (index, kind) in [
        GeneratedOperationKind::Insert,
        GeneratedOperationKind::Remove,
        GeneratedOperationKind::Get,
        GeneratedOperationKind::LowerBound,
    ]
    .into_iter()
    .enumerate()
    {
        descriptors.extend(build_descriptors(
            kind,
            counts[index],
            special[index],
            &mut rng,
        )?);
    }
    Ok(OperationSchedule {
        counts,
        special,
        descriptors,
    })
}

fn allocate_operation_counts(
    count: u32,
    weights: OperationWeights,
) -> Result<[u32; 4], GenerationError> {
    let weights = [
        weights.insert,
        weights.remove,
        weights.get,
        weights.lower_bound,
    ];
    let total = weights.iter().map(|weight| u64::from(*weight)).sum::<u64>();
    if total == 0 {
        return Err(GenerationError::Invalid(
            "operation weight sum must be positive",
        ));
    }
    let mut counts = [0_u32; 4];
    let mut remainders = [(0_u64, 0_usize); 4];
    let mut allocated = 0_u32;
    for (index, weight) in weights.into_iter().enumerate() {
        let product = u64::from(count) * u64::from(weight);
        counts[index] = u32::try_from(product / total)
            .map_err(|_| GenerationError::Invalid("operation count overflow"))?;
        remainders[index] = (product % total, index);
        allocated += counts[index];
    }
    remainders.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    for (_, index) in remainders.into_iter().take((count - allocated) as usize) {
        counts[index] += 1;
    }
    Ok(counts)
}

#[derive(Clone, Copy)]
enum CandidateSet {
    Present,
    Absent,
    Any,
}

const fn candidate_for(class: TransitionClass) -> CandidateSet {
    match class {
        TransitionClass::NewInsert | TransitionClass::RequiresAbsent => CandidateSet::Absent,
        TransitionClass::HitRemove | TransitionClass::RequiresPresent => CandidateSet::Present,
        TransitionClass::AlwaysFeasible => CandidateSet::Any,
    }
}

struct KeySelector {
    distribution: KeyDistribution,
    ascending_cursor: u64,
    descending_cursor: u64,
}

impl KeySelector {
    const fn new(common: ValidatedCommon) -> Self {
        Self {
            distribution: common.distribution,
            ascending_cursor: common.minimum,
            descending_cursor: common.maximum,
        }
    }

    fn select(
        &mut self,
        index: &UniverseIndex,
        candidates: CandidateSet,
        rng: &mut RngV1,
    ) -> Result<u64, GenerationError> {
        let length = candidate_len(index, candidates);
        if length == 0 {
            return Err(GenerationError::Invalid("generator candidate set is empty"));
        }
        let rank = match self.distribution {
            KeyDistribution::Uniform => sample_rank(rng, length)?,
            KeyDistribution::Ascending => {
                let before = candidate_rank_before(index, candidates, self.ascending_cursor);
                if before < length { before } else { 0 }
            }
            KeyDistribution::Descending => {
                let through = candidate_rank_through(index, candidates, self.descending_cursor);
                if through == 0 {
                    length - 1
                } else {
                    through - 1
                }
            }
            KeyDistribution::Hotspot => {
                let (minimum, maximum) = index.bounds();
                let width = u128::from(maximum) - u128::from(minimum) + 1;
                let hot_length = width.div_ceil(5);
                let hot_max = u64::try_from(u128::from(minimum) + hot_length - 1)
                    .map_err(|_| GenerationError::Invalid("hotspot range overflow"))?;
                let hot_count = candidate_rank_through(index, candidates, hot_max);
                if hot_count > 0 && rng.bernoulli(4, 5)? {
                    sample_rank(rng, hot_count)?
                } else {
                    sample_rank(rng, length)?
                }
            }
        };
        let selected = select_candidate(index, candidates, rank)
            .ok_or(GenerationError::Invalid("generator rank selection failed"))?;
        self.ascending_cursor = selected.checked_add(1).unwrap_or(index.bounds().0);
        self.descending_cursor = selected.checked_sub(1).unwrap_or(index.bounds().1);
        Ok(selected)
    }
}

fn candidate_len(index: &UniverseIndex, candidates: CandidateSet) -> u128 {
    match candidates {
        CandidateSet::Present => u128::from(index.present_len()),
        CandidateSet::Absent => index.absent_len(),
        CandidateSet::Any => {
            let (minimum, maximum) = index.bounds();
            u128::from(maximum) - u128::from(minimum) + 1
        }
    }
}

fn candidate_rank_before(index: &UniverseIndex, candidates: CandidateSet, key: u64) -> u128 {
    let (minimum, _) = index.bounds();
    match candidates {
        CandidateSet::Present => u128::from(index.rank_present(key)),
        CandidateSet::Absent => index.rank_absent(key),
        CandidateSet::Any => u128::from(key.saturating_sub(minimum)),
    }
}

fn candidate_rank_through(index: &UniverseIndex, candidates: CandidateSet, key: u64) -> u128 {
    if key == u64::MAX {
        candidate_len(index, candidates)
    } else {
        candidate_rank_before(index, candidates, key + 1)
    }
}

fn select_candidate(index: &UniverseIndex, candidates: CandidateSet, rank: u128) -> Option<u64> {
    match candidates {
        CandidateSet::Present => u64::try_from(rank)
            .ok()
            .and_then(|rank| index.select_present(rank)),
        CandidateSet::Absent => index.select_absent(rank),
        CandidateSet::Any => {
            let (minimum, _) = index.bounds();
            u64::try_from(u128::from(minimum) + rank).ok()
        }
    }
}

fn sample_rank(rng: &mut RngV1, bound: u128) -> Result<u128, GenerationError> {
    const U64_CARDINALITY: u128 = u64::MAX as u128 + 1;
    if bound == U64_CARDINALITY {
        Ok(u128::from(rng.next_u64()))
    } else {
        let bound = u64::try_from(bound)
            .map_err(|_| GenerationError::Invalid("generator rank bound overflow"))?;
        Ok(u128::from(rng.bounded_u64(bound)?))
    }
}

fn generate_value(
    prefix: &str,
    suffix_capacity: u32,
    rng: &mut RngV1,
) -> Result<String, GenerationError> {
    const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let suffix_length = usize::try_from(rng.bounded_u64(u64::from(suffix_capacity) + 1)?)
        .map_err(|_| GenerationError::Invalid("generator suffix length overflow"))?;
    let mut value = String::with_capacity(prefix.len() + suffix_length);
    value.push_str(prefix);
    for _ in 0..suffix_length {
        let index = usize::try_from(rng.bounded_u64(36)?)
            .map_err(|_| GenerationError::Invalid("generator alphabet index overflow"))?;
        value.push(char::from(ALPHABET[index]));
    }
    Ok(value)
}

fn descriptor_kind(id: u32) -> Result<GeneratedOperationKind, GenerationError> {
    match id >> 24 {
        0 => Ok(GeneratedOperationKind::Insert),
        1 => Ok(GeneratedOperationKind::Remove),
        2 => Ok(GeneratedOperationKind::Get),
        3 => Ok(GeneratedOperationKind::LowerBound),
        _ => Err(GenerationError::Invalid(
            "unknown generator descriptor kind",
        )),
    }
}

pub(crate) fn materialized_operation(operation: &Operation) -> MaterializedItem {
    match operation {
        Operation::Insert { key, value } => MaterializedItem::Insert {
            key: key.clone(),
            value: value.clone(),
        },
        Operation::Remove { key } => MaterializedItem::Remove { key: key.clone() },
        Operation::Get { key } => MaterializedItem::Get { key: key.clone() },
        Operation::LowerBound { key } => MaterializedItem::LowerBound { key: key.clone() },
    }
}

fn make_provenance<T: Serialize>(
    spec: &T,
    materialized: &[MaterializedItem],
    stats: &GeneratorStats,
) -> Result<GeneratorProvenanceJson, GenerationError> {
    Ok(GeneratorProvenanceJson {
        generator_revision: SUPPORTED_GENERATOR_REVISION.to_owned(),
        payload: serde_json::to_value(GeneratorProvenanceV1 {
            spec,
            digest_algorithm: "sha256".to_owned(),
            materialized_digest: materialized_digest(materialized)?,
            stats: stats.clone(),
        })?,
    })
}

/// Builds one operation-type descriptor block.
///
/// Type-local ordinals `0..special_count` are deterministically assigned to
/// overwrite/hit outcomes. Every descriptor consumes exactly one raw priority
/// word, independent of the later schedule.
///
/// # Errors
///
/// Rejects a special count larger than `count`, a non-zero lower-bound special
/// count, or an ordinal that cannot fit in the canonical ID encoding.
pub fn build_descriptors(
    kind: GeneratedOperationKind,
    count: u32,
    special_count: u32,
    rng: &mut RngV1,
) -> Result<Vec<Descriptor>, ScheduleError> {
    if special_count > count
        || (kind == GeneratedOperationKind::LowerBound && special_count != 0)
        || count > 0x00ff_ffff
    {
        return Err(ScheduleError::InvalidOutcomeCount);
    }

    Ok((0..count)
        .map(|ordinal| Descriptor {
            id: (u32::from(kind as u8) << 24) | ordinal,
            priority: rng.next_u64(),
            class: descriptor_class(kind, ordinal < special_count),
        })
        .collect())
}

const fn descriptor_class(kind: GeneratedOperationKind, special: bool) -> TransitionClass {
    match (kind, special) {
        (GeneratedOperationKind::Insert | GeneratedOperationKind::Get, true) => {
            TransitionClass::RequiresPresent
        }
        (GeneratedOperationKind::Insert, false) => TransitionClass::NewInsert,
        (GeneratedOperationKind::Remove, true) => TransitionClass::HitRemove,
        (GeneratedOperationKind::Remove | GeneratedOperationKind::Get, false) => {
            TransitionClass::RequiresAbsent
        }
        (GeneratedOperationKind::LowerBound, _) => TransitionClass::AlwaysFeasible,
    }
}

/// Class-separated deterministic scheduler.
#[derive(Clone, Debug)]
pub struct DescriptorScheduler {
    state: FeasibilityState,
    queues: [BinaryHeap<Reverse<QueueEntry>>; CLASS_COUNT],
    remaining: [u32; CLASS_COUNT],
}

impl DescriptorScheduler {
    /// Validates and constructs a scheduler.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid initial size, duplicate canonical IDs,
    /// or a descriptor multiset with no feasible ordering.
    pub fn new(
        state: FeasibilityState,
        descriptors: impl IntoIterator<Item = Descriptor>,
    ) -> Result<Self, ScheduleError> {
        if state.size > state.capacity {
            return Err(ScheduleError::InvalidState);
        }

        let mut queues = array::from_fn(|_| BinaryHeap::new());
        let mut remaining = [0_u32; CLASS_COUNT];
        let mut ids = std::collections::HashSet::new();
        for descriptor in descriptors {
            if !ids.insert(descriptor.id) {
                return Err(ScheduleError::DuplicateDescriptorId);
            }
            queues[descriptor.class.index()].push(Reverse(descriptor.into()));
            remaining[descriptor.class.index()] += 1;
        }

        if !can_complete(state, remaining) {
            return Err(ScheduleError::Infeasible);
        }
        Ok(Self {
            state,
            queues,
            remaining,
        })
    }

    /// Returns the current size state.
    pub const fn state(&self) -> FeasibilityState {
        self.state
    }

    /// Returns whether every descriptor has been consumed.
    pub fn is_empty(&self) -> bool {
        self.remaining.iter().all(|count| *count == 0)
    }

    /// Selects the lowest `(priority, descriptor_id)` among safe class heads.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::Infeasible`] if remaining descriptors cannot be
    /// scheduled. This indicates a predicate or state corruption because
    /// construction and every previous transition preserve feasibility.
    pub fn next_descriptor(&mut self) -> Result<Option<Descriptor>, ScheduleError> {
        if self.is_empty() {
            return Ok(None);
        }

        let mut selected: Option<(QueueEntry, TransitionClass, FeasibilityState)> = None;
        for class in TransitionClass::ALL {
            let Some(Reverse(entry)) = self.queues[class.index()].peek().copied() else {
                continue;
            };
            let Some(next_state) = apply_transition(self.state, class) else {
                continue;
            };
            let mut remaining = self.remaining;
            remaining[class.index()] -= 1;
            if !can_complete(next_state, remaining) {
                continue;
            }
            if selected.is_none_or(|(current, _, _)| entry < current) {
                selected = Some((entry, class, next_state));
            }
        }

        let Some((entry, class, next_state)) = selected else {
            return Err(ScheduleError::Infeasible);
        };
        let popped = self.queues[class.index()]
            .pop()
            .ok_or(ScheduleError::Infeasible)?
            .0;
        if popped != entry {
            return Err(ScheduleError::Infeasible);
        }
        self.remaining[class.index()] -= 1;
        self.state = next_state;
        Ok(Some(Descriptor {
            id: entry.id,
            priority: entry.priority,
            class,
        }))
    }
}

/// Exact finite-state feasibility predicate for the five transition classes.
pub fn can_complete(state: FeasibilityState, remaining: [u32; CLASS_COUNT]) -> bool {
    if state.size > state.capacity {
        return false;
    }

    let inserts = remaining[TransitionClass::NewInsert.index()];
    let removes = remaining[TransitionClass::HitRemove.index()];
    let requires_present = remaining[TransitionClass::RequiresPresent.index()];
    let requires_absent = remaining[TransitionClass::RequiresAbsent.index()];

    if state.capacity == 0 {
        return inserts == 0 && removes == 0 && requires_present == 0 && requires_absent == 0;
    }

    let Some(final_size) = state
        .size
        .checked_add(u128::from(inserts))
        .and_then(|size| size.checked_sub(u128::from(removes)))
    else {
        return false;
    };
    if final_size > state.capacity {
        return false;
    }
    if requires_present > 0 && state.size == 0 && inserts == 0 {
        return false;
    }
    if requires_absent > 0 && state.size == state.capacity && removes == 0 {
        return false;
    }
    true
}

fn apply_transition(state: FeasibilityState, class: TransitionClass) -> Option<FeasibilityState> {
    let size = match class {
        TransitionClass::NewInsert if state.size < state.capacity => state.size + 1,
        TransitionClass::HitRemove if state.size > 0 => state.size - 1,
        TransitionClass::RequiresPresent if state.size > 0 => state.size,
        TransitionClass::RequiresAbsent if state.size < state.capacity => state.size,
        TransitionClass::AlwaysFeasible => state.size,
        TransitionClass::NewInsert
        | TransitionClass::HitRemove
        | TransitionClass::RequiresPresent
        | TransitionClass::RequiresAbsent => return None,
    };
    Some(FeasibilityState {
        size,
        capacity: state.capacity,
    })
}

#[cfg(test)]
mod tests;
