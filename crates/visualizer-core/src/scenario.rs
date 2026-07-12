//! Strict Scenario V1 DTO, JSON Schema, and TypeScript contracts.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use thiserror::Error;
use ts_rs::TS;

use crate::generator::{
    InitialGeneratorSpec, OperationGeneratorSpec, generate_initial, generate_operations,
    materialized_operation,
};
use crate::provenance::{
    GeneratorProvenanceV1, MAX_PROVENANCE_PAYLOAD_BYTES, MaterializedItem,
    SUPPORTED_GENERATOR_REVISION, materialized_digest,
};

/// Scenario JSON encoding revision understood by V1.
pub const SCENARIO_ENCODING_REVISION: &str = "rfc8785-jcs/1";
/// JSON Schema artifact revision.
pub const SCENARIO_SCHEMA_REVISION: &str = "scenario-schema/1";
/// Algorithm semantics implemented by this build.
pub const ALGORITHM_REVISION: &str = "ordered-map/1";
/// Plugin result contract implemented by this build.
pub const PLUGIN_RESULT_REVISION: &str = "ordered-map-result/1";
/// Metrics contract implemented by this build.
pub const METRICS_CATALOG_REVISION: &str = "ordered-map-metrics/1";
/// RNG contract implemented by this build.
pub const RNG_VERSION: u32 = 1;
/// Renderer-independent trace catalog emitted by this build.
pub const TRACE_REVISION: &str = "ordered-map-trace/3";
/// Ordered-map projection contract emitted by this build.
pub const PROJECTION_REVISION: &str = "ordered-map-projection/2";
/// Deterministic layout contract emitted by this build.
pub const LAYOUT_REVISION: &str = "ordered-map-layout/1";
/// Worker frame encoding emitted by this build.
pub const FRAME_ENCODING_REVISION: &str = "scene-frame/5";

/// Strict generic envelope decoded before selecting a build-time plugin.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScenarioEnvelopeV1 {
    /// Schema major.
    pub schema_version: u32,
    /// Canonical JSON encoding.
    pub scenario_encoding_revision: String,
    /// Build-time plugin identifier.
    pub plugin: String,
    /// Declared reproducibility revisions.
    pub reproducibility: ReproducibilityMetadata,
    /// Bounded plugin-owned raw payload.
    pub payload: Box<RawValue>,
}

/// Fully typed ordered-map scenario.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScenarioV1 {
    /// Must equal `1`.
    pub schema_version: u32,
    /// Must equal [`SCENARIO_ENCODING_REVISION`].
    pub scenario_encoding_revision: String,
    /// Must equal `ordered-map`.
    pub plugin: String,
    /// Declared reproducibility revisions.
    pub reproducibility: ReproducibilityMetadata,
    /// Ordered-map-owned payload.
    pub payload: OrderedMapScenarioPayloadV1,
}

/// Revisions declared by a persisted Scenario.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityMetadata {
    /// Persisted revision set; the engine's effective set remains runtime-only.
    pub declared: ReproducibilityRevisionSet,
}

/// Components that affect reproducible persisted or derived output.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityRevisionSet {
    /// Algorithm semantic revision.
    pub algorithm_revision: String,
    /// RNG contract revision.
    pub rng_version: u32,
    /// Plugin result revision.
    pub plugin_result_revision: String,
    /// Metric catalog revision.
    pub metrics_catalog_revision: String,
    /// Trace revision.
    pub trace_revision: String,
    /// Projection revision.
    pub projection_revision: String,
    /// Layout revision.
    pub layout_revision: String,
    /// Canonical frame encoding revision.
    pub frame_encoding_revision: String,
}

impl ScenarioV1 {
    /// Marks materialized content as explicitly edited by this build.
    pub fn declare_current_derived_revisions(&mut self) {
        let declared = &mut self.reproducibility.declared;
        TRACE_REVISION.clone_into(&mut declared.trace_revision);
        PROJECTION_REVISION.clone_into(&mut declared.projection_revision);
        LAYOUT_REVISION.clone_into(&mut declared.layout_revision);
        FRAME_ENCODING_REVISION.clone_into(&mut declared.frame_encoding_revision);
    }

    /// Reports whether derived output declared by this Scenario differs from
    /// what the current build will produce.
    #[must_use]
    pub fn has_legacy_derived_revisions(&self) -> bool {
        let declared = &self.reproducibility.declared;
        declared.trace_revision != TRACE_REVISION
            || declared.projection_revision != PROJECTION_REVISION
            || declared.layout_revision != LAYOUT_REVISION
            || declared.frame_encoding_revision != FRAME_ENCODING_REVISION
    }
}

/// Ordered-map Scenario V1 payload.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OrderedMapScenarioPayloadV1 {
    /// Selected algorithm and its bounded configuration.
    pub algorithm: AlgorithmSpec,
    /// Canonical unsigned decimal seed.
    pub algorithm_seed: String,
    /// Initial materialized entries.
    pub initial: InitialInput,
    /// Materialized operations.
    pub operations: OperationInput,
}

/// Initial entries and optional generator provenance.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InitialInput {
    /// Entries applied in array order with normal insert semantics.
    pub entries: Vec<Entry>,
    /// Whether initial construction appears on the timeline.
    pub show_build: bool,
    /// Opaque until its generator revision is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provenance: Option<GeneratorProvenanceJson>,
}

/// Operations and optional generator provenance.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OperationInput {
    /// Operations executed in array order.
    pub items: Vec<Operation>,
    /// Opaque until its generator revision is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provenance: Option<GeneratorProvenanceJson>,
}

/// Persisted raw generator provenance representation.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GeneratorProvenanceJson {
    /// Revision string remains open for forward-compatible retention.
    pub generator_revision: String,
    /// Revision-owned JSON payload.
    pub payload: serde_json::Value,
}

/// Initial map entry.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Canonical unsigned decimal key.
    pub key: String,
    /// Unmodified Unicode value.
    pub value: String,
}

/// Ordered-map operation.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, TS)]
#[serde(deny_unknown_fields, tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Insert or overwrite.
    Insert {
        /// Canonical unsigned decimal key.
        key: String,
        /// Unmodified Unicode value.
        value: String,
    },
    /// Remove when present.
    Remove {
        /// Canonical unsigned decimal key.
        key: String,
    },
    /// Lookup.
    Get {
        /// Canonical unsigned decimal key.
        key: String,
    },
    /// Lower-bound query.
    LowerBound {
        /// Canonical unsigned decimal key.
        key: String,
    },
}

/// Algorithm variant and bounded configuration.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(
    deny_unknown_fields,
    tag = "id",
    content = "config",
    rename_all = "kebab-case"
)]
pub enum AlgorithmSpec {
    /// AVL tree.
    Avl(EmptyConfig),
    /// Weight-balanced tree.
    Wbt(EmptyConfig),
    /// AA tree.
    Aa(EmptyConfig),
    /// Left-leaning red-black tree.
    Llrb(EmptyConfig),
    /// Treap.
    Treap(EmptyConfig),
    /// Zip tree.
    Zip(EmptyConfig),
    /// Splay tree.
    Splay(EmptyConfig),
    /// Scapegoat tree.
    Scapegoat(ScapegoatConfig),
    /// Skip list.
    SkipList(SkipListConfig),
    /// B-tree.
    BTree(BTreeConfig),
    /// Sparse van Emde Boas tree.
    Veb(WordConfig),
    /// X-fast trie.
    XFast(WordConfig),
    /// Y-fast trie.
    YFast(WordConfig),
}

/// Explicit empty object; unknown configuration fields are rejected.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EmptyConfig {}

/// Scapegoat rational alpha.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScapegoatConfig {
    /// Alpha numerator.
    pub alpha_numerator: u32,
    /// Alpha denominator.
    pub alpha_denominator: u32,
}

/// Skip-list probability and level bound.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SkipListConfig {
    /// `1/2` or `1/4`.
    pub promotion: String,
    /// Inclusive tower-height bound.
    pub max_level: u32,
}

/// B-tree minimum degree.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BTreeConfig {
    /// CLRS-style minimum degree in `2..=16`.
    pub min_degree: u32,
}

/// Integer-universe word width.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WordConfig {
    /// Key width in `1..=64`.
    pub word_bits: u32,
}

/// Strict decode or semantic-validation failure.
#[derive(Debug, Error)]
pub enum ScenarioError {
    /// JSON syntax or strict field decoding failed.
    #[error("invalid Scenario JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// Unsupported envelope or plugin revision.
    #[error("unsupported Scenario contract: {0}")]
    Unsupported(&'static str),
    /// A bounded semantic constraint failed.
    #[error("invalid Scenario value: {0}")]
    Invalid(&'static str),
}

/// Decodes the generic envelope, selects the ordered-map plugin, and then
/// strictly decodes its payload.
///
/// # Errors
///
/// Rejects duplicate/unknown fields, unsupported versions/plugins, noncanonical
/// unsigned decimal strings, and parameter/resource-limit violations.
pub fn decode_ordered_map(bytes: &[u8]) -> Result<ScenarioV1, ScenarioError> {
    let raw: RawScenarioEnvelopeV1 = serde_json::from_slice(bytes)?;
    if raw.schema_version != 1 {
        return Err(ScenarioError::Unsupported("schema_version"));
    }
    if raw.scenario_encoding_revision != SCENARIO_ENCODING_REVISION {
        return Err(ScenarioError::Unsupported("scenario_encoding_revision"));
    }
    if raw.plugin != "ordered-map" {
        return Err(ScenarioError::Unsupported("plugin"));
    }
    let payload: OrderedMapScenarioPayloadV1 = serde_json::from_str(raw.payload.get())?;
    let scenario = ScenarioV1 {
        schema_version: raw.schema_version,
        scenario_encoding_revision: raw.scenario_encoding_revision,
        plugin: raw.plugin,
        reproducibility: raw.reproducibility,
        payload,
    };
    validate(&scenario)?;
    Ok(scenario)
}

/// Generates the normative draft 2020-12 schema from the Rust DTO.
pub fn scenario_schema() -> schemars::Schema {
    SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<ScenarioV1>()
}

fn validate(scenario: &ScenarioV1) -> Result<(), ScenarioError> {
    if scenario.schema_version != 1 || scenario.plugin != "ordered-map" {
        return Err(ScenarioError::Unsupported("typed envelope"));
    }
    validate_required_revisions(&scenario.reproducibility.declared)?;
    parse_canonical_u64(&scenario.payload.algorithm_seed)?;
    if scenario.payload.initial.entries.len() > 10_000 {
        return Err(ScenarioError::Invalid("initial entry limit"));
    }
    if scenario.payload.operations.items.len() > 100_000 {
        return Err(ScenarioError::Invalid("operation limit"));
    }
    for entry in &scenario.payload.initial.entries {
        parse_canonical_u64(&entry.key)?;
        if entry.value.chars().count() > 256 {
            return Err(ScenarioError::Invalid("value scalar limit"));
        }
    }
    for operation in &scenario.payload.operations.items {
        let (key, value) = match operation {
            Operation::Insert { key, value } => (key, Some(value)),
            Operation::Remove { key } | Operation::Get { key } | Operation::LowerBound { key } => {
                (key, None)
            }
        };
        parse_canonical_u64(key)?;
        if value.is_some_and(|value| value.chars().count() > 256) {
            return Err(ScenarioError::Invalid("value scalar limit"));
        }
    }
    match &scenario.payload.algorithm {
        AlgorithmSpec::Scapegoat(config) => {
            let numerator = config.alpha_numerator;
            let denominator = config.alpha_denominator;
            if denominator > 64
                || numerator == 0
                || numerator >= denominator
                || u64::from(numerator) * 2 <= u64::from(denominator)
                || greatest_common_divisor(numerator, denominator) != 1
            {
                return Err(ScenarioError::Invalid("scapegoat alpha"));
            }
        }
        AlgorithmSpec::SkipList(config) => {
            if !matches!(config.promotion.as_str(), "1/2" | "1/4")
                || !(1..=64).contains(&config.max_level)
            {
                return Err(ScenarioError::Invalid("skip-list config"));
            }
        }
        AlgorithmSpec::BTree(config) if !(2..=16).contains(&config.min_degree) => {
            return Err(ScenarioError::Invalid("B-tree degree"));
        }
        AlgorithmSpec::Veb(config)
        | AlgorithmSpec::XFast(config)
        | AlgorithmSpec::YFast(config)
            if !(1..=64).contains(&config.word_bits) =>
        {
            return Err(ScenarioError::Invalid("word_bits"));
        }
        _ => {}
    }
    validate_word_universe(scenario)?;
    validate_generator_provenance(scenario)?;
    Ok(())
}

fn validate_required_revisions(
    revisions: &ReproducibilityRevisionSet,
) -> Result<(), ScenarioError> {
    if revisions.algorithm_revision != ALGORITHM_REVISION {
        return Err(ScenarioError::Unsupported("algorithm_revision"));
    }
    if revisions.rng_version != RNG_VERSION {
        return Err(ScenarioError::Unsupported("rng_version"));
    }
    if revisions.plugin_result_revision != PLUGIN_RESULT_REVISION {
        return Err(ScenarioError::Unsupported("plugin_result_revision"));
    }
    if revisions.metrics_catalog_revision != METRICS_CATALOG_REVISION {
        return Err(ScenarioError::Unsupported("metrics_catalog_revision"));
    }
    Ok(())
}

fn validate_word_universe(scenario: &ScenarioV1) -> Result<(), ScenarioError> {
    let word_bits = match &scenario.payload.algorithm {
        AlgorithmSpec::Veb(config)
        | AlgorithmSpec::XFast(config)
        | AlgorithmSpec::YFast(config) => config.word_bits,
        _ => return Ok(()),
    };
    if word_bits == 64 {
        return Ok(());
    }
    let limit = 1_u64 << word_bits;
    for entry in &scenario.payload.initial.entries {
        if parse_canonical_u64(&entry.key)? >= limit {
            return Err(ScenarioError::Invalid("key exceeds algorithm universe"));
        }
    }
    for operation in &scenario.payload.operations.items {
        let key = match operation {
            Operation::Insert { key, .. }
            | Operation::Remove { key }
            | Operation::Get { key }
            | Operation::LowerBound { key } => key,
        };
        if parse_canonical_u64(key)? >= limit {
            return Err(ScenarioError::Invalid("key exceeds algorithm universe"));
        }
    }
    Ok(())
}

fn validate_generator_provenance(scenario: &ScenarioV1) -> Result<(), ScenarioError> {
    if let Some(provenance) = &scenario.payload.initial.provenance {
        validate_provenance_size(provenance)?;
        if provenance.generator_revision == SUPPORTED_GENERATOR_REVISION {
            let payload: GeneratorProvenanceV1<InitialGeneratorSpec> =
                serde_json::from_value(provenance.payload.clone())
                    .map_err(|_| ScenarioError::Invalid("initial provenance payload"))?;
            let materialized = scenario
                .payload
                .initial
                .entries
                .iter()
                .map(|entry| MaterializedItem::Entry {
                    key: entry.key.clone(),
                    value: entry.value.clone(),
                })
                .collect::<Vec<_>>();
            validate_provenance_digest(&payload, &materialized)?;
            let regenerated = generate_initial(&payload.spec)
                .map_err(|_| ScenarioError::Invalid("initial provenance regeneration"))?;
            if regenerated.entries != scenario.payload.initial.entries
                || regenerated.stats != payload.stats
            {
                return Err(ScenarioError::Invalid("initial provenance mismatch"));
            }
        }
    }
    if let Some(provenance) = &scenario.payload.operations.provenance {
        validate_provenance_size(provenance)?;
        if provenance.generator_revision == SUPPORTED_GENERATOR_REVISION {
            let payload: GeneratorProvenanceV1<OperationGeneratorSpec> =
                serde_json::from_value(provenance.payload.clone())
                    .map_err(|_| ScenarioError::Invalid("operation provenance payload"))?;
            let materialized = scenario
                .payload
                .operations
                .items
                .iter()
                .map(materialized_operation)
                .collect::<Vec<_>>();
            validate_provenance_digest(&payload, &materialized)?;
            let regenerated = generate_operations(&payload.spec, &scenario.payload.initial.entries)
                .map_err(|_| ScenarioError::Invalid("operation provenance regeneration"))?;
            if regenerated.operations != scenario.payload.operations.items
                || regenerated.stats != payload.stats
            {
                return Err(ScenarioError::Invalid("operation provenance mismatch"));
            }
        }
    }
    Ok(())
}

fn validate_provenance_size(provenance: &GeneratorProvenanceJson) -> Result<(), ScenarioError> {
    let encoded = serde_json::to_vec(&provenance.payload)
        .map_err(|_| ScenarioError::Invalid("provenance serialization"))?;
    if encoded.len() > MAX_PROVENANCE_PAYLOAD_BYTES {
        return Err(ScenarioError::Invalid("provenance payload limit"));
    }
    Ok(())
}

fn validate_provenance_digest<T>(
    payload: &GeneratorProvenanceV1<T>,
    materialized: &[MaterializedItem],
) -> Result<(), ScenarioError> {
    let digest = materialized_digest(materialized)
        .map_err(|_| ScenarioError::Invalid("provenance digest"))?;
    if payload.digest_algorithm != "sha256" || payload.materialized_digest != digest {
        return Err(ScenarioError::Invalid("provenance digest mismatch"));
    }
    Ok(())
}

fn parse_canonical_u64(value: &str) -> Result<u64, ScenarioError> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ScenarioError::Invalid("noncanonical u64 decimal"));
    }
    value
        .parse()
        .map_err(|_| ScenarioError::Invalid("u64 decimal overflow"))
}

const fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use ts_rs::{Config, TS};

    use super::*;
    use crate::generator::{KeyDistribution, OperationWeights};

    fn valid_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": SCENARIO_ENCODING_REVISION,
            "plugin": "ordered-map",
            "reproducibility": {
                "declared": {
                    "algorithm_revision": ALGORITHM_REVISION,
                    "rng_version": RNG_VERSION,
                    "plugin_result_revision": PLUGIN_RESULT_REVISION,
                    "metrics_catalog_revision": METRICS_CATALOG_REVISION,
                    "trace_revision": "ordered-map-trace/3",
                    "projection_revision": "ordered-map-projection/2",
                    "layout_revision": "tree-layout/1",
                    "frame_encoding_revision": "scene-frame/5"
                }
            },
            "payload": {
                "algorithm": { "id": "avl", "config": {} },
                "algorithm_seed": "18446744073709551615",
                "initial": { "entries": [{ "key": "7", "value": "α" }], "show_build": true },
                "operations": { "items": [{ "op": "lower_bound", "key": "7" }] }
            }
        })
        .to_string()
    }

    #[test]
    fn strict_two_stage_decode_accepts_valid_scenario() {
        let scenario = decode_ordered_map(valid_json().as_bytes()).expect("valid scenario");

        assert_eq!(scenario.payload.initial.entries.len(), 1);
        assert_eq!(scenario.payload.operations.items.len(), 1);
    }

    #[test]
    fn unknown_and_duplicate_fields_are_rejected() {
        let unknown = valid_json().replace(
            "\"algorithm_seed\":",
            "\"algoritm_seed\":\"1\",\"algorithm_seed\":",
        );
        assert!(decode_ordered_map(unknown.as_bytes()).is_err());

        let duplicate = valid_json().replace(
            "\"plugin\":\"ordered-map\"",
            "\"plugin\":\"ordered-map\",\"plugin\":\"ordered-map\"",
        );
        assert!(decode_ordered_map(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn noncanonical_u64_is_rejected() {
        let invalid = valid_json().replace(
            "\"algorithm_seed\":\"18446744073709551615\"",
            "\"algorithm_seed\":\"01\"",
        );

        assert!(matches!(
            decode_ordered_map(invalid.as_bytes()),
            Err(ScenarioError::Invalid("noncanonical u64 decimal"))
        ));
    }

    #[test]
    fn unsupported_execution_revision_is_rejected_without_relabeling_derived_revisions() {
        let mut future: serde_json::Value =
            serde_json::from_str(&valid_json()).expect("fixture JSON is valid");
        future["reproducibility"]["declared"]["algorithm_revision"] =
            serde_json::json!("ordered-map/999");

        assert!(matches!(
            decode_ordered_map(future.to_string().as_bytes()),
            Err(ScenarioError::Unsupported("algorithm_revision"))
        ));

        let mut historical_trace: serde_json::Value =
            serde_json::from_str(&valid_json()).expect("fixture JSON is valid");
        historical_trace["reproducibility"]["declared"]["trace_revision"] =
            serde_json::json!("ordered-map-trace/2");
        let mut decoded = decode_ordered_map(historical_trace.to_string().as_bytes())
            .expect("materialized input remains executable");
        assert_eq!(
            decoded.reproducibility.declared.trace_revision,
            "ordered-map-trace/2"
        );
        assert!(decoded.has_legacy_derived_revisions());
        decoded.declare_current_derived_revisions();
        assert!(!decoded.has_legacy_derived_revisions());
        assert_eq!(
            decoded.reproducibility.declared.trace_revision,
            TRACE_REVISION
        );
        assert_eq!(
            decoded.reproducibility.declared.projection_revision,
            PROJECTION_REVISION
        );
    }

    #[test]
    fn word_algorithms_reject_every_out_of_universe_operation_before_playback() {
        for algorithm in ["veb", "x-fast", "y-fast"] {
            let mut value: serde_json::Value =
                serde_json::from_str(&valid_json()).expect("fixture JSON is valid");
            value["payload"]["algorithm"] =
                serde_json::json!({ "id": algorithm, "config": { "word_bits": 3 } });
            value["payload"]["initial"]["entries"] = serde_json::json!([]);
            value["payload"]["operations"]["items"] =
                serde_json::json!([{ "op": "get", "key": "8" }]);

            assert!(matches!(
                decode_ordered_map(value.to_string().as_bytes()),
                Err(ScenarioError::Invalid("key exceeds algorithm universe"))
            ));
        }
    }

    #[test]
    fn known_initial_provenance_is_verified_against_materialized_entries() {
        let spec = InitialGeneratorSpec {
            seed: "11".to_owned(),
            count: 4,
            key_min: "0".to_owned(),
            key_max: "15".to_owned(),
            distribution: KeyDistribution::Uniform,
            value_prefix: "v".to_owned(),
            value_max_scalar_values: 8,
            overwrite_rate_bps: 0,
        };
        let generated = generate_initial(&spec).expect("fixture generation succeeds");
        let mut value: serde_json::Value =
            serde_json::from_str(&valid_json()).expect("fixture JSON is valid");
        value["payload"]["initial"]["entries"] =
            serde_json::to_value(&generated.entries).expect("entries serialize");
        value["payload"]["initial"]["provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        value["payload"]["operations"]["items"] = serde_json::json!([]);
        decode_ordered_map(value.to_string().as_bytes())
            .expect("unmodified generated input verifies");

        value["payload"]["initial"]["entries"][0]["value"] = serde_json::json!("tampered");
        assert!(matches!(
            decode_ordered_map(value.to_string().as_bytes()),
            Err(ScenarioError::Invalid("provenance digest mismatch"))
        ));
    }

    #[test]
    fn known_operation_provenance_is_verified_against_materialized_operations() {
        let spec = OperationGeneratorSpec {
            seed: "17".to_owned(),
            count: 4,
            key_min: "0".to_owned(),
            key_max: "15".to_owned(),
            distribution: KeyDistribution::Ascending,
            value_prefix: String::new(),
            value_max_scalar_values: 0,
            weights: OperationWeights {
                insert: 0,
                remove: 0,
                get: 1,
                lower_bound: 0,
            },
            get_hit_rate_bps: 10_000,
            remove_hit_rate_bps: 0,
            insert_overwrite_rate_bps: 0,
        };
        let initial = vec![Entry {
            key: "7".to_owned(),
            value: "seven".to_owned(),
        }];
        let generated = generate_operations(&spec, &initial).expect("fixture generation succeeds");
        let mut value: serde_json::Value =
            serde_json::from_str(&valid_json()).expect("fixture JSON is valid");
        value["payload"]["initial"]["entries"] =
            serde_json::to_value(&initial).expect("entries serialize");
        value["payload"]["operations"]["items"] =
            serde_json::to_value(&generated.operations).expect("operations serialize");
        value["payload"]["operations"]["provenance"] =
            serde_json::to_value(&generated.provenance).expect("provenance serializes");
        decode_ordered_map(value.to_string().as_bytes())
            .expect("unmodified generated operations verify");

        value["payload"]["operations"]["items"][0]["key"] = serde_json::json!("6");
        assert!(matches!(
            decode_ordered_map(value.to_string().as_bytes()),
            Err(ScenarioError::Invalid("provenance digest mismatch"))
        ));
    }

    #[test]
    fn schema_explicitly_uses_draft_2020_12_and_closed_objects() {
        let schema = serde_json::to_value(scenario_schema()).expect("schema is JSON");

        assert_eq!(
            schema.get("$schema").and_then(serde_json::Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    #[test]
    fn typescript_binding_is_generated_from_the_same_dto() {
        let declaration = ScenarioV1::decl(&Config::default());

        assert!(declaration.contains("type ScenarioV1"));
        assert!(declaration.contains("scenario_encoding_revision: string"));
        assert!(declaration.contains("payload: OrderedMapScenarioPayloadV1"));
    }
}
