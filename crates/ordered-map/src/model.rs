//! Common operation, trace, metric, and snapshot contracts.

use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;
use visualizer_core::arena::{ArenaError, ArenaKey};
use visualizer_core::rng::RngError;

/// Maximum structural entities admitted to one visual frame.
pub const MAX_VISUAL_ENTITIES: usize = 250_000;
/// Maximum trace events admitted to one operation commit.
pub const MAX_TRACE_EVENTS: usize = 250_000;

/// Stable identity of a structural node.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub ArenaKey);

/// Stable identity of a logical key/value entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EntryId(pub ArenaKey);

/// Stable identity of a summary, prefix, cluster, sentinel, or other
/// non-entry-bearing auxiliary structure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AuxiliaryId(pub ArenaKey);

/// Kind-preserving identity used by algorithm-neutral structure snapshots.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "kebab-case")]
pub enum StructureEntityId {
    /// Primary structural node.
    Node(NodeId),
    /// Auxiliary structural entity.
    Auxiliary(AuxiliaryId),
}

/// One ordered-map request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Insert a new entry or overwrite a present value.
    Insert {
        /// Ordered key.
        key: u64,
        /// Value payload.
        value: String,
    },
    /// Remove an entry when present.
    Remove {
        /// Ordered key.
        key: u64,
    },
    /// Read a value.
    Get {
        /// Ordered key.
        key: u64,
    },
    /// Find the least entry whose key is not less than the query.
    LowerBound {
        /// Lower-bound query key.
        key: u64,
    },
}

/// Public result of an operation. Misses are normal results.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum OperationResult {
    /// A new key was inserted.
    Inserted {
        /// Newly allocated entry identity.
        entry: EntryId,
    },
    /// An existing value was replaced.
    Overwritten {
        /// Preserved entry identity.
        entry: EntryId,
        /// Replaced value.
        previous: String,
    },
    /// A present value was removed.
    Removed {
        /// Invalidated entry identity.
        entry: EntryId,
        /// Removed value.
        value: String,
    },
    /// A lookup or removal did not find a key.
    Miss,
    /// A query found an entry.
    Found {
        /// Found entry identity.
        entry: EntryId,
        /// Found key.
        #[serde(serialize_with = "serialize_u64_decimal")]
        key: u64,
        /// Found value.
        value: String,
    },
}

/// Absolute metrics accumulated by a map instance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Key comparisons.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub comparisons: u64,
    /// Structural node visits.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub node_visits: u64,
    /// Binary-universe bit tests.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub bit_tests: u64,
    /// Tree rotations.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub rotations: u64,
    /// Color changes.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub recolors: u64,
    /// Multiway node splits.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub splits: u64,
    /// Multiway node merges.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub merges: u64,
    /// Entries participating in rebuilds.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub rebuild_items: u64,
    /// Logical node/entry allocations.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub allocations: u64,
    /// Logical node/entry frees.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub frees: u64,
}

/// Stable semantic category for one trace record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TraceKind {
    /// Compare the query with an entry.
    Compare,
    /// Follow a structural link.
    Descend,
    /// Allocate an entry and node.
    Insert,
    /// Replace only an entry value.
    Overwrite,
    /// Remove an entry and node.
    Remove,
    /// Rotate a binary-tree link.
    RotateLeft,
    /// Rotate a binary-tree link.
    RotateRight,
    /// Update height, size, level, color, or another invariant field.
    UpdateMetadata,
    /// Rebuild a contiguous subtree while preserving node and entry identity.
    Rebuild,
    /// Split one multiway structural node.
    Split,
    /// Merge adjacent multiway structural nodes.
    Merge,
    /// Move an existing entry between structural nodes.
    MoveEntry,
    /// Return an operation result without structural change.
    Result,
}

/// An atomic, renderer-independent algorithm event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceEvent {
    /// Numeric catalog identifier, stable within the trace revision.
    pub catalog_id: u32,
    /// Semantic event kind.
    pub kind: TraceKind,
    /// Primary structural entity when applicable.
    pub node: Option<StructureEntityId>,
    /// Destination structural entity for a traversed link.
    pub target: Option<StructureEntityId>,
    /// Related logical entry when applicable.
    pub entry: Option<EntryId>,
    /// Query or affected key.
    #[serde(serialize_with = "serialize_optional_u64_decimal")]
    pub key: Option<u64>,
    /// First record in the commit-local reversible state patch table.
    pub patch_start: u32,
    /// Number of commit-local reversible state patch records for this event.
    pub patch_count: u32,
}

impl TraceEvent {
    /// Associates a structural-link destination with this event.
    #[must_use]
    pub const fn with_target(mut self, target: Option<StructureEntityId>) -> Self {
        self.target = target;
        self
    }
}

/// Stable ordinal for one cumulative metric in a reversible trace patch.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricOrdinal {
    /// Key comparisons.
    Comparisons,
    /// Structural node visits.
    NodeVisits,
    /// Binary-universe bit tests.
    BitTests,
    /// Tree rotations.
    Rotations,
    /// Color changes.
    Recolors,
    /// Multiway node splits.
    Splits,
    /// Multiway node merges.
    Merges,
    /// Entries participating in rebuilds.
    RebuildItems,
    /// Logical node/entry allocations.
    Allocations,
    /// Logical node/entry frees.
    Frees,
}

/// Key/value oracle independent of physical shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEntry {
    /// Stable entry identity.
    pub id: EntryId,
    /// Ordered key.
    #[serde(serialize_with = "serialize_u64_decimal")]
    pub key: u64,
    /// Stored value.
    pub value: String,
}

/// Canonical ordered contents and absolute metrics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalSnapshot {
    /// Entries sorted by key.
    pub entries: Vec<CanonicalEntry>,
    /// Absolute metrics at this boundary.
    pub metrics: Metrics,
}

/// One node in a structure-oriented snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructureNode {
    /// Stable node identity.
    pub id: StructureEntityId,
    /// Algorithm-neutral role used by the presentation plugin.
    pub role: String,
    /// Entries held by the node in local order.
    pub entries: Vec<EntryId>,
    /// Keys corresponding one-to-one with `entries`.
    #[serde(serialize_with = "serialize_u64_vector_decimal")]
    pub keys: Vec<u64>,
    /// Ordered structural links. Slots remain stable within a structure kind.
    pub links: Vec<StructureLink>,
    /// Algorithm-specific metadata exposed as stable numeric pairs.
    #[serde(serialize_with = "serialize_metadata_decimal")]
    pub metadata: Vec<(String, u64)>,
}

/// One directed structural link in a physical-shape snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructureLink {
    /// Variant-defined stable link slot.
    pub slot: u32,
    /// Human-readable link role for diagnostics and plugin presentation.
    pub role: String,
    /// Target structural node.
    pub target: StructureEntityId,
}

/// Physical shape oracle used by the renderer and invariant tests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructureSnapshot {
    /// Root node when nonempty.
    pub root: Option<StructureEntityId>,
    /// Nodes sorted by stable ID.
    pub nodes: Vec<StructureNode>,
}

/// One reversible change in an event-owned state transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum StatePatchRecord {
    /// Replace the projection root.
    Root {
        /// Root required before forward application.
        before: Option<StructureEntityId>,
        /// Root produced by forward application.
        after: Option<StructureEntityId>,
    },
    /// Create, replace, or delete one structural entity.
    Node {
        /// Stable entity identity being changed.
        id: StructureEntityId,
        /// Entity required before forward application.
        before: Option<Box<StructureNode>>,
        /// Entity produced by forward application.
        after: Option<Box<StructureNode>>,
    },
    /// Create, replace, or delete one canonical entry.
    Entry {
        /// Stable entry identity being changed.
        id: EntryId,
        /// Entry required before forward application.
        before: Option<Box<CanonicalEntry>>,
        /// Entry produced by forward application.
        after: Option<Box<CanonicalEntry>>,
    },
    /// Replace one absolute cumulative metric.
    Metric {
        /// Stable metric catalog ordinal.
        ordinal: MetricOrdinal,
        /// Value required before forward application.
        #[serde(serialize_with = "serialize_u64_decimal")]
        before: u64,
        /// Value produced by forward application.
        #[serde(serialize_with = "serialize_u64_decimal")]
        after: u64,
    },
}

/// Algorithm or bounded-resource failure.
#[derive(Debug, Error)]
pub enum MapError {
    /// Stable arena operation failed.
    #[error(transparent)]
    Arena(#[from] ArenaError),
    /// Versioned random sampling failed.
    #[error(transparent)]
    Rng(#[from] RngError),
    /// A checked counter or metadata calculation overflowed.
    #[error("ordered-map arithmetic overflow")]
    ArithmeticOverflow,
    /// Internal links violate a precondition required to complete an operation.
    #[error("ordered-map structure is corrupt: {0}")]
    Corrupt(&'static str),
    /// A public algorithm parameter is outside its versioned domain.
    #[error("invalid ordered-map configuration: {0}")]
    InvalidConfiguration(&'static str),
    /// A reversible trace state or patch violates its contract.
    #[error("invalid ordered-map trace state: {0}")]
    TraceState(&'static str),
    /// One event produced a state patch that violates the trace contract.
    #[error("invalid ordered-map trace event {catalog_id}: {message}")]
    TraceEventState {
        /// Stable event catalog identifier.
        catalog_id: u32,
        /// Stable validation failure.
        message: &'static str,
    },
    /// A non-arena collection could not reserve bounded capacity.
    #[error("ordered-map allocation failed")]
    AllocationFailed,
    /// A public bounded-resource contract would be exceeded.
    #[error("ordered-map resource limit exceeded: {0}")]
    ResourceLimit(&'static str),
}

/// Debug/test invariant failure with a stable code.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invariant violation: {code}")]
pub struct InvariantViolation {
    /// Stable machine-readable violation code.
    pub code: &'static str,
}

/// Common contract implemented by each concrete ordered-map variant.
pub trait OrderedMap {
    /// Applies exactly one operation and appends its atomic events.
    ///
    /// # Errors
    ///
    /// Returns a bounded resource, arithmetic, RNG, or internal-consistency
    /// error without substituting another algorithm.
    fn apply(
        &mut self,
        operation: Operation,
        trace: &mut Vec<TraceEvent>,
    ) -> Result<OperationResult, MapError>;

    /// Returns sorted contents independent of physical structure.
    fn canonical_snapshot(&self) -> CanonicalSnapshot;

    /// Returns a stable physical structure snapshot.
    fn structure_snapshot(&self) -> StructureSnapshot;

    /// Returns the number of entities a structure snapshot would contain
    /// without allocating the snapshot.
    fn structure_entity_count(&self) -> usize;

    /// Checks every variant-specific invariant.
    ///
    /// # Errors
    ///
    /// Returns the first deterministic invariant code.
    fn check_invariants(&self) -> Result<(), InvariantViolation>;

    /// Returns owned allocation capacity used for admission accounting.
    fn estimated_bytes(&self) -> usize;
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde `serialize_with` requires `&T`.
fn serialize_u64_decimal<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

#[allow(clippy::ref_option)] // serde `serialize_with` requires `&T`.
fn serialize_optional_u64_decimal<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value.map(|value| value.to_string()).serialize(serializer)
}

fn serialize_u64_vector_decimal<S>(values: &[u64], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let decimal: Vec<_> = values.iter().map(u64::to_string).collect();
    decimal.serialize(serializer)
}

fn serialize_metadata_decimal<S>(values: &[(String, u64)], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for (name, value) in values {
        sequence.serialize_element(&(name, value.to_string()))?;
    }
    sequence.end()
}
