//! Build-time plugin registry and generic envelope contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ordered-map is permanently assigned ordinal one; new plugins append.
pub const ORDERED_MAP_PLUGIN_ORDINAL: u32 = 1;
/// Flow is permanently assigned ordinal two.
pub const FLOW_PLUGIN_ORDINAL: u32 = 2;
/// Current closed runtime-handshake schema.
pub const ENGINE_CONTRACT_SCHEMA_VERSION: u32 = 1;

/// Versioned build-time plugin contract advertised during handshake.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginContractDescriptor {
    /// Append-only registry identity.
    pub plugin_ordinal: u32,
    /// Stable human-readable plugin ID.
    pub plugin_id: String,
    /// Accepted plugin-result schema versions.
    pub result_schema_versions: Vec<u32>,
    /// Accepted metrics catalog revisions.
    pub metrics_catalog_revisions: Vec<String>,
    /// Number of counters in each metrics vector.
    pub metrics_vector_length: u32,
    /// Accepted trace catalog revisions.
    pub trace_catalog_revisions: Vec<String>,
}

/// Opaque result transported by playback core without plugin-specific unions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginResultEnvelope {
    /// Plugin that owns the payload schema.
    pub plugin_ordinal: u32,
    /// Plugin-local result schema version.
    pub schema_version: u32,
    /// Canonical plugin-local binary payload.
    pub payload: Vec<u8>,
}

/// Catalog-ordered absolute metric values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsVector {
    /// Revision that defines counter meaning and order.
    pub catalog_revision: String,
    /// Unsigned absolute values in catalog order.
    pub values: Vec<u64>,
}

/// Generic logical commit fields inspected by playback core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreCommitEnvelope {
    /// Numeric plugin phase ID; meaning remains in the plugin catalog.
    pub phase_id: u32,
    /// Four absolute metrics scopes.
    pub metrics: [MetricsVector; 4],
    /// Optional plugin-specific operation result.
    pub plugin_result: Option<PluginResultEnvelope>,
}

/// Closed plugin row advertised by the runtime handshake.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnginePluginContractV1 {
    /// Stable human-readable plugin ID.
    pub plugin_id: String,
    /// Append-only plugin ordinal.
    pub plugin_ordinal: u32,
    /// Human-readable plugin result revision.
    pub result_revision_name: String,
    /// Numeric result payload schema carried by V6.
    pub result_schema_version: u32,
    /// Human-readable metrics catalog revision.
    pub metrics_revision_name: String,
    /// Human-readable trace catalog revision.
    pub trace_revision_name: String,
    /// Plugin-local frame revisions accepted by this runtime.
    pub accepted_frame_revisions: Vec<String>,
}

/// Closed runtime contract checked before a session can be created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineContractV1 {
    /// Must equal `ENGINE_CONTRACT_SCHEMA_VERSION`.
    pub contract_schema_version: u32,
    /// Transport containers accepted by this build.
    pub accepted_transport_versions: Vec<u16>,
    /// Append-only plugin rows in ordinal order.
    pub plugins: Vec<EnginePluginContractV1>,
}

/// Returns the runtime handshake authority in append-only ordinal order.
#[must_use]
pub fn engine_contract_v1() -> EngineContractV1 {
    EngineContractV1 {
        contract_schema_version: ENGINE_CONTRACT_SCHEMA_VERSION,
        accepted_transport_versions: vec![5, 6],
        plugins: vec![
            EnginePluginContractV1 {
                plugin_id: "ordered-map".to_owned(),
                plugin_ordinal: ORDERED_MAP_PLUGIN_ORDINAL,
                result_revision_name: "ordered-map-result/1".to_owned(),
                result_schema_version: 1,
                metrics_revision_name: "ordered-map-metrics/1".to_owned(),
                trace_revision_name: "ordered-map-trace/3".to_owned(),
                accepted_frame_revisions: vec!["scene-frame/5".to_owned()],
            },
            EnginePluginContractV1 {
                plugin_id: "flow".to_owned(),
                plugin_ordinal: FLOW_PLUGIN_ORDINAL,
                result_revision_name: "flow-result/9".to_owned(),
                result_schema_version: 9,
                metrics_revision_name: "flow-metrics/6".to_owned(),
                trace_revision_name: "flow-trace/9".to_owned(),
                accepted_frame_revisions: vec!["flow-scene/9".to_owned()],
            },
        ],
    }
}

/// Plugin registry validation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PluginContractError {
    /// Ordinal zero is reserved for core and handshake messages.
    #[error("plugin ordinal zero is reserved")]
    ReservedOrdinal,
    /// Registry ordinals and IDs are both unique.
    #[error("duplicate plugin registry entry")]
    DuplicatePlugin,
    /// A commit used a metrics vector with the wrong catalog or length.
    #[error("metrics vector does not match plugin catalog")]
    InvalidMetrics,
    /// A result names an unknown plugin or unsupported schema version.
    #[error("plugin result schema is unsupported")]
    UnsupportedResult,
}

/// Immutable build-time registry used by core validation.
#[derive(Clone, Debug)]
pub struct PluginRegistry {
    by_ordinal: BTreeMap<u32, PluginContractDescriptor>,
}

impl PluginRegistry {
    /// Creates a validated registry.
    ///
    /// # Errors
    ///
    /// Returns an error for ordinal zero or duplicate IDs/ordinals.
    pub fn new(
        descriptors: impl IntoIterator<Item = PluginContractDescriptor>,
    ) -> Result<Self, PluginContractError> {
        let mut by_ordinal = BTreeMap::new();
        let mut plugin_ids = std::collections::HashSet::new();
        for descriptor in descriptors {
            if descriptor.plugin_ordinal == 0 {
                return Err(PluginContractError::ReservedOrdinal);
            }
            if !plugin_ids.insert(descriptor.plugin_id.clone())
                || by_ordinal
                    .insert(descriptor.plugin_ordinal, descriptor)
                    .is_some()
            {
                return Err(PluginContractError::DuplicatePlugin);
            }
        }
        Ok(Self { by_ordinal })
    }

    /// Validates only generic envelope limits and advertised revisions.
    ///
    /// # Errors
    ///
    /// Returns an error without interpreting any result bytes or counter
    /// meaning.
    pub fn validate_commit(
        &self,
        plugin_ordinal: u32,
        commit: &CoreCommitEnvelope,
    ) -> Result<(), PluginContractError> {
        let descriptor = self
            .by_ordinal
            .get(&plugin_ordinal)
            .ok_or(PluginContractError::UnsupportedResult)?;
        for metrics in &commit.metrics {
            if metrics.values.len() != descriptor.metrics_vector_length as usize
                || !descriptor
                    .metrics_catalog_revisions
                    .contains(&metrics.catalog_revision)
            {
                return Err(PluginContractError::InvalidMetrics);
            }
        }
        if let Some(result) = &commit.plugin_result
            && (result.plugin_ordinal != plugin_ordinal
                || !descriptor
                    .result_schema_versions
                    .contains(&result.schema_version))
        {
            return Err(PluginContractError::UnsupportedResult);
        }
        Ok(())
    }
}

/// Returns the plugin registry shipped by this application.
///
/// # Errors
///
/// Returns an error if a source-level fixture change violates registry
/// identity invariants.
pub fn plugin_registry() -> Result<PluginRegistry, PluginContractError> {
    PluginRegistry::new([
        PluginContractDescriptor {
            plugin_ordinal: ORDERED_MAP_PLUGIN_ORDINAL,
            plugin_id: "ordered-map".to_owned(),
            result_schema_versions: vec![1],
            metrics_catalog_revisions: vec!["ordered-map-metrics/1".to_owned()],
            metrics_vector_length: 10,
            trace_catalog_revisions: vec!["ordered-map-trace/3".to_owned()],
        },
        PluginContractDescriptor {
            plugin_ordinal: FLOW_PLUGIN_ORDINAL,
            plugin_id: "flow".to_owned(),
            result_schema_versions: vec![9],
            metrics_catalog_revisions: vec!["flow-metrics/6".to_owned()],
            metrics_vector_length: 16,
            trace_catalog_revisions: vec!["flow-trace/9".to_owned()],
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(revision: &str, values: &[u64]) -> MetricsVector {
        MetricsVector {
            catalog_revision: revision.to_owned(),
            values: values.to_vec(),
        }
    }

    #[test]
    fn another_plugin_uses_the_unchanged_core_envelope() {
        let registry = PluginRegistry::new([PluginContractDescriptor {
            plugin_ordinal: 2,
            plugin_id: "test-plugin".to_owned(),
            result_schema_versions: vec![7],
            metrics_catalog_revisions: vec!["test-metrics/3".to_owned()],
            metrics_vector_length: 2,
            trace_catalog_revisions: vec!["test-trace/5".to_owned()],
        }])
        .expect("test registry is valid");
        let vector = metrics("test-metrics/3", &[4, 9]);
        let commit = CoreCommitEnvelope {
            phase_id: 81,
            metrics: [vector.clone(), vector.clone(), vector.clone(), vector],
            plugin_result: Some(PluginResultEnvelope {
                plugin_ordinal: 2,
                schema_version: 7,
                payload: vec![0xde, 0xad, 0xbe, 0xef],
            }),
        };

        assert_eq!(registry.validate_commit(2, &commit), Ok(()));
    }

    #[test]
    fn metric_length_is_rejected_before_plugin_decoding() {
        let registry = plugin_registry().expect("plugin registry is valid");
        let vector = metrics("ordered-map-metrics/1", &[0; 9]);
        let commit = CoreCommitEnvelope {
            phase_id: 0,
            metrics: [vector.clone(), vector.clone(), vector.clone(), vector],
            plugin_result: None,
        };

        assert_eq!(
            registry.validate_commit(ORDERED_MAP_PLUGIN_ORDINAL, &commit),
            Err(PluginContractError::InvalidMetrics)
        );
    }

    #[test]
    fn engine_contract_ordinals_and_revisions_are_frozen() {
        let contract = engine_contract_v1();

        assert_eq!(contract.contract_schema_version, 1);
        assert_eq!(contract.accepted_transport_versions, [5, 6]);
        assert_eq!(
            contract
                .plugins
                .iter()
                .map(|plugin| (plugin.plugin_ordinal, plugin.plugin_id.as_str()))
                .collect::<Vec<_>>(),
            [(1, "ordered-map"), (2, "flow")]
        );
        assert_eq!(
            contract.plugins[1].accepted_frame_revisions,
            ["flow-scene/9"]
        );
    }

    #[test]
    fn engine_contract_matches_canonical_cross_language_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../fixtures/contracts/engine-contract-v1.json"
        ))
        .expect("engine contract fixture is valid JSON");
        let expected: EngineContractV1 = serde_json::from_value(fixture["contract"].clone())
            .expect("engine contract fixture is closed and typed");
        assert_eq!(engine_contract_v1(), expected);

        let bytes = serde_json::to_vec(&expected).expect("contract serializes");
        let canonical = crate::jcs::canonicalize(&bytes).expect("contract canonicalizes");
        assert_eq!(
            std::str::from_utf8(&canonical).expect("canonical contract is UTF-8"),
            fixture["canonical"]
                .as_str()
                .expect("canonical fixture is a string")
        );
        assert_eq!(
            crate::jcs::sha256_hex(&canonical),
            fixture["sha256"]
                .as_str()
                .expect("fixture digest is a string")
        );

        let mut unknown = fixture["contract"].clone();
        unknown["future"] = serde_json::json!(true);
        assert!(serde_json::from_value::<EngineContractV1>(unknown).is_err());
    }
}
