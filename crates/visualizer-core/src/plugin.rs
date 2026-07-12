//! Build-time plugin registry and generic envelope contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ordered-map is permanently assigned ordinal one; new plugins append.
pub const ORDERED_MAP_PLUGIN_ORDINAL: u32 = 1;

/// Versioned build-time plugin contract advertised during handshake.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
pub struct MetricsVector {
    /// Revision that defines counter meaning and order.
    pub catalog_revision: String,
    /// Unsigned absolute values in catalog order.
    pub values: Vec<u64>,
}

/// Generic logical commit fields inspected by playback core.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CoreCommitEnvelope {
    /// Numeric plugin phase ID; meaning remains in the plugin catalog.
    pub phase_id: u32,
    /// Four absolute metrics scopes.
    pub metrics: [MetricsVector; 4],
    /// Optional plugin-specific operation result.
    pub plugin_result: Option<PluginResultEnvelope>,
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
    PluginRegistry::new([PluginContractDescriptor {
        plugin_ordinal: ORDERED_MAP_PLUGIN_ORDINAL,
        plugin_id: "ordered-map".to_owned(),
        result_schema_versions: vec![1],
        metrics_catalog_revisions: vec!["ordered-map-metrics/1".to_owned()],
        metrics_vector_length: 10,
        trace_catalog_revisions: vec!["ordered-map-trace/3".to_owned()],
    }])
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
}
