//! Visual identity ownership and independent snapshot/delta scene paths.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable generational identity supplied by an algorithm-owned arena.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct StableId {
    /// Arena slot index.
    pub index: u32,
    /// Slot generation.
    pub generation: u32,
}

/// Renderer identity derived only from stable owner identity and an immutable slot.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct VisualEntityId {
    /// Plugin entity-kind ordinal.
    pub kind: u8,
    /// Stable algorithm owner.
    pub owner: StableId,
    /// Immutable role-local slot.
    pub slot: u8,
}

/// Edge endpoint whose identity may change for a particular ownership rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EdgeOwnership {
    /// Binary-tree and skip-list edges are owned by `(source, slot)`; only the
    /// target can change.
    SourceSlot,
    /// B-tree incoming edges are owned by the target; only source/slot can change.
    Target,
}

/// Canonical logical renderer entity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SceneEntity {
    /// Structural node, separate from the entry it currently displays.
    Node {
        /// Visual identity.
        id: VisualEntityId,
        /// Algorithm node identity.
        node: StableId,
    },
    /// Logical map entry that may move between structural nodes.
    Entry {
        /// Visual identity.
        id: VisualEntityId,
        /// Entry identity.
        entry: StableId,
        /// Canonical numeric key.
        key: u64,
        /// Displayed value.
        value: String,
        /// Current structural host.
        node: StableId,
    },
    /// Topology edge with an explicit ownership policy.
    Edge {
        /// Visual identity.
        id: VisualEntityId,
        /// Endpoint ownership rule.
        ownership: EdgeOwnership,
        /// Current source node.
        source: StableId,
        /// Current source-local slot.
        source_slot: u8,
        /// Current target node.
        target: StableId,
    },
}

impl SceneEntity {
    const fn id(&self) -> VisualEntityId {
        match self {
            Self::Node { id, .. } | Self::Entry { id, .. } | Self::Edge { id, .. } => *id,
        }
    }
}

/// Canonically ID-ordered logical scene; Pixi objects are not part of this state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalScene {
    entities: BTreeMap<VisualEntityId, SceneEntity>,
}

impl CanonicalScene {
    /// Returns one entity by renderer identity.
    pub fn get(&self, id: VisualEntityId) -> Option<&SceneEntity> {
        self.entities.get(&id)
    }

    /// Returns entities in canonical identity order.
    pub fn entities(&self) -> impl ExactSizeIterator<Item = &SceneEntity> {
        self.entities.values()
    }

    /// Applies a validated delta transaction to a private copy.
    ///
    /// # Errors
    ///
    /// Rejects duplicate/missing IDs and endpoint mutations forbidden by the
    /// entity's ownership rule. The original scene remains unchanged on error.
    pub fn apply_transaction(&self, deltas: &[SceneDelta]) -> Result<Self, SceneError> {
        let mut staged = self.clone();
        for delta in deltas {
            staged.apply(delta)?;
        }
        Ok(staged)
    }

    fn apply(&mut self, delta: &SceneDelta) -> Result<(), SceneError> {
        match delta {
            SceneDelta::Create(entity) => {
                if self.entities.insert(entity.id(), entity.clone()).is_some() {
                    return Err(SceneError::DuplicateEntity);
                }
            }
            SceneDelta::Delete { id } => {
                self.entities.remove(id).ok_or(SceneError::UnknownEntity)?;
            }
            SceneDelta::MoveEntry { id, node } => {
                let SceneEntity::Entry {
                    node: current_node, ..
                } = self.entities.get_mut(id).ok_or(SceneError::UnknownEntity)?
                else {
                    return Err(SceneError::WrongEntityKind);
                };
                *current_node = *node;
            }
            SceneDelta::RetargetSourceEdge { id, target } => {
                let SceneEntity::Edge {
                    ownership,
                    target: current_target,
                    ..
                } = self.entities.get_mut(id).ok_or(SceneError::UnknownEntity)?
                else {
                    return Err(SceneError::WrongEntityKind);
                };
                if *ownership != EdgeOwnership::SourceSlot {
                    return Err(SceneError::WrongOwnership);
                }
                *current_target = *target;
            }
            SceneDelta::ReattachTargetEdge {
                id,
                source,
                source_slot,
            } => {
                let SceneEntity::Edge {
                    ownership,
                    source: current_source,
                    source_slot: current_slot,
                    ..
                } = self.entities.get_mut(id).ok_or(SceneError::UnknownEntity)?
                else {
                    return Err(SceneError::WrongEntityKind);
                };
                if *ownership != EdgeOwnership::Target {
                    return Err(SceneError::WrongOwnership);
                }
                *current_source = *source;
                *current_slot = *source_slot;
            }
        }
        Ok(())
    }
}

/// Atomic logical scene mutation. It contains topology facts; the renderer
/// never infers them from explanation IDs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SceneDelta {
    /// Add a complete entity.
    Create(SceneEntity),
    /// Delete an entity.
    Delete {
        /// Existing identity.
        id: VisualEntityId,
    },
    /// Move an entry while retaining its identity.
    MoveEntry {
        /// Entry entity identity.
        id: VisualEntityId,
        /// New structural host.
        node: StableId,
    },
    /// Change only the target of a source-slot-owned edge.
    RetargetSourceEdge {
        /// Edge identity.
        id: VisualEntityId,
        /// New target.
        target: StableId,
    },
    /// Change only source/slot of a target-owned incoming edge.
    ReattachTargetEdge {
        /// Edge identity.
        id: VisualEntityId,
        /// New source.
        source: StableId,
        /// New source slot.
        source_slot: u8,
    },
}

/// Projection-only record used by the independent full-snapshot path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionRecord {
    /// Structural identity.
    pub node: StableId,
    /// Entry currently hosted by the node.
    pub entry: StableId,
    /// Numeric key.
    pub key: u64,
    /// Value.
    pub value: String,
    /// Optional source-owned left edge target.
    pub left: Option<StableId>,
    /// Optional source-owned right edge target.
    pub right: Option<StableId>,
}

/// Builds a full logical scene directly from read-only projection records.
/// This code does not call the delta replayer.
///
/// # Errors
///
/// Rejects duplicate derived visual IDs.
pub fn project_snapshot(records: &[ProjectionRecord]) -> Result<CanonicalScene, SceneError> {
    let mut entities = BTreeMap::new();
    for record in records {
        insert_projected(
            &mut entities,
            SceneEntity::Node {
                id: node_entity_id(record.node),
                node: record.node,
            },
        )?;
        insert_projected(
            &mut entities,
            SceneEntity::Entry {
                id: entry_entity_id(record.entry),
                entry: record.entry,
                key: record.key,
                value: record.value.clone(),
                node: record.node,
            },
        )?;
        for (slot, target) in [(0, record.left), (1, record.right)] {
            if let Some(target) = target {
                insert_projected(
                    &mut entities,
                    SceneEntity::Edge {
                        id: source_edge_id(record.node, slot),
                        ownership: EdgeOwnership::SourceSlot,
                        source: record.node,
                        source_slot: slot,
                        target,
                    },
                )?;
            }
        }
    }
    Ok(CanonicalScene { entities })
}

/// Derives a node renderer ID.
pub const fn node_entity_id(node: StableId) -> VisualEntityId {
    VisualEntityId {
        kind: 1,
        owner: node,
        slot: 0,
    }
}

/// Derives an entry renderer ID independently from its current node.
pub const fn entry_entity_id(entry: StableId) -> VisualEntityId {
    VisualEntityId {
        kind: 2,
        owner: entry,
        slot: 0,
    }
}

/// Derives a source-slot-owned edge ID.
pub const fn source_edge_id(source: StableId, slot: u8) -> VisualEntityId {
    VisualEntityId {
        kind: 3,
        owner: source,
        slot,
    }
}

/// Derives a target-owned B-tree incoming edge ID.
pub const fn target_edge_id(target: StableId) -> VisualEntityId {
    VisualEntityId {
        kind: 4,
        owner: target,
        slot: 0,
    }
}

fn insert_projected(
    entities: &mut BTreeMap<VisualEntityId, SceneEntity>,
    entity: SceneEntity,
) -> Result<(), SceneError> {
    if entities.insert(entity.id(), entity).is_some() {
        Err(SceneError::DuplicateEntity)
    } else {
        Ok(())
    }
}

/// Scene validation failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SceneError {
    /// Create or projection reused a renderer identity.
    #[error("duplicate visual entity identity")]
    DuplicateEntity,
    /// Delta references a missing entity.
    #[error("unknown visual entity identity")]
    UnknownEntity,
    /// Delta kind does not match the referenced entity.
    #[error("scene delta references the wrong entity kind")]
    WrongEntityKind,
    /// Edge endpoint mutation violates its immutable ownership rule.
    #[error("scene edge mutation violates identity ownership")]
    WrongOwnership,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: StableId = StableId {
        index: 0,
        generation: 1,
    };
    const LEFT: StableId = StableId {
        index: 1,
        generation: 1,
    };
    const RIGHT: StableId = StableId {
        index: 2,
        generation: 1,
    };
    const ENTRY: StableId = StableId {
        index: 9,
        generation: 3,
    };

    fn initial_records() -> Vec<ProjectionRecord> {
        vec![
            ProjectionRecord {
                node: ROOT,
                entry: ENTRY,
                key: 10,
                value: "root".to_owned(),
                left: Some(LEFT),
                right: None,
            },
            ProjectionRecord {
                node: LEFT,
                entry: StableId {
                    index: 10,
                    generation: 1,
                },
                key: 5,
                value: "left".to_owned(),
                left: None,
                right: None,
            },
        ]
    }

    #[test]
    fn source_edge_retarget_keeps_identity_and_matches_independent_snapshot() {
        let before = project_snapshot(&initial_records()).expect("valid projection");
        let edge_id = source_edge_id(ROOT, 0);
        let after = before
            .apply_transaction(&[SceneDelta::RetargetSourceEdge {
                id: edge_id,
                target: RIGHT,
            }])
            .expect("source edge can retarget");

        let mut records = initial_records();
        records[0].left = Some(RIGHT);
        let oracle = project_snapshot(&records).expect("valid independent projection");

        assert_eq!(after, oracle);
        assert!(matches!(
            after.get(edge_id),
            Some(SceneEntity::Edge { target: RIGHT, .. })
        ));
    }

    #[test]
    fn entry_identity_survives_structural_movement() {
        let before = project_snapshot(&initial_records()).expect("valid projection");
        let entry_id = entry_entity_id(ENTRY);
        let after = before
            .apply_transaction(&[SceneDelta::MoveEntry {
                id: entry_id,
                node: RIGHT,
            }])
            .expect("entry can move");

        assert!(matches!(
            after.get(entry_id),
            Some(SceneEntity::Entry {
                entry: ENTRY,
                node: RIGHT,
                ..
            })
        ));
    }

    #[test]
    fn target_owned_edge_rejects_retarget_and_accepts_reattach() {
        let id = target_edge_id(LEFT);
        let edge = SceneEntity::Edge {
            id,
            ownership: EdgeOwnership::Target,
            source: ROOT,
            source_slot: 0,
            target: LEFT,
        };
        let scene = CanonicalScene::default()
            .apply_transaction(&[SceneDelta::Create(edge)])
            .expect("valid edge create");

        assert_eq!(
            scene.apply_transaction(&[SceneDelta::RetargetSourceEdge { id, target: RIGHT }]),
            Err(SceneError::WrongOwnership)
        );
        let reattached = scene
            .apply_transaction(&[SceneDelta::ReattachTargetEdge {
                id,
                source: RIGHT,
                source_slot: 3,
            }])
            .expect("target-owned edge can move source");
        assert!(matches!(
            reattached.get(id),
            Some(SceneEntity::Edge {
                source: RIGHT,
                source_slot: 3,
                target: LEFT,
                ..
            })
        ));
    }

    #[test]
    fn failed_transaction_does_not_mutate_original_scene() {
        let original = project_snapshot(&initial_records()).expect("valid projection");
        let result = original.apply_transaction(&[
            SceneDelta::Delete {
                id: node_entity_id(ROOT),
            },
            SceneDelta::Delete {
                id: node_entity_id(RIGHT),
            },
        ]);

        assert_eq!(result, Err(SceneError::UnknownEntity));
        assert!(original.get(node_entity_id(ROOT)).is_some());
    }
}
