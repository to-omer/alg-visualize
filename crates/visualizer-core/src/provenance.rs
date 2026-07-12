//! Raw, bounded generator provenance and materialized-item verification.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use thiserror::Error;

/// The generator revision implemented by this contract version.
pub const SUPPORTED_GENERATOR_REVISION: &str = "ordered-map-generator/1";
/// Maximum encoded raw payload retained for one provenance envelope.
pub const MAX_PROVENANCE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// A raw envelope preserves unsupported revisions without pretending to decode them.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorProvenanceEnvelope {
    /// Versioned generator contract.
    pub generator_revision: String,
    /// Bounded raw JSON; typed decoding happens only for supported revisions.
    pub payload: Box<RawValue>,
}

/// Statistics that are verified with regenerated materialized items.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorStats {
    /// Number of generated items.
    pub item_count: u32,
    /// Number of unique keys after the generated sequence.
    pub final_unique_count: u32,
    /// Insert operation count.
    pub insert_count: u32,
    /// Remove operation count.
    pub remove_count: u32,
    /// Get operation count.
    pub get_count: u32,
    /// Lower-bound operation count.
    pub lower_bound_count: u32,
    /// Realized successful get count.
    pub achieved_get_hits: u32,
    /// Realized successful remove count.
    pub achieved_remove_hits: u32,
    /// Realized insert-overwrite count.
    pub achieved_insert_overwrites: u32,
}

/// Typed payload for a supported generator revision.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"), deny_unknown_fields)]
pub struct GeneratorProvenanceV1<T> {
    /// Generator input.
    pub spec: T,
    /// Must be `sha256` for revision 1.
    pub digest_algorithm: String,
    /// Lowercase digest of the materialized-item binary encoding.
    pub materialized_digest: String,
    /// Expected generation statistics.
    pub stats: GeneratorStats,
}

/// Canonical item used by the versioned digest codec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializedItem {
    /// Initial entry.
    Entry {
        /// Canonical decimal key.
        key: String,
        /// Unmodified Unicode value.
        value: String,
    },
    /// Insert operation.
    Insert {
        /// Canonical decimal key.
        key: String,
        /// Unmodified Unicode value.
        value: String,
    },
    /// Remove operation.
    Remove {
        /// Canonical decimal key.
        key: String,
    },
    /// Get operation.
    Get {
        /// Canonical decimal key.
        key: String,
    },
    /// Lower-bound operation.
    LowerBound {
        /// Canonical decimal key.
        key: String,
    },
}

/// Session verification state. The envelope itself is retained in every state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvenanceState {
    /// Supported payload is accepted but regeneration has not run.
    Pending,
    /// Digest, items, and statistics exactly match regeneration.
    Verified,
    /// The supported payload failed validation or exact regeneration.
    Invalid,
    /// The revision is unknown and its raw payload remains opaque.
    Unsupported,
}

/// Imported provenance plus runtime-only verification state.
#[derive(Clone, Debug)]
pub struct ImportedProvenance {
    envelope: GeneratorProvenanceEnvelope,
    state: ProvenanceState,
}

impl ImportedProvenance {
    /// Returns the preserved raw envelope.
    pub const fn envelope(&self) -> &GeneratorProvenanceEnvelope {
        &self.envelope
    }

    /// Returns the runtime-only verification state.
    pub const fn state(&self) -> ProvenanceState {
        self.state
    }
}

/// Provenance import failure before a session state can be constructed.
#[derive(Debug, Error)]
pub enum ProvenanceError {
    /// Raw payload exceeds the per-envelope limit.
    #[error("provenance payload exceeds {MAX_PROVENANCE_PAYLOAD_BYTES} bytes")]
    PayloadTooLarge,
    /// A supported revision cannot be decoded strictly.
    #[error("supported provenance payload is invalid: {0}")]
    InvalidPayload(#[from] serde_json::Error),
    /// The versioned materialized-item codec cannot represent a length.
    #[error("materialized input length exceeds the V1 u64 codec")]
    MaterializedLengthOverflow,
}

/// Imports raw provenance, retaining unknown revisions as unsupported.
///
/// Supported revisions enter `Pending` only when the declared digest already
/// matches the materialized items. A mismatch is immediately `Invalid`.
///
/// # Errors
///
/// Returns an error for an oversized raw payload or malformed supported payload.
pub fn import_provenance<T: DeserializeOwned>(
    envelope: GeneratorProvenanceEnvelope,
    materialized: &[MaterializedItem],
) -> Result<ImportedProvenance, ProvenanceError> {
    if envelope.payload.get().len() > MAX_PROVENANCE_PAYLOAD_BYTES {
        return Err(ProvenanceError::PayloadTooLarge);
    }
    if envelope.generator_revision != SUPPORTED_GENERATOR_REVISION {
        return Ok(ImportedProvenance {
            envelope,
            state: ProvenanceState::Unsupported,
        });
    }

    let payload: GeneratorProvenanceV1<T> = serde_json::from_str(envelope.payload.get())?;
    let valid_digest_algorithm = payload.digest_algorithm == "sha256";
    let valid_digest = payload.materialized_digest == materialized_digest(materialized)?;
    Ok(ImportedProvenance {
        envelope,
        state: if valid_digest_algorithm && valid_digest {
            ProvenanceState::Pending
        } else {
            ProvenanceState::Invalid
        },
    })
}

/// Regenerates and verifies a pending supported envelope.
///
/// Calling this for an invalid or unsupported envelope is a no-op so state
/// transitions remain monotonic.
///
/// # Errors
///
/// Returns an error if a previously accepted supported payload cannot be decoded.
pub fn verify_pending<T, F>(
    imported: &mut ImportedProvenance,
    materialized: &[MaterializedItem],
    regenerate: F,
) -> Result<(), ProvenanceError>
where
    T: DeserializeOwned,
    F: FnOnce(&T) -> (Vec<MaterializedItem>, GeneratorStats),
{
    if imported.state != ProvenanceState::Pending {
        return Ok(());
    }
    let payload: GeneratorProvenanceV1<T> = serde_json::from_str(imported.envelope.payload.get())?;
    let (regenerated, stats) = regenerate(&payload.spec);
    imported.state = if regenerated == materialized
        && stats == payload.stats
        && materialized_digest(&regenerated)? == payload.materialized_digest
    {
        ProvenanceState::Verified
    } else {
        ProvenanceState::Invalid
    };
    Ok(())
}

/// Drops provenance when the corresponding materialized content is edited.
pub fn discard_on_edit(imported: &mut Option<ImportedProvenance>) {
    *imported = None;
}

/// Computes the lowercase SHA-256 of the versioned canonical binary item stream.
///
/// # Errors
///
/// Returns an error on targets whose addressable input length does not fit the
/// codec's `u64` length fields.
pub fn materialized_digest(items: &[MaterializedItem]) -> Result<String, ProvenanceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"alg-visualize/materialized-items/1\0");
    hasher.update(
        u64::try_from(items.len())
            .map_err(|_| ProvenanceError::MaterializedLengthOverflow)?
            .to_le_bytes(),
    );
    for item in items {
        match item {
            MaterializedItem::Entry { key, value } => {
                hasher.update([0]);
                encode_string(&mut hasher, key)?;
                encode_string(&mut hasher, value)?;
            }
            MaterializedItem::Insert { key, value } => {
                hasher.update([1]);
                encode_string(&mut hasher, key)?;
                encode_string(&mut hasher, value)?;
            }
            MaterializedItem::Remove { key } => {
                hasher.update([2]);
                encode_string(&mut hasher, key)?;
            }
            MaterializedItem::Get { key } => {
                hasher.update([3]);
                encode_string(&mut hasher, key)?;
            }
            MaterializedItem::LowerBound { key } => {
                hasher.update([4]);
                encode_string(&mut hasher, key)?;
            }
        }
    }
    Ok(hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        }))
}

fn encode_string(output: &mut Sha256, value: &str) -> Result<(), ProvenanceError> {
    let length =
        u64::try_from(value.len()).map_err(|_| ProvenanceError::MaterializedLengthOverflow)?;
    output.update(length.to_le_bytes());
    output.update(value.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::value::to_raw_value;

    use super::*;

    #[derive(Clone, Debug, Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct DemoSpec {
        count: u32,
    }

    fn item() -> MaterializedItem {
        MaterializedItem::Entry {
            key: "7".to_owned(),
            value: "value".to_owned(),
        }
    }

    fn envelope(revision: &str, digest: String) -> GeneratorProvenanceEnvelope {
        GeneratorProvenanceEnvelope {
            generator_revision: revision.to_owned(),
            payload: to_raw_value(&GeneratorProvenanceV1 {
                spec: DemoSpec { count: 1 },
                digest_algorithm: "sha256".to_owned(),
                materialized_digest: digest,
                stats: GeneratorStats {
                    item_count: 1,
                    final_unique_count: 1,
                    ..GeneratorStats::default()
                },
            })
            .expect("serializable payload"),
        }
    }

    #[test]
    fn supported_payload_moves_pending_to_verified() {
        let items = vec![item()];
        let mut imported = import_provenance::<DemoSpec>(
            envelope(
                SUPPORTED_GENERATOR_REVISION,
                materialized_digest(&items).expect("bounded digest"),
            ),
            &items,
        )
        .expect("valid provenance");
        assert_eq!(imported.state(), ProvenanceState::Pending);

        verify_pending::<DemoSpec, _>(&mut imported, &items, |_| {
            (
                items.clone(),
                GeneratorStats {
                    item_count: 1,
                    final_unique_count: 1,
                    ..GeneratorStats::default()
                },
            )
        })
        .expect("verification succeeds");

        assert_eq!(imported.state(), ProvenanceState::Verified);
    }

    #[test]
    fn one_byte_materialized_change_is_invalid() {
        let original = vec![item()];
        let changed = vec![MaterializedItem::Entry {
            key: "7".to_owned(),
            value: "valuE".to_owned(),
        }];
        let imported = import_provenance::<DemoSpec>(
            envelope(
                SUPPORTED_GENERATOR_REVISION,
                materialized_digest(&original).expect("bounded digest"),
            ),
            &changed,
        )
        .expect("well-formed provenance");

        assert_eq!(imported.state(), ProvenanceState::Invalid);
    }

    #[test]
    fn unknown_revision_stays_raw_and_does_not_block_execution() {
        let items = vec![item()];
        let imported = import_provenance::<DemoSpec>(
            GeneratorProvenanceEnvelope {
                generator_revision: "ordered-map-generator/999".to_owned(),
                payload: RawValue::from_string("{\"future\":true}".to_owned())
                    .expect("valid raw JSON"),
            },
            &items,
        )
        .expect("unsupported payload is retained");

        assert_eq!(imported.state(), ProvenanceState::Unsupported);
        assert_eq!(imported.envelope().payload.get(), "{\"future\":true}");
    }

    #[test]
    fn editing_discards_provenance_instead_of_relabeling_it() {
        let items = vec![item()];
        let mut imported = Some(
            import_provenance::<DemoSpec>(
                envelope(
                    SUPPORTED_GENERATOR_REVISION,
                    materialized_digest(&items).expect("bounded digest"),
                ),
                &items,
            )
            .expect("valid provenance"),
        );

        discard_on_edit(&mut imported);

        assert!(imported.is_none());
    }
}
