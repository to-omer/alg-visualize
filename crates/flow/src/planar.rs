//! Validated combinatorial embeddings for planar-flow algorithms.

use std::collections::VecDeque;

use thiserror::Error;

use crate::model::{EdgeId, EdgeIndex, FlowNetwork, NodeIndex};
use crate::scenario::{FlowPlanarDartDirectionV1, FlowPlanarDartV1, FlowPlanarEmbeddingV1};

/// One oriented original edge in a combinatorial embedding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlanarDart {
    edge: EdgeIndex,
    direction: FlowPlanarDartDirectionV1,
}

impl PlanarDart {
    pub(crate) const fn new(edge: EdgeIndex, direction: FlowPlanarDartDirectionV1) -> Self {
        Self { edge, direction }
    }

    /// Returns the original edge.
    #[must_use]
    pub const fn edge(self) -> EdgeIndex {
        self.edge
    }

    /// Returns the orientation relative to the original edge.
    #[must_use]
    pub const fn direction(self) -> FlowPlanarDartDirectionV1 {
        self.direction
    }

    /// Returns the opposite dart of the same original edge.
    #[must_use]
    pub const fn reverse(self) -> Self {
        Self {
            edge: self.edge,
            direction: match self.direction {
                FlowPlanarDartDirectionV1::Forward => FlowPlanarDartDirectionV1::Reverse,
                FlowPlanarDartDirectionV1::Reverse => FlowPlanarDartDirectionV1::Forward,
            },
        }
    }

    pub(crate) fn ordinal(self) -> usize {
        self.edge.as_usize() * 2
            + usize::from(matches!(self.direction, FlowPlanarDartDirectionV1::Reverse))
    }

    pub(crate) fn tail(self, network: &FlowNetwork) -> Result<NodeIndex, PlanarEmbeddingError> {
        let edge = network
            .edge(self.edge)
            .ok_or(PlanarEmbeddingError::InvalidDart)?;
        Ok(match self.direction {
            FlowPlanarDartDirectionV1::Forward => edge.from(),
            FlowPlanarDartDirectionV1::Reverse => edge.to(),
        })
    }

    pub(crate) fn head(self, network: &FlowNetwork) -> Result<NodeIndex, PlanarEmbeddingError> {
        self.reverse().tail(network)
    }
}

/// One counterclockwise face boundary induced by the clockwise rotations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanarFace {
    darts: Box<[PlanarDart]>,
}

impl PlanarFace {
    /// Returns the face boundary as a cyclic dart sequence.
    #[must_use]
    pub const fn darts(&self) -> &[PlanarDart] {
        &self.darts
    }
}

/// Validated connected genus-zero rotation system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanarEmbedding {
    rotations: Box<[Box<[PlanarDart]>]>,
    faces: Box<[PlanarFace]>,
    left_face_by_dart: Box<[usize]>,
    outer_face: usize,
    terminal_corners: Option<(PlanarDart, PlanarDart)>,
}

impl PlanarEmbedding {
    /// Validates a complete clockwise rotation system for one connected graph.
    ///
    /// The face permutation is exactly `rotation ◦ reverse`, matching the
    /// combinatorial embedding used by Borradaile and Klein. Planarity is not
    /// inferred from drawing coordinates: every dart must occur once, the
    /// underlying graph must be connected, and Euler's formula must hold.
    ///
    /// # Errors
    ///
    /// Rejects incomplete or inconsistent rotations, non-planar rotation
    /// systems, invalid face anchors, and ambiguous st-planar corners.
    #[expect(
        clippy::too_many_lines,
        reason = "rotation, face, Euler, and terminal validation form one atomic admission check"
    )]
    pub fn new(
        network: &FlowNetwork,
        source: NodeIndex,
        sink: NodeIndex,
        declared: &FlowPlanarEmbeddingV1,
    ) -> Result<Self, PlanarEmbeddingError> {
        if network.nodes().is_empty() || network.edges().is_empty() {
            return Err(PlanarEmbeddingError::EmptyGraph);
        }
        if declared.rotations.len() != network.nodes().len() {
            return Err(PlanarEmbeddingError::RotationCount);
        }

        let dart_count = network
            .edges()
            .len()
            .checked_mul(2)
            .ok_or(PlanarEmbeddingError::ResourceLimit)?;
        let mut seen = vec![false; dart_count];
        let mut rotation_successor = vec![None; dart_count];
        let mut rotations = Vec::with_capacity(network.nodes().len());

        for (expected_node, declared_rotation) in network.node_indices().zip(&declared.rotations) {
            let expected_id = network
                .node(expected_node)
                .ok_or(PlanarEmbeddingError::RotationNodeOrder)?
                .id()
                .as_str();
            if declared_rotation.node_id != expected_id || declared_rotation.darts.is_empty() {
                return Err(PlanarEmbeddingError::RotationNodeOrder);
            }
            let mut rotation = Vec::with_capacity(declared_rotation.darts.len());
            for declared_dart in &declared_rotation.darts {
                let dart = resolve_dart(network, declared_dart)?;
                if dart.tail(network)? != expected_node {
                    return Err(PlanarEmbeddingError::WrongDartTail);
                }
                let ordinal = dart.ordinal();
                let slot = seen
                    .get_mut(ordinal)
                    .ok_or(PlanarEmbeddingError::InvalidDart)?;
                if *slot {
                    return Err(PlanarEmbeddingError::DuplicateDart);
                }
                *slot = true;
                rotation.push(dart);
            }
            for index in 0..rotation.len() {
                let dart = rotation[index];
                let successor = rotation[(index + 1) % rotation.len()];
                rotation_successor[dart.ordinal()] = Some(successor);
            }
            rotations.push(rotation.into_boxed_slice());
        }
        if seen.iter().any(|seen| !seen) {
            return Err(PlanarEmbeddingError::MissingDart);
        }
        validate_connected(network)?;

        let mut left_face_by_dart = vec![usize::MAX; dart_count];
        let mut faces = Vec::new();
        for edge in network.edge_indices() {
            for direction in [
                FlowPlanarDartDirectionV1::Forward,
                FlowPlanarDartDirectionV1::Reverse,
            ] {
                let start = PlanarDart { edge, direction };
                if left_face_by_dart[start.ordinal()] != usize::MAX {
                    continue;
                }
                let face_index = faces.len();
                let mut boundary = Vec::new();
                let mut current = start;
                loop {
                    let ordinal = current.ordinal();
                    if left_face_by_dart[ordinal] != usize::MAX {
                        if current != start {
                            return Err(PlanarEmbeddingError::InvalidFacePermutation);
                        }
                        break;
                    }
                    left_face_by_dart[ordinal] = face_index;
                    boundary.push(current);
                    current = rotation_successor[current.reverse().ordinal()]
                        .ok_or(PlanarEmbeddingError::InvalidFacePermutation)?;
                }
                faces.push(PlanarFace {
                    darts: boundary.into_boxed_slice(),
                });
            }
        }

        let euler_characteristic = i128::try_from(network.nodes().len())
            .ok()
            .and_then(|nodes| {
                i128::try_from(network.edges().len())
                    .ok()
                    .map(|edges| (nodes, edges))
            })
            .and_then(|(nodes, edges)| {
                i128::try_from(faces.len())
                    .ok()
                    .map(|face_count| nodes - edges + face_count)
            })
            .ok_or(PlanarEmbeddingError::ResourceLimit)?;
        if euler_characteristic != 2 {
            return Err(PlanarEmbeddingError::NonPlanarRotationSystem);
        }

        let outer_dart = resolve_dart(network, &declared.outer_face)?;
        let outer_face = *left_face_by_dart
            .get(outer_dart.ordinal())
            .ok_or(PlanarEmbeddingError::InvalidOuterFace)?;
        let terminal_corners = declared
            .terminal_corners
            .as_ref()
            .map(|corners| {
                let source_corner = resolve_dart(network, &corners.source)?;
                let sink_corner = resolve_dart(network, &corners.sink)?;
                if source_corner.tail(network)? != source || sink_corner.tail(network)? != sink {
                    return Err(PlanarEmbeddingError::InvalidTerminalCorner);
                }
                if left_face_by_dart[source_corner.ordinal()] != outer_face
                    || left_face_by_dart[sink_corner.ordinal()] != outer_face
                {
                    return Err(PlanarEmbeddingError::TerminalCornersNotOnOuterFace);
                }
                Ok((source_corner, sink_corner))
            })
            .transpose()?;

        Ok(Self {
            rotations: rotations.into_boxed_slice(),
            faces: faces.into_boxed_slice(),
            left_face_by_dart: left_face_by_dart.into_boxed_slice(),
            outer_face,
            terminal_corners,
        })
    }

    /// Returns clockwise rotations in canonical node-ID order.
    #[must_use]
    pub const fn rotations(&self) -> &[Box<[PlanarDart]>] {
        &self.rotations
    }

    /// Returns faces in canonical first-unseen-dart order.
    #[must_use]
    pub const fn faces(&self) -> &[PlanarFace] {
        &self.faces
    }

    /// Returns the designated outer-face index.
    #[must_use]
    pub const fn outer_face(&self) -> usize {
        self.outer_face
    }

    /// Returns the face to the left of a dart.
    #[must_use]
    pub fn left_face(&self, dart: PlanarDart) -> Option<usize> {
        self.left_face_by_dart.get(dart.ordinal()).copied()
    }

    /// Returns explicit source and sink corners for an st-planar construction.
    #[must_use]
    pub const fn terminal_corners(&self) -> Option<(PlanarDart, PlanarDart)> {
        self.terminal_corners
    }
}

fn resolve_dart(
    network: &FlowNetwork,
    declared: &FlowPlanarDartV1,
) -> Result<PlanarDart, PlanarEmbeddingError> {
    let edge_id =
        EdgeId::parse(&declared.edge_id).map_err(|_| PlanarEmbeddingError::InvalidDart)?;
    let edge = network
        .edge_index(&edge_id)
        .ok_or(PlanarEmbeddingError::InvalidDart)?;
    Ok(PlanarDart {
        edge,
        direction: declared.direction,
    })
}

fn validate_connected(network: &FlowNetwork) -> Result<(), PlanarEmbeddingError> {
    let start = network
        .node_indices()
        .next()
        .ok_or(PlanarEmbeddingError::EmptyGraph)?;
    let mut seen = vec![false; network.nodes().len()];
    seen[start.as_usize()] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        for edge_index in network
            .outgoing_edges(node)
            .iter()
            .chain(network.incoming_edges(node))
        {
            let edge = network
                .edge(*edge_index)
                .ok_or(PlanarEmbeddingError::InvalidDart)?;
            for neighbor in [edge.from(), edge.to()] {
                if !seen[neighbor.as_usize()] {
                    seen[neighbor.as_usize()] = true;
                    queue.push_back(neighbor);
                }
            }
        }
    }
    if seen.iter().all(|seen| *seen) {
        Ok(())
    } else {
        Err(PlanarEmbeddingError::DisconnectedGraph)
    }
}

/// Stable validation failures suitable for an invalid-embedding certificate.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlanarEmbeddingError {
    /// The graph has no edge or node orbit.
    #[error("planar embedding requires a nonempty graph")]
    EmptyGraph,
    /// The number of rotations differs from the number of nodes.
    #[error("planar embedding must contain one rotation per node")]
    RotationCount,
    /// Rotation nodes are missing, empty, duplicated, or out of canonical order.
    #[error("planar rotations must follow canonical node order and be nonempty")]
    RotationNodeOrder,
    /// A dart identity or direction does not resolve to an original edge.
    #[error("planar embedding contains an invalid dart")]
    InvalidDart,
    /// A rotation contains a dart whose tail is another node.
    #[error("planar rotation contains a dart with the wrong tail")]
    WrongDartTail,
    /// One oriented edge occurs more than once.
    #[error("planar embedding repeats a dart")]
    DuplicateDart,
    /// One oriented edge is absent from all rotations.
    #[error("planar embedding omits a dart")]
    MissingDart,
    /// The face permutation is not a disjoint cycle cover.
    #[error("planar face permutation is inconsistent")]
    InvalidFacePermutation,
    /// A rotation system for a disconnected graph does not determine component nesting.
    #[error("planar flow requires a connected embedded graph")]
    DisconnectedGraph,
    /// The connected rotation system has nonzero genus.
    #[error("rotation system fails the planar Euler characteristic")]
    NonPlanarRotationSystem,
    /// The outer-face dart is invalid.
    #[error("planar embedding has an invalid outer-face anchor")]
    InvalidOuterFace,
    /// An st-planar corner does not leave its declared terminal.
    #[error("st-planar terminal corner has the wrong tail")]
    InvalidTerminalCorner,
    /// Source and sink corners do not belong to the designated outer face.
    #[error("st-planar terminal corners must lie on the designated outer face")]
    TerminalCornersNotOnOuterFace,
    /// A checked allocation or index conversion exceeded the supported representation.
    #[error("planar embedding exceeds the supported resource band")]
    ResourceLimit,
}

impl PlanarEmbeddingError {
    /// Returns a stable machine-readable certificate code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptyGraph => "empty-graph",
            Self::RotationCount => "rotation-count",
            Self::RotationNodeOrder => "rotation-node-order",
            Self::InvalidDart => "invalid-dart",
            Self::WrongDartTail => "wrong-dart-tail",
            Self::DuplicateDart => "duplicate-dart",
            Self::MissingDart => "missing-dart",
            Self::InvalidFacePermutation => "invalid-face-permutation",
            Self::DisconnectedGraph => "disconnected-graph",
            Self::NonPlanarRotationSystem => "non-planar-euler-characteristic",
            Self::InvalidOuterFace => "invalid-outer-face",
            Self::InvalidTerminalCorner => "invalid-terminal-corner",
            Self::TerminalCornersNotOnOuterFace => "terminal-corners-not-on-outer-face",
            Self::ResourceLimit => "resource-limit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::scenario::{FlowPlanarRotationV1, FlowPlanarTerminalCornersV1};

    fn network(nodes: &[&str], edges: &[(&str, &str, &str)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|id| FlowNode::new(NodeId::parse(id).expect("node"), 0))
                .collect(),
            edges
                .iter()
                .map(|(id, from, to)| UnresolvedFlowEdge {
                    id: EdgeId::parse(id).expect("edge"),
                    from: NodeId::parse(from).expect("from"),
                    to: NodeId::parse(to).expect("to"),
                    lower: 0,
                    capacity: 1,
                    cost: 0,
                })
                .collect(),
        )
        .expect("network")
    }

    fn dart(edge_id: &str, direction: FlowPlanarDartDirectionV1) -> FlowPlanarDartV1 {
        FlowPlanarDartV1 {
            edge_id: edge_id.to_owned(),
            direction,
        }
    }

    fn triangle_embedding() -> FlowPlanarEmbeddingV1 {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        FlowPlanarEmbeddingV1 {
            rotations: vec![
                FlowPlanarRotationV1 {
                    node_id: "a".to_owned(),
                    darts: vec![dart("ab", F), dart("ac", F)],
                },
                FlowPlanarRotationV1 {
                    node_id: "b".to_owned(),
                    darts: vec![dart("ab", R), dart("bc", F)],
                },
                FlowPlanarRotationV1 {
                    node_id: "c".to_owned(),
                    darts: vec![dart("bc", R), dart("ac", R)],
                },
            ],
            outer_face: dart("ab", R),
            terminal_corners: Some(FlowPlanarTerminalCornersV1 {
                source: dart("ac", F),
                sink: dart("bc", R),
            }),
        }
    }

    #[test]
    fn validates_faces_outer_anchor_and_st_corners_without_coordinates() {
        let network = network(
            &["a", "b", "c"],
            &[("ab", "a", "b"), ("ac", "a", "c"), ("bc", "b", "c")],
        );
        let source = network.node_index(&NodeId::parse("a").unwrap()).unwrap();
        let sink = network.node_index(&NodeId::parse("c").unwrap()).unwrap();
        let embedding =
            PlanarEmbedding::new(&network, source, sink, &triangle_embedding()).unwrap();

        assert_eq!(embedding.faces().len(), 2);
        assert_eq!(embedding.faces()[embedding.outer_face()].darts().len(), 3);
        let (source_corner, sink_corner) = embedding.terminal_corners().unwrap();
        assert_eq!(
            embedding.left_face(source_corner),
            Some(embedding.outer_face())
        );
        assert_eq!(
            embedding.left_face(sink_corner),
            Some(embedding.outer_face())
        );
    }

    #[test]
    fn preserves_parallel_edges_and_both_self_loop_darts() {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let network = network(&["s", "t"], &[("e", "s", "t"), ("loop", "s", "s")]);
        let source = network.node_index(&NodeId::parse("s").unwrap()).unwrap();
        let sink = network.node_index(&NodeId::parse("t").unwrap()).unwrap();
        let declared = FlowPlanarEmbeddingV1 {
            rotations: vec![
                FlowPlanarRotationV1 {
                    node_id: "s".to_owned(),
                    darts: vec![dart("e", F), dart("loop", F), dart("loop", R)],
                },
                FlowPlanarRotationV1 {
                    node_id: "t".to_owned(),
                    darts: vec![dart("e", R)],
                },
            ],
            outer_face: dart("e", F),
            terminal_corners: None,
        };

        let embedding = PlanarEmbedding::new(&network, source, sink, &declared).unwrap();
        assert_eq!(embedding.faces().len(), 2);
        assert_eq!(embedding.rotations()[source.as_usize()].len(), 3);
    }

    #[test]
    fn rejects_duplicate_wrong_tail_disconnected_and_nonplanar_rotations() {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let triangle = network(
            &["a", "b", "c"],
            &[("ab", "a", "b"), ("ac", "a", "c"), ("bc", "b", "c")],
        );
        let source = triangle.node_index(&NodeId::parse("a").unwrap()).unwrap();
        let sink = triangle.node_index(&NodeId::parse("c").unwrap()).unwrap();

        let mut duplicate = triangle_embedding();
        duplicate.rotations[0].darts[1] = dart("ab", F);
        assert_eq!(
            PlanarEmbedding::new(&triangle, source, sink, &duplicate).unwrap_err(),
            PlanarEmbeddingError::DuplicateDart
        );

        let mut wrong_tail = triangle_embedding();
        wrong_tail.rotations[0].darts[0] = dart("ab", R);
        assert_eq!(
            PlanarEmbedding::new(&triangle, source, sink, &wrong_tail).unwrap_err(),
            PlanarEmbeddingError::WrongDartTail
        );

        let disconnected = network(&["a", "b", "c"], &[("ab", "a", "b"), ("cc", "c", "c")]);
        let disconnected_source = disconnected
            .node_index(&NodeId::parse("a").unwrap())
            .unwrap();
        let disconnected_sink = disconnected
            .node_index(&NodeId::parse("b").unwrap())
            .unwrap();
        let disconnected_embedding = FlowPlanarEmbeddingV1 {
            rotations: vec![
                FlowPlanarRotationV1 {
                    node_id: "a".to_owned(),
                    darts: vec![dart("ab", F)],
                },
                FlowPlanarRotationV1 {
                    node_id: "b".to_owned(),
                    darts: vec![dart("ab", R)],
                },
                FlowPlanarRotationV1 {
                    node_id: "c".to_owned(),
                    darts: vec![dart("cc", F), dart("cc", R)],
                },
            ],
            outer_face: dart("ab", F),
            terminal_corners: None,
        };
        assert_eq!(
            PlanarEmbedding::new(
                &disconnected,
                disconnected_source,
                disconnected_sink,
                &disconnected_embedding,
            )
            .unwrap_err(),
            PlanarEmbeddingError::DisconnectedGraph
        );

        let k33 = network(
            &["a0", "a1", "a2", "b0", "b1", "b2"],
            &[
                ("e00", "a0", "b0"),
                ("e01", "a0", "b1"),
                ("e02", "a0", "b2"),
                ("e10", "a1", "b0"),
                ("e11", "a1", "b1"),
                ("e12", "a1", "b2"),
                ("e20", "a2", "b0"),
                ("e21", "a2", "b1"),
                ("e22", "a2", "b2"),
            ],
        );
        let k33_source = k33.node_index(&NodeId::parse("a0").unwrap()).unwrap();
        let k33_sink = k33.node_index(&NodeId::parse("b0").unwrap()).unwrap();
        let rotations = ["a0", "a1", "a2"]
            .into_iter()
            .enumerate()
            .map(|(row, node_id)| FlowPlanarRotationV1 {
                node_id: node_id.to_owned(),
                darts: (0..3)
                    .map(|column| dart(&format!("e{row}{column}"), F))
                    .collect(),
            })
            .chain(
                ["b0", "b1", "b2"]
                    .into_iter()
                    .enumerate()
                    .map(|(column, node_id)| FlowPlanarRotationV1 {
                        node_id: node_id.to_owned(),
                        darts: (0..3)
                            .map(|row| dart(&format!("e{row}{column}"), R))
                            .collect(),
                    }),
            )
            .collect();
        let nonplanar = FlowPlanarEmbeddingV1 {
            rotations,
            outer_face: dart("e00", F),
            terminal_corners: None,
        };
        assert_eq!(
            PlanarEmbedding::new(&k33, k33_source, k33_sink, &nonplanar).unwrap_err(),
            PlanarEmbeddingError::NonPlanarRotationSystem
        );
    }

    #[test]
    fn rejects_terminal_corners_outside_the_designated_face() {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let network = network(
            &["a", "b", "c"],
            &[("ab", "a", "b"), ("ac", "a", "c"), ("bc", "b", "c")],
        );
        let source = network.node_index(&NodeId::parse("a").unwrap()).unwrap();
        let sink = network.node_index(&NodeId::parse("c").unwrap()).unwrap();
        let mut declared = triangle_embedding();
        declared.terminal_corners = Some(FlowPlanarTerminalCornersV1 {
            source: dart("ab", F),
            sink: dart("ac", R),
        });

        assert_eq!(
            PlanarEmbedding::new(&network, source, sink, &declared).unwrap_err(),
            PlanarEmbeddingError::TerminalCornersNotOnOuterFace
        );
    }
}
