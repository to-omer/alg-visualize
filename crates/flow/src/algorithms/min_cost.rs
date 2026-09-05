//! Exact successive-shortest-path solvers.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use thiserror::Error;

use crate::certificate::{
    CertificateError, MinCostFlowCertificate, check_min_cost_flow,
    check_residual_min_cost_optimality, divergences,
};
use crate::feasibility::{FeasibilityError, FeasibilityExecution, FeasibilityUse};
use crate::model::{FlowEdge, FlowNetwork, NodeIndex};
use crate::residual::{ResidualArcId, ResidualError, ResidualState};
use crate::scenario::TraceGranularityV1;
use crate::trace::{
    FlowTraceDirection, FlowTraceEntityRef, FlowTraceError, FlowTraceEvent, FlowTraceEventMetadata,
    FlowTraceMetrics, FlowTraceRecorder, FlowTraceSnapshot, apply_trace_event,
    residual_arc_entity_refs, residual_path_node_order,
};

/// Conservative interactive admission limit for Bellman–Ford SSP.
pub const BELLMAN_FORD_SSP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge admission limit for Bellman–Ford SSP.
pub const BELLMAN_FORD_SSP_MAX_EDGES: usize = 20_000;
/// Deterministic safety guard against pseudo-polynomial augmentation counts.
pub const BELLMAN_FORD_SSP_MAX_AUGMENTATIONS: u64 = 1_000_000;

/// Conservative interactive admission limit for potential + Dijkstra SSP.
pub const POTENTIAL_DIJKSTRA_SSP_MAX_NODES: usize = 2_000;
/// Conservative interactive edge admission limit for potential + Dijkstra SSP.
pub const POTENTIAL_DIJKSTRA_SSP_MAX_EDGES: usize = 20_000;
/// Deterministic safety guard against pseudo-polynomial augmentation counts.
pub const POTENTIAL_DIJKSTRA_SSP_MAX_AUGMENTATIONS: u64 = 1_000_000;

/// Exact deterministic counters from Bellman–Ford SSP.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BellmanFordSspMetrics {
    /// Successful residual path augmentations.
    pub augmentations: u64,
    /// Bellman–Ford outer relaxation passes.
    pub relaxation_passes: u64,
    /// Positive residual arcs inspected during relaxation.
    pub residual_arc_scans: u128,
}

/// Certified canonical minimum-cost-flow result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BellmanFordSspResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: BellmanFordSspMetrics,
}

/// Certified Bellman–Ford SSP result with reversible search/augmentation events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BellmanFordSspTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: BellmanFordSspResult,
    /// Replay boundary at the lower-bound pseudoflow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after verified minimum-cost optimality.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Canonical result of Jewell's successive-shortest-path method.
///
/// Bellman–Ford is the deterministic shortest-path subroutine selected by this
/// bounded implementation; it is not a substitute flow algorithm.
pub type SuccessiveShortestPathResult = BellmanFordSspResult;

/// Reversible result of Jewell's successive-shortest-path method.
pub type SuccessiveShortestPathTraceResult = BellmanFordSspTraceResult;

/// Construction or verification failure for successive shortest path.
pub type SuccessiveShortestPathError = BellmanFordSspError;

/// Independent trace-contract failure for successive shortest path.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SuccessiveShortestPathTraceCheckError {
    /// A reversible event failed its before/after transaction contract.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
    /// An augmentation boundary was not minimum-cost for its current balance.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// A trace event did not belong to the successive-shortest-path vocabulary.
    #[error("successive-shortest-path trace contains an unexpected event")]
    UnexpectedEvent,
    /// Forward or reverse replay did not reach the declared boundary.
    #[error("successive-shortest-path trace boundary mismatch")]
    BoundaryMismatch,
}

/// Bellman–Ford SSP construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BellmanFordSspError {
    /// Input exceeds the practical admission band.
    #[error("graph exceeds Bellman-Ford SSP admission limits")]
    AdmissionLimit,
    /// The pseudo-polynomial augmentation guard was reached.
    #[error("Bellman-Ford SSP augmentation limit reached")]
    AugmentationLimit,
    /// A feasibility precheck proved that the requested balances are impossible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Initial compatibility or final independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("Bellman-Ford SSP arithmetic overflow")]
    ArithmeticOverflow,
    /// A feasible precheck and the SSP residual search disagreed.
    #[error("Bellman-Ford SSP could not route a feasible remaining imbalance")]
    MissingPath,
    /// A shortest-path predecessor chain was inconsistent.
    #[error("Bellman-Ford SSP predecessor invariant failed")]
    PredecessorInvariant,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves an exact balanced min-cost flow from the lower-bound pseudoflow.
///
/// A feasibility precheck is used only for rejection; its arbitrary feasible
/// flow is discarded. SSP starts from lower bounds, requires no negative cycle
/// in any residual component, and routes each canonical surplus along a current
/// shortest residual path to a deficit.
///
/// # Errors
///
/// Rejects admission, feasibility, negative-cycle compatibility, arithmetic,
/// path, residual mutation, augmentation-limit, or final certificate failure.
pub fn solve_bellman_ford_ssp(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<BellmanFordSspResult, BellmanFordSspError> {
    solve_bellman_ford_ssp_internal(
        graph,
        required_divergence,
        false,
        SspTraceIdentity::BellmanFord,
    )
    .map(|run| run.result)
}

/// Solves Bellman--Ford SSP while reporting its feasibility precheck to the
/// enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_bellman_ford_ssp_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<BellmanFordSspResult, BellmanFordSspError> {
    solve_bellman_ford_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        SspTraceIdentity::BellmanFord,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves Jewell's successive-shortest-path method using a deterministic
/// Bellman–Ford shortest-path subroutine.
///
/// # Errors
///
/// Returns [`SuccessiveShortestPathError`] when admission, initialization,
/// augmentation, exact arithmetic, or the independent optimum checker fails.
pub fn solve_successive_shortest_path(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<SuccessiveShortestPathResult, SuccessiveShortestPathError> {
    solve_bellman_ford_ssp_internal(
        graph,
        required_divergence,
        false,
        SspTraceIdentity::SuccessiveShortestPath,
    )
    .map(|run| run.result)
}

/// Solves Jewell SSP while reporting its feasibility precheck to the enclosing
/// execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_successive_shortest_path_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<SuccessiveShortestPathResult, SuccessiveShortestPathError> {
    solve_bellman_ford_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        SspTraceIdentity::SuccessiveShortestPath,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves Bellman–Ford SSP while recording every shortest-path phase and
/// residual augmentation.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace transaction
/// invariant failures.
pub fn trace_bellman_ford_ssp(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<BellmanFordSspTraceResult, BellmanFordSspError> {
    let run = solve_bellman_ford_ssp_internal(
        graph,
        required_divergence,
        true,
        SspTraceIdentity::BellmanFord,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(BellmanFordSspError::PredecessorInvariant)?;
    Ok(BellmanFordSspTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Runs successive shortest path with descriptor-specific reversible events.
///
/// # Errors
///
/// Returns the same failures as [`solve_successive_shortest_path`] plus trace
/// transaction invariant failures.
pub fn trace_successive_shortest_path(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<SuccessiveShortestPathTraceResult, SuccessiveShortestPathError> {
    let run = solve_bellman_ford_ssp_internal(
        graph,
        required_divergence,
        true,
        SspTraceIdentity::SuccessiveShortestPath,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(BellmanFordSspError::PredecessorInvariant)?;
    Ok(BellmanFordSspTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces Bellman--Ford SSP while explicitly publishing its feasibility
/// precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_bellman_ford_ssp_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<BellmanFordSspTraceResult, BellmanFordSspError> {
    trace_bellman_ford_variant_with_feasibility(
        graph,
        required_divergence,
        SspTraceIdentity::BellmanFord,
        feasibility,
    )
}

/// Traces Jewell SSP while explicitly publishing its feasibility precheck to
/// the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_successive_shortest_path_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<SuccessiveShortestPathTraceResult, SuccessiveShortestPathError> {
    trace_bellman_ford_variant_with_feasibility(
        graph,
        required_divergence,
        SspTraceIdentity::SuccessiveShortestPath,
        feasibility,
    )
}

fn trace_bellman_ford_variant_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    trace_identity: SspTraceIdentity,
    feasibility: &mut FeasibilityExecution,
) -> Result<BellmanFordSspTraceResult, BellmanFordSspError> {
    let run = solve_bellman_ford_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        trace_identity,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) =
        run.trace.ok_or(BellmanFordSspError::PredecessorInvariant)?;
    Ok(BellmanFordSspTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Verifies descriptor identity, reversible replay, and Jewell's partial-flow
/// optimality invariant independently of the construction routine.
///
/// # Errors
///
/// Rejects unexpected event identities, invalid patches, a nonoptimal
/// augmentation prefix, or a mismatched final/reverse boundary.
pub fn check_successive_shortest_path_trace(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    run: &SuccessiveShortestPathTraceResult,
) -> Result<(), SuccessiveShortestPathTraceCheckError> {
    let mut replay = run.base_snapshot.clone();
    for event in &run.events {
        let is_search = matches!(
            event.catalog_id.as_str(),
            "successive-shortest-path.select-source"
                | "successive-shortest-path.inspect-residual-arc"
                | "successive-shortest-path.relax"
                | "successive-shortest-path.shortest-path"
                | "successive-shortest-path.reconstruct-path"
                | "successive-shortest-path.bottleneck"
        );
        let is_augment = event.catalog_id == "successive-shortest-path.augment";
        let is_optimal = event.catalog_id == "successive-shortest-path.optimal";
        if !(is_search || is_augment || is_optimal) {
            return Err(SuccessiveShortestPathTraceCheckError::UnexpectedEvent);
        }
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Forward)?;
        if is_augment || is_optimal {
            let current_divergence = divergences(graph, &replay.flows)?;
            check_min_cost_flow(graph, &current_divergence, &replay.flows)?;
        }
    }
    if replay != run.final_snapshot
        || replay.flows != run.result.flows
        || divergences(graph, &replay.flows)? != required_divergence
    {
        return Err(SuccessiveShortestPathTraceCheckError::BoundaryMismatch);
    }
    for event in run.events.iter().rev() {
        apply_trace_event(graph, &mut replay, event, FlowTraceDirection::Reverse)?;
    }
    if replay != run.base_snapshot {
        return Err(SuccessiveShortestPathTraceCheckError::BoundaryMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum SspTraceIdentity {
    BellmanFord,
    SuccessiveShortestPath,
}

impl SspTraceIdentity {
    fn event_id(self, suffix: &'static str) -> &'static str {
        match (self, suffix) {
            (Self::BellmanFord, "shortest-path") => "bellman-ford-ssp.shortest-path",
            (Self::BellmanFord, "select-source") => "bellman-ford-ssp.select-source",
            (Self::BellmanFord, "inspect-residual-arc") => "bellman-ford-ssp.inspect-residual-arc",
            (Self::BellmanFord, "relax") => "bellman-ford-ssp.relax",
            (Self::BellmanFord, "reconstruct-path") => "bellman-ford-ssp.reconstruct-path",
            (Self::BellmanFord, "bottleneck") => "bellman-ford-ssp.bottleneck",
            (Self::BellmanFord, "augment") => "bellman-ford-ssp.augment",
            (Self::BellmanFord, "optimal") => "bellman-ford-ssp.optimal",
            (Self::SuccessiveShortestPath, "shortest-path") => {
                "successive-shortest-path.shortest-path"
            }
            (Self::SuccessiveShortestPath, "select-source") => {
                "successive-shortest-path.select-source"
            }
            (Self::SuccessiveShortestPath, "inspect-residual-arc") => {
                "successive-shortest-path.inspect-residual-arc"
            }
            (Self::SuccessiveShortestPath, "relax") => "successive-shortest-path.relax",
            (Self::SuccessiveShortestPath, "reconstruct-path") => {
                "successive-shortest-path.reconstruct-path"
            }
            (Self::SuccessiveShortestPath, "bottleneck") => "successive-shortest-path.bottleneck",
            (Self::SuccessiveShortestPath, "augment") => "successive-shortest-path.augment",
            (Self::SuccessiveShortestPath, "optimal") => "successive-shortest-path.optimal",
            _ => "successive-shortest-path.invalid-event",
        }
    }

    fn pseudocode_line(self, suffix: &'static str) -> &'static str {
        match (self, suffix) {
            (Self::BellmanFord, "shortest-path") => "bellman-ford-ssp:relax-residual-arcs",
            (Self::BellmanFord, "select-source") => "bellman-ford-ssp:select-surplus-source",
            (Self::BellmanFord, "inspect-residual-arc") => {
                "bellman-ford-ssp:inspect-one-residual-arc"
            }
            (Self::BellmanFord, "relax") => "bellman-ford-ssp:relax-one-residual-arc",
            (Self::BellmanFord, "reconstruct-path") => "bellman-ford-ssp:follow-predecessor-edge",
            (Self::BellmanFord, "bottleneck") => "bellman-ford-ssp:measure-bottleneck",
            (Self::BellmanFord, "augment") => "bellman-ford-ssp:augment-shortest-path",
            (Self::BellmanFord, "optimal") => "bellman-ford-ssp:return-minimum-cost-flow",
            (Self::SuccessiveShortestPath, "shortest-path") => {
                "successive-shortest-path:find-minimum-cost-residual-path"
            }
            (Self::SuccessiveShortestPath, "select-source") => {
                "successive-shortest-path:select-surplus-source"
            }
            (Self::SuccessiveShortestPath, "inspect-residual-arc") => {
                "successive-shortest-path:inspect-one-residual-arc"
            }
            (Self::SuccessiveShortestPath, "relax") => {
                "successive-shortest-path:relax-one-residual-arc"
            }
            (Self::SuccessiveShortestPath, "reconstruct-path") => {
                "successive-shortest-path:follow-predecessor-edge"
            }
            (Self::SuccessiveShortestPath, "bottleneck") => {
                "successive-shortest-path:measure-bottleneck"
            }
            (Self::SuccessiveShortestPath, "augment") => {
                "successive-shortest-path:augment-bottleneck"
            }
            (Self::SuccessiveShortestPath, "optimal") => {
                "successive-shortest-path:return-minimum-cost-flow"
            }
            _ => "successive-shortest-path:invalid-event",
        }
    }
}

struct BellmanFordSspInternalRun {
    result: BellmanFordSspResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

fn solve_bellman_ford_ssp_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    trace_identity: SspTraceIdentity,
) -> Result<BellmanFordSspInternalRun, BellmanFordSspError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_bellman_ford_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        record_trace,
        trace_identity,
        &mut feasibility,
    )
}

fn solve_bellman_ford_ssp_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    trace_identity: SspTraceIdentity,
    feasibility: &mut FeasibilityExecution,
) -> Result<BellmanFordSspInternalRun, BellmanFordSspError> {
    validate_admission(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    check_residual_min_cost_optimality(graph, &lower_flows)?;
    let current_divergence = divergences(graph, &lower_flows)?;
    if current_divergence.len() != required_divergence.len() {
        return Err(BellmanFordSspError::MissingPath);
    }
    let mut remaining = required_divergence
        .iter()
        .zip(current_divergence)
        .map(|(&required, current)| {
            required
                .checked_sub(current)
                .ok_or(BellmanFordSspError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut state = ResidualState::from_flows(graph, &lower_flows)?;
    let mut metrics = BellmanFordSspMetrics::default();
    let mut recorder = start_trace_recorder(graph, &state, &remaining, metrics, record_trace)?;

    while let Some(source) = graph
        .node_indices()
        .find(|node| remaining[node.as_usize()] > 0)
    {
        if metrics.augmentations >= BELLMAN_FORD_SSP_MAX_AUGMENTATIONS {
            return Err(BellmanFordSspError::AugmentationLimit);
        }
        let search = shortest_path_to_deficit(&state, source, &remaining, &mut metrics)?;
        record_ssp_search_trace(
            &mut recorder,
            graph,
            &state,
            &remaining,
            (metrics, trace_identity),
            source,
            &search,
        )?;
        trace_and_augment_ssp_path(
            &mut recorder,
            graph,
            &mut state,
            &mut remaining,
            (&mut metrics, trace_identity),
            source,
            &search,
        )?;
    }
    if remaining.iter().any(|&value| value != 0) {
        return Err(BellmanFordSspError::MissingPath);
    }
    let flows = state.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_ssp_trace(
        recorder.as_mut(),
        graph,
        &state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: trace_identity.event_id("optimal"),
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: trace_identity.pseudocode_line("optimal"),
        },
        BellmanFordTraceView::empty(graph, remaining),
        None,
    )?;
    let result = BellmanFordSspResult {
        flows,
        certificate,
        metrics,
    };
    Ok(BellmanFordSspInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn record_ssp_search_trace(
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    remaining: &[i128],
    trace: (BellmanFordSspMetrics, SspTraceIdentity),
    source: NodeIndex,
    search: &BellmanFordSearch,
) -> Result<(), BellmanFordSspError> {
    let (metrics, trace_identity) = trace;
    record_ssp_trace(
        recorder.as_mut(),
        graph,
        state,
        search.metrics_before,
        FlowTraceEventMetadata {
            catalog_id: trace_identity.event_id("select-source"),
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: trace_identity.pseudocode_line("select-source"),
        },
        BellmanFordTraceView::search_prefix(search, 1, remaining.to_vec(), Vec::new()),
        Some(("source", source.as_usize() as i128)),
    )?;
    for checkpoint in &search.checkpoints {
        let (action, active_arc, detail) = match &checkpoint.kind {
            BellmanFordSearchCheckpointKind::Inspect { active_arc } => (
                "inspect-residual-arc",
                active_arc,
                (
                    "residual-arc scans",
                    i128::try_from(checkpoint.metrics.residual_arc_scans)
                        .map_err(|_| BellmanFordSspError::ArithmeticOverflow)?,
                ),
            ),
            BellmanFordSearchCheckpointKind::Relax {
                active_arc,
                relaxed,
            } => ("relax", active_arc, ("relaxed", relaxed.as_usize() as i128)),
        };
        record_ssp_trace(
            recorder.as_mut(),
            graph,
            state,
            checkpoint.metrics,
            FlowTraceEventMetadata {
                catalog_id: trace_identity.event_id(action),
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: trace_identity.pseudocode_line(action),
            },
            BellmanFordTraceView {
                distances: checkpoint.distances.clone(),
                search_order: checkpoint.search_order.clone(),
                path: vec![active_arc.clone()],
                remaining: remaining.to_vec(),
            },
            Some(detail),
        )?;
    }
    record_ssp_trace(
        recorder.as_mut(),
        graph,
        state,
        metrics,
        FlowTraceEventMetadata {
            catalog_id: trace_identity.event_id("shortest-path"),
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: trace_identity.pseudocode_line("shortest-path"),
        },
        BellmanFordTraceView::search_prefix(
            search,
            search.search_order.len(),
            remaining.to_vec(),
            Vec::new(),
        ),
        Some(("reachable", search.search_order.len() as i128)),
    )?;
    Ok(())
}

fn trace_and_augment_ssp_path(
    recorder: &mut Option<FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &mut ResidualState<'_>,
    remaining: &mut [i128],
    trace: (&mut BellmanFordSspMetrics, SspTraceIdentity),
    source: NodeIndex,
    search: &BellmanFordSearch,
) -> Result<(), BellmanFordSspError> {
    let (metrics, trace_identity) = trace;
    let sink = search.sink.ok_or(BellmanFordSspError::MissingPath)?;
    let path = search
        .path
        .as_ref()
        .ok_or(BellmanFordSspError::MissingPath)?;
    for prefix_length in 1..=path.len() {
        record_ssp_trace(
            recorder.as_mut(),
            graph,
            state,
            *metrics,
            FlowTraceEventMetadata {
                catalog_id: trace_identity.event_id("reconstruct-path"),
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: trace_identity.pseudocode_line("reconstruct-path"),
            },
            BellmanFordTraceView {
                distances: search.distances.clone(),
                search_order: search.search_order.clone(),
                path: path[..prefix_length].to_vec(),
                remaining: remaining.to_vec(),
            },
            Some(("path-edges", prefix_length as i128)),
        )?;
    }
    let amount = augmentation_amount_to_deficit(state, path, source, sink, remaining)?;
    record_ssp_trace(
        recorder.as_mut(),
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: trace_identity.event_id("bottleneck"),
            minimum_granularity: TraceGranularityV1::Micro,
            pseudocode_line: trace_identity.pseudocode_line("bottleneck"),
        },
        BellmanFordTraceView {
            distances: search.distances.clone(),
            search_order: search.search_order.clone(),
            path: path.clone(),
            remaining: remaining.to_vec(),
        },
        Some(("bottleneck", i128::from(amount))),
    )?;
    augment_to_deficit(state, path, source, sink, amount, remaining, metrics)?;
    record_ssp_trace(
        recorder.as_mut(),
        graph,
        state,
        *metrics,
        FlowTraceEventMetadata {
            catalog_id: trace_identity.event_id("augment"),
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: trace_identity.pseudocode_line("augment"),
        },
        BellmanFordTraceView {
            distances: search.distances.clone(),
            search_order: search.search_order.clone(),
            path: path.clone(),
            remaining: remaining.to_vec(),
        },
        Some(("amount", i128::from(amount))),
    )?;
    Ok(())
}

fn validate_admission(graph: &FlowNetwork) -> Result<(), BellmanFordSspError> {
    if graph.nodes().len() > BELLMAN_FORD_SSP_MAX_NODES
        || graph.edges().len() > BELLMAN_FORD_SSP_MAX_EDGES
    {
        return Err(BellmanFordSspError::AdmissionLimit);
    }
    Ok(())
}

fn augment_to_deficit(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    source: NodeIndex,
    sink: NodeIndex,
    amount: u64,
    remaining: &mut [i128],
    metrics: &mut BellmanFordSspMetrics,
) -> Result<(), BellmanFordSspError> {
    state.augment(path, amount)?;
    remaining[source.as_usize()] = remaining[source.as_usize()]
        .checked_sub(i128::from(amount))
        .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
    remaining[sink.as_usize()] = remaining[sink.as_usize()]
        .checked_add(i128::from(amount))
        .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
    Ok(())
}

fn augmentation_amount_to_deficit(
    state: &ResidualState<'_>,
    path: &[ResidualArcId],
    source: NodeIndex,
    sink: NodeIndex,
    remaining: &[i128],
) -> Result<u64, BellmanFordSspError> {
    let bottleneck = path
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(BellmanFordSspError::PredecessorInvariant)?;
    let source_remaining = u128::try_from(remaining[source.as_usize()])
        .map_err(|_| BellmanFordSspError::ArithmeticOverflow)?;
    let sink_remaining = remaining[sink.as_usize()]
        .checked_neg()
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
    let amount = u64::try_from(
        u128::from(bottleneck)
            .min(source_remaining)
            .min(sink_remaining),
    )
    .map_err(|_| BellmanFordSspError::ArithmeticOverflow)?;
    if amount == 0 {
        return Err(BellmanFordSspError::MissingPath);
    }
    Ok(amount)
}

fn start_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    remaining: &[i128],
    metrics: BellmanFordSspMetrics,
    record_trace: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_trace {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        remaining.to_vec(),
        trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct BellmanFordTraceView {
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
    remaining: Vec<i128>,
}

impl BellmanFordTraceView {
    fn empty(graph: &FlowNetwork, remaining: Vec<i128>) -> Self {
        Self {
            distances: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
            remaining,
        }
    }

    fn search_prefix(
        search: &BellmanFordSearch,
        prefix_length: usize,
        remaining: Vec<i128>,
        path: Vec<ResidualArcId>,
    ) -> Self {
        let prefix_length = prefix_length.min(search.search_order.len());
        let mut visible = vec![false; search.distances.len()];
        for node in &search.search_order[..prefix_length] {
            visible[node.as_usize()] = true;
        }
        Self {
            distances: search
                .distances
                .iter()
                .enumerate()
                .map(|(index, distance)| visible[index].then_some(*distance).flatten())
                .collect(),
            search_order: search.search_order[..prefix_length].to_vec(),
            path,
            remaining,
        }
    }
}

fn record_ssp_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: BellmanFordSspMetrics,
    metadata: FlowTraceEventMetadata,
    view: BellmanFordTraceView,
    detail: Option<(&'static str, i128)>,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let focus = ssp_trace_focus(graph, state, metadata.catalog_id, &view)?;
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.distances,
        view.search_order,
        view.path,
        view.remaining,
        trace_metrics(metrics),
    );
    recorder.record_transition_with_detail_and_focus(metadata, &snapshot, detail, focus)
}

fn ssp_trace_focus(
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    catalog_id: &str,
    view: &BellmanFordTraceView,
) -> Result<Vec<FlowTraceEntityRef>, FlowTraceError> {
    let action = catalog_id
        .rsplit_once('.')
        .map(|(_, action)| action)
        .ok_or(FlowTraceError::Precondition)?;
    let node_ref = |node: NodeIndex| {
        graph
            .node(node)
            .map(|value| FlowTraceEntityRef::Node(value.id().clone()))
            .ok_or(FlowTraceError::MissingEntity)
    };
    let residual_refs = || {
        Ok(view
            .path
            .iter()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .collect::<Vec<_>>())
    };
    match action {
        "select-source" => view
            .search_order
            .first()
            .copied()
            .map(node_ref)
            .transpose()
            .map(|value| value.into_iter().collect()),
        "inspect-residual-arc" | "augment" => residual_refs(),
        "reconstruct-path" => Ok(view
            .path
            .last()
            .cloned()
            .map(FlowTraceEntityRef::ResidualArc)
            .into_iter()
            .collect()),
        "bottleneck" => Ok(view
            .path
            .iter()
            .filter_map(|id| state.arc(id).map(|arc| (arc.capacity, id)))
            .min()
            .map(|(_, id)| FlowTraceEntityRef::ResidualArc(id.clone()))
            .into_iter()
            .collect()),
        "relax" => {
            let mut focus = residual_refs()?;
            let relaxed = view
                .search_order
                .last()
                .copied()
                .ok_or(FlowTraceError::Precondition)?;
            focus.push(node_ref(relaxed)?);
            Ok(focus)
        }
        "shortest-path" => view.search_order.iter().copied().map(node_ref).collect(),
        "optimal" => Ok(Vec::new()),
        _ => Err(FlowTraceError::Precondition),
    }
}

const fn trace_metrics(metrics: BellmanFordSspMetrics) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: metrics.relaxation_passes as u128,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: 0,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: 0,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: 0,
    }
}

struct BellmanFordSearch {
    sink: Option<NodeIndex>,
    path: Option<Vec<ResidualArcId>>,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    metrics_before: BellmanFordSspMetrics,
    checkpoints: Vec<BellmanFordSearchCheckpoint>,
}

struct BellmanFordSearchCheckpoint {
    metrics: BellmanFordSspMetrics,
    kind: BellmanFordSearchCheckpointKind,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

enum BellmanFordSearchCheckpointKind {
    Inspect {
        active_arc: ResidualArcId,
    },
    Relax {
        active_arc: ResidualArcId,
        relaxed: NodeIndex,
    },
}

/// Exact deterministic counters from potential + Dijkstra SSP.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PotentialDijkstraSspMetrics {
    /// Successful residual path augmentations.
    pub augmentations: u64,
    /// Complete reduced-cost Dijkstra searches.
    pub dijkstra_runs: u64,
    /// Vertices permanently settled across all Dijkstra searches.
    pub settled_nodes: u128,
    /// Positive residual arcs inspected by Dijkstra.
    pub residual_arc_scans: u128,
    /// Dual-potential update phases.
    pub potential_updates: u64,
}

/// Certified canonical potential + Dijkstra SSP result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PotentialDijkstraSspResult {
    /// Original-edge flows in canonical edge-ID order.
    pub flows: Vec<u64>,
    /// Independently reconstructed objective and dual certificate.
    pub certificate: MinCostFlowCertificate,
    /// Deterministic kernel counters.
    pub metrics: PotentialDijkstraSspMetrics,
}

/// Certified potential + Dijkstra SSP result with reversible events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PotentialDijkstraSspTraceResult {
    /// Same canonical result produced by the fast profile.
    pub result: PotentialDijkstraSspResult,
    /// Replay boundary at the lower-bound pseudoflow.
    pub base_snapshot: FlowTraceSnapshot,
    /// Canonical reversible event sequence.
    pub events: Vec<FlowTraceEvent>,
    /// Replay boundary after verified minimum-cost optimality.
    pub final_snapshot: FlowTraceSnapshot,
}

/// Potential + Dijkstra SSP construction or verification failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PotentialDijkstraSspError {
    /// Input exceeds the practical admission band.
    #[error("graph exceeds potential + Dijkstra SSP admission limits")]
    AdmissionLimit,
    /// The pseudo-polynomial augmentation guard was reached.
    #[error("potential + Dijkstra SSP augmentation limit reached")]
    AugmentationLimit,
    /// A feasibility precheck proved that the requested balances are impossible.
    #[error(transparent)]
    Feasibility(#[from] FeasibilityError),
    /// Residual mutation failed atomically.
    #[error(transparent)]
    Residual(#[from] ResidualError),
    /// Initial compatibility or final independent certificate failed.
    #[error(transparent)]
    Certificate(#[from] CertificateError),
    /// Checked exact arithmetic exceeded its declared domain.
    #[error("potential + Dijkstra SSP arithmetic overflow")]
    ArithmeticOverflow,
    /// A feasible precheck and the SSP residual search disagreed.
    #[error("potential + Dijkstra SSP could not route a feasible remaining imbalance")]
    MissingPath,
    /// A shortest-path predecessor chain was inconsistent.
    #[error("potential + Dijkstra SSP predecessor invariant failed")]
    PredecessorInvariant,
    /// A residual arc violated the nonnegative reduced-cost invariant.
    #[error("potential + Dijkstra SSP encountered a negative reduced cost")]
    NegativeReducedCost,
    /// Reversible trace construction contradicted algorithm state.
    #[error(transparent)]
    Trace(#[from] FlowTraceError),
}

/// Solves exact balanced min-cost flow with feasible potentials and Dijkstra.
///
/// The lower-bound pseudoflow must be optimal for its current divergence. A
/// solver-independent Bellman–Ford check reconstructs initial feasible
/// potentials across every residual component. Every later shortest-path phase
/// uses only nonnegative reduced costs and updates potentials with a distance
/// cutoff, preserving dual feasibility even for unreachable components.
///
/// # Errors
///
/// Rejects admission, feasibility, negative-cycle compatibility, arithmetic,
/// reduced-cost, path, residual mutation, work-limit, or certificate failure.
pub fn solve_potential_dijkstra_ssp(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PotentialDijkstraSspResult, PotentialDijkstraSspError> {
    solve_potential_dijkstra_ssp_internal(graph, required_divergence, false).map(|run| run.result)
}

/// Solves potential + Dijkstra SSP while reporting its feasibility precheck to
/// the enclosing execution context.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn solve_potential_dijkstra_ssp_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PotentialDijkstraSspResult, PotentialDijkstraSspError> {
    solve_potential_dijkstra_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        false,
        feasibility,
    )
    .map(|run| run.result)
}

/// Solves potential + Dijkstra SSP while recording prices and shortest paths.
///
/// # Errors
///
/// Returns the same failures as the fast profile plus trace invariant failures.
pub fn trace_potential_dijkstra_ssp(
    graph: &FlowNetwork,
    required_divergence: &[i128],
) -> Result<PotentialDijkstraSspTraceResult, PotentialDijkstraSspError> {
    let run = solve_potential_dijkstra_ssp_internal(graph, required_divergence, true)?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
    Ok(PotentialDijkstraSspTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

/// Traces potential + Dijkstra SSP while explicitly publishing its
/// feasibility precheck to the enclosing execution.
/// # Errors
///
/// Returns the algorithm-specific error when the input contract, checked
/// execution, replay, or certificate validation fails.
pub fn trace_potential_dijkstra_ssp_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PotentialDijkstraSspTraceResult, PotentialDijkstraSspError> {
    let run = solve_potential_dijkstra_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        true,
        feasibility,
    )?;
    let (base_snapshot, events, final_snapshot) = run
        .trace
        .ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
    Ok(PotentialDijkstraSspTraceResult {
        result: run.result,
        base_snapshot,
        events,
        final_snapshot,
    })
}

struct PotentialDijkstraSspInternalRun {
    result: PotentialDijkstraSspResult,
    trace: Option<(FlowTraceSnapshot, Vec<FlowTraceEvent>, FlowTraceSnapshot)>,
}

struct PotentialDijkstraWorkingState<'graph> {
    residual: ResidualState<'graph>,
    remaining: Vec<i128>,
    potentials: Vec<i128>,
    metrics: PotentialDijkstraSspMetrics,
}

fn solve_potential_dijkstra_ssp_internal(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
) -> Result<PotentialDijkstraSspInternalRun, PotentialDijkstraSspError> {
    let mut feasibility = FeasibilityExecution::untracked();
    solve_potential_dijkstra_ssp_internal_with_feasibility(
        graph,
        required_divergence,
        record_trace,
        &mut feasibility,
    )
}

fn solve_potential_dijkstra_ssp_internal_with_feasibility(
    graph: &FlowNetwork,
    required_divergence: &[i128],
    record_trace: bool,
    feasibility: &mut FeasibilityExecution,
) -> Result<PotentialDijkstraSspInternalRun, PotentialDijkstraSspError> {
    let mut work = initialize_potential_dijkstra(graph, required_divergence, feasibility)?;
    let mut recorder = start_potential_dijkstra_trace_recorder(
        graph,
        &work.residual,
        &work.remaining,
        work.metrics,
        record_trace,
    )?;
    record_potential_dijkstra_trace(
        recorder.as_mut(),
        graph,
        &work.residual,
        work.metrics,
        FlowTraceEventMetadata {
            catalog_id: "potential-dijkstra-ssp.initial-potentials",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "potential-dijkstra-ssp:initialize-feasible-potentials",
        },
        PotentialDijkstraTraceView::potentials(&work.potentials, work.remaining.clone()),
    )?;

    while let Some(source) = graph
        .node_indices()
        .find(|node| work.remaining[node.as_usize()] > 0)
    {
        run_potential_dijkstra_phase(graph, &mut work, source, &mut recorder)?;
    }
    if work.remaining.iter().any(|&value| value != 0) {
        return Err(PotentialDijkstraSspError::MissingPath);
    }
    let flows = work.residual.flows().to_vec();
    let certificate = check_min_cost_flow(graph, required_divergence, &flows)?;
    record_potential_dijkstra_trace(
        recorder.as_mut(),
        graph,
        &work.residual,
        work.metrics,
        FlowTraceEventMetadata {
            catalog_id: "potential-dijkstra-ssp.optimal",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "potential-dijkstra-ssp:return-minimum-cost-flow",
        },
        PotentialDijkstraTraceView::empty(graph, work.remaining),
    )?;
    let result = PotentialDijkstraSspResult {
        flows,
        certificate,
        metrics: work.metrics,
    };
    Ok(PotentialDijkstraSspInternalRun {
        result,
        trace: recorder.map(FlowTraceRecorder::finish),
    })
}

fn initialize_potential_dijkstra<'graph>(
    graph: &'graph FlowNetwork,
    required_divergence: &[i128],
    feasibility: &mut FeasibilityExecution,
) -> Result<PotentialDijkstraWorkingState<'graph>, PotentialDijkstraSspError> {
    validate_potential_dijkstra_admission(graph)?;
    feasibility.find_feasible_flow(graph, required_divergence, FeasibilityUse::PrecheckOnly)?;
    let lower_flows = graph
        .edges()
        .iter()
        .map(FlowEdge::lower)
        .collect::<Vec<_>>();
    let potentials = check_residual_min_cost_optimality(graph, &lower_flows)?;
    let current_divergence = divergences(graph, &lower_flows)?;
    if current_divergence.len() != required_divergence.len() {
        return Err(PotentialDijkstraSspError::MissingPath);
    }
    let remaining = required_divergence
        .iter()
        .zip(current_divergence)
        .map(|(&required, current)| {
            required
                .checked_sub(current)
                .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PotentialDijkstraWorkingState {
        residual: ResidualState::from_flows(graph, &lower_flows)?,
        remaining,
        potentials,
        metrics: PotentialDijkstraSspMetrics::default(),
    })
}

fn run_potential_dijkstra_phase(
    graph: &FlowNetwork,
    work: &mut PotentialDijkstraWorkingState<'_>,
    source: NodeIndex,
    recorder: &mut Option<FlowTraceRecorder<'_>>,
) -> Result<(), PotentialDijkstraSspError> {
    if work.metrics.augmentations >= POTENTIAL_DIJKSTRA_SSP_MAX_AUGMENTATIONS {
        return Err(PotentialDijkstraSspError::AugmentationLimit);
    }
    let search = reduced_cost_shortest_path_to_deficit(
        &work.residual,
        source,
        &work.remaining,
        &work.potentials,
        &mut work.metrics,
    )?;
    for checkpoint in &search.scan_checkpoints {
        record_potential_dijkstra_trace(
            recorder.as_mut(),
            graph,
            &work.residual,
            checkpoint.metrics,
            FlowTraceEventMetadata {
                catalog_id: "potential-dijkstra-ssp.inspect-residual-arc",
                minimum_granularity: TraceGranularityV1::Micro,
                pseudocode_line: "potential-dijkstra-ssp:inspect-one-reduced-cost-arc",
            },
            PotentialDijkstraTraceView {
                labels: checkpoint.distances.clone(),
                search_order: checkpoint.search_order.clone(),
                path: vec![checkpoint.active_arc.clone()],
                remaining: work.remaining.clone(),
            },
        )?;
    }
    record_potential_dijkstra_trace(
        recorder.as_mut(),
        graph,
        &work.residual,
        work.metrics,
        FlowTraceEventMetadata {
            catalog_id: "potential-dijkstra-ssp.shortest-path",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "potential-dijkstra-ssp:dijkstra-reduced-costs",
        },
        PotentialDijkstraTraceView::from_search(&search, work.remaining.clone()),
    )?;
    let sink = search.sink.ok_or(PotentialDijkstraSspError::MissingPath)?;
    let path = search
        .path
        .clone()
        .ok_or(PotentialDijkstraSspError::MissingPath)?;
    update_feasible_potentials(
        &mut work.potentials,
        &search.distances,
        sink,
        &mut work.metrics,
    )?;
    record_potential_dijkstra_trace(
        recorder.as_mut(),
        graph,
        &work.residual,
        work.metrics,
        FlowTraceEventMetadata {
            catalog_id: "potential-dijkstra-ssp.update-potentials",
            minimum_granularity: TraceGranularityV1::Phase,
            pseudocode_line: "potential-dijkstra-ssp:update-potentials",
        },
        PotentialDijkstraTraceView {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order: search.search_order.clone(),
            path: path.clone(),
            remaining: work.remaining.clone(),
        },
    )?;
    augment_to_deficit_dijkstra(
        &mut work.residual,
        &path,
        source,
        sink,
        &mut work.remaining,
        &mut work.metrics,
    )?;
    validate_reduced_costs_nonnegative(&work.residual, &work.potentials)?;
    record_potential_dijkstra_trace(
        recorder.as_mut(),
        graph,
        &work.residual,
        work.metrics,
        FlowTraceEventMetadata {
            catalog_id: "potential-dijkstra-ssp.augment",
            minimum_granularity: TraceGranularityV1::Operation,
            pseudocode_line: "potential-dijkstra-ssp:augment-shortest-path",
        },
        PotentialDijkstraTraceView {
            labels: work.potentials.iter().copied().map(Some).collect(),
            search_order: search.search_order,
            path,
            remaining: work.remaining.clone(),
        },
    )?;
    Ok(())
}

fn validate_potential_dijkstra_admission(
    graph: &FlowNetwork,
) -> Result<(), PotentialDijkstraSspError> {
    if graph.nodes().len() > POTENTIAL_DIJKSTRA_SSP_MAX_NODES
        || graph.edges().len() > POTENTIAL_DIJKSTRA_SSP_MAX_EDGES
    {
        return Err(PotentialDijkstraSspError::AdmissionLimit);
    }
    Ok(())
}

struct PotentialDijkstraSearch {
    sink: Option<NodeIndex>,
    path: Option<Vec<ResidualArcId>>,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    scan_checkpoints: Vec<PotentialDijkstraScanCheckpoint>,
}

struct PotentialDijkstraScanCheckpoint {
    metrics: PotentialDijkstraSspMetrics,
    active_arc: ResidualArcId,
    distances: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
}

#[allow(clippy::too_many_lines)]
fn reduced_cost_shortest_path_to_deficit(
    state: &ResidualState<'_>,
    source: NodeIndex,
    remaining: &[i128],
    potentials: &[i128],
    metrics: &mut PotentialDijkstraSspMetrics,
) -> Result<PotentialDijkstraSearch, PotentialDijkstraSspError> {
    let node_count = state.graph().nodes().len();
    if potentials.len() != node_count {
        return Err(PotentialDijkstraSspError::PredecessorInvariant);
    }
    metrics.dijkstra_runs = metrics
        .dijkstra_runs
        .checked_add(1)
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    let mut distances = vec![None; node_count];
    let mut predecessor = vec![None; node_count];
    let mut settled = vec![false; node_count];
    let mut search_order = Vec::new();
    let mut scan_checkpoints = Vec::new();
    let mut heap = BinaryHeap::new();
    distances[source.as_usize()] = Some(0_i128);
    heap.push(Reverse((0_i128, source)));
    while let Some(Reverse((distance, node))) = heap.pop() {
        if settled[node.as_usize()] || distances[node.as_usize()] != Some(distance) {
            continue;
        }
        settled[node.as_usize()] = true;
        search_order.push(node);
        metrics.settled_nodes = metrics
            .settled_nodes
            .checked_add(1)
            .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
        for arc in state.outgoing_arcs(node) {
            metrics.residual_arc_scans = metrics
                .residual_arc_scans
                .checked_add(1)
                .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
            let reduced_cost = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
            if reduced_cost < 0 {
                return Err(PotentialDijkstraSspError::NegativeReducedCost);
            }
            let candidate = distance
                .checked_add(reduced_cost)
                .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
            if distances[arc.to.as_usize()].is_none_or(|current| candidate < current) {
                distances[arc.to.as_usize()] = Some(candidate);
                predecessor[arc.to.as_usize()] = Some(arc.id.clone());
                heap.push(Reverse((candidate, arc.to)));
            }
            scan_checkpoints.push(PotentialDijkstraScanCheckpoint {
                metrics: *metrics,
                active_arc: arc.id,
                distances: distances.clone(),
                search_order: search_order.clone(),
            });
        }
    }
    let mut sink = None;
    for node in state
        .graph()
        .node_indices()
        .filter(|node| remaining[node.as_usize()] < 0)
    {
        let Some(distance) = distances[node.as_usize()] else {
            continue;
        };
        let original_cost = distance
            .checked_add(potentials[node.as_usize()])
            .and_then(|value| value.checked_sub(potentials[source.as_usize()]))
            .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
        let candidate = (original_cost, node);
        if sink.is_none_or(|current| candidate < current) {
            sink = Some(candidate);
        }
    }
    let Some((_, sink)) = sink else {
        return Ok(PotentialDijkstraSearch {
            sink: None,
            path: None,
            distances,
            search_order,
            scan_checkpoints,
        });
    };
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor[cursor.as_usize()]
            .clone()
            .ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
        let arc = state
            .arc(&id)
            .ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
        if arc.to != cursor {
            return Err(PotentialDijkstraSspError::PredecessorInvariant);
        }
        cursor = arc.from;
        reversed.push(id);
        if reversed.len() > node_count {
            return Err(PotentialDijkstraSspError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(PotentialDijkstraSearch {
        sink: Some(sink),
        path: Some(reversed),
        distances,
        search_order,
        scan_checkpoints,
    })
}

fn update_feasible_potentials(
    potentials: &mut [i128],
    distances: &[Option<i128>],
    sink: NodeIndex,
    metrics: &mut PotentialDijkstraSspMetrics,
) -> Result<(), PotentialDijkstraSspError> {
    if potentials.len() != distances.len() {
        return Err(PotentialDijkstraSspError::PredecessorInvariant);
    }
    let cutoff =
        distances[sink.as_usize()].ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
    for (potential, distance) in potentials.iter_mut().zip(distances) {
        let adjustment = distance.map_or(cutoff, |value| value.min(cutoff));
        *potential = potential
            .checked_add(adjustment)
            .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    }
    metrics.potential_updates = metrics
        .potential_updates
        .checked_add(1)
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    Ok(())
}

fn augment_to_deficit_dijkstra(
    state: &mut ResidualState<'_>,
    path: &[ResidualArcId],
    source: NodeIndex,
    sink: NodeIndex,
    remaining: &mut [i128],
    metrics: &mut PotentialDijkstraSspMetrics,
) -> Result<(), PotentialDijkstraSspError> {
    let bottleneck = path
        .iter()
        .filter_map(|id| state.arc(id).map(|arc| arc.capacity))
        .min()
        .ok_or(PotentialDijkstraSspError::PredecessorInvariant)?;
    let source_remaining = u128::try_from(remaining[source.as_usize()])
        .map_err(|_| PotentialDijkstraSspError::ArithmeticOverflow)?;
    let sink_remaining = remaining[sink.as_usize()]
        .checked_neg()
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    let amount = u64::try_from(
        u128::from(bottleneck)
            .min(source_remaining)
            .min(sink_remaining),
    )
    .map_err(|_| PotentialDijkstraSspError::ArithmeticOverflow)?;
    if amount == 0 {
        return Err(PotentialDijkstraSspError::MissingPath);
    }
    state.augment(path, amount)?;
    remaining[source.as_usize()] = remaining[source.as_usize()]
        .checked_sub(i128::from(amount))
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    remaining[sink.as_usize()] = remaining[sink.as_usize()]
        .checked_add(i128::from(amount))
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    metrics.augmentations = metrics
        .augmentations
        .checked_add(1)
        .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
    Ok(())
}

fn validate_reduced_costs_nonnegative(
    state: &ResidualState<'_>,
    potentials: &[i128],
) -> Result<(), PotentialDijkstraSspError> {
    for node in state.graph().node_indices() {
        for arc in state.outgoing_arcs(node) {
            let reduced = arc
                .cost
                .checked_add(potentials[arc.from.as_usize()])
                .and_then(|value| value.checked_sub(potentials[arc.to.as_usize()]))
                .ok_or(PotentialDijkstraSspError::ArithmeticOverflow)?;
            if reduced < 0 {
                return Err(PotentialDijkstraSspError::NegativeReducedCost);
            }
        }
    }
    Ok(())
}

fn start_potential_dijkstra_trace_recorder<'graph>(
    graph: &'graph FlowNetwork,
    state: &ResidualState<'_>,
    remaining: &[i128],
    metrics: PotentialDijkstraSspMetrics,
    record_trace: bool,
) -> Result<Option<FlowTraceRecorder<'graph>>, FlowTraceError> {
    if !record_trace {
        return Ok(None);
    }
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        vec![None; graph.nodes().len()],
        Vec::new(),
        Vec::new(),
        remaining.to_vec(),
        potential_dijkstra_trace_metrics(metrics),
    );
    FlowTraceRecorder::new(graph, snapshot).map(Some)
}

struct PotentialDijkstraTraceView {
    labels: Vec<Option<i128>>,
    search_order: Vec<NodeIndex>,
    path: Vec<ResidualArcId>,
    remaining: Vec<i128>,
}

impl PotentialDijkstraTraceView {
    fn from_search(search: &PotentialDijkstraSearch, remaining: Vec<i128>) -> Self {
        Self {
            labels: search.distances.clone(),
            search_order: search.search_order.clone(),
            path: search.path.clone().unwrap_or_default(),
            remaining,
        }
    }

    fn potentials(potentials: &[i128], remaining: Vec<i128>) -> Self {
        Self {
            labels: potentials.iter().copied().map(Some).collect(),
            search_order: Vec::new(),
            path: Vec::new(),
            remaining,
        }
    }

    fn empty(graph: &FlowNetwork, remaining: Vec<i128>) -> Self {
        Self {
            labels: vec![None; graph.nodes().len()],
            search_order: Vec::new(),
            path: Vec::new(),
            remaining,
        }
    }
}

fn record_potential_dijkstra_trace(
    recorder: Option<&mut FlowTraceRecorder<'_>>,
    graph: &FlowNetwork,
    state: &ResidualState<'_>,
    metrics: PotentialDijkstraSspMetrics,
    metadata: FlowTraceEventMetadata,
    view: PotentialDijkstraTraceView,
) -> Result<(), FlowTraceError> {
    let Some(recorder) = recorder else {
        return Ok(());
    };
    let focus_arcs = view.path.clone();
    let search_order = if view.path.is_empty() {
        view.search_order
    } else {
        residual_path_node_order(state, &view.path)?
    };
    let snapshot = FlowTraceSnapshot::capture(
        graph,
        state,
        view.labels,
        search_order,
        view.path,
        view.remaining,
        potential_dijkstra_trace_metrics(metrics),
    );
    recorder.record_transition_with_detail_and_focus(
        metadata,
        &snapshot,
        None,
        residual_arc_entity_refs(graph, state, &focus_arcs)?,
    )
}

const fn potential_dijkstra_trace_metrics(
    metrics: PotentialDijkstraSspMetrics,
) -> FlowTraceMetrics {
    FlowTraceMetrics {
        bfs_runs: 0,
        relaxation_passes: 0,
        residual_arc_scans: metrics.residual_arc_scans,
        augmentations: metrics.augmentations as u128,
        path_searches: metrics.dijkstra_runs as u128,
        scaling_phases: 0,
        blocking_flow_phases: 0,
        relabels: metrics.potential_updates as u128,
        retreats: 0,
        reverse_bfs_runs: 0,
        gap_terminations: 0,
        pushes: 0,
        saturating_pushes: 0,
        nonsaturating_pushes: 0,
        discharges: 0,
        active_vertex_selections: metrics.settled_nodes,
    }
}

fn shortest_path_to_deficit(
    state: &ResidualState<'_>,
    source: NodeIndex,
    remaining: &[i128],
    metrics: &mut BellmanFordSspMetrics,
) -> Result<BellmanFordSearch, BellmanFordSspError> {
    let metrics_before = *metrics;
    let node_count = state.graph().nodes().len();
    let mut distance = vec![None; node_count];
    let mut predecessor = vec![None; node_count];
    let mut search_order = vec![source];
    let mut checkpoints = Vec::new();
    distance[source.as_usize()] = Some(0_i128);
    for _ in 1..node_count {
        metrics.relaxation_passes = metrics
            .relaxation_passes
            .checked_add(1)
            .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
        let mut changed = false;
        for node in state.graph().node_indices() {
            let Some(node_distance) = distance[node.as_usize()] else {
                continue;
            };
            for arc in state.outgoing_arcs(node) {
                metrics.residual_arc_scans = metrics
                    .residual_arc_scans
                    .checked_add(1)
                    .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
                let candidate = node_distance
                    .checked_add(arc.cost)
                    .ok_or(BellmanFordSspError::ArithmeticOverflow)?;
                let relaxes = distance[arc.to.as_usize()].is_none_or(|current| candidate < current);
                checkpoints.push(BellmanFordSearchCheckpoint {
                    metrics: *metrics,
                    kind: BellmanFordSearchCheckpointKind::Inspect {
                        active_arc: arc.id.clone(),
                    },
                    distances: distance.clone(),
                    search_order: search_order.clone(),
                });
                if relaxes {
                    distance[arc.to.as_usize()] = Some(candidate);
                    predecessor[arc.to.as_usize()] = Some(arc.id.clone());
                    search_order.push(arc.to);
                    changed = true;
                    checkpoints.push(BellmanFordSearchCheckpoint {
                        metrics: *metrics,
                        kind: BellmanFordSearchCheckpointKind::Relax {
                            active_arc: arc.id,
                            relaxed: arc.to,
                        },
                        distances: distance.clone(),
                        search_order: search_order.clone(),
                    });
                }
            }
        }
        if !changed {
            break;
        }
    }
    let sink = state
        .graph()
        .node_indices()
        .filter(|node| remaining[node.as_usize()] < 0)
        .filter_map(|node| distance[node.as_usize()].map(|cost| (cost, node)))
        .min_by_key(|&(cost, node)| (cost, node));
    let Some((_, sink)) = sink else {
        return Ok(BellmanFordSearch {
            sink: None,
            path: None,
            distances: distance,
            search_order,
            metrics_before,
            checkpoints,
        });
    };
    let mut reversed = Vec::new();
    let mut cursor = sink;
    while cursor != source {
        let id = predecessor[cursor.as_usize()]
            .clone()
            .ok_or(BellmanFordSspError::PredecessorInvariant)?;
        let arc = state
            .arc(&id)
            .ok_or(BellmanFordSspError::PredecessorInvariant)?;
        if arc.to != cursor {
            return Err(BellmanFordSspError::PredecessorInvariant);
        }
        cursor = arc.from;
        reversed.push(id);
        if reversed.len() > node_count {
            return Err(BellmanFordSspError::PredecessorInvariant);
        }
    }
    reversed.reverse();
    Ok(BellmanFordSearch {
        sink: Some(sink),
        path: Some(reversed),
        distances: distance,
        search_order,
        metrics_before,
        checkpoints,
    })
}

#[cfg(test)]
mod tests {
    use crate::certificate::fixed_flow_divergences;
    use crate::model::{EdgeId, FlowNode, NodeId, UnresolvedFlowEdge};
    use crate::trace::{FlowTraceDirection, apply_trace_event};

    use super::*;

    fn network(nodes: &[(&str, i64)], edges: &[(&str, &str, &str, u64, u64, i64)]) -> FlowNetwork {
        FlowNetwork::new(
            nodes
                .iter()
                .map(|&(id, supply)| FlowNode::new(NodeId::parse(id).expect("node id"), supply))
                .collect(),
            edges
                .iter()
                .map(
                    |&(id, from, to, lower, capacity, cost)| UnresolvedFlowEdge {
                        id: EdgeId::parse(id).expect("edge id"),
                        from: NodeId::parse(from).expect("tail"),
                        to: NodeId::parse(to).expect("head"),
                        lower,
                        capacity,
                        cost,
                    },
                )
                .collect(),
        )
        .expect("valid graph")
    }

    fn terminals(graph: &FlowNetwork) -> (NodeIndex, NodeIndex) {
        (
            graph
                .node_index(&NodeId::parse("s").expect("source id"))
                .expect("source"),
            graph
                .node_index(&NodeId::parse("t").expect("sink id"))
                .expect("sink"),
        )
    }

    fn flow_for(graph: &FlowNetwork, result: &BellmanFordSspResult, edge_id: &str) -> u64 {
        let index = graph
            .edge_index(&EdgeId::parse(edge_id).expect("edge id"))
            .expect("edge");
        result.flows[index.as_usize()]
    }

    #[test]
    fn accepts_negative_edge_without_cycle_and_finds_minimum_cost() {
        let graph = network(
            &[("a", 0), ("s", 0), ("t", 0)],
            &[
                ("negative", "s", "a", 0, 2, -3),
                ("finish", "a", "t", 0, 2, 4),
                ("direct", "s", "t", 0, 2, 5),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 2).expect("target");
        let result = solve_bellman_ford_ssp(&graph, &target).expect("minimum cost");

        assert_eq!(result.certificate.total_cost, 2);
        assert_eq!(flow_for(&graph, &result, "negative"), 2);
        assert_eq!(flow_for(&graph, &result, "finish"), 2);
        assert_eq!(flow_for(&graph, &result, "direct"), 0);
    }

    #[test]
    fn rejects_disconnected_negative_cycle_before_augmentation() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0)],
            &[("path", "s", "t", 0, 1, 1), ("bad", "x", "x", 0, 1, -1)],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");

        assert_eq!(
            solve_bellman_ford_ssp(&graph, &target),
            Err(BellmanFordSspError::Certificate(
                CertificateError::NegativeCycle
            ))
        );
    }

    #[test]
    fn combines_lower_bounds_supplies_and_fixed_terminal_flow() {
        let graph = network(
            &[("a", -1), ("s", 1), ("t", 0)],
            &[
                ("sa", "s", "a", 1, 3, -2),
                ("at", "a", "t", 0, 2, 3),
                ("st", "s", "t", 0, 2, 5),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");
        let result = solve_bellman_ford_ssp(&graph, &target).expect("minimum cost");

        check_min_cost_flow(&graph, &target, &result.flows).expect("certificate");
    }

    #[test]
    fn trace_replays_both_directions_and_matches_fast_result() {
        let graph = network(
            &[("a", 0), ("b", 0), ("s", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 2, -2),
                ("at", "a", "t", 0, 2, 3),
                ("sb", "s", "b", 0, 2, 1),
                ("bt", "b", "t", 0, 2, 2),
                ("st", "s", "t", 0, 3, 7),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 4).expect("target");
        let fast = solve_bellman_ford_ssp(&graph, &target).expect("fast result");
        let traced = trace_bellman_ford_ssp(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "bellman-ford-ssp.shortest-path")
        );
        assert!(
            traced
                .events
                .iter()
                .any(|event| event.catalog_id == "bellman-ford-ssp.augment")
        );

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        assert!(replay.remaining_divergence.iter().all(|&value| value == 0));
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }

    #[test]
    fn successive_shortest_path_has_dedicated_identity_and_independent_prefix_checker() {
        let graph = network(
            &[("a", 0), ("b", 0), ("s", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 2, -2),
                ("at", "a", "t", 0, 2, 3),
                ("sb", "s", "b", 0, 2, 1),
                ("bt", "b", "t", 0, 2, 2),
                ("st", "s", "t", 0, 3, 7),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 4).expect("target");
        let fast = solve_successive_shortest_path(&graph, &target).expect("fast SSP");
        let traced = trace_successive_shortest_path(&graph, &target).expect("trace SSP");

        assert_eq!(traced.result, fast);
        assert!(traced.events.iter().all(|event| {
            event.catalog_id.starts_with("successive-shortest-path.")
                && event
                    .pseudocode_line
                    .starts_with("successive-shortest-path:")
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "successive-shortest-path.relax"
                && event.minimum_granularity == TraceGranularityV1::Micro
                && event
                    .entity_refs
                    .iter()
                    .filter(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                    .count()
                    == 1
                && event
                    .entity_refs
                    .iter()
                    .filter(|entity| matches!(entity, FlowTraceEntityRef::Node(_)))
                    .count()
                    == 1
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "successive-shortest-path.reconstruct-path"
                && event.minimum_granularity == TraceGranularityV1::Micro
                && event
                    .entity_refs
                    .iter()
                    .filter(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                    .count()
                    == 1
        }));
        assert!(traced.events.iter().any(|event| {
            event.catalog_id == "successive-shortest-path.bottleneck"
                && event.minimum_granularity == TraceGranularityV1::Micro
                && event
                    .entity_refs
                    .iter()
                    .filter(|entity| matches!(entity, FlowTraceEntityRef::ResidualArc(_)))
                    .count()
                    == 1
        }));
        assert!(traced.events.iter().all(|event| {
            event.minimum_granularity != TraceGranularityV1::Operation
                || event.catalog_id == "successive-shortest-path.augment"
        }));
        let bottleneck_index = traced
            .events
            .iter()
            .position(|event| event.catalog_id == "successive-shortest-path.bottleneck")
            .expect("precommit bottleneck event");
        let augment_index = traced
            .events
            .iter()
            .position(|event| event.catalog_id == "successive-shortest-path.augment")
            .expect("commit event");
        assert!(bottleneck_index < augment_index);
        assert!(
            traced.events[bottleneck_index]
                .patches
                .iter()
                .all(|patch| !matches!(patch, crate::trace::FlowTracePatch::EdgeFlow { .. }))
        );
        assert!(
            traced.events[augment_index]
                .detail
                .as_ref()
                .is_some_and(|detail| detail.label == "amount" && detail.value > 0)
        );
        check_successive_shortest_path_trace(&graph, &target, &traced)
            .expect("source invariant and replay checker");

        let mut corrupted = traced.clone();
        corrupted.events[0].catalog_id = "bellman-ford-ssp.shortest-path".to_owned();
        assert_eq!(
            check_successive_shortest_path_trace(&graph, &target, &corrupted),
            Err(SuccessiveShortestPathTraceCheckError::UnexpectedEvent)
        );
    }

    #[test]
    fn potential_dijkstra_accepts_negative_edges_and_matches_bellman_ford() {
        let graph = network(
            &[("a", 0), ("b", 0), ("s", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 2, -4),
                ("at", "a", "t", 0, 2, 5),
                ("sb", "s", "b", 0, 2, 1),
                ("bt", "b", "t", 0, 2, 2),
                ("st", "s", "t", 0, 3, 8),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 4).expect("target");

        let bellman = solve_bellman_ford_ssp(&graph, &target).expect("Bellman-Ford SSP");
        let dijkstra =
            solve_potential_dijkstra_ssp(&graph, &target).expect("potential Dijkstra SSP");

        assert_eq!(dijkstra.flows, bellman.flows);
        assert_eq!(dijkstra.certificate.total_cost, 8);
        assert_eq!(dijkstra.metrics.augmentations, 2);
        assert_eq!(dijkstra.metrics.dijkstra_runs, 2);
        assert_eq!(dijkstra.metrics.potential_updates, 2);
        assert!(dijkstra.metrics.settled_nodes >= 2);
        assert!(dijkstra.metrics.residual_arc_scans > 0);
    }

    #[test]
    fn potential_update_preserves_unreachable_residual_components() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0), ("y", 0)],
            &[("path", "s", "t", 0, 2, 3), ("xy", "x", "y", 0, 1, -7)],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 2).expect("target");

        let result =
            solve_potential_dijkstra_ssp(&graph, &target).expect("disconnected component safe");

        assert_eq!(result.certificate.total_cost, 6);
        check_min_cost_flow(&graph, &target, &result.flows).expect("independent certificate");
    }

    #[test]
    fn potential_dijkstra_selects_deficit_by_original_not_reduced_distance() {
        let graph = network(
            &[("a-deficit", -1), ("s", 2), ("z-deficit", -1)],
            &[
                ("to-a", "s", "a-deficit", 0, 1, -3),
                ("to-z", "s", "z-deficit", 0, 1, -4),
            ],
        );
        let target = crate::certificate::supply_divergences(&graph).expect("balanced target");
        let traced = trace_potential_dijkstra_ssp(&graph, &target).expect("minimum cost trace");
        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("replay search boundary");
            if event.catalog_id == "potential-dijkstra-ssp.shortest-path" {
                break;
            }
        }
        let selected = &replay.active_path;

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].original_edge().as_str(), "to-z");
        assert_eq!(traced.result.certificate.total_cost, -7);
    }

    #[test]
    fn potential_dijkstra_rejects_negative_cycle_before_search() {
        let graph = network(
            &[("s", 0), ("t", 0), ("x", 0)],
            &[("path", "s", "t", 0, 1, 1), ("bad", "x", "x", 0, 1, -1)],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");

        assert_eq!(
            solve_potential_dijkstra_ssp(&graph, &target),
            Err(PotentialDijkstraSspError::Certificate(
                CertificateError::NegativeCycle
            ))
        );
    }

    #[test]
    fn potential_dijkstra_combines_lower_bounds_supplies_and_fixed_flow() {
        let graph = network(
            &[("a", -1), ("s", 1), ("t", 0)],
            &[
                ("sa", "s", "a", 1, 3, -2),
                ("at", "a", "t", 0, 2, 3),
                ("st", "s", "t", 0, 2, 5),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 1).expect("target");

        let result = solve_potential_dijkstra_ssp(&graph, &target).expect("minimum cost");

        check_min_cost_flow(&graph, &target, &result.flows).expect("certificate");
    }

    #[test]
    fn potential_dijkstra_trace_replays_and_exposes_price_phases() {
        let graph = network(
            &[("a", 0), ("b", 0), ("s", 0), ("t", 0)],
            &[
                ("sa", "s", "a", 0, 2, -2),
                ("at", "a", "t", 0, 2, 3),
                ("sb", "s", "b", 0, 2, 1),
                ("bt", "b", "t", 0, 2, 2),
                ("st", "s", "t", 0, 3, 7),
            ],
        );
        let (source, sink) = terminals(&graph);
        let target = fixed_flow_divergences(&graph, source, sink, 4).expect("target");
        let fast = solve_potential_dijkstra_ssp(&graph, &target).expect("fast result");
        let traced = trace_potential_dijkstra_ssp(&graph, &target).expect("trace result");

        assert_eq!(traced.result, fast);
        for catalog_id in [
            "potential-dijkstra-ssp.initial-potentials",
            "potential-dijkstra-ssp.shortest-path",
            "potential-dijkstra-ssp.update-potentials",
            "potential-dijkstra-ssp.augment",
            "potential-dijkstra-ssp.optimal",
        ] {
            assert!(
                traced
                    .events
                    .iter()
                    .any(|event| event.catalog_id == catalog_id),
                "missing {catalog_id}"
            );
        }
        let shortest_path = traced
            .events
            .iter()
            .find(|event| event.catalog_id == "potential-dijkstra-ssp.shortest-path")
            .expect("shortest-path event");
        assert!(shortest_path.patches.iter().any(|patch| matches!(
            patch,
            crate::trace::FlowTracePatch::ActivePath { after, .. } if !after.is_empty()
        )));

        let mut replay = traced.base_snapshot.clone();
        for event in &traced.events {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Forward)
                .expect("forward event");
        }
        assert_eq!(replay, traced.final_snapshot);
        assert_eq!(replay.flows, fast.flows);
        assert!(replay.remaining_divergence.iter().all(|&value| value == 0));
        for event in traced.events.iter().rev() {
            apply_trace_event(&graph, &mut replay, event, FlowTraceDirection::Reverse)
                .expect("reverse event");
        }
        assert_eq!(replay, traced.base_snapshot);
    }
}
