//! Hassin's dual-shortest-path algorithm for embedded st-planar networks.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow};
use crate::model::{FlowNetwork, NodeIndex};
use crate::planar::{PlanarDart, PlanarEmbedding, PlanarEmbeddingError};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::{FlowPlanarDartDirectionV1, FlowPlanarEmbeddingV1, TraceGranularityV1};
use crate::trace::{
    FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics,
    FlowTraceRecorder, FlowTraceSnapshot,
};

/// Conservative node admission for the explicit dual implementation.
pub const HASSIN_MAX_NODES: usize = 256;
/// Conservative edge admission for the explicit dual implementation.
pub const HASSIN_MAX_EDGES: usize = 2_048;
/// Hard ceiling for dual-arc inspections.
pub const HASSIN_MAX_DUAL_ARC_SCANS: u128 = 1_000_000;
/// Preserve every small dual scan and geometric progress on larger inputs.
const HASSIN_TRACE_SCAN_PREFIX: u128 = 512;

/// Exact counters from the deterministic dual shortest-path kernel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HassinMetrics {
    /// Number of faces after splitting the designated outer face.
    pub dual_faces: u64,
    /// Dual Dijkstra runs. A successful invocation uses exactly one.
    pub dual_shortest_path_runs: u64,
    /// Dual arcs inspected by Dijkstra, including the artificial edge pair.
    pub dual_arc_scans: u128,
    /// Dual faces permanently settled by Dijkstra.
    pub settled_faces: u64,
    /// Original edges carrying positive reconstructed flow.
    pub positive_flow_edges: u64,
}

/// Certified st-planar maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HassinResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact deterministic kernel counters.
    pub metrics: HassinMetrics,
    /// Shortest distances on the split dual, in face-index order.
    pub dual_distances: Vec<u128>,
}

/// Certified Hassin result plus a reversible semantic trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HassinTraceResult {
    /// Same result as the fast profile.
    pub result: HassinResult,
    /// Zero-flow boundary before outer-face splitting.
    pub base_snapshot: FlowTraceSnapshot,
    /// Face-settlement, flow-reconstruction, and optimality events.
    pub events: Vec<FlowTraceEvent>,
    /// Verified final flow boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Hassin input, arithmetic, work, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HassinError {
    /// Input exceeds the bounded interactive implementation.
    #[error("graph exceeds Hassin st-planar admission limits")]
    AdmissionLimit,
    /// The source-specific positive-capacity st-planar preconditions fail.
    #[error(
        "Hassin st-planar requires positive capacities, zero lower bounds, no self-loops, and explicit terminal corners"
    )]
    InputRequirement,
    /// Explicit dual work exceeded its deterministic ceiling.
    #[error("Hassin st-planar dual work limit reached")]
    WorkLimit,
    /// A checked sum, difference, or metric conversion overflowed.
    #[error("Hassin st-planar arithmetic overflow")]
    ArithmeticOverflow,
    /// The outer-face corner split or dual reconstruction contradicted the embedding.
    #[error("Hassin st-planar dual invariant failed")]
    DualInvariant,
    /// The declared rotation system is invalid.
    #[error(transparent)]
    Embedding(#[from] PlanarEmbeddingError),
    /// A final residual state could not be reconstructed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// The independently checked maximum-flow certificate rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves a positive-capacity embedded st-planar maximum-flow instance.
///
/// # Errors
///
/// Rejects invalid embeddings, missing outer-face terminal corners,
/// nonpositive capacities, lower bounds, self-loops, bounded work overflow,
/// or a flow that fails the independent maximum-flow certificate.
pub fn solve_hassin_st_planar(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
) -> Result<HassinResult, HassinError> {
    solve_hassin_internal(graph, source, sink, declared, false).map(|run| run.result)
}

/// Solves Hassin's algorithm and records the split-dual semantic boundaries.
///
/// # Errors
///
/// Returns the same failures as [`solve_hassin_st_planar`] plus reversible
/// trace-construction failures.
pub fn trace_hassin_st_planar(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
) -> Result<HassinTraceResult, HassinError> {
    let run = solve_hassin_internal(graph, source, sink, declared, true)?;
    let (base_snapshot, events, final_snapshot) = run.trace.ok_or(HassinError::DualInvariant)?;
    Ok(HassinTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct HassinInternalRun {
    result: HassinResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone)]
struct DualArc {
    to: usize,
    length: u128,
    tie: usize,
    primal_arc: Option<ResidualArcId>,
}

#[derive(Clone)]
enum DualTraceCheckpoint {
    SettleFace {
        distance: u128,
        scans: u128,
        settled_faces: u64,
        via: Option<ResidualArcId>,
    },
    InspectArc {
        length: u128,
        scans: u128,
        settled_faces: u64,
        primal_arc: Option<ResidualArcId>,
    },
}

struct SplitDual {
    adjacency: Vec<Vec<DualArc>>,
    source_face: usize,
    sink_face: usize,
    left_face_by_dart: Vec<usize>,
}

fn solve_hassin_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
    record_trace: bool,
) -> Result<HassinInternalRun, HassinError> {
    if graph.nodes().len() > HASSIN_MAX_NODES || graph.edges().len() > HASSIN_MAX_EDGES {
        return Err(HassinError::AdmissionLimit);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.capacity() == 0 || edge.from() == edge.to())
    {
        return Err(HassinError::InputRequirement);
    }
    let embedding = PlanarEmbedding::new(graph, source, sink, declared)?;
    if embedding.terminal_corners().is_none() {
        return Err(HassinError::InputRequirement);
    }
    let dual = build_split_dual(graph, &embedding)?;
    let (distances, checkpoints, mut metrics) = dual_shortest_paths(&dual, record_trace)?;
    metrics.dual_faces =
        u64::try_from(dual.adjacency.len()).map_err(|_| HassinError::ArithmeticOverflow)?;

    let flows = graph
        .edge_indices()
        .map(|edge| {
            let left = *dual
                .left_face_by_dart
                .get(2 * edge.as_usize())
                .ok_or(HassinError::DualInvariant)?;
            let right = *dual
                .left_face_by_dart
                .get(2 * edge.as_usize() + 1)
                .ok_or(HassinError::DualInvariant)?;
            let left_distance = *distances.get(left).ok_or(HassinError::DualInvariant)?;
            let right_distance = *distances.get(right).ok_or(HassinError::DualInvariant)?;
            let flow = right_distance
                .checked_sub(left_distance)
                .ok_or(HassinError::DualInvariant)?;
            u64::try_from(flow).map_err(|_| HassinError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    metrics.positive_flow_edges = u64::try_from(flows.iter().filter(|&&flow| flow > 0).count())
        .map_err(|_| HassinError::ArithmeticOverflow)?;
    let certificate = check_max_flow(graph, source, sink, &flows)?;
    let dual_value = i128::try_from(
        *distances
            .get(dual.sink_face)
            .ok_or(HassinError::DualInvariant)?,
    )
    .map_err(|_| HassinError::ArithmeticOverflow)?;
    if dual.source_face == dual.sink_face || certificate.value != dual_value {
        return Err(HassinError::DualInvariant);
    }

    let trace = build_trace(
        graph,
        &flows,
        &checkpoints,
        metrics,
        &certificate,
        record_trace,
    )?;
    Ok(HassinInternalRun {
        result: HassinResult {
            flows,
            certificate,
            metrics,
            dual_distances: distances,
        },
        trace,
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "the outer-face split and every dual arc stay auditable in one construction"
)]
fn build_split_dual(
    graph: &FlowNetwork,
    embedding: &PlanarEmbedding,
) -> Result<SplitDual, HassinError> {
    let (source_corner, sink_corner) = embedding
        .terminal_corners()
        .ok_or(HassinError::InputRequirement)?;
    let boundary = embedding
        .faces()
        .get(embedding.outer_face())
        .ok_or(HassinError::DualInvariant)?
        .darts();
    let source_position = boundary
        .iter()
        .position(|&dart| dart == source_corner)
        .ok_or(HassinError::DualInvariant)?;
    let sink_position = boundary
        .iter()
        .position(|&dart| dart == sink_corner)
        .ok_or(HassinError::DualInvariant)?;
    if source_position == sink_position {
        return Err(HassinError::DualInvariant);
    }

    let dart_count = graph
        .edges()
        .len()
        .checked_mul(2)
        .ok_or(HassinError::ArithmeticOverflow)?;
    let mut first_outer_segment = vec![false; dart_count];
    let mut cursor = source_position;
    for _ in 0..boundary.len() {
        if cursor == sink_position {
            break;
        }
        let dart = *boundary.get(cursor).ok_or(HassinError::DualInvariant)?;
        first_outer_segment[dart.ordinal()] = true;
        cursor = (cursor + 1) % boundary.len();
    }
    if cursor != sink_position {
        return Err(HassinError::DualInvariant);
    }

    let second_outer_face = embedding.faces().len();
    let face_count = second_outer_face
        .checked_add(1)
        .ok_or(HassinError::ArithmeticOverflow)?;
    let mut left_face_by_dart = vec![usize::MAX; dart_count];
    for edge in graph.edge_indices() {
        for direction in [
            FlowPlanarDartDirectionV1::Forward,
            FlowPlanarDartDirectionV1::Reverse,
        ] {
            let dart = PlanarDart::new(edge, direction);
            let original_face = embedding
                .left_face(dart)
                .ok_or(HassinError::DualInvariant)?;
            left_face_by_dart[dart.ordinal()] =
                if original_face != embedding.outer_face() || first_outer_segment[dart.ordinal()] {
                    original_face
                } else {
                    second_outer_face
                };
        }
    }

    let mut adjacency = vec![Vec::new(); face_count];
    for edge_index in graph.edge_indices() {
        let edge = graph.edge(edge_index).ok_or(HassinError::DualInvariant)?;
        let forward = PlanarDart::new(edge_index, FlowPlanarDartDirectionV1::Forward);
        let reverse = forward.reverse();
        let left = left_face_by_dart[forward.ordinal()];
        let right = left_face_by_dart[reverse.ordinal()];
        adjacency[left].push(DualArc {
            to: right,
            length: u128::from(edge.capacity()),
            tie: forward.ordinal(),
            primal_arc: Some(ResidualArcId::new(
                edge.id().clone(),
                ResidualDirection::Forward,
            )),
        });
        adjacency[right].push(DualArc {
            to: left,
            length: 0,
            tie: reverse.ordinal(),
            primal_arc: Some(ResidualArcId::new(
                edge.id().clone(),
                ResidualDirection::Reverse,
            )),
        });
    }
    let infinite = graph.edges().iter().try_fold(1_u128, |sum, edge| {
        sum.checked_add(u128::from(edge.capacity()))
            .ok_or(HassinError::ArithmeticOverflow)
    })?;
    adjacency[embedding.outer_face()].push(DualArc {
        to: second_outer_face,
        length: infinite,
        tie: dart_count,
        primal_arc: None,
    });
    adjacency[second_outer_face].push(DualArc {
        to: embedding.outer_face(),
        length: 0,
        tie: dart_count
            .checked_add(1)
            .ok_or(HassinError::ArithmeticOverflow)?,
        primal_arc: None,
    });
    for arcs in &mut adjacency {
        arcs.sort_unstable_by(|left, right| {
            (left.to, left.length, left.tie).cmp(&(right.to, right.length, right.tie))
        });
    }
    Ok(SplitDual {
        adjacency,
        source_face: embedding.outer_face(),
        sink_face: second_outer_face,
        left_face_by_dart,
    })
}

fn dual_shortest_paths(
    dual: &SplitDual,
    record_trace: bool,
) -> Result<(Vec<u128>, Vec<DualTraceCheckpoint>, HassinMetrics), HassinError> {
    let mut distances = vec![u128::MAX; dual.adjacency.len()];
    let mut predecessor = vec![None; dual.adjacency.len()];
    let mut settled = vec![false; dual.adjacency.len()];
    let mut heap = BinaryHeap::new();
    let mut checkpoints = Vec::new();
    let mut metrics = HassinMetrics {
        dual_shortest_path_runs: 1,
        ..HassinMetrics::default()
    };
    distances[dual.source_face] = 0;
    heap.push(Reverse((0_u128, dual.source_face)));
    while let Some(Reverse((distance, face))) = heap.pop() {
        if settled[face] || distance != distances[face] {
            continue;
        }
        settled[face] = true;
        metrics.settled_faces = metrics
            .settled_faces
            .checked_add(1)
            .ok_or(HassinError::ArithmeticOverflow)?;
        if record_trace {
            checkpoints.push(DualTraceCheckpoint::SettleFace {
                distance,
                scans: metrics.dual_arc_scans,
                settled_faces: metrics.settled_faces,
                via: predecessor[face].clone(),
            });
        }
        for arc in dual.adjacency.get(face).ok_or(HassinError::DualInvariant)? {
            if metrics.dual_arc_scans >= HASSIN_MAX_DUAL_ARC_SCANS {
                return Err(HassinError::WorkLimit);
            }
            metrics.dual_arc_scans = metrics
                .dual_arc_scans
                .checked_add(1)
                .ok_or(HassinError::ArithmeticOverflow)?;
            if record_trace
                && (metrics.dual_arc_scans <= HASSIN_TRACE_SCAN_PREFIX
                    || metrics.dual_arc_scans.is_power_of_two())
            {
                checkpoints.push(DualTraceCheckpoint::InspectArc {
                    length: arc.length,
                    scans: metrics.dual_arc_scans,
                    settled_faces: metrics.settled_faces,
                    primal_arc: arc.primal_arc.clone(),
                });
            }
            let candidate = distance
                .checked_add(arc.length)
                .ok_or(HassinError::ArithmeticOverflow)?;
            if candidate < distances[arc.to] {
                distances[arc.to] = candidate;
                predecessor[arc.to].clone_from(&arc.primal_arc);
                heap.push(Reverse((candidate, arc.to)));
            }
        }
    }
    if distances.contains(&u128::MAX) {
        return Err(HassinError::DualInvariant);
    }
    Ok((distances, checkpoints, metrics))
}

#[expect(
    clippy::too_many_lines,
    reason = "the four pedagogical dual/primal event kinds remain adjacent and explicit"
)]
fn build_trace(
    graph: &FlowNetwork,
    flows: &[u64],
    checkpoints: &[DualTraceCheckpoint],
    metrics: HassinMetrics,
    certificate: &MaxFlowCertificate,
    record_trace: bool,
) -> Result<Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>, HassinError> {
    if !record_trace {
        return Ok(None);
    }
    let zero_state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
    let base = FlowTraceSnapshot::capture(
        graph,
        &zero_state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    let mut recorder = FlowTraceRecorder::new(graph, base)?;
    recorder.record_transition(
        FlowTraceEventMetadata {
            catalog_id: "hassin-st-planar.split-outer-face",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "hassin-st-planar:split-terminal-face",
        },
        &FlowTraceSnapshot::capture(
            graph,
            &zero_state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            trace_metrics(HassinMetrics {
                dual_faces: metrics.dual_faces,
                dual_shortest_path_runs: 1,
                ..HassinMetrics::default()
            }),
        ),
    )?;
    for checkpoint in checkpoints {
        let (metadata, active_path, checkpoint_metrics, detail) = match checkpoint {
            DualTraceCheckpoint::SettleFace {
                distance,
                scans,
                settled_faces,
                via,
            } => (
                FlowTraceEventMetadata {
                    catalog_id: "hassin-st-planar.settle-dual-face",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "hassin-st-planar:dijkstra-settle-face",
                },
                via.clone().into_iter().collect::<Vec<_>>(),
                HassinMetrics {
                    dual_faces: metrics.dual_faces,
                    dual_shortest_path_runs: 1,
                    dual_arc_scans: *scans,
                    settled_faces: *settled_faces,
                    positive_flow_edges: 0,
                },
                Some((
                    "dual-distance",
                    i128::try_from(*distance).map_err(|_| HassinError::ArithmeticOverflow)?,
                )),
            ),
            DualTraceCheckpoint::InspectArc {
                length,
                scans,
                settled_faces,
                primal_arc,
            } => (
                FlowTraceEventMetadata {
                    catalog_id: "hassin-st-planar.inspect-dual-arc",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "hassin-st-planar:inspect-outgoing-dual-arc",
                },
                primal_arc.clone().into_iter().collect::<Vec<_>>(),
                HassinMetrics {
                    dual_faces: metrics.dual_faces,
                    dual_shortest_path_runs: 1,
                    dual_arc_scans: *scans,
                    settled_faces: *settled_faces,
                    positive_flow_edges: 0,
                },
                Some((
                    "dual-length",
                    i128::try_from(*length).map_err(|_| HassinError::ArithmeticOverflow)?,
                )),
            ),
        };
        let focus = active_path
            .iter()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .collect();
        recorder.record_transition_with_detail_and_focus(
            metadata,
            &FlowTraceSnapshot::capture(
                graph,
                &zero_state,
                vec![None; graph.nodes().len()],
                Vec::new(),
                active_path,
                Vec::new(),
                trace_metrics(checkpoint_metrics),
            ),
            detail,
            focus,
        )?;
    }
    let final_state = ResidualState::from_flows(graph, flows)?;
    recorder.record_transition_with_detail(
        FlowTraceEventMetadata {
            catalog_id: "hassin-st-planar.reconstruct-primal-flow",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "hassin-st-planar:flow-is-right-distance-minus-left-distance",
        },
        &FlowTraceSnapshot::capture(
            graph,
            &final_state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            trace_metrics(metrics),
        ),
        Some((
            "positive-flow-edges",
            i128::from(metrics.positive_flow_edges),
        )),
    )?;
    recorder.record_transition_with_detail(
        FlowTraceEventMetadata {
            catalog_id: "hassin-st-planar.optimal-dual-cut",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "hassin-st-planar:tight-dual-path-is-minimum-cut",
        },
        &FlowTraceSnapshot::capture(
            graph,
            &final_state,
            vec![None; graph.nodes().len()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            trace_metrics(metrics),
        ),
        Some(("cut", certificate.cut_bound)),
    )?;
    Ok(Some(recorder.finish()))
}

const fn trace_metrics(metrics: HassinMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.dual_arc_scans,
        augmentations: 0,
        path_searches: metrics.dual_shortest_path_runs as u128,
        scaling_phases: metrics.dual_faces as u128,
        blocking_flow_phases: 0,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: metrics.positive_flow_edges as u128,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.settled_faces as u128,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::scenario::{FlowPlanarDartV1, FlowPlanarRotationV1, FlowPlanarTerminalCornersV1};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn dart(edge_id: &str, direction: FlowPlanarDartDirectionV1) -> FlowPlanarDartV1 {
        FlowPlanarDartV1 {
            edge_id: edge_id.to_owned(),
            direction,
        }
    }

    fn triangle() -> (FlowNetwork, NodeIndex, NodeIndex, FlowPlanarEmbeddingV1) {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let graph = FlowNetwork::new(
            ["a", "b", "c"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).unwrap(), 0))
                .collect(),
            [
                ("ab", "a", "b", 5),
                ("ac", "a", "c", 2),
                ("bc", "b", "c", 3),
            ]
            .into_iter()
            .map(|(id, from, to, capacity)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).unwrap(),
                from: NodeId::parse(from).unwrap(),
                to: NodeId::parse(to).unwrap(),
                lower: 0,
                capacity,
                cost: 0,
            })
            .collect(),
        )
        .unwrap();
        let source = graph.node_index(&NodeId::parse("a").unwrap()).unwrap();
        let sink = graph.node_index(&NodeId::parse("c").unwrap()).unwrap();
        let embedding = FlowPlanarEmbeddingV1 {
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
        };
        (graph, source, sink, embedding)
    }

    #[test]
    fn reconstructs_triangle_flow_from_split_dual_distances() {
        let (graph, source, sink, embedding) = triangle();
        let result = solve_hassin_st_planar(&graph, source, sink, &embedding).unwrap();
        assert_eq!(result.flows, [3, 2, 3]);
        assert_eq!(result.certificate.value, 5);
        assert_eq!(result.certificate.cut_bound, 5);
        assert_eq!(result.metrics.dual_shortest_path_runs, 1);
        assert_eq!(result.metrics.dual_faces, 3);
    }

    #[test]
    fn trace_is_bidirectionally_replayable_and_matches_fast_result() {
        let (graph, source, sink, embedding) = triangle();
        let fast = solve_hassin_st_planar(&graph, source, sink, &embedding).unwrap();
        let traced = trace_hassin_st_planar(&graph, source, sink, &embedding).unwrap();
        assert_eq!(traced.result, fast);
        assert!(
            traced
                .events
                .iter()
                .any(|event| { event.catalog_id == "hassin-st-planar.reconstruct-primal-flow" })
        );
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward).unwrap();
        }
        assert_eq!(replay, traced.final_snapshot);
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse).unwrap();
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn rejects_missing_corners_and_zero_capacity() {
        let (graph, source, sink, mut embedding) = triangle();
        embedding.terminal_corners = None;
        assert_eq!(
            solve_hassin_st_planar(&graph, source, sink, &embedding).unwrap_err(),
            HassinError::InputRequirement
        );

        let zero = FlowNetwork::new(
            ["s", "t"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).unwrap(), 0))
                .collect(),
            vec![UnresolvedFlowEdge {
                id: EdgeId::parse("st").unwrap(),
                from: NodeId::parse("s").unwrap(),
                to: NodeId::parse("t").unwrap(),
                lower: 0,
                capacity: 0,
                cost: 0,
            }],
        )
        .unwrap();
        let s = zero.node_index(&NodeId::parse("s").unwrap()).unwrap();
        let t = zero.node_index(&NodeId::parse("t").unwrap()).unwrap();
        assert_eq!(
            solve_hassin_st_planar(&zero, s, t, &embedding).unwrap_err(),
            HassinError::InputRequirement
        );
    }

    #[test]
    fn agrees_with_edmonds_karp_on_the_triangle() {
        let (graph, source, sink, embedding) = triangle();
        let actual = solve_hassin_st_planar(&graph, source, sink, &embedding).unwrap();
        let expected = solve_edmonds_karp(&graph, source, sink).unwrap();
        assert_eq!(actual.certificate.value, expected.certificate.value);
        assert_eq!(actual.certificate.cut_bound, expected.certificate.cut_bound);
    }

    #[test]
    fn rejects_the_first_graph_outside_the_split_dual_admission_band() {
        let graph = FlowNetwork::new(
            (0..=HASSIN_MAX_NODES)
                .map(|index| FlowNode::new(NodeId::parse(&format!("v{index:03}")).unwrap(), 0))
                .collect(),
            Vec::<UnresolvedFlowEdge>::new(),
        )
        .unwrap();
        let source = graph.node_indices().next().unwrap();
        let sink = graph.node_indices().last().unwrap();
        let embedding = FlowPlanarEmbeddingV1 {
            rotations: Vec::new(),
            outer_face: dart("unused", FlowPlanarDartDirectionV1::Forward),
            terminal_corners: None,
        };
        assert_eq!(
            solve_hassin_st_planar(&graph, source, sink, &embedding),
            Err(HassinError::AdmissionLimit)
        );
    }
}
