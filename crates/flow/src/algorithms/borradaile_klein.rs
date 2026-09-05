//! Bounded explicit-tree Borradaile–Klein planar maximum flow.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{CertificateError, MaxFlowCertificate, check_max_flow, divergences};
use crate::model::{FlowNetwork, NodeIndex};
use crate::planar::{PlanarDart, PlanarEmbedding, PlanarEmbeddingError};
use crate::residual::{ResidualArcId, ResidualDirection, ResidualError, ResidualState};
use crate::scenario::{FlowPlanarDartDirectionV1, FlowPlanarEmbeddingV1, TraceGranularityV1};
use crate::trace::{
    FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata, FlowTraceMetrics, FlowTraceRecorder,
    FlowTraceSnapshot, residual_arc_entity_refs,
};

/// Conservative node admission for rebuilding the right-first tree explicitly.
pub const BORRADAILE_KLEIN_MAX_NODES: usize = 256;
/// Conservative edge admission for the explicit primal/dual implementation.
pub const BORRADAILE_KLEIN_MAX_EDGES: usize = 2_048;
/// Hard ceiling for dual and rotation-dart inspections.
pub const BORRADAILE_KLEIN_MAX_DART_SCANS: u128 = 2_000_000;
/// Maximum eager event count retained by the interactive trace profile.
pub const BORRADAILE_KLEIN_MAX_TRACE_EVENTS: usize = 100_000;

/// Exact counters from preprocessing and repeated right-first searches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BorradaileKleinMetrics {
    /// Number of faces in the validated embedding.
    pub dual_faces: u64,
    /// Dual shortest-path preprocessing runs. A successful run uses one.
    pub preprocessing_runs: u64,
    /// Dual darts inspected during clockwise-cycle removal.
    pub dual_arc_scans: u128,
    /// Right-first reverse searches, including the final failed search.
    pub right_first_searches: u64,
    /// Clockwise rotation darts inspected by those searches.
    pub rotation_dart_scans: u128,
    /// Leftmost residual paths saturated.
    pub augmentations: u64,
    /// Path darts made nonresidual by saturation, counted per augmentation.
    pub saturated_path_darts: u64,
    /// Vertices discovered across all rebuilt right-first trees.
    pub discovered_vertices: u128,
}

/// Certified planar maximum-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorradaileKleinResult {
    /// Original-edge flows after preprocessing and all leftmost augmentations.
    pub flows: Vec<u64>,
    /// The circulation obtained from unsplit-dual shortest distances.
    pub preprocessing_flows: Vec<u64>,
    /// Independently reconstructed maximum-flow/minimum-cut certificate.
    pub certificate: MaxFlowCertificate,
    /// Exact bounded-kernel counters.
    pub metrics: BorradaileKleinMetrics,
    /// Unsplit-dual distances from the designated infinite face.
    pub dual_distances: Vec<u128>,
}

/// Certified result plus reversible preprocessing/search/augmentation boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorradaileKleinTraceResult {
    /// Same result as the fast profile.
    pub result: BorradaileKleinResult,
    /// Zero-flow boundary before clockwise-cycle preprocessing.
    pub base_snapshot: FlowTraceSnapshot,
    /// Reversible semantic events.
    pub events: Vec<FlowTraceEvent>,
    /// Verified final maximum-flow boundary.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Input, bounded work, arithmetic, or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BorradaileKleinError {
    /// Input exceeds the explicit educational implementation's admission band.
    #[error("graph exceeds Borradaile-Klein explicit-tree admission limits")]
    AdmissionLimit,
    /// Source-specific assumptions from the paper are not materialized.
    #[error(
        "Borradaile-Klein requires positive capacities, zero lower bounds/supplies, no self-loops, and a sink-incident infinite face"
    )]
    InputRequirement,
    /// Explicit dual/tree work exceeded its deterministic ceiling.
    #[error("Borradaile-Klein explicit-tree work limit reached")]
    WorkLimit,
    /// A checked conversion or operation counter overflowed.
    #[error("Borradaile-Klein arithmetic overflow")]
    ArithmeticOverflow,
    /// Dual preprocessing or right-first tree reconstruction contradicted the embedding.
    #[error("Borradaile-Klein planar invariant failed")]
    PlanarInvariant,
    /// The declared rotation system is invalid.
    #[error(transparent)]
    Embedding(#[from] PlanarEmbeddingError),
    /// Residual reconstruction or augmentation failed.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Independent maximum-flow verification rejected the result.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Reversible trace construction failed.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Runs the bounded explicit-tree variant.
///
/// This implementation follows the paper's abstract algorithm but deliberately
/// rebuilds the right-first tree after each augmentation. It therefore does not
/// claim the paper's dynamic-tree `O(n log n)` bound.
///
/// # Errors
///
/// Rejects invalid embeddings, a designated infinite face not incident to the
/// sink, nonpositive capacities, lower bounds/supplies/self-loops, bounded work
/// overflow, or a result rejected by the independent certificate checker.
pub fn solve_borradaile_klein_planar(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
) -> Result<BorradaileKleinResult, BorradaileKleinError> {
    solve_internal(graph, source, sink, declared, false).map(|run| run.result)
}

/// Runs the same kernel and records reversible semantic boundaries.
///
/// # Errors
///
/// Returns the same failures as [`solve_borradaile_klein_planar`] plus trace
/// construction failures.
pub fn trace_borradaile_klein_planar(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
) -> Result<BorradaileKleinTraceResult, BorradaileKleinError> {
    let run = solve_internal(graph, source, sink, declared, true)?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(BorradaileKleinError::PlanarInvariant)?;
    Ok(BorradaileKleinTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct InternalRun {
    result: BorradaileKleinResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

#[derive(Clone)]
struct DualArc {
    to: usize,
    length: u128,
    tie: usize,
    dart: PlanarDart,
}

struct RightFirstSearch {
    path: Option<Vec<ResidualArcId>>,
    order: Vec<NodeIndex>,
}

#[derive(Clone, Copy)]
struct PlanarTerminals {
    source: NodeIndex,
    sink: NodeIndex,
}

#[expect(
    clippy::too_many_lines,
    reason = "the complete bounded primal/dual phase order is kept visible in one kernel"
)]
fn solve_internal(
    graph: &FlowNetwork,
    source: NodeIndex,
    sink: NodeIndex,
    declared: &FlowPlanarEmbeddingV1,
    record_trace: bool,
) -> Result<InternalRun, BorradaileKleinError> {
    if graph.nodes().len() > BORRADAILE_KLEIN_MAX_NODES
        || graph.edges().len() > BORRADAILE_KLEIN_MAX_EDGES
    {
        return Err(BorradaileKleinError::AdmissionLimit);
    }
    if graph.nodes().iter().any(|node| node.supply() != 0)
        || graph
            .edges()
            .iter()
            .any(|edge| edge.lower() != 0 || edge.capacity() == 0 || edge.from() == edge.to())
    {
        return Err(BorradaileKleinError::InputRequirement);
    }
    let embedding = PlanarEmbedding::new(graph, source, sink, declared)?;
    let root_corner = sink_outer_corner(graph, sink, &embedding)?;
    let zero_state = ResidualState::from_flows(graph, &vec![0; graph.edges().len()])?;
    let base_snapshot = snapshot(
        graph,
        &zero_state,
        Vec::new(),
        Vec::new(),
        FlowTraceMetrics::default(),
    );
    let mut recorder = record_trace
        .then(|| FlowTraceRecorder::new(graph, base_snapshot.clone()))
        .transpose()?;

    let (dual_distances, preprocessing_flows, mut metrics) =
        clockwise_acyclic_preprocessing(graph, &embedding, &zero_state, &mut recorder)?;
    let preprocessing_divergence = divergences(graph, &preprocessing_flows)?;
    if preprocessing_divergence.iter().any(|&value| value != 0) {
        return Err(BorradaileKleinError::PlanarInvariant);
    }
    let mut state = ResidualState::from_flows(graph, &preprocessing_flows)?;
    record(
        &mut recorder,
        graph,
        &state,
        FlowTraceEventMetadata {
            catalog_id: "borradaile-klein-planar.preprocess-clockwise-cycles",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "borradaile-klein:dual-shortest-circulation",
        },
        &snapshot(
            graph,
            &state,
            Vec::new(),
            Vec::new(),
            trace_metrics(metrics),
        ),
        Some(("dual-faces", i128::from(metrics.dual_faces))),
    )?;

    let augmentation_ceiling = graph
        .edges()
        .len()
        .checked_mul(3)
        .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
    loop {
        metrics.right_first_searches = metrics
            .right_first_searches
            .checked_add(1)
            .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
        let search = right_first_search(
            graph,
            &state,
            &embedding,
            root_corner,
            PlanarTerminals { source, sink },
            &mut metrics,
            &mut recorder,
        )?;
        let Some(path) = search.path else {
            record(
                &mut recorder,
                graph,
                &state,
                FlowTraceEventMetadata {
                    catalog_id: "borradaile-klein-planar.no-residual-path",
                    minimum_granularity: TraceGranularityV1::Phase,
                    pseudocode_line: "borradaile-klein:right-first-search-fails",
                },
                &snapshot(
                    graph,
                    &state,
                    search.order,
                    Vec::new(),
                    trace_metrics(metrics),
                ),
                None,
            )?;
            break;
        };
        let path_length =
            i128::try_from(path.len()).map_err(|_| BorradaileKleinError::ArithmeticOverflow)?;
        let path_order = residual_path_nodes(&state, &path)?;
        record(
            &mut recorder,
            graph,
            &state,
            FlowTraceEventMetadata {
                catalog_id: "borradaile-klein-planar.right-first-leftmost-path",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "borradaile-klein:rebuild-right-first-tree",
            },
            &snapshot(
                graph,
                &state,
                path_order,
                path.clone(),
                trace_metrics(metrics),
            ),
            Some(("path-darts", path_length)),
        )?;

        let capacities = path
            .iter()
            .map(|arc| {
                state
                    .arc(arc)
                    .map(|value| value.capacity)
                    .ok_or(BorradaileKleinError::PlanarInvariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let amount = capacities
            .iter()
            .copied()
            .min()
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        let saturated = u64::try_from(
            capacities
                .iter()
                .filter(|&&capacity| capacity == amount)
                .count(),
        )
        .map_err(|_| BorradaileKleinError::ArithmeticOverflow)?;
        state.augment(&path, amount)?;
        metrics.augmentations = metrics
            .augmentations
            .checked_add(1)
            .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
        metrics.saturated_path_darts = metrics
            .saturated_path_darts
            .checked_add(saturated)
            .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
        if usize::try_from(metrics.augmentations).map_or(true, |value| value > augmentation_ceiling)
        {
            return Err(BorradaileKleinError::PlanarInvariant);
        }
        record(
            &mut recorder,
            graph,
            &state,
            FlowTraceEventMetadata {
                catalog_id: "borradaile-klein-planar.saturate-leftmost-path",
                minimum_granularity: TraceGranularityV1::Operation,
                pseudocode_line: "borradaile-klein:saturate-leftmost-residual-path",
            },
            &snapshot(graph, &state, Vec::new(), path, trace_metrics(metrics)),
            Some(("delta", i128::from(amount))),
        )?;
    }

    let certificate = check_max_flow(graph, source, sink, state.flows())?;
    record(
        &mut recorder,
        graph,
        &state,
        FlowTraceEventMetadata {
            catalog_id: "borradaile-klein-planar.optimal-cut",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "borradaile-klein:no-residual-path-is-maximum",
        },
        &snapshot(
            graph,
            &state,
            Vec::new(),
            Vec::new(),
            trace_metrics(metrics),
        ),
        Some(("cut", certificate.cut_bound)),
    )?;
    let trace = recorder.map(FlowTraceRecorder::finish);
    Ok(InternalRun {
        result: BorradaileKleinResult {
            flows: state.flows().to_vec(),
            preprocessing_flows,
            certificate,
            metrics,
            dual_distances,
        },
        trace,
    })
}

fn clockwise_acyclic_preprocessing(
    graph: &FlowNetwork,
    embedding: &PlanarEmbedding,
    state: &ResidualState<'_>,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(Vec<u128>, Vec<u64>, BorradaileKleinMetrics), BorradaileKleinError> {
    let adjacency = dual_adjacency(graph, embedding)?;
    let mut distances = vec![u128::MAX; adjacency.len()];
    let mut settled = vec![false; adjacency.len()];
    let mut heap = BinaryHeap::new();
    let mut metrics = BorradaileKleinMetrics {
        dual_faces: u64::try_from(adjacency.len())
            .map_err(|_| BorradaileKleinError::ArithmeticOverflow)?,
        preprocessing_runs: 1,
        ..BorradaileKleinMetrics::default()
    };
    distances[embedding.outer_face()] = 0;
    heap.push(Reverse((0_u128, embedding.outer_face())));
    while let Some(Reverse((distance, face))) = heap.pop() {
        if settled[face] || distance != distances[face] {
            continue;
        }
        settled[face] = true;
        for arc in adjacency
            .get(face)
            .ok_or(BorradaileKleinError::PlanarInvariant)?
        {
            add_scan(&mut metrics.dual_arc_scans, 1)?;
            let candidate = distance
                .checked_add(arc.length)
                .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
            let improved = candidate < distances[arc.to];
            if improved {
                distances[arc.to] = candidate;
                heap.push(Reverse((candidate, arc.to)));
            }
            let arc_id = residual_id(graph, arc.dart)?;
            let residual_arc = state
                .arc(&arc_id)
                .ok_or(BorradaileKleinError::PlanarInvariant)?;
            record(
                recorder,
                graph,
                state,
                FlowTraceEventMetadata {
                    catalog_id: "borradaile-klein-planar.inspect-dual-arc",
                    minimum_granularity: TraceGranularityV1::Micro,
                    pseudocode_line: "borradaile-klein:inspect-one-dual-dart",
                },
                &snapshot(
                    graph,
                    state,
                    vec![residual_arc.from, residual_arc.to],
                    vec![arc_id],
                    trace_metrics(metrics),
                ),
                Some(("improved", i128::from(improved))),
            )?;
        }
    }
    if distances.contains(&u128::MAX) {
        return Err(BorradaileKleinError::PlanarInvariant);
    }
    let flows = primal_flows_from_dual_distances(graph, embedding, &distances)?;
    Ok((distances, flows, metrics))
}

fn dual_adjacency(
    graph: &FlowNetwork,
    embedding: &PlanarEmbedding,
) -> Result<Vec<Vec<DualArc>>, BorradaileKleinError> {
    let mut adjacency = vec![Vec::new(); embedding.faces().len()];
    for edge_index in graph.edge_indices() {
        let edge = graph
            .edge(edge_index)
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        let forward = PlanarDart::new(edge_index, FlowPlanarDartDirectionV1::Forward);
        let reverse = forward.reverse();
        let left = embedding
            .left_face(forward)
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        let right = embedding
            .left_face(reverse)
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        adjacency[left].push(DualArc {
            to: right,
            length: u128::from(edge.capacity()),
            tie: forward.ordinal(),
            dart: forward,
        });
        adjacency[right].push(DualArc {
            to: left,
            length: 0,
            tie: reverse.ordinal(),
            dart: reverse,
        });
    }
    for arcs in &mut adjacency {
        arcs.sort_unstable_by_key(|arc| (arc.to, arc.length, arc.tie));
    }
    Ok(adjacency)
}

fn primal_flows_from_dual_distances(
    graph: &FlowNetwork,
    embedding: &PlanarEmbedding,
    distances: &[u128],
) -> Result<Vec<u64>, BorradaileKleinError> {
    graph
        .edge_indices()
        .map(|edge_index| {
            let forward = PlanarDart::new(edge_index, FlowPlanarDartDirectionV1::Forward);
            let left = embedding
                .left_face(forward)
                .ok_or(BorradaileKleinError::PlanarInvariant)?;
            let right = embedding
                .left_face(forward.reverse())
                .ok_or(BorradaileKleinError::PlanarInvariant)?;
            let flow = distances[right]
                .checked_sub(distances[left])
                .ok_or(BorradaileKleinError::PlanarInvariant)?;
            let flow = u64::try_from(flow).map_err(|_| BorradaileKleinError::ArithmeticOverflow)?;
            let capacity = graph
                .edge(edge_index)
                .ok_or(BorradaileKleinError::PlanarInvariant)?
                .capacity();
            if flow > capacity {
                return Err(BorradaileKleinError::PlanarInvariant);
            }
            Ok(flow)
        })
        .collect()
}

fn sink_outer_corner(
    graph: &FlowNetwork,
    sink: NodeIndex,
    embedding: &PlanarEmbedding,
) -> Result<PlanarDart, BorradaileKleinError> {
    embedding
        .rotations()
        .get(sink.as_usize())
        .ok_or(BorradaileKleinError::PlanarInvariant)?
        .iter()
        .copied()
        .find(|&dart| {
            dart.tail(graph) == Ok(sink)
                && embedding.left_face(dart) == Some(embedding.outer_face())
        })
        .ok_or(BorradaileKleinError::InputRequirement)
}

fn right_first_search(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    embedding: &PlanarEmbedding,
    root_corner: PlanarDart,
    terminals: PlanarTerminals,
    metrics: &mut BorradaileKleinMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<RightFirstSearch, BorradaileKleinError> {
    let mut visited = vec![false; graph.nodes().len()];
    let mut parent = vec![None; graph.nodes().len()];
    let mut order = vec![terminals.sink];
    visited[terminals.sink.as_usize()] = true;
    let found = right_first_visit(
        graph,
        state,
        embedding,
        terminals.sink,
        root_corner,
        terminals.source,
        &mut visited,
        &mut parent,
        &mut order,
        metrics,
        recorder,
    )?;
    metrics.discovered_vertices = metrics
        .discovered_vertices
        .checked_add(
            u128::try_from(order.len()).map_err(|_| BorradaileKleinError::ArithmeticOverflow)?,
        )
        .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
    if !found {
        return Ok(RightFirstSearch { path: None, order });
    }
    let mut path = Vec::new();
    let mut current = terminals.source;
    for _ in 0..graph.nodes().len() {
        if current == terminals.sink {
            return Ok(RightFirstSearch {
                path: Some(path),
                order,
            });
        }
        let arc_id = parent[current.as_usize()]
            .clone()
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        let arc = state
            .arc(&arc_id)
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        path.push(arc_id);
        current = arc.to;
    }
    Err(BorradaileKleinError::PlanarInvariant)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the recursive right-first search keeps its deterministic state explicit"
)]
fn right_first_visit(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    embedding: &PlanarEmbedding,
    node: NodeIndex,
    start: PlanarDart,
    source: NodeIndex,
    visited: &mut [bool],
    parent: &mut [Option<ResidualArcId>],
    order: &mut Vec<NodeIndex>,
    metrics: &mut BorradaileKleinMetrics,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<bool, BorradaileKleinError> {
    let rotation = embedding
        .rotations()
        .get(node.as_usize())
        .ok_or(BorradaileKleinError::PlanarInvariant)?;
    let start_index = rotation
        .iter()
        .position(|&dart| dart == start)
        .ok_or(BorradaileKleinError::PlanarInvariant)?;
    for offset in 1..=rotation.len() {
        add_rotation_scan(metrics)?;
        let leaving = rotation[(start_index + offset) % rotation.len()];
        let incoming = leaving.reverse();
        let id = residual_id(graph, incoming)?;
        let arc = state
            .arc(&id)
            .ok_or(BorradaileKleinError::PlanarInvariant)?;
        let usable = arc.capacity > 0 && !visited[arc.from.as_usize()];
        record(
            recorder,
            graph,
            state,
            FlowTraceEventMetadata {
                catalog_id: "borradaile-klein-planar.inspect-right-first-dart",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "borradaile-klein:inspect-one-clockwise-dart",
            },
            &snapshot(
                graph,
                state,
                vec![arc.from, arc.to],
                vec![id.clone()],
                trace_metrics(*metrics),
            ),
            Some(("usable", i128::from(usable))),
        )?;
        if !usable {
            continue;
        }
        if incoming.tail(graph)? != arc.from || incoming.head(graph)? != node {
            return Err(BorradaileKleinError::PlanarInvariant);
        }
        visited[arc.from.as_usize()] = true;
        parent[arc.from.as_usize()] = Some(id);
        order.push(arc.from);
        if arc.from == source
            || right_first_visit(
                graph, state, embedding, arc.from, incoming, source, visited, parent, order,
                metrics, recorder,
            )?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn residual_id(
    graph: &FlowNetwork,
    dart: PlanarDart,
) -> Result<ResidualArcId, BorradaileKleinError> {
    let edge = graph
        .edge(dart.edge())
        .ok_or(BorradaileKleinError::PlanarInvariant)?;
    Ok(ResidualArcId::new(
        edge.id().clone(),
        match dart.direction() {
            FlowPlanarDartDirectionV1::Forward => ResidualDirection::Forward,
            FlowPlanarDartDirectionV1::Reverse => ResidualDirection::Reverse,
        },
    ))
}

fn add_scan(counter: &mut u128, amount: u128) -> Result<(), BorradaileKleinError> {
    *counter = counter
        .checked_add(amount)
        .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
    if *counter > BORRADAILE_KLEIN_MAX_DART_SCANS {
        return Err(BorradaileKleinError::WorkLimit);
    }
    Ok(())
}

fn add_rotation_scan(metrics: &mut BorradaileKleinMetrics) -> Result<(), BorradaileKleinError> {
    metrics.rotation_dart_scans = metrics
        .rotation_dart_scans
        .checked_add(1)
        .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
    let total = metrics
        .dual_arc_scans
        .checked_add(metrics.rotation_dart_scans)
        .ok_or(BorradaileKleinError::ArithmeticOverflow)?;
    if total > BORRADAILE_KLEIN_MAX_DART_SCANS {
        return Err(BorradaileKleinError::WorkLimit);
    }
    Ok(())
}

fn snapshot(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    search_order: Vec<NodeIndex>,
    active_path: Vec<ResidualArcId>,
    metrics: FlowTraceMetrics,
) -> FlowTraceSnapshot {
    FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        search_order,
        active_path,
        Vec::new(),
        metrics,
    )
}

fn residual_path_nodes(
    state: &ResidualState<'_>,
    path: &[ResidualArcId],
) -> Result<Vec<NodeIndex>, BorradaileKleinError> {
    let Some(first) = path.first() else {
        return Ok(Vec::new());
    };
    let first_arc = state
        .arc(first)
        .ok_or(BorradaileKleinError::PlanarInvariant)?;
    let mut nodes = vec![first_arc.from, first_arc.to];
    for id in &path[1..] {
        let arc = state.arc(id).ok_or(BorradaileKleinError::PlanarInvariant)?;
        if nodes.last().copied() != Some(arc.from) {
            return Err(BorradaileKleinError::PlanarInvariant);
        }
        nodes.push(arc.to);
    }
    Ok(nodes)
}

fn record(
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metadata: FlowTraceEventMetadata,
    snapshot: &FlowTraceSnapshot,
    detail: Option<(&'static str, i128)>,
) -> Result<(), BorradaileKleinError> {
    if let Some(recorder) = recorder {
        if recorder.event_count() >= BORRADAILE_KLEIN_MAX_TRACE_EVENTS {
            return Err(BorradaileKleinError::WorkLimit);
        }
        recorder.record_transition_with_detail_and_focus(
            metadata,
            snapshot,
            detail,
            residual_arc_entity_refs(graph, state, &snapshot.active_path)?,
        )?;
    }
    Ok(())
}

const fn trace_metrics(metrics: BorradaileKleinMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: metrics.right_first_searches as u128,
        relaxation_passes: metrics.preprocessing_runs as u128,
        residual_arc_scans: metrics.dual_arc_scans + metrics.rotation_dart_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.right_first_searches as u128,
        scaling_phases: metrics.dual_faces as u128,
        blocking_flow_phases: metrics.preprocessing_runs as u128,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: metrics.saturated_path_darts as u128,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.discovered_vertices,
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::solve_edmonds_karp;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::scenario::{FlowPlanarDartV1, FlowPlanarRotationV1, FlowPlanarTerminalCornersV1};
    use crate::trace::{FlowTraceDirection, FlowTraceEntityRef, apply_trace_event};

    use super::*;

    fn dart(edge_id: &str, direction: FlowPlanarDartDirectionV1) -> FlowPlanarDartV1 {
        FlowPlanarDartV1 {
            edge_id: edge_id.to_owned(),
            direction,
        }
    }

    fn triangle(
        clockwise_cycle: bool,
    ) -> (FlowNetwork, NodeIndex, NodeIndex, FlowPlanarEmbeddingV1) {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let edge_data = if clockwise_cycle {
            vec![
                ("ab", "a", "b", 3),
                ("bc", "b", "c", 3),
                ("ca", "c", "a", 3),
            ]
        } else {
            vec![
                ("ab", "a", "b", 5),
                ("ac", "a", "c", 2),
                ("bc", "b", "c", 3),
            ]
        };
        let graph = FlowNetwork::new(
            ["a", "b", "c"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).unwrap(), 0))
                .collect(),
            edge_data
                .iter()
                .map(|&(id, from, to, capacity)| UnresolvedFlowEdge {
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
        let embedding = if clockwise_cycle {
            FlowPlanarEmbeddingV1 {
                rotations: vec![
                    FlowPlanarRotationV1 {
                        node_id: "a".to_owned(),
                        darts: vec![dart("ab", F), dart("ca", R)],
                    },
                    FlowPlanarRotationV1 {
                        node_id: "b".to_owned(),
                        darts: vec![dart("ab", R), dart("bc", F)],
                    },
                    FlowPlanarRotationV1 {
                        node_id: "c".to_owned(),
                        darts: vec![dart("bc", R), dart("ca", F)],
                    },
                ],
                outer_face: dart("ab", F),
                terminal_corners: None,
            }
        } else {
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
        };
        (graph, source, sink, embedding)
    }

    #[test]
    fn right_first_rebuild_saturates_the_two_leftmost_triangle_paths() {
        let (graph, source, sink, embedding) = triangle(false);
        let result = solve_borradaile_klein_planar(&graph, source, sink, &embedding).unwrap();
        assert_eq!(result.flows, [3, 2, 3]);
        assert_eq!(result.preprocessing_flows, [0, 0, 0]);
        assert_eq!(result.certificate.value, 5);
        assert_eq!(result.metrics.preprocessing_runs, 1);
        assert_eq!(result.metrics.augmentations, 2);
        assert_eq!(result.metrics.right_first_searches, 3);
    }

    #[test]
    fn dual_preprocessing_saturates_a_clockwise_input_cycle() {
        let (graph, source, sink, embedding) = triangle(true);
        let result = solve_borradaile_klein_planar(&graph, source, sink, &embedding).unwrap();
        assert_eq!(result.preprocessing_flows, [3, 3, 3]);
        assert_eq!(result.certificate.value, 3);
        assert_eq!(result.certificate.cut_bound, 3);
        assert_eq!(
            result.certificate.value,
            solve_edmonds_karp(&graph, source, sink)
                .unwrap()
                .certificate
                .value
        );
    }

    #[test]
    fn trace_replays_both_directions_and_matches_fast_result() {
        let (graph, source, sink, embedding) = triangle(false);
        let fast = solve_borradaile_klein_planar(&graph, source, sink, &embedding).unwrap();
        let trace = trace_borradaile_klein_planar(&graph, source, sink, &embedding).unwrap();
        assert_eq!(trace.result, fast);
        let mut replay = trace.base_snapshot.clone();
        for event in &trace.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward).unwrap();
        }
        assert_eq!(replay, trace.final_snapshot);
        for event in trace.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse).unwrap();
        }
        assert_eq!(replay, trace.base_snapshot);
        let semantic_ids = trace
            .events
            .iter()
            .filter(|event| event.minimum_granularity != TraceGranularityV1::Micro)
            .map(|event| event.catalog_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            semantic_ids,
            [
                "borradaile-klein-planar.preprocess-clockwise-cycles",
                "borradaile-klein-planar.right-first-leftmost-path",
                "borradaile-klein-planar.saturate-leftmost-path",
                "borradaile-klein-planar.right-first-leftmost-path",
                "borradaile-klein-planar.saturate-leftmost-path",
                "borradaile-klein-planar.no-residual-path",
                "borradaile-klein-planar.optimal-cut",
            ]
        );
        let primitive_events = trace
            .events
            .iter()
            .filter(|event| event.minimum_granularity == TraceGranularityV1::Micro)
            .collect::<Vec<_>>();
        assert_eq!(
            u128::try_from(primitive_events.len()).unwrap(),
            trace.result.metrics.dual_arc_scans + trace.result.metrics.rotation_dart_scans
        );
        assert!(primitive_events.iter().all(|event| {
            event
                .entity_refs
                .iter()
                .filter(|entity| matches!(entity, FlowTraceEntityRef::Node(_)))
                .count()
                == 2
                && event
                    .entity_refs
                    .iter()
                    .any(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
        }));
    }

    #[test]
    fn rejects_an_infinite_face_not_incident_to_the_sink() {
        use FlowPlanarDartDirectionV1::{Forward as F, Reverse as R};
        let graph = FlowNetwork::new(
            ["a", "b", "c", "d"]
                .into_iter()
                .map(|id| FlowNode::new(NodeId::parse(id).unwrap(), 0))
                .collect(),
            [
                ("ab", "a", "b"),
                ("bc", "b", "c"),
                ("cd", "c", "d"),
                ("ad", "a", "d"),
                ("ac", "a", "c"),
            ]
            .into_iter()
            .map(|(id, from, to)| UnresolvedFlowEdge {
                id: EdgeId::parse(id).unwrap(),
                from: NodeId::parse(from).unwrap(),
                to: NodeId::parse(to).unwrap(),
                lower: 0,
                capacity: 1,
                cost: 0,
            })
            .collect(),
        )
        .unwrap();
        let source = graph.node_index(&NodeId::parse("a").unwrap()).unwrap();
        let sink = graph.node_index(&NodeId::parse("d").unwrap()).unwrap();
        let embedding = FlowPlanarEmbeddingV1 {
            rotations: vec![
                FlowPlanarRotationV1 {
                    node_id: "a".to_owned(),
                    darts: vec![dart("ab", F), dart("ac", F), dart("ad", F)],
                },
                FlowPlanarRotationV1 {
                    node_id: "b".to_owned(),
                    darts: vec![dart("ab", R), dart("bc", F)],
                },
                FlowPlanarRotationV1 {
                    node_id: "c".to_owned(),
                    darts: vec![dart("bc", R), dart("cd", F), dart("ac", R)],
                },
                FlowPlanarRotationV1 {
                    node_id: "d".to_owned(),
                    darts: vec![dart("cd", R), dart("ad", R)],
                },
            ],
            outer_face: dart("ab", R),
            terminal_corners: None,
        };
        assert_eq!(
            solve_borradaile_klein_planar(&graph, source, sink, &embedding),
            Err(BorradaileKleinError::InputRequirement)
        );
    }

    #[test]
    fn rejects_the_first_graph_outside_the_explicit_tree_admission_band() {
        let graph = FlowNetwork::new(
            (0..=BORRADAILE_KLEIN_MAX_NODES)
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
            solve_borradaile_klein_planar(&graph, source, sink, &embedding),
            Err(BorradaileKleinError::AdmissionLimit)
        );
    }
}
