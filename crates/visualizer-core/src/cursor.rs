//! Canonical timeline cursor and binary ordinal mapping.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Human-readable plugin timeline item reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimelineItemRef {
    /// Plugin-owned stable stream ID.
    pub stream_id: String,
    /// Zero-based item index within the stream.
    pub index: u32,
}

/// Canonical state boundary. There is no representation for zero applied
/// events; pre-group boundaries canonicalize to a previous boundary or start.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TimelineCursor {
    /// Effective timeline start state.
    Start,
    /// State after one or more events of a semantic group.
    Boundary {
        /// Timeline item containing the group.
        item: TimelineItemRef,
        /// Zero-based semantic group index.
        semantic_group: u32,
        /// One-based applied event count within the group.
        applied_atomic_events: u32,
    },
}

/// Fixed-width cursor fields placed in a binary frame header.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum BinaryTimelineCursor {
    /// Encoded start state.
    Start,
    /// Encoded plugin boundary using a catalog ordinal, never a string.
    Boundary {
        /// Plugin-catalog stream ordinal.
        stream_ordinal: u32,
        /// Zero-based item index.
        item_index: u32,
        /// Zero-based semantic group index.
        semantic_group: u32,
        /// One-based applied event count.
        applied_atomic_events: u32,
    },
}

/// Cursor validation or catalog lookup failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CursorError {
    /// Boundary cursors cannot encode a pre-event state.
    #[error("boundary cursor must include at least one applied event")]
    ZeroAppliedEvents,
    /// The plugin catalog does not define the named stream.
    #[error("timeline stream is not present in the plugin catalog")]
    UnknownStream,
    /// Stream ordinals are non-zero and unique.
    #[error("timeline stream catalog is invalid")]
    InvalidCatalog,
}

/// Immutable mapping from plugin stream IDs to stable binary ordinals.
#[derive(Clone, Debug)]
pub struct StreamCatalog {
    ordinals: BTreeMap<String, u32>,
}

impl StreamCatalog {
    /// Creates a validated append-only stream catalog.
    ///
    /// # Errors
    ///
    /// Rejects ordinal zero, duplicate stream IDs, and duplicate ordinals.
    pub fn new(entries: impl IntoIterator<Item = (String, u32)>) -> Result<Self, CursorError> {
        let mut ordinals = BTreeMap::new();
        let mut used_ordinals = std::collections::HashSet::new();
        for (stream_id, ordinal) in entries {
            if ordinal == 0
                || !used_ordinals.insert(ordinal)
                || ordinals.insert(stream_id, ordinal).is_some()
            {
                return Err(CursorError::InvalidCatalog);
            }
        }
        Ok(Self { ordinals })
    }

    /// Converts a logical cursor to its fixed-width canonical wire form.
    ///
    /// # Errors
    ///
    /// Rejects zero-event boundaries and unknown stream IDs.
    pub fn encode(&self, cursor: &TimelineCursor) -> Result<BinaryTimelineCursor, CursorError> {
        match cursor {
            TimelineCursor::Start => Ok(BinaryTimelineCursor::Start),
            TimelineCursor::Boundary {
                item,
                semantic_group,
                applied_atomic_events,
            } => {
                if *applied_atomic_events == 0 {
                    return Err(CursorError::ZeroAppliedEvents);
                }
                let stream_ordinal = self
                    .ordinals
                    .get(&item.stream_id)
                    .copied()
                    .ok_or(CursorError::UnknownStream)?;
                Ok(BinaryTimelineCursor::Boundary {
                    stream_ordinal,
                    item_index: item.index,
                    semantic_group: *semantic_group,
                    applied_atomic_events: *applied_atomic_events,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_cursor_uses_catalog_ordinal() {
        let catalog = StreamCatalog::new([("initial".to_owned(), 1), ("operations".to_owned(), 2)])
            .expect("fixture catalog is valid");
        let cursor = TimelineCursor::Boundary {
            item: TimelineItemRef {
                stream_id: "operations".to_owned(),
                index: 17,
            },
            semantic_group: 3,
            applied_atomic_events: 1,
        };

        assert_eq!(
            catalog.encode(&cursor),
            Ok(BinaryTimelineCursor::Boundary {
                stream_ordinal: 2,
                item_index: 17,
                semantic_group: 3,
                applied_atomic_events: 1,
            })
        );
    }

    #[test]
    fn zero_event_boundary_is_not_a_cache_key() {
        let catalog =
            StreamCatalog::new([("operations".to_owned(), 2)]).expect("fixture catalog is valid");
        let cursor = TimelineCursor::Boundary {
            item: TimelineItemRef {
                stream_id: "operations".to_owned(),
                index: 0,
            },
            semantic_group: 0,
            applied_atomic_events: 0,
        };

        assert_eq!(catalog.encode(&cursor), Err(CursorError::ZeroAppliedEvents));
    }
}
