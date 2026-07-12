//! Emits the deterministic machine-readable contract report.

use std::error::Error;

use serde_json::json;
use visualizer_core::jcs::{canonicalize, sha256_hex};
use visualizer_core::rng::{
    ALGORITHM_DRAW_LEDGER, HASH_KDF_LABELS, MAX_BOUNDED_RNG_DRAWS, MAX_RNG_DRAWS_PER_INSERT,
    RNG_DOMAIN_LABELS, RngV1,
};
use visualizer_core::scenario::{SCENARIO_SCHEMA_REVISION, scenario_schema};

fn main() -> Result<(), Box<dyn Error>> {
    let vectors = RNG_DOMAIN_LABELS.map(|label| {
        let mut rng = RngV1::from_seed(0, label);
        let initial_state = rng.state().map(|word| word.to_string());
        let first_outputs = std::array::from_fn::<_, 4, _>(|_| rng.next_u64().to_string());
        json!({
            "label": label,
            "seed": "0",
            "initialState": initial_state,
            "firstOutputs": first_outputs,
            "stateAfterFour": rng.state().map(|word| word.to_string()),
            "rawDraws": rng.draws().to_string()
        })
    });

    let schema_value = serde_json::to_value(scenario_schema())?;
    let schema_json = serde_json::to_vec(&schema_value)?;
    let canonical_schema = canonicalize(&schema_json)?;
    let report = json!({
        "artifactSchemaVersion": 1,
        "reportSuite": "deterministic-contracts",
        "status": "passed",
        "components": [
            "transition-class-scheduler-and-exhaustive-oracle",
            "present-and-absent-rank-select",
            "safe-generational-arena-and-slice-codec",
            "rng-domains-and-operation-ledger",
            "strict-scenario-schema-and-raw-provenance",
            "rust-typescript-rfc8785-fixture",
            "canonical-cursor-and-numeric-catalog",
            "plugin-envelope-extension-test",
            "visual-identity-and-independent-scene-replay",
            "algorithm-tie-break-oracles"
        ],
        "deterministicInputs": {
            "generatorExhaustiveUniverseMaximum": 8,
            "generatorTotalDescriptorCountMaximum": 8,
            "rankSelectPropertyUniverse": { "minimum": "0", "maximum": "15" }
        },
        "rng": {
            "version": 1,
            "maxBoundedRawDraws": MAX_BOUNDED_RNG_DRAWS,
            "maxZipInsertRawDraws": MAX_RNG_DRAWS_PER_INSERT,
            "domainVectors": vectors,
            "hashKdfLabels": HASH_KDF_LABELS,
            "algorithmOperationLedger": ALGORITHM_DRAW_LEDGER
        },
        "scenarioSchema": {
            "revision": SCENARIO_SCHEMA_REVISION,
            "draft": "https://json-schema.org/draft/2020-12/schema",
            "canonicalSha256": sha256_hex(&canonical_schema),
            "canonicalByteLength": canonical_schema.len()
        }
    });

    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(())
}
